//! Engine telemetry and console events.

use std::sync::Arc;

use ghs_core::{ControllerInfo, ProbeResult, Setting, SettingValue};
use ghs_proto::CodeTable;

/// Severity/kind of one console log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogLevel {
    /// Informational controller output or engine state.
    Info,
    /// A line written to the controller.
    Command,
    /// An `ok` acknowledgement.
    Ok,
    /// A recoverable warning.
    Warn,
    /// An error, alarm or rejected line.
    Error,
}

/// One event published by the engine thread.
///
/// Logs and telemetry events share one bounded queue. The engine drops the
/// oldest event when the queue is full, so a UI that stops draining can never
/// stall the control loop.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum EngineEvent {
    /// The handshake completed and telemetry is flowing.
    Connected,

    /// The transport was closed, either on request or because the link failed.
    Disconnected,

    /// The handshake failed before the session became ready.
    ///
    /// A [`EngineEvent::Disconnected`] follows this event.
    ConnectFailed(String),

    /// One console line.
    Log {
        /// Line severity or kind.
        level: LogLevel,
        /// Text without control characters.
        text: String,
    },

    /// The controller raised an alarm code (`ALARM:<n>` or an `Alarm` status).
    Alarm(u16),

    /// The controller rejected a line with `error:<n>`.
    Error(u16),

    /// A controller banner, for example `Grbl 1.1f ['$' for help]`.
    Banner(String),

    /// The identity and capability record parsed from `$I+`.
    ControllerInfo(Arc<ControllerInfo>),

    /// Every setting value read during discovery, enriched with metadata from
    /// `$ES`/`$EG` when the controller advertises `ENUMS`.
    Settings(Arc<Vec<Setting>>),

    /// Runtime alarm and error descriptions from `$EA`/`$EE`.
    CodeTables {
        /// Alarm code descriptions.
        alarms: Arc<CodeTable>,
        /// Error code descriptions.
        errors: Arc<CodeTable>,
    },

    /// A probe result from `[PRB:...]` or the status `PRB:` field.
    Probe(ProbeResult),

    /// A verified settings write.
    SettingWritten {
        /// Setting number.
        number: u16,
        /// Value reported by the read-back.
        value: SettingValue,
        /// Whether the read-back matched the value that was requested.
        verified: bool,
    },
}
