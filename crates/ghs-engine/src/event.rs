//! Engine telemetry and console events.

use std::sync::Arc;
use std::time::Duration;

use ghs_core::{ControllerInfo, ProbeResult, Setting, SettingValue, ToolEntry};
use ghs_gcode::Program;
use ghs_proto::{CodeTable, SettingDescription, SettingGroup};

/// Why a running job stopped feeding lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobPauseReason {
    /// The operator pressed pause and the engine sent a feed hold.
    Operator,
    /// The controller reported `Hold` without a job-level pause first.
    FeedHold,
    /// An `M0` block reached the controller.
    ProgramStop,
    /// An `M1` block reached the controller while optional stop was enabled.
    OptionalStop,
    /// The virtual single-block switch paused after an acknowledged line.
    SingleBlock,
    /// An enabled breakpoint line was about to be sent.
    Breakpoint,
    /// The status watchdog stopped receiving telemetry during a job.
    Watchdog,
}

/// Why a job stopped before reaching `M2`/`M30`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobAbortReason {
    /// The operator stopped the job.
    OperatorStop,
    /// The controller raised an alarm or an E-stop.
    Alarm,
    /// The safety door opened.
    Door,
    /// The link was closed or lost.
    Disconnected,
    /// A soft reset (`0x18`) interrupted the job.
    SoftReset,
    /// A soft E-stop (`0x9F`) interrupted the job.
    EmergencyStop,
    /// The program could not be streamed.
    ProgramError,
    /// The controller entered sleep during a job.
    ControllerSleep,
}

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

    /// Typed setting metadata and groups from `$ES`/`$EG`, for generated forms.
    SettingMetadata {
        /// Typed setting descriptions.
        descriptions: Arc<Vec<SettingDescription>>,
        /// Hierarchical setting groups.
        groups: Arc<Vec<SettingGroup>>,
    },

    /// Runtime alarm and error descriptions from `$EA`/`$EE`.
    CodeTables {
        /// Alarm code descriptions.
        alarms: Arc<CodeTable>,
        /// Error code descriptions.
        errors: Arc<CodeTable>,
    },

    /// The controller-reported `[T:...]` tool table changed.
    ///
    /// Entries are sorted by tool number and are read-only. The same list is
    /// available from [`crate::EngineHandle::controller_tools`]; the event
    /// avoids polling and never mixes these entries with the application's own
    /// editable tool table.
    ControllerTools(Arc<Vec<ToolEntry>>),

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

    /// A validated program was loaded into the job runner.
    JobLoaded {
        /// Program name or path supplied by the caller.
        name: String,
        /// Number of source lines that will be streamed: the source up to and
        /// including the first `M2`/`M30`, or the whole file when it has no
        /// program-end block.
        total_lines: u32,
        /// Parsed program, shared so the UI can render the line list and
        /// toolpath without copying it.
        program: Arc<Program>,
    },

    /// Streaming started for the loaded program.
    JobStarted {
        /// First 1-based source line of this run.
        from_line: u32,
        /// Number of source lines in the run.
        total_lines: u32,
    },

    /// Throttled job progress.
    ///
    /// `completed` counts source lines resolved: controller-acknowledged
    /// blocks plus blank/comment-only and block-deleted lines that needed no
    /// controller traffic.
    JobProgress {
        /// Current 1-based source line.
        current_line: u32,
        /// Resolved source lines.
        completed: u32,
        /// Source lines in this run.
        total: u32,
        /// Active job time; paused periods are excluded.
        elapsed: Duration,
    },

    /// The job stopped feeding lines but retained its position.
    JobPaused {
        /// Source line at which the job paused.
        line: u32,
        /// Why the job paused.
        reason: JobPauseReason,
    },

    /// A paused job resumed after an explicit operator action.
    JobResumed,

    /// The program reached its end and every line was resolved.
    JobCompleted {
        /// Active job time.
        elapsed: Duration,
    },

    /// The job stopped before completion.
    JobAborted {
        /// Source line reached when the job stopped.
        line: u32,
        /// Why the job stopped.
        reason: JobAbortReason,
    },

    /// A manual tool change is required.
    ///
    /// No further program lines are fed until [`crate::Command::AcknowledgeToolChange`]
    /// and [`crate::Command::ResumeProgram`] arrive.
    ToolChangeRequired {
        /// Requested tool number.
        tool: u32,
        /// Source line carrying the `M6`.
        line: u32,
    },

    /// The controller rejected a program line.
    JobError {
        /// Source line that the controller rejected.
        line: u32,
        /// Controller error code from `error:<n>`; zero for an engine-side
        /// stream failure that never reached the controller.
        code: u16,
        /// Human-readable description.
        message: String,
    },
}
