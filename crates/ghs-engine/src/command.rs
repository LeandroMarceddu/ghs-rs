//! Typed commands accepted by the engine gateway.

use std::fmt;

use ghs_core::SettingValue;
use ghs_proto::{JogCommand, RealtimeCommand, SystemCommand};
use ghs_serial::{SerialConfig, Transport};

/// How to establish a transport.
pub enum ConnectTarget {
    /// Open a serial port with the given configuration.
    ///
    /// The engine validates the configuration and opens the port on its own
    /// thread.
    Serial(SerialConfig),
    /// Adopt a transport that the caller already created.
    ///
    /// This is the in-process loopback path used by tests and demo mode. The
    /// engine takes ownership, reads its events, and closes it on disconnect.
    Transport(Box<dyn Transport>),
}

impl fmt::Debug for ConnectTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectTarget::Serial(config) => f.debug_tuple("Serial").field(config).finish(),
            ConnectTarget::Transport(_) => f.write_str("Transport(..)"),
        }
    }
}

/// A command accepted by [`crate::EngineHandle::send`].
///
/// The engine classifies every command and applies the machine-state gateway
/// before anything reaches the transport. Motion, jog and MDI are refused in
/// `Alarm`, `Door` and other locked states; configuration writes are idle-only;
/// nothing is accepted while a pendant owns the input stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// A real-time byte or byte sequence, always sent out of band.
    Realtime(RealtimeCommand),
    /// A `$` system command.
    ///
    /// Setting writes must use [`Command::SetSetting`] so they are read back
    /// and verified; [`SystemCommand::WriteSetting`] is rejected here.
    System(SystemCommand),
    /// A jog motion, encoded as `$J=G21 ...`.
    Jog(JogCommand),
    /// One or more MDI lines.
    ///
    /// Lines are sent one at a time and stopped on the first controller error.
    /// System commands (`$...`) inside MDI are rejected; use [`Command::System`]
    /// or the dedicated variants instead.
    Mdi(String),
    /// Set the feed override to an absolute percentage in `10..=200`.
    SetFeedOverride(u16),
    /// Set the spindle override to an absolute percentage in `10..=200`.
    SetSpindleOverride(u16),
    /// Set the rapid override to one of 100, 50, 25 or 5 percent.
    SetRapidOverride(u16),
    /// Write a controller setting and read it back for verification.
    SetSetting {
        /// Setting number, the `<n>` in `$<n>=<value>`.
        number: u16,
        /// New value.
        value: SettingValue,
    },
    /// Home all axes (`$H`). Explicit intent; allowed in `Alarm`.
    Home,
    /// Unlock the alarm state (`$X`). Explicit intent.
    Unlock,
    /// Soft reset (`0x18`).
    Reset,
    /// Enter sleep (`$SLP`); idle only.
    Sleep,
    /// Request a status report immediately.
    RequestStatus,
    /// Clear the controller's sticky error state with an empty line.
    ClearError,
}
