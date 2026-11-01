//! Serial port enumeration and hotplug polling.

use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, TrySendError};
use tracing::{error, warn};

use crate::TransportError;

/// Minimum interval accepted by [`watch_ports`].
const MIN_WATCH_INTERVAL: Duration = Duration::from_millis(100);

/// Maximum interval accepted by [`watch_ports`].
///
/// Also keeps a nonsensical or overflowing duration away from `thread::sleep`.
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
/// The result is sorted by port name and may be empty. On Linux this uses
/// udev; on macOS and Windows it uses the platform device APIs.
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
    ports.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(ports)
}

/// Starts a background thread that polls the serial port list.
///
/// The first snapshot is published immediately, then a new snapshot is sent
/// only when the list changes. The channel holds at most one snapshot; if the
/// receiver is slow, the next poll retries with the latest list.
///
/// The poller stops when publishing a snapshot fails because the receiver was
/// dropped. While the port list is stable no snapshot is published, so the
/// thread is only reclaimed at the next change; keep the receiver alive for as
/// long as the connection UI is open.
///
/// `interval` is clamped to 100 ms through one hour to avoid needless polling
/// and to keep a nonsensical duration away from the platform sleep call.
#[must_use]
pub fn watch_ports(interval: Duration) -> Receiver<Vec<PortInfo>> {
    let interval = interval.clamp(MIN_WATCH_INTERVAL, MAX_WATCH_INTERVAL);
    let (sender, receiver) = crossbeam_channel::bounded(1);

    let spawn = thread::Builder::new()
        .name("ghs-serial-hp".into())
        .spawn(move || watch_loop(sender, interval));
    if let Err(error) = spawn {
        error!(%error, "failed to start serial port watcher");
    }

    receiver
}

fn watch_loop(sender: crossbeam_channel::Sender<Vec<PortInfo>>, interval: Duration) {
    let mut last: Option<Vec<PortInfo>> = None;
    let mut last_error: Option<String> = None;

    loop {
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

        thread::sleep(interval);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_ports_returns_without_panicking() {
        let ports = list_ports().expect("port enumeration should succeed on this host");
        for port in &ports {
            assert!(!port.name.is_empty(), "a port must have a name");
        }
    }

    #[test]
    fn watch_ports_publishes_an_initial_snapshot() {
        let receiver = watch_ports(Duration::from_millis(100));
        let snapshot = receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("initial snapshot");
        for port in &snapshot {
            assert!(!port.name.is_empty());
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
