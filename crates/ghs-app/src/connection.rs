//! Connection-panel view model.
//!
//! The panel is plain data plus one [`PortWatcher`]. It owns the list of serial
//! ports while the dialog is open, keeps an explicit selection while that port
//! is present, and stops the polling thread as soon as the dialog closes.
//! Rendering lives in `shell.rs`; this module has no GPUI dependency so the
//! picker can be tested without a display or a controller.

use std::time::Duration;

use ghs_serial::{PortInfo, PortWatcher, SerialConfig, watch_ports};

/// How often the open dialog re-enumerates serial ports.
const WATCH_INTERVAL: Duration = Duration::from_millis(700);

/// Baud rates offered by the picker. A grblHAL controller commonly runs at
/// 115200 (the default) or 250000.
pub const BAUD_RATES: [u32; 9] = [
    9_600, 19_200, 38_400, 57_600, 115_200, 230_400, 250_000, 500_000, 1_000_000,
];

/// Transport selected in the connection dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportChoice {
    /// In-process FlexiHAL simulator over the loopback transport.
    Demo,
    /// A real serial port.
    Serial,
}

impl TransportChoice {
    /// Label used by the transport segmented control.
    pub fn label(self) -> &'static str {
        match self {
            TransportChoice::Demo => "Demo FlexiHAL",
            TransportChoice::Serial => "Serial port",
        }
    }
}

/// Connection dialog state: transport choice, port picker, baud rate and the
/// last error reported by an engine connect attempt.
pub struct ConnectionPanel {
    /// True while the overlay is visible.
    pub open: bool,
    /// Selected transport.
    pub transport: TransportChoice,
    /// Latest port snapshot from the watcher.
    pub ports: Vec<PortInfo>,
    /// Selected port name, remembered across refreshes when it is still present.
    pub selected_port: Option<String>,
    /// Baud rate for the serial draft configuration.
    pub baud: u32,
    /// Last connect error, shown in the dialog.
    pub error: Option<String>,
    /// Enumeration error, kept separate from a connect error.
    pub ports_error: Option<String>,
    watcher: Option<PortWatcher>,
}

impl ConnectionPanel {
    /// Create a closed panel with the demo transport and 115200 baud selected.
    pub fn new() -> Self {
        Self {
            open: false,
            transport: TransportChoice::Demo,
            ports: Vec::new(),
            selected_port: None,
            baud: SerialConfig::default().baud,
            error: None,
            ports_error: None,
            watcher: None,
        }
    }

    /// Open the panel and start watching for serial ports.
    ///
    /// The watcher runs only while the dialog is open; [`Self::close`] stops
    /// and joins it.
    pub fn open(&mut self) {
        self.open = true;
        self.error = None;
        self.ports_error = None;
        self.refresh();
    }

    /// Close the panel and stop the port watcher.
    pub fn close(&mut self) {
        self.open = false;
        self.stop_watcher();
    }

    /// Restart the watcher so the list is re-enumerated immediately.
    pub fn refresh(&mut self) {
        self.stop_watcher();
        self.ports_error = None;
        match watch_ports(WATCH_INTERVAL) {
            Ok(watcher) => self.watcher = Some(watcher),
            Err(error) => self.ports_error = Some(error.to_string()),
        }
    }

    /// Drain watcher snapshots. Returns `true` when the visible port list
    /// changed.
    pub fn poll(&mut self) -> bool {
        let mut snapshots = Vec::new();
        if let Some(watcher) = self.watcher.as_ref() {
            while let Ok(ports) = watcher.receiver().try_recv() {
                snapshots.push(ports);
            }
        }
        let changed = !snapshots.is_empty();
        for ports in snapshots {
            self.apply_ports(ports);
        }
        changed
    }

    /// Select the transport without touching the engine.
    pub fn select_transport(&mut self, transport: TransportChoice) {
        self.transport = transport;
        self.error = None;
    }

    /// Select a port by name.
    pub fn select_port(&mut self, name: impl Into<String>) {
        self.selected_port = Some(name.into());
        self.error = None;
    }

    /// Set the draft baud rate.
    pub fn set_baud(&mut self, baud: u32) {
        if baud > 0 {
            self.baud = baud;
            self.error = None;
        }
    }

    /// Build the draft serial configuration, or `None` when no port is
    /// selected.
    ///
    /// Line settings stay at the [`SerialConfig`] defaults: 8N1, no flow
    /// control, and modem line control off, so opening a port never pulses
    /// DTR. A board that needs a DTR reset must get that as a separate,
    /// explicit feature.
    pub fn selected_config(&self) -> Option<SerialConfig> {
        let port = self.selected_port.as_deref()?.trim();
        if port.is_empty() {
            return None;
        }
        let mut config = SerialConfig::new(port);
        config.baud = self.baud;
        Some(config)
    }

    /// Human-readable line-control defaults for the dialog hint.
    pub fn line_control_summary(&self) -> String {
        let config = SerialConfig::default();
        format!(
            "line control {} | DTR {} | RTS {} | flow control {}",
            if config.line_control { "on" } else { "off" },
            if config.dtr { "high" } else { "low" },
            if config.rts { "high" } else { "low" },
            "none",
        )
    }

    /// Replace the snapshot and drop a selection that is no longer present.
    ///
    /// Selection is always explicit: a refresh never opens a port the operator
    /// did not tap, and a vanished port is cleared rather than silently
    /// replaced by whichever adapter enumerated first.
    fn apply_ports(&mut self, ports: Vec<PortInfo>) {
        self.ports = ports;
        self.ports_error = None;
        let selected_present = self
            .selected_port
            .as_ref()
            .is_some_and(|selected| self.ports.iter().any(|port| &port.name == selected));
        if !selected_present {
            self.selected_port = None;
        }
    }

    fn stop_watcher(&mut self) {
        if let Some(mut watcher) = self.watcher.take() {
            watcher.stop();
        }
    }
}

impl Default for ConnectionPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ConnectionPanel {
    fn drop(&mut self) {
        self.stop_watcher();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(name: &str) -> PortInfo {
        PortInfo {
            name: name.to_string(),
            description: None,
            manufacturer: None,
            serial_number: None,
            vid: None,
            pid: None,
        }
    }

    #[test]
    fn defaults_are_demo_at_115200() {
        let panel = ConnectionPanel::new();
        assert!(!panel.open);
        assert_eq!(panel.transport, TransportChoice::Demo);
        assert_eq!(panel.baud, 115_200);
        assert!(panel.selected_port.is_none());
        assert!(panel.selected_config().is_none());
        assert!(
            panel.line_control_summary().contains("line control off"),
            "the dialog must state the SerialConfig line-control default"
        );
    }

    #[test]
    fn port_selection_is_explicit_and_follows_the_snapshot() {
        let mut panel = ConnectionPanel::new();
        panel.apply_ports(vec![port("/dev/ttyUSB0"), port("/dev/ttyACM0")]);
        assert!(
            panel.selected_port.is_none(),
            "opening the picker must not preselect a port"
        );

        // The user's choice survives a refresh while it is still present.
        panel.select_port("/dev/ttyACM0");
        panel.apply_ports(vec![port("/dev/ttyUSB0"), port("/dev/ttyACM0")]);
        assert_eq!(panel.selected_port.as_deref(), Some("/dev/ttyACM0"));

        // A vanished port is cleared, not silently replaced by another port.
        panel.apply_ports(vec![port("/dev/ttyUSB0")]);
        assert!(panel.selected_port.is_none());
        assert!(panel.selected_config().is_none());

        // An empty snapshot also clears the selection.
        panel.select_port("/dev/ttyS0");
        panel.apply_ports(Vec::new());
        assert!(panel.selected_port.is_none());
    }

    #[test]
    fn draft_config_is_8n1_without_line_control() {
        let mut panel = ConnectionPanel::new();
        panel.apply_ports(vec![port("/dev/ttyACM0")]);
        panel.select_port("/dev/ttyACM0");
        panel.set_baud(250_000);
        let config = panel.selected_config().expect("a port is selected");
        assert_eq!(config.port, "/dev/ttyACM0");
        assert_eq!(config.baud, 250_000);
        assert_eq!(config.data_bits, ghs_serial::DataBits::Eight);
        assert_eq!(config.parity, ghs_serial::Parity::None);
        assert_eq!(config.stop_bits, ghs_serial::StopBits::One);
        assert_eq!(config.flow_control, ghs_serial::FlowControl::None);
        assert!(!config.line_control);
        assert!(!config.dtr);
        assert!(!config.rts);
        assert!(config.validate().is_ok());

        panel.set_baud(0);
        assert_eq!(panel.baud, 250_000, "a zero baud rate must be ignored");
    }

    #[test]
    fn closing_the_dialog_stops_the_watcher() {
        let mut panel = ConnectionPanel::new();
        panel.open();
        assert!(panel.open);
        assert!(panel.watcher.is_some(), "opening starts the port watcher");
        panel.close();
        assert!(!panel.open);
        assert!(panel.watcher.is_none(), "closing must stop the watcher");
    }
}
