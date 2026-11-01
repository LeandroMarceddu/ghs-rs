//! Serial port enumeration and hotplug polling.

use std::cmp::Ordering;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TrySendError, bounded, unbounded};
use tracing::{error, warn};

use crate::TransportError;

/// Minimum interval accepted by [`watch_ports`].
const MIN_WATCH_INTERVAL: Duration = Duration::from_millis(100);

/// Maximum interval accepted by [`watch_ports`].
///
/// Also keeps a nonsensical or overflowing duration away from the channel wait.
const MAX_WATCH_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// A serial port reported by the operating system.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PortInfo {
    /// Operating system port name, for example `/dev/ttyUSB0` or `COM3`.
    pub name: String,
    /// Human-readable description, when the operating system provides one.
    pub description: Option<String>,
    /// USB manufacturer string, when available.
    pub manufacturer: Option<String>,
    /// USB serial number, when available.
    pub serial_number: Option<String>,
    /// USB vendor ID, when the port is a USB device.
    pub vid: Option<u16>,
    /// USB product ID, when the port is a USB device.
    pub pid: Option<u16>,
}

impl From<serialport::SerialPortInfo> for PortInfo {
    fn from(info: serialport::SerialPortInfo) -> Self {
        match info.port_type {
            serialport::SerialPortType::UsbPort(usb) => Self {
                name: info.port_name,
                description: usb.product,
                manufacturer: usb.manufacturer,
                serial_number: usb.serial_number,
                vid: Some(usb.vid),
                pid: Some(usb.pid),
            },
            _ => Self {
                name: info.port_name,
                description: None,
                manufacturer: None,
                serial_number: None,
                vid: None,
                pid: None,
            },
        }
    }
}

/// Lists the serial ports currently known to the operating system.
///
/// The result is naturally sorted by port name (`COM2` before `COM10`) and may
/// be empty. On Linux this uses udev; on macOS and Windows it uses the
/// platform device APIs.
///
/// # Errors
///
/// Returns [`TransportError::Serial`] when enumeration itself fails, for
/// example because the platform device database cannot be opened.
pub fn list_ports() -> Result<Vec<PortInfo>, TransportError> {
    let mut ports: Vec<PortInfo> = serialport::available_ports()?
        .into_iter()
        .map(PortInfo::from)
        .collect();
    ports.sort_by(|left, right| natural_cmp(&left.name, &right.name));
    Ok(ports)
}

/// Owns the hotplug poller thread and its snapshot receiver.
///
/// Dropping the watcher stops and joins the thread. Call [`Self::receiver`] to
/// read snapshots, and [`Self::stop`] when the connection UI closes before the
/// watcher itself is dropped.
pub struct PortWatcher {
    receiver: Receiver<Vec<PortInfo>>,
    stop: Arc<AtomicBool>,
    stop_tx: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl PortWatcher {
    /// Returns the snapshot receiver.
    ///
    /// The first snapshot is published immediately. After that a snapshot is
    /// sent only when the port list changes; the channel holds at most one
    /// snapshot, and the next poll retries if the receiver is slow. Use one
    /// consumer; if the receiver is cloned, do not also read the original.
    #[must_use]
    pub fn receiver(&self) -> &Receiver<Vec<PortInfo>> {
        &self.receiver
    }

    /// Stops and joins the poller.
    ///
    /// This is idempotent. It may wait for an in-progress port enumeration to
    /// finish, but it does not wait for the polling interval.
    pub fn stop(&mut self) {
        self.stop.store(true, AtomicOrdering::Release);
        let _ = self.stop_tx.send(());
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            error!("serial port watcher thread panicked");
        }
    }
}

impl Drop for PortWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Starts a background thread that polls the serial port list.
///
/// The first snapshot is published immediately, then a new snapshot is sent
/// only when the list changes. The channel holds at most one snapshot; if the
/// receiver is slow, the next poll retries with the latest list.
///
/// `interval` is clamped to 100 ms through one hour to avoid needless polling
/// and to keep a nonsensical duration away from the channel wait.
///
/// # Errors
///
/// Returns [`TransportError::Io`] when the watcher thread cannot be started.
/// Enumeration errors are logged and retried; they are not fatal.
pub fn watch_ports(interval: Duration) -> Result<PortWatcher, TransportError> {
    let interval = interval.clamp(MIN_WATCH_INTERVAL, MAX_WATCH_INTERVAL);
    let (sender, receiver) = bounded(1);
    let (stop_tx, stop_rx) = unbounded();
    let stop = Arc::new(AtomicBool::new(false));

    let thread_stop = Arc::clone(&stop);
    let thread = thread::Builder::new()
        .name("ghs-serial-hp".into())
        .spawn(move || watch_loop(sender, stop_rx, thread_stop, interval))
        .map_err(TransportError::Io)?;

    Ok(PortWatcher {
        receiver,
        stop,
        stop_tx,
        thread: Some(thread),
    })
}

fn watch_loop(
    sender: Sender<Vec<PortInfo>>,
    stop_rx: Receiver<()>,
    stop: Arc<AtomicBool>,
    interval: Duration,
) {
    let mut last: Option<Vec<PortInfo>> = None;
    let mut last_error: Option<String> = None;

    loop {
        if stop.load(AtomicOrdering::Acquire) {
            break;
        }

        match list_ports() {
            Ok(ports) => {
                last_error = None;
                if last.as_ref() != Some(&ports) {
                    match sender.try_send(ports.clone()) {
                        Ok(()) => last = Some(ports),
                        // The receiver has not consumed the previous snapshot yet.
                        // Leave `last` unchanged so the next poll tries again.
                        Err(TrySendError::Full(_)) => {}
                        Err(TrySendError::Disconnected(_)) => break,
                    }
                }
            }
            Err(error) => {
                let message = error.to_string();
                if last_error.as_deref() != Some(message.as_str()) {
                    warn!(%error, "serial port enumeration failed");
                    last_error = Some(message);
                }
            }
        }

        match stop_rx.recv_timeout(interval) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// Compares port names naturally, treating runs of ASCII digits as numbers.
///
/// This keeps `COM2` before `COM10` instead of sorting them lexicographically.
/// The comparison is a total order, so the result is deterministic.
fn natural_cmp(left: &str, right: &str) -> Ordering {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut left_index = 0;
    let mut right_index = 0;

    while left_index < left.len() && right_index < right.len() {
        let left_byte = left[left_index];
        let right_byte = right[right_index];

        if left_byte.is_ascii_digit() && right_byte.is_ascii_digit() {
            let left_end = digit_run_end(left, left_index);
            let right_end = digit_run_end(right, right_index);
            let left_digits = &left[left_index..left_end];
            let right_digits = &right[right_index..right_end];
            let left_number = trim_leading_zeros(left_digits);
            let right_number = trim_leading_zeros(right_digits);

            let value_order = left_number
                .len()
                .cmp(&right_number.len())
                .then_with(|| left_number.cmp(right_number));
            match value_order {
                Ordering::Equal => {
                    // Same numeric value: prefer the shorter run without
                    // leading zeros, then keep the comparison total.
                    match left_digits.len().cmp(&right_digits.len()) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                }
                other => return other,
            }

            left_index = left_end;
            right_index = right_end;
        } else {
            match left_byte.cmp(&right_byte) {
                Ordering::Equal => {
                    left_index += 1;
                    right_index += 1;
                }
                other => return other,
            }
        }
    }

    (left.len() - left_index)
        .cmp(&(right.len() - right_index))
        .then_with(|| left.cmp(right))
}

fn digit_run_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    end
}

fn trim_leading_zeros(digits: &[u8]) -> &[u8] {
    let mut start = 0;
    while start + 1 < digits.len() && digits[start] == b'0' {
        start += 1;
    }
    &digits[start..]
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn list_ports_returns_without_panicking() {
        let ports = list_ports().expect("port enumeration should succeed on this host");
        for port in &ports {
            assert!(!port.name.is_empty(), "a port must have a name");
        }
    }

    #[test]
    fn watcher_publishes_an_initial_snapshot_and_stops() {
        let mut watcher = watch_ports(Duration::from_secs(5)).expect("watcher thread should start");
        let snapshot = watcher
            .receiver()
            .recv_timeout(Duration::from_secs(5))
            .expect("initial snapshot");
        for port in &snapshot {
            assert!(!port.name.is_empty());
        }

        let started = Instant::now();
        watcher.stop();
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "stop should wake and join the poller"
        );

        // Stopping twice is a no-op.
        watcher.stop();
    }

    #[test]
    fn dropping_the_watcher_joins_the_thread() {
        let watcher = watch_ports(Duration::from_secs(5)).expect("watcher thread should start");
        watcher
            .receiver()
            .recv_timeout(Duration::from_secs(5))
            .expect("initial snapshot");

        let started = Instant::now();
        drop(watcher);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "drop should wake and join the poller"
        );
    }

    #[test]
    fn watcher_receiver_can_be_selected_and_stopped() {
        let mut watcher = watch_ports(Duration::from_millis(100)).expect("watcher thread");
        crossbeam_channel::select! {
            recv(watcher.receiver()) -> snapshot => {
                assert!(snapshot.is_ok(), "the initial snapshot should arrive");
                watcher.stop();
            }
            default(Duration::from_secs(5)) => panic!("no initial snapshot"),
        }
    }

    #[test]
    fn port_names_sort_naturally() {
        let mut names = vec![
            "COM10", "COM2", "COM1", "ttyUSB10", "ttyUSB2", "ttyS1", "COM02",
        ];
        names.sort_by(|left, right| natural_cmp(left, right));
        assert_eq!(
            names,
            [
                "COM1", "COM2", "COM02", "COM10", "ttyS1", "ttyUSB2", "ttyUSB10"
            ]
        );
    }

    #[test]
    fn natural_sort_is_deterministic() {
        for _ in 0..10 {
            let mut names = vec!["COM10", "COM2", "COM1", "COM100", "COM20"];
            names.sort_by(|left, right| natural_cmp(left, right));
            assert_eq!(names, ["COM1", "COM2", "COM10", "COM20", "COM100"]);
        }
    }

    #[test]
    fn usb_port_fields_are_preserved() {
        let info = serialport::SerialPortInfo {
            port_name: "/dev/ttyACM0".into(),
            port_type: serialport::SerialPortType::UsbPort(serialport::UsbPortInfo {
                vid: 0x0483,
                pid: 0x5740,
                serial_number: Some("SN123".into()),
                manufacturer: Some("Acme".into()),
                product: Some("FlexiHAL".into()),
            }),
        };
        let port = PortInfo::from(info);
        assert_eq!(port.name, "/dev/ttyACM0");
        assert_eq!(port.description.as_deref(), Some("FlexiHAL"));
        assert_eq!(port.manufacturer.as_deref(), Some("Acme"));
        assert_eq!(port.serial_number.as_deref(), Some("SN123"));
        assert_eq!(port.vid, Some(0x0483));
        assert_eq!(port.pid, Some(0x5740));
    }

    #[test]
    fn non_usb_ports_have_no_usb_fields() {
        let info = serialport::SerialPortInfo {
            port_name: "/dev/ttyS0".into(),
            port_type: serialport::SerialPortType::PciPort,
        };
        let port = PortInfo::from(info);
        assert_eq!(port.name, "/dev/ttyS0");
        assert_eq!(port.description, None);
        assert_eq!(port.manufacturer, None);
        assert_eq!(port.serial_number, None);
        assert_eq!(port.vid, None);
        assert_eq!(port.pid, None);
    }
}
