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
    ///
    /// Clears the virtual optional-stop, single-block and block-delete flags
    /// and invalidates the cached probe result. grblHAL clears its runtime
    /// switch state on reset; a physical switch is re-read by the controller
    /// and reported in the status `Pn` field.
    Reset,
    /// Enter sleep (`$SLP`); idle only.
    Sleep,
    /// Request a status report immediately.
    RequestStatus,
    /// Clear the controller's sticky error state with an empty line.
    ClearError,
    /// Parse and validate a program for streaming.
    ///
    /// Parsing runs on the engine thread. The program is retained until it is
    /// replaced, so a stop or disconnect never discards the source.
    LoadProgram {
        /// Program name or path shown in the UI.
        name: String,
        /// Complete program source text.
        source: String,
    },
    /// Stream the loaded program one acknowledged line at a time.
    ///
    /// `from_line` is deferred: only `None` or `Some(1)` is accepted until the
    /// modal preamble can be produced safely. A larger line returns
    /// [`crate::EngineError::Unsupported`].
    StartProgram {
        /// Optional 1-based source line to start from.
        from_line: Option<u32>,
    },
    /// Feed hold the running job and freeze program output.
    PauseProgram,
    /// Cycle start a paused job or continue after a tool change.
    ResumeProgram,
    /// Stop the job and retain the loaded program.
    StopProgram {
        /// Send a soft reset after stopping. A reset discards the controller's
        /// buffered motion and pending tool change.
        reset: bool,
    },
    /// Acknowledge a pending manual tool change with `0xA3`.
    AcknowledgeToolChange,
    /// Enable or disable the virtual optional-stop flag.
    ///
    /// The accepted value is published by [`crate::EngineHandle::optional_stop`].
    SetOptionalStop(bool),
    /// Enable or disable the virtual single-block flag.
    ///
    /// The accepted value is published by [`crate::EngineHandle::single_block`].
    SetSingleBlock(bool),
    /// Enable or disable block delete.
    ///
    /// The accepted value is published by [`crate::EngineHandle::block_delete`].
    SetBlockDelete(bool),
    /// Enable or disable a breakpoint at a 1-based source line.
    SetBreakpoint {
        /// Source line number.
        line: u32,
        /// Whether the job pauses before this line.
        enabled: bool,
    },
    /// Enter controller check mode and stream the loaded program.
    ///
    /// Deferred until check mode is wired end to end.
    StartCheckRun,
}
