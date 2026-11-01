//! Operating-system serial transport with a dedicated reader thread.

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use serialport::{ClearBuffer, SerialPort};
use tracing::{debug, error};

use crate::TransportError;
use crate::config::{FlowControl, SerialConfig};
use crate::transport::{Transport, TransportEvent};

/// Read and write timeout applied to the port.
///
/// Reads use it to re-check shutdown, so an idle link costs only a few dozen
/// wakeups per second and close waits at most this long. The same timeout is
/// handed to the driver for writes, so [`Transport::send`] should carry a
/// bounded protocol line; a single very large write can hit the driver
/// timeout, especially on Windows at low baud rates.
const READ_WRITE_TIMEOUT: Duration = Duration::from_millis(25);

/// Size of each read from the port. Protocol frames are much smaller; this
/// simply bounds each channel message.
const READ_BUFFER_BYTES: usize = 4096;

/// How long repeated zero-byte reads are tolerated before treating them as EOF.
///
/// A single transient zero read is ignored. This grace period only starts once
/// the reader sees the first zero; a timeout between zeros resets it.
const ZERO_READ_GRACE: Duration = Duration::from_millis(100);

/// Number of consecutive zero-byte reads required before the EOF decision.
const ZERO_READ_MIN_COUNT: u32 = 3;

/// Back-off used between zero-byte reads, which some drivers report as EOF.
const ZERO_READ_BACKOFF: Duration = Duration::from_millis(2);

/// Upper bound for [`SerialTransport::pulse_dtr`].
///
/// A reset pulse is normally tens to hundreds of milliseconds. The bound keeps
/// a bad caller from parking the engine thread for an unbounded time.
const MAX_DTR_PULSE: Duration = Duration::from_secs(2);

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
/// the original handle for writes and for closing the port. [`Transport::events`]
/// is the only receive path, so the engine can use
/// `crossbeam_channel::select!`.
///
/// The port is opened with a 25 ms read/write timeout. Keep sends to a bounded
/// protocol line; a single very large write can reach the driver timeout,
/// especially on Windows at low baud rates.
pub struct SerialTransport {
    port_name: String,
    line_control: bool,
    writer: Mutex<Option<Box<dyn SerialPort>>>,
    events: Receiver<TransportEvent>,
    state: Arc<ReaderState>,
    reader: Option<JoinHandle<()>>,
}

impl SerialTransport {
    /// Opens and configures a serial port.
    ///
    /// Applies the baud rate, character format, flow control and timeout,
    /// clears both buffers, then applies DTR/RTS and starts the reader. Buffers
    /// are cleared before DTR so a banner produced by a DTR reset is preserved.
    ///
    /// DTR and RTS are only touched when [`SerialConfig::line_control`] is
    /// enabled. In that case DTR is driven to [`SerialConfig::dtr`]; RTS is
    /// driven to [`SerialConfig::rts`] unless hardware flow control is active,
    /// in which case the driver owns RTS. With line control disabled, no
    /// modem-control ioctl is attempted, which is required for virtual and
    /// Bluetooth ports. A failing line-control ioctl is returned as
    /// [`TransportError::Serial`] and the port is closed.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::InvalidConfig`] for a rejected configuration,
    /// [`TransportError::Serial`] when the port cannot be opened or configured,
    /// and [`TransportError::Io`] when the reader thread cannot be started.
    pub fn open(config: SerialConfig) -> Result<Self, TransportError> {
        config.validate()?;

        let mut port = serialport::new(&config.port, config.baud)
            .data_bits(config.data_bits.native())
            .parity(config.parity.native())
            .stop_bits(config.stop_bits.native())
            .flow_control(config.flow_control.native())
            .timeout(READ_WRITE_TIMEOUT)
            .open()?;

        // Clear stale data before touching DTR: asserting DTR may reset the
        // board, and a banner emitted right after the reset must survive.
        port.clear(ClearBuffer::All).map_err(|error| {
            TransportError::Serial(serialport::Error::new(
                error.kind,
                format!("failed to clear port buffers: {}", error.description),
            ))
        })?;

        if config.line_control {
            port.write_data_terminal_ready(config.dtr)
                .map_err(|error| line_control_error("DTR", error))?;

            if config.flow_control == FlowControl::Hardware {
                debug!(
                    port = %config.port,
                    "hardware flow control owns RTS; rts setting is ignored"
                );
            } else {
                port.write_request_to_send(config.rts)
                    .map_err(|error| line_control_error("RTS", error))?;
            }
        }

        let mut reader = port.try_clone()?;
        reader.set_timeout(READ_WRITE_TIMEOUT)?;

        let state = Arc::new(ReaderState::default());
        state.open.store(true, Ordering::Release);

        let (sender, events) = unbounded();
        let reader_state = Arc::clone(&state);
        let spawn = thread::Builder::new()
            .name("ghs-serial-rx".into())
            .spawn(move || {
                reader_loop(&mut *reader, &reader_state, &sender, ZERO_READ_GRACE);
            });

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

        Ok(Self {
            port_name: config.port,
            line_control: config.line_control,
            writer: Mutex::new(Some(port)),
            events,
            state,
            reader: Some(handle),
        })
    }

    /// Name of the port this transport was opened with.
    #[must_use]
    pub fn port_name(&self) -> &str {
        &self.port_name
    }

    /// Drives DTR to `assert` on the open port.
    ///
    /// This is the engine's explicit control for an optional reset-on-connect;
    /// it is not part of the [`Transport`] trait. The line is only available
    /// when [`SerialConfig::line_control`] is enabled.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::LineControlDisabled`] when the port was opened
    /// with line control disabled, [`TransportError::Closed`] when the link is
    /// closed, and [`TransportError::Serial`] when the driver rejects the ioctl.
    pub fn set_dtr(&mut self, assert: bool) -> Result<(), TransportError> {
        if !self.line_control {
            return Err(TransportError::LineControlDisabled);
        }
        if !self.is_open() {
            return Err(TransportError::Closed);
        }

        let mut writer = self
            .writer
            .lock()
            .map_err(|_| TransportError::Poisoned("serial writer mutex"))?;
        let Some(port) = writer.as_mut() else {
            return Err(TransportError::Closed);
        };
        port.write_data_terminal_ready(assert)
            .map_err(|error| line_control_error("DTR", error))
    }

    /// Asserts DTR for `duration`, then deasserts it.
    ///
    /// This is the optional reset pulse for boards wired to reset on DTR.
    /// `duration` is clamped to a maximum of two seconds; on success, DTR is
    /// left deasserted afterwards. The call blocks the calling thread for the
    /// pulse width.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::LineControlDisabled`] when the port was opened
    /// with line control disabled, [`TransportError::Closed`] when the link is
    /// closed, and [`TransportError::Serial`] when the driver rejects either
    /// line transition. If the deassert step fails, the line may remain
    /// asserted; close the transport rather than continuing to use the link.
    pub fn pulse_dtr(&mut self, duration: Duration) -> Result<(), TransportError> {
        if !self.line_control {
            return Err(TransportError::LineControlDisabled);
        }

        let duration = duration.min(MAX_DTR_PULSE);
        self.set_dtr(true)?;
        thread::sleep(duration);
        self.set_dtr(false)
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

    fn events(&self) -> &Receiver<TransportEvent> {
        &self.events
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

fn line_control_error(line: &str, error: serialport::Error) -> TransportError {
    TransportError::Serial(serialport::Error::new(
        error.kind,
        format!("failed to set {line}: {}", error.description),
    ))
}

/// Reads until shutdown, forwarding bytes and reporting the final close.
fn reader_loop<R>(
    reader: &mut R,
    state: &ReaderState,
    sender: &Sender<TransportEvent>,
    zero_read_grace: Duration,
) where
    R: Read + ?Sized,
{
    let mut buffer = vec![0u8; READ_BUFFER_BYTES];
    let mut receiver_alive = true;
    let mut zero_reads: u32 = 0;
    let mut first_zero: Option<Instant> = None;

    loop {
        if state.shutdown.load(Ordering::Acquire) {
            break;
        }

        match reader.read(&mut buffer) {
            Ok(0) => {
                let now = Instant::now();
                let since = *first_zero.get_or_insert(now);
                zero_reads = zero_reads.saturating_add(1);
                if zero_reads >= ZERO_READ_MIN_COUNT && now.duration_since(since) >= zero_read_grace
                {
                    let _ = sender.send(TransportEvent::Error(
                        "serial port reached end of file".into(),
                    ));
                    break;
                }
                thread::sleep(ZERO_READ_BACKOFF);
            }
            Ok(count) => {
                zero_reads = 0;
                first_zero = None;

                // Trust but verify: a misbehaving driver must not make us
                // index past the buffer.
                let count = count.min(buffer.len());
                let event = TransportEvent::Bytes(buffer[..count].to_vec());
                if sender.send(event).is_err() {
                    receiver_alive = false;
                    break;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                zero_reads = 0;
                first_zero = None;
            }
            Err(error) if is_timeout(&error) => {
                zero_reads = 0;
                first_zero = None;
            }
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

    struct EofReader;

    impl Read for EofReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Ok(0)
        }
    }

    struct SingleZeroReader {
        saw_zero: bool,
        sent_data: bool,
    }

    impl Read for SingleZeroReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !self.saw_zero {
                self.saw_zero = true;
                return Ok(0);
            }
            if !self.sent_data {
                self.sent_data = true;
                let bytes = b"ok";
                let count = bytes.len().min(buffer.len());
                buffer[..count].copy_from_slice(&bytes[..count]);
                return Ok(count);
            }
            thread::sleep(Duration::from_millis(2));
            Err(io::Error::new(io::ErrorKind::TimedOut, "read timeout"))
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
            reader_loop(
                &mut reader,
                &thread_state,
                &sender,
                Duration::from_millis(100),
            );
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
            reader_loop(
                &mut reader,
                &thread_state,
                &sender,
                Duration::from_millis(100),
            );
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
    fn reader_loop_tolerates_one_transient_zero_read() {
        let state = Arc::new(ReaderState::default());
        state.open.store(true, Ordering::Release);
        let (sender, receiver) = unbounded();

        let thread_state = Arc::clone(&state);
        let handle = thread::spawn(move || {
            let mut reader = SingleZeroReader {
                saw_zero: false,
                sent_data: false,
            };
            reader_loop(
                &mut reader,
                &thread_state,
                &sender,
                Duration::from_millis(100),
            );
        });

        assert_eq!(
            receiver
                .recv_timeout(Duration::from_secs(1))
                .expect("bytes after the transient zero"),
            TransportEvent::Bytes(b"ok".to_vec())
        );

        state.shutdown.store(true, Ordering::Release);
        handle.join().expect("reader thread should stop");

        let events: Vec<_> = receiver.try_iter().collect();
        assert_eq!(events, vec![TransportEvent::Closed]);
    }

    #[test]
    fn reader_loop_reports_repeated_zero_reads_as_eof() {
        let state = Arc::new(ReaderState::default());
        state.open.store(true, Ordering::Release);
        let (sender, receiver) = unbounded();

        let thread_state = Arc::clone(&state);
        let handle = thread::spawn(move || {
            reader_loop(
                &mut EofReader,
                &thread_state,
                &sender,
                Duration::from_millis(20),
            );
        });
        handle.join().expect("reader thread should stop");

        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(TransportEvent::Error(message)) => {
                assert!(
                    message.contains("end of file"),
                    "unexpected message: {message}"
                );
            }
            other => panic!("expected an EOF Error event, got {other:?}"),
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
    fn reader_loop_reports_read_errors_then_closes() {
        let state = Arc::new(ReaderState::default());
        state.open.store(true, Ordering::Release);
        let (sender, receiver) = unbounded();

        let thread_state = Arc::clone(&state);
        let handle = thread::spawn(move || {
            reader_loop(&mut FailingReader, &thread_state, &sender, ZERO_READ_GRACE);
        });
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
    fn open_rejects_dtr_without_line_control() {
        let config = SerialConfig {
            port: "test".into(),
            dtr: true,
            ..SerialConfig::default()
        };
        let error = match SerialTransport::open(config) {
            Ok(_) => panic!("DTR without line control must be rejected"),
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

    fn open_pty() -> Option<(SerialTransport, TTYPort)> {
        let (mut master, port) = pty_pair()?;
        master.set_timeout(Duration::from_millis(200)).ok()?;
        let config = SerialConfig {
            port,
            ..SerialConfig::default()
        };
        let transport = SerialTransport::open(config).ok()?;
        Some((transport, master))
    }

    #[test]
    fn pty_read_write_and_close() {
        let Some((mut transport, mut master)) = open_pty() else {
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
            match transport.events().recv_timeout(Duration::from_millis(200)) {
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
            match transport.events().recv_timeout(Duration::from_millis(200)) {
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
        let Some((transport, _master)) = open_pty() else {
            return;
        };
        let events = transport.events().clone();
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
    fn pty_events_read_and_close_stops() {
        let Some((mut transport, mut master)) = open_pty() else {
            return;
        };

        master.write_all(b"ok\r\n").expect("write answer");

        let mut received = Vec::new();
        while received.len() < 4 {
            match transport.events().recv_timeout(Duration::from_secs(1)) {
                Ok(TransportEvent::Bytes(bytes)) => received.extend_from_slice(&bytes),
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
                transport.events().recv_timeout(Duration::from_millis(200)),
                Ok(TransportEvent::Closed)
            ),
            "close should end the event stream"
        );
    }

    #[test]
    fn line_control_disabled_rejects_dtr() {
        let Some((mut transport, _master)) = open_pty() else {
            return;
        };
        assert!(matches!(
            transport.set_dtr(true),
            Err(TransportError::LineControlDisabled)
        ));
        assert!(matches!(
            transport.pulse_dtr(Duration::from_millis(1)),
            Err(TransportError::LineControlDisabled)
        ));
    }
}
