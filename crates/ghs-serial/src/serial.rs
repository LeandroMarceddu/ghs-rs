//! Operating-system serial transport with a dedicated reader thread.

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};
use serialport::{ClearBuffer, SerialPort};
use tracing::{debug, error};

use crate::TransportError;
use crate::config::SerialConfig;
use crate::transport::{Transport, TransportEvent};

/// How long a single `read` may block before the reader re-checks shutdown.
///
/// Short enough to stop promptly, long enough that an idle port costs only a
/// few dozen wakeups per second.
const READ_POLL_TIMEOUT: Duration = Duration::from_millis(25);

/// Size of each read from the port. Protocol frames are much smaller; this
/// simply bounds each channel message.
const READ_BUFFER_BYTES: usize = 4096;

/// Back-off used if a platform reports end-of-file instead of a timeout.
const ZERO_READ_BACKOFF: Duration = Duration::from_millis(2);

/// State shared with the reader thread.
#[derive(Debug, Default)]
struct ReaderState {
    open: AtomicBool,
    shutdown: AtomicBool,
}

/// A serial port opened for grblHAL communication.
///
/// The port is opened and configured once. A dedicated reader thread owns a
/// cloned handle and publishes [`TransportEvent`] values; the transport keeps
/// the original handle for writes and for closing the port.
///
/// The receiver returned by [`SerialTransport::open`] and
/// [`Transport::recv_timeout`] share one event queue. Use one style of
/// consumption per link, not both: the engine will normally keep the receiver
/// for `select!`, while `recv_timeout` is convenient for single-link callers.
pub struct SerialTransport {
    port_name: String,
    writer: Mutex<Option<Box<dyn SerialPort>>>,
    events: Receiver<TransportEvent>,
    state: Arc<ReaderState>,
    reader: Option<JoinHandle<()>>,
}

impl SerialTransport {
    /// Opens and configures a serial port.
    ///
    /// Applies the baud rate, character format, flow control, timeout, DTR and
    /// RTS settings, then clears both buffers before starting the reader.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::InvalidConfig`] for a rejected configuration,
    /// [`TransportError::Serial`] when the port cannot be opened or configured,
    /// and [`TransportError::Io`] when the reader thread cannot be started.
    pub fn open(config: SerialConfig) -> Result<(Self, Receiver<TransportEvent>), TransportError> {
        Self::open_internal(config, true)
    }

    /// Opens a port, optionally skipping DTR/RTS.
    ///
    /// Tests use `apply_line_control = false` because pseudo-terminals do not
    /// implement the modem-control ioctls.
    fn open_internal(
        config: SerialConfig,
        apply_line_control: bool,
    ) -> Result<(Self, Receiver<TransportEvent>), TransportError> {
        config.validate()?;

        let mut port = serialport::new(&config.port, config.baud)
            .data_bits(config.data_bits.native())
            .parity(config.parity.native())
            .stop_bits(config.stop_bits.native())
            .flow_control(config.flow_control.native())
            .timeout(READ_POLL_TIMEOUT)
            .open()?;

        if apply_line_control {
            port.write_data_terminal_ready(config.dtr)
                .map_err(|error| {
                    TransportError::Serial(serialport::Error::new(
                        error.kind,
                        format!("failed to set DTR: {}", error.description),
                    ))
                })?;
            port.write_request_to_send(config.rts).map_err(|error| {
                TransportError::Serial(serialport::Error::new(
                    error.kind,
                    format!("failed to set RTS: {}", error.description),
                ))
            })?;
        }

        port.clear(ClearBuffer::All).map_err(|error| {
            TransportError::Serial(serialport::Error::new(
                error.kind,
                format!("failed to clear port buffers: {}", error.description),
            ))
        })?;

        let mut reader = port.try_clone()?;
        reader.set_timeout(READ_POLL_TIMEOUT)?;

        let state = Arc::new(ReaderState::default());
        state.open.store(true, Ordering::Release);

        let (sender, receiver) = unbounded();
        let reader_state = Arc::clone(&state);
        let spawn = thread::Builder::new()
            .name("ghs-serial-rx".into())
            .spawn(move || reader_loop(&mut *reader, &reader_state, &sender));

        let handle = match spawn {
            Ok(handle) => handle,
            Err(error) => {
                state.shutdown.store(true, Ordering::Release);
                state.open.store(false, Ordering::Release);
                drop(port);
                return Err(TransportError::Io(error));
            }
        };

        debug!(port = %config.port, baud = config.baud, "serial port opened");

        let transport = Self {
            port_name: config.port,
            writer: Mutex::new(Some(port)),
            events: receiver.clone(),
            state,
            reader: Some(handle),
        };
        Ok((transport, receiver))
    }

    /// Name of the port this transport was opened with.
    #[must_use]
    pub fn port_name(&self) -> &str {
        &self.port_name
    }
}

impl Transport for SerialTransport {
    fn send(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        if !self.is_open() {
            return Err(TransportError::Closed);
        }
        if bytes.is_empty() {
            return Ok(());
        }

        let mut writer = self
            .writer
            .lock()
            .map_err(|_| TransportError::Poisoned("serial writer mutex"))?;
        let Some(port) = writer.as_mut() else {
            return Err(TransportError::Closed);
        };

        if let Err(error) = port.write_all(bytes) {
            self.state.open.store(false, Ordering::Release);
            return Err(TransportError::Io(error));
        }
        port.flush()?;
        Ok(())
    }

    fn recv_timeout(&self, timeout: Duration) -> Result<TransportEvent, TransportError> {
        match self.events.recv_timeout(timeout) {
            Ok(event) => Ok(event),
            Err(RecvTimeoutError::Timeout) => Err(TransportError::Timeout(timeout)),
            // The reader always publishes `Closed` before exiting. A
            // disconnected channel therefore means the same thing.
            Err(RecvTimeoutError::Disconnected) => Ok(TransportEvent::Closed),
        }
    }

    fn close(&mut self) {
        self.state.shutdown.store(true, Ordering::Release);
        self.state.open.store(false, Ordering::Release);

        {
            let mut writer = match self.writer.lock() {
                Ok(writer) => writer,
                Err(poisoned) => poisoned.into_inner(),
            };
            writer.take();
        }

        if let Some(reader) = self.reader.take()
            && reader.join().is_err()
        {
            error!(port = %self.port_name, "serial reader thread panicked");
        }
    }

    fn is_open(&self) -> bool {
        self.state.open.load(Ordering::Acquire)
    }
}

impl Drop for SerialTransport {
    fn drop(&mut self) {
        self.close();
    }
}

/// Reads until shutdown, forwarding bytes and reporting the final close.
fn reader_loop<R>(reader: &mut R, state: &ReaderState, sender: &Sender<TransportEvent>)
where
    R: Read + ?Sized,
{
    let mut buffer = vec![0u8; READ_BUFFER_BYTES];
    let mut receiver_alive = true;

    loop {
        if state.shutdown.load(Ordering::Acquire) {
            break;
        }

        match reader.read(&mut buffer) {
            Ok(0) => thread::sleep(ZERO_READ_BACKOFF),
            Ok(count) => {
                // Trust but verify: a misbehaving driver must not make us
                // index past the buffer.
                let count = count.min(buffer.len());
                let event = TransportEvent::Bytes(buffer[..count].to_vec());
                if sender.send(event).is_err() {
                    receiver_alive = false;
                    break;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if is_timeout(&error) => {}
            Err(error) => {
                let _ = sender.send(TransportEvent::Error(format!(
                    "serial read failed: {error}"
                )));
                break;
            }
        }
    }

    state.open.store(false, Ordering::Release);
    if receiver_alive {
        let _ = sender.send(TransportEvent::Closed);
    }
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::AtomicUsize;

    use super::*;

    struct TimeoutReader {
        reads: Arc<AtomicUsize>,
        delay: Duration,
    }

    impl Read for TimeoutReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            thread::sleep(self.delay);
            Err(io::Error::new(io::ErrorKind::TimedOut, "read timeout"))
        }
    }

    struct ScriptedReader {
        chunks: VecDeque<Vec<u8>>,
    }

    impl Read for ScriptedReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            match self.chunks.pop_front() {
                Some(chunk) => {
                    let count = chunk.len().min(buffer.len());
                    buffer[..count].copy_from_slice(&chunk[..count]);
                    Ok(count)
                }
                None => {
                    thread::sleep(Duration::from_millis(2));
                    Err(io::Error::new(io::ErrorKind::TimedOut, "read timeout"))
                }
            }
        }
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("device unplugged"))
        }
    }

    #[test]
    fn reader_loop_stops_without_spurious_errors() {
        let state = Arc::new(ReaderState::default());
        state.open.store(true, Ordering::Release);
        let reads = Arc::new(AtomicUsize::new(0));
        let (sender, receiver) = unbounded();

        let thread_state = Arc::clone(&state);
        let thread_reads = Arc::clone(&reads);
        let handle = thread::spawn(move || {
            let mut reader = TimeoutReader {
                reads: thread_reads,
                delay: Duration::from_millis(5),
            };
            reader_loop(&mut reader, &thread_state, &sender);
        });

        thread::sleep(Duration::from_millis(60));
        state.shutdown.store(true, Ordering::Release);
        handle.join().expect("reader thread should stop");

        assert!(!state.open.load(Ordering::Acquire));
        let events: Vec<_> = receiver.try_iter().collect();
        assert_eq!(events, vec![TransportEvent::Closed]);

        let reads = reads.load(Ordering::Relaxed);
        assert!(
            reads < 100,
            "reader likely busy-spun: {reads} reads in 60 ms"
        );
    }

    #[test]
    fn reader_loop_forwards_bytes_in_order() {
        let state = Arc::new(ReaderState::default());
        state.open.store(true, Ordering::Release);
        let (sender, receiver) = unbounded();

        let thread_state = Arc::clone(&state);
        let handle = thread::spawn(move || {
            let mut reader = ScriptedReader {
                chunks: VecDeque::from([b"Grbl 1.1f\r\n".to_vec(), b"ok\r\n".to_vec()]),
            };
            reader_loop(&mut reader, &thread_state, &sender);
        });

        let first = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("first chunk");
        let second = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("second chunk");
        assert_eq!(first, TransportEvent::Bytes(b"Grbl 1.1f\r\n".to_vec()));
        assert_eq!(second, TransportEvent::Bytes(b"ok\r\n".to_vec()));

        state.shutdown.store(true, Ordering::Release);
        handle.join().expect("reader thread should stop");
        assert_eq!(
            receiver
                .recv_timeout(Duration::from_secs(1))
                .expect("closed event"),
            TransportEvent::Closed
        );
    }

    #[test]
    fn reader_loop_reports_read_errors_then_closes() {
        let state = Arc::new(ReaderState::default());
        state.open.store(true, Ordering::Release);
        let (sender, receiver) = unbounded();

        let thread_state = Arc::clone(&state);
        let handle = thread::spawn(move || reader_loop(&mut FailingReader, &thread_state, &sender));
        handle.join().expect("reader thread should stop");

        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(TransportEvent::Error(message)) => {
                assert!(
                    message.contains("device unplugged"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("expected an Error event, got {other:?}"),
        }
        assert_eq!(
            receiver
                .recv_timeout(Duration::from_secs(1))
                .expect("closed event"),
            TransportEvent::Closed
        );
        assert!(!state.open.load(Ordering::Acquire));
    }

    #[test]
    fn open_rejects_empty_port() {
        let error = match SerialTransport::open(SerialConfig::default()) {
            Ok(_) => panic!("an empty port name must be rejected"),
            Err(error) => error,
        };
        assert!(matches!(error, TransportError::InvalidConfig(_)));
    }

    #[test]
    fn open_rejects_zero_baud() {
        let config = SerialConfig {
            port: "test".into(),
            baud: 0,
            ..SerialConfig::default()
        };
        let error = match SerialTransport::open(config) {
            Ok(_) => panic!("a zero baud rate must be rejected"),
            Err(error) => error,
        };
        assert!(matches!(error, TransportError::InvalidConfig(_)));
    }

    #[test]
    fn open_reports_a_missing_port() {
        #[cfg(unix)]
        let port = "/dev/ghs-rs-no-such-port";
        #[cfg(windows)]
        let port = "COM255";
        #[cfg(not(any(unix, windows)))]
        let port = "ghs-rs-no-such-port";

        let config = SerialConfig {
            port: port.into(),
            ..SerialConfig::default()
        };
        let error = match SerialTransport::open(config) {
            Ok(_) => panic!("opening a missing port must fail"),
            Err(error) => error,
        };
        assert!(
            matches!(error, TransportError::Serial(_) | TransportError::Io(_)),
            "unexpected error: {error}"
        );
    }
    #[test]
    fn transports_are_send() {
        fn assert_send<T: Send>() {}
        assert_send::<SerialTransport>();
        assert_send::<crate::LoopbackTransport>();

        // The engine stores transports behind a trait object.
        fn assert_object_safe(_transport: &dyn crate::Transport) {}
        let (left, _right) = crate::loopback_pair();
        assert_object_safe(&left);
    }
}

#[cfg(all(test, unix))]
mod unix_tests {
    use std::io::Write;
    use std::time::Instant;

    use serialport::TTYPort;

    use super::*;

    /// Creates a pseudo-terminal and returns its master plus the slave path.
    ///
    /// Returns `None` when the host has no usable pseudo-terminal support, for
    /// example in a restricted container.
    fn pty_pair() -> Option<(TTYPort, String)> {
        let (master, slave) = TTYPort::pair().ok()?;
        let name = slave.name()?;
        drop(slave);
        Some((master, name))
    }

    fn open_pty() -> Option<(SerialTransport, Receiver<TransportEvent>, TTYPort)> {
        let (mut master, port) = pty_pair()?;
        master.set_timeout(Duration::from_millis(200)).ok()?;
        let config = SerialConfig {
            port,
            ..SerialConfig::default()
        };
        let (transport, events) = SerialTransport::open_internal(config, false).ok()?;
        Some((transport, events, master))
    }

    #[test]
    fn pty_read_write_and_close() {
        let Some((mut transport, events, mut master)) = open_pty() else {
            return;
        };
        assert!(transport.is_open());

        master
            .write_all(b"Grbl 1.1f ['$' for help]\r\n")
            .expect("write banner");

        let mut received = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !received.windows(4).any(|window| window == b"Grbl") {
            assert!(Instant::now() < deadline, "banner was not received");
            match events.recv_timeout(Duration::from_millis(200)) {
                Ok(TransportEvent::Bytes(bytes)) => received.extend_from_slice(&bytes),
                other => panic!("unexpected event while reading banner: {other:?}"),
            }
        }

        transport.send(b"?\r\n").expect("send status query");

        let mut query = [0u8; 3];
        master
            .read_exact(&mut query)
            .expect("read status query on master");
        assert_eq!(&query, b"?\r\n");

        transport.close();
        assert!(!transport.is_open());
        assert!(matches!(transport.send(b"x"), Err(TransportError::Closed)));

        let mut closed = false;
        for _ in 0..5 {
            match events.recv_timeout(Duration::from_millis(200)) {
                Ok(TransportEvent::Closed) => {
                    closed = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert!(closed, "close should publish Closed");
    }

    #[test]
    fn pty_drop_publishes_closed() {
        let Some((transport, events, _master)) = open_pty() else {
            return;
        };
        drop(transport);

        let mut saw_closed = false;
        for _ in 0..5 {
            match events.recv_timeout(Duration::from_millis(200)) {
                Ok(TransportEvent::Closed) => {
                    saw_closed = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert!(saw_closed, "dropping the transport should publish Closed");
    }

    #[test]
    fn pty_trait_recv_timeout_reads_and_close_stops() {
        let Some((mut transport, _events, mut master)) = open_pty() else {
            return;
        };

        master.write_all(b"ok\r\n").expect("write answer");

        let mut received = Vec::new();
        while received.len() < 4 {
            match transport
                .recv_timeout(Duration::from_secs(1))
                .expect("event")
            {
                TransportEvent::Bytes(bytes) => received.extend_from_slice(&bytes),
                other => panic!("unexpected event: {other:?}"),
            }
        }
        assert_eq!(received, b"ok\r\n");

        let started = Instant::now();
        transport.close();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "close should join promptly"
        );
        assert!(
            matches!(
                transport.recv_timeout(Duration::from_millis(200)),
                Ok(TransportEvent::Closed)
            ),
            "close should end the event stream"
        );
    }
}
