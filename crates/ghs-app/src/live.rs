//! Live engine link for demo and serial transports.
//!
//! One engine thread owns the transport. This module owns the engine handle,
//! keeps a simulator session alive in demo mode, drains events without
//! blocking, and maps telemetry into [`AppState`]. It never writes controller
//! bytes: every action is a typed [`Command`] through the engine's safety
//! gateway.
//!
//! [`LinkMode`] records what the current (or last requested) session is. Demo
//! sessions hold a [`SimSession`]; serial sessions hold only the configuration
//! that was handed to the engine.

use std::sync::Arc;

use ghs_demo::{SimProfile, SimSession};
use ghs_engine::{
    Command, ConnectTarget, ControllerInfo, Engine, EngineConfig, EngineError, EngineEvent,
    EngineHandle, JobAbortReason, LogLevel, MachineSnapshot, SerialConfig, StatusReport,
};

use crate::state::{
    AppState, ConnectionState, ConsoleLevel, LeftTab, Units, abort_reason_label, format_elapsed,
    pause_reason_label,
};

/// Bundled program loaded automatically for the demo controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoProgram {
    /// Pocket outline with a half-circle arc.
    Pocket,
    /// Two-tool job that stops at `M6 T2` for the manual handshake.
    ToolChange,
    /// Same pocket with an `M0` after the first cut, for pause/resume review.
    ProgramStop,
}

impl DemoProgram {
    /// Name shown in the job header and console.
    pub const fn name(self) -> &'static str {
        match self {
            DemoProgram::Pocket => "demo-pocket.nc",
            DemoProgram::ToolChange => "demo-toolchange.nc",
            DemoProgram::ProgramStop => "demo-program-stop.nc",
        }
    }

    /// Full grblHAL source sent with `Command::LoadProgram`.
    pub const fn source(self) -> &'static str {
        match self {
            DemoProgram::Pocket => POCKET_SOURCE,
            DemoProgram::ToolChange => TOOL_CHANGE_SOURCE,
            DemoProgram::ProgramStop => PROGRAM_STOP_SOURCE,
        }
    }
}

/// Default demo: a small pocket with one arc. Ends in `M30`.
const POCKET_SOURCE: &str = "\
G21 G90 G54 G17
G0 Z5
G0 X0 Y0
M3 S12000
G1 Z-1 F200
G1 X40 F800
G2 X40 Y40 I0 J20
G1 X0
G1 Y0
M5
M30
";

/// Program-stop demo: the pocket interrupted by `M0`, then the same finish.
const PROGRAM_STOP_SOURCE: &str = "\
G21 G90 G54 G17
G0 Z5
G0 X0 Y0
M3 S12000
G1 Z-1 F200
G1 X40 F800
M0
G2 X40 Y40 I0 J20
G1 X0
G1 Y0
M5
M30
";

/// Tool-change demo: identical shape, then `M6 T2`, a second pass and `M30`.
const TOOL_CHANGE_SOURCE: &str = "\
G21 G90 G54 G17
G0 Z5
G0 X0 Y0
M3 S12000
G1 Z-1 F200
G1 X20 F800
M5
G0 Z5
M6 T2
M3 S9000
G0 X10 Y10
G1 Z-1 F200
G1 X30 F800
M5
M30
";

/// Transport used by a live session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkMode {
    /// In-process FlexiHAL simulator over the loopback transport.
    Demo,
    /// A real serial port opened by the engine.
    Serial(SerialConfig),
}

impl LinkMode {
    /// Short name for the connection dialog and console.
    pub fn describe(&self) -> String {
        match self {
            LinkMode::Demo => "Demo - FlexiHAL loopback".to_string(),
            LinkMode::Serial(config) => {
                format!("{} at {} baud, 8N1", config.port, config.baud)
            }
        }
    }
}

/// One live engine plus the optional in-process simulator feeding it.
pub struct LiveLink {
    engine: EngineHandle,
    sim: Option<SimSession>,
    mode: Option<LinkMode>,
    demo_program: DemoProgram,
    demo_pending: Option<DemoProgram>,
    last_info: Option<Arc<ControllerInfo>>,
    last_snapshot: Option<Arc<MachineSnapshot>>,
    last_status: Option<Arc<StatusReport>>,
}

impl LiveLink {
    /// Spawn an engine and connect it to a fresh FlexiHal demo simulator,
    /// preloading the requested bundled program once the handshake completes.
    pub fn start_demo_with(program: DemoProgram) -> Result<Self, String> {
        Self::start(LinkMode::Demo, program)
    }

    /// Spawn an engine and open a serial port.
    ///
    /// The engine validates the configuration and blocks until the port is
    /// open; the handshake then continues asynchronously through [`Self::poll`].
    pub fn start_serial(config: SerialConfig) -> Result<Self, String> {
        Self::start(LinkMode::Serial(config), DemoProgram::Pocket)
    }

    fn start(mode: LinkMode, demo_program: DemoProgram) -> Result<Self, String> {
        let engine = Engine::spawn(EngineConfig::default()).map_err(|error| error.to_string())?;
        let mut link = Self {
            engine,
            sim: None,
            mode: None,
            demo_program,
            demo_pending: None,
            last_info: None,
            last_snapshot: None,
            last_status: None,
        };
        link.connect(mode)?;
        Ok(link)
    }

    /// Connect the existing engine to a fresh FlexiHAL simulator and preload
    /// `program` after the handshake.
    pub fn connect_demo_with(&mut self, program: DemoProgram) -> Result<(), String> {
        self.demo_program = program;
        self.connect(LinkMode::Demo)
    }

    /// Connect the existing engine to a serial port.
    ///
    /// Any demo simulator is stopped first. On failure the engine stays
    /// disconnected and the error text is returned for the dialog.
    pub fn connect_serial(&mut self, config: SerialConfig) -> Result<(), String> {
        self.connect(LinkMode::Serial(config))
    }

    /// Open the requested transport. The engine must not already be connected;
    /// the shell presents Disconnect before Connect for that case.
    fn connect(&mut self, mode: LinkMode) -> Result<(), String> {
        if self.engine.is_connected() {
            return Err("disconnect the current link first".to_string());
        }
        self.stop_sim();
        self.drain_events();
        self.reset_cache();
        match &mode {
            LinkMode::Demo => {
                let (endpoint, sim) =
                    ghs_demo::start(SimProfile::FlexiHal).map_err(|e| e.to_string())?;
                self.sim = Some(sim);
                if let Err(error) = self
                    .engine
                    .connect(ConnectTarget::Transport(Box::new(endpoint)))
                {
                    self.stop_sim();
                    return Err(error.to_string());
                }
            }
            LinkMode::Serial(config) => {
                if let Err(error) = self.engine.connect(ConnectTarget::Serial(config.clone())) {
                    return Err(error.to_string());
                }
            }
        }
        self.demo_pending = match &mode {
            LinkMode::Demo => Some(self.demo_program),
            LinkMode::Serial(_) => None,
        };
        self.mode = Some(mode);
        Ok(())
    }

    /// Discard events left in the queue by the previous session.
    ///
    /// The engine event receiver survives disconnect and reconnect; without
    /// this, a late "Disconnected" log from the old session can appear in the
    /// fresh session's console.
    fn drain_events(&self) {
        while self.engine.events().try_recv().is_ok() {}
    }

    fn stop_sim(&mut self) {
        if let Some(mut sim) = self.sim.take() {
            sim.stop();
        }
    }

    /// Close the transport. The engine thread stays alive for a reconnect.
    pub fn disconnect(&self) {
        self.engine.disconnect();
    }

    /// True while a transport is open, including during the handshake.
    pub fn is_connected(&self) -> bool {
        self.engine.is_connected()
    }

    /// Transport requested for the current session, if one was opened.
    pub fn mode(&self) -> Option<&LinkMode> {
        self.mode.as_ref()
    }

    /// Send one typed command through the engine gateway.
    ///
    /// # Errors
    ///
    /// Returns the gateway or transport error unchanged.
    pub fn send(&self, command: Command) -> Result<(), EngineError> {
        self.engine.send(command)
    }

    /// Drain events, refresh connection state, and apply new telemetry.
    ///
    /// Returns `true` when the view model changed and the shell should redraw.
    pub fn poll(&mut self, state: &mut AppState) -> bool {
        let mut changed = false;
        let mut events = Vec::new();
        while let Ok(event) = self.engine.events().try_recv() {
            events.push(event);
        }
        for event in events {
            changed |= self.apply_event(state, event);
        }

        let connected = self.engine.is_connected();
        let ready = self.engine.is_ready();
        if ready && state.connection != ConnectionState::Ready {
            state.connection = ConnectionState::Ready;
            changed = true;
        } else if connected
            && !ready
            && !matches!(
                state.connection,
                ConnectionState::Connecting | ConnectionState::Ready
            )
        {
            state.connection = ConnectionState::Connecting;
            changed = true;
        } else if !connected
            && matches!(
                state.connection,
                ConnectionState::Connecting | ConnectionState::Ready
            )
        {
            state.connection = ConnectionState::Disconnected;
            changed = true;
        }

        // Demo mode supplies its own source: send it exactly once per
        // connection after the handshake, never on every poll.
        if ready && let Some(program) = self.demo_pending.take() {
            let name = program.name().to_string();
            if let Err(error) = self.send(Command::LoadProgram {
                name,
                source: program.source().to_string(),
            }) {
                state.push_console(
                    ConsoleLevel::Error,
                    format!("Demo program load failed: {error}"),
                );
                changed = true;
            }
        }

        if let Some(info) = self.engine.controller_info()
            && self
                .last_info
                .as_ref()
                .is_none_or(|last| !Arc::ptr_eq(last, &info))
        {
            state.apply_controller_info(&info);
            self.last_info = Some(info);
            changed = true;
        }

        if let Some(parser) = self.engine.parser_state()
            && let Some(units) = parser.units
        {
            let display = Units::from_core(units);
            if state.profile.capabilities.units != display {
                state.set_units(units);
                changed = true;
            }
        }

        if ready {
            let snapshot = self.engine.snapshot();
            let status = self.engine.last_status();
            let snapshot_changed = self
                .last_snapshot
                .as_ref()
                .is_none_or(|last| !Arc::ptr_eq(last, &snapshot));
            let status_changed = option_arc_changed(&self.last_status, &status);
            if snapshot_changed || status_changed {
                state.apply_snapshot(&snapshot, status.as_deref());
                self.last_snapshot = Some(snapshot);
                self.last_status = status;
                changed = true;
            }
        }

        changed
    }

    fn apply_event(&mut self, state: &mut AppState, event: EngineEvent) -> bool {
        // The job view model owns its event state machine; this function adds
        // the console lines and the tab switch. Never fabricate an `ok` line:
        // the engine streams the real command and acknowledgement.
        let job_changed = state.job.apply_event(&event);
        match event {
            EngineEvent::Connected => {
                state.connection = ConnectionState::Ready;
                state.push_console(
                    ConsoleLevel::Info,
                    "Controller connected; live telemetry is flowing",
                );
                true
            }
            EngineEvent::Disconnected => {
                state.connection = ConnectionState::Disconnected;
                // Never leave a tool-change sheet behind a dead link.
                state.job.interrupted();
                state.push_console(
                    ConsoleLevel::Warn,
                    "Disconnected; the last telemetry is stale",
                );
                true
            }
            EngineEvent::ConnectFailed(reason) => {
                state.connection = ConnectionState::Failed(reason.clone());
                state.push_console(ConsoleLevel::Error, format!("Connect failed: {reason}"));
                true
            }
            EngineEvent::Log { level, text } => {
                if discovery_noise(&text) {
                    return false;
                }
                state.push_console(log_level(level), text);
                true
            }
            EngineEvent::Alarm(code) => {
                state.machine_state = "Alarm";
                state.push_console(ConsoleLevel::Error, format!("ALARM:{code}"));
                true
            }
            EngineEvent::Error(code) => {
                state.push_console(ConsoleLevel::Error, format!("error:{code}"));
                true
            }
            EngineEvent::Banner(_) => {
                // The engine already logged the banner as an Info line.
                false
            }
            EngineEvent::ControllerInfo(info) => {
                if self
                    .last_info
                    .as_ref()
                    .is_some_and(|last| Arc::ptr_eq(last, &info))
                {
                    return false;
                }
                state.apply_controller_info(&info);
                self.last_info = Some(info);
                true
            }
            EngineEvent::Settings(_)
            | EngineEvent::CodeTables { .. }
            | EngineEvent::Probe(_)
            | EngineEvent::SettingWritten { .. } => false,
            EngineEvent::JobLoaded {
                name, total_lines, ..
            } => {
                // Loading is an explicit "show me the job" action for the demo
                // and future file picker; reveal the parsed list immediately.
                state.left_tab = LeftTab::Program;
                state.push_console(
                    ConsoleLevel::Info,
                    format!("Program loaded: {name} ({total_lines} lines)"),
                );
                true
            }
            EngineEvent::JobStarted {
                from_line,
                total_lines,
            } => {
                state.push_console(
                    ConsoleLevel::Info,
                    format!("Job started at line {from_line} ({total_lines} lines)"),
                );
                true
            }
            EngineEvent::JobProgress { .. } => job_changed,
            EngineEvent::JobPaused { line, reason } => {
                state.push_console(
                    ConsoleLevel::Warn,
                    format!("Job paused at line {line}: {}", pause_reason_label(reason)),
                );
                true
            }
            EngineEvent::JobResumed => {
                state.push_console(ConsoleLevel::Info, "Job resumed");
                true
            }
            EngineEvent::JobCompleted { elapsed } => {
                state.push_console(
                    ConsoleLevel::Ok,
                    format!("Job completed in {}", format_elapsed(elapsed)),
                );
                true
            }
            EngineEvent::JobAborted { line, reason } => {
                let level = if matches!(
                    reason,
                    JobAbortReason::Alarm | JobAbortReason::EmergencyStop
                ) {
                    ConsoleLevel::Error
                } else {
                    ConsoleLevel::Warn
                };
                state.push_console(
                    level,
                    format!("Job aborted at line {line}: {}", abort_reason_label(reason)),
                );
                true
            }
            EngineEvent::JobError {
                line,
                code,
                message,
            } => {
                let text = if code == 0 {
                    format!("Job error at line {line}: {message}")
                } else {
                    format!("Job error at line {line}: {message} (error:{code})")
                };
                state.push_console(ConsoleLevel::Error, text);
                true
            }
            EngineEvent::ToolChangeRequired { tool, line } => {
                state.push_console(
                    ConsoleLevel::Warn,
                    format!(
                        "Tool T{tool} required at line {line}; acknowledge, then resume the job"
                    ),
                );
                true
            }
            // Engine events are non-exhaustive; new probe events land here
            // until the matching UI panel consumes them.
            _ => false,
        }
    }

    fn reset_cache(&mut self) {
        self.last_info = None;
        self.last_snapshot = None;
        self.last_status = None;
    }
}

fn option_arc_changed<T>(last: &Option<Arc<T>>, current: &Option<Arc<T>>) -> bool {
    match (last, current) {
        (None, None) => false,
        (Some(last), Some(current)) => !Arc::ptr_eq(last, current),
        _ => true,
    }
}

fn log_level(level: LogLevel) -> ConsoleLevel {
    match level {
        LogLevel::Info => ConsoleLevel::Info,
        LogLevel::Command => ConsoleLevel::Command,
        LogLevel::Ok => ConsoleLevel::Ok,
        LogLevel::Warn => ConsoleLevel::Warn,
        LogLevel::Error => ConsoleLevel::Error,
    }
}

/// Drop the per-setting `$110=...` dump from discovery.
///
/// A controller can return hundreds of settings during the handshake. They are
/// real controller output and remain in the engine's settings snapshot, but
/// they would otherwise evict the banner, identity and every user command from
/// the bounded 200-line console before the first screen is drawn.
fn discovery_noise(text: &str) -> bool {
    text.strip_prefix('$')
        .is_some_and(|rest| rest.starts_with(|ch: char| ch.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use ghs_engine::{Axis, MachineState};

    use super::*;

    fn wait_until(mut predicate: impl FnMut() -> bool, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if predicate() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        predicate()
    }

    #[test]
    fn demo_link_connects_sends_mdi_and_reconnects() {
        let mut link = LiveLink::start_demo_with(DemoProgram::Pocket).expect("start demo engine");
        assert_eq!(link.mode(), Some(&LinkMode::Demo));
        assert!(
            wait_until(|| link.engine.is_ready(), Duration::from_secs(5)),
            "demo handshake did not complete"
        );
        assert!(
            wait_until(
                || link.engine.snapshot().state == MachineState::Idle,
                Duration::from_secs(2)
            ),
            "demo controller did not report Idle"
        );

        link.send(Command::Mdi("G0 X1 Y2".into()))
            .expect("MDI send");
        assert!(
            wait_until(
                || {
                    let position = link.engine.snapshot().mpos;
                    (position.get(Axis::X) - 1.0).abs() < 0.001
                        && (position.get(Axis::Y) - 2.0).abs() < 0.001
                },
                Duration::from_secs(3)
            ),
            "snapshot position did not follow the MDI command"
        );

        link.disconnect();
        assert!(
            wait_until(|| !link.engine.is_connected(), Duration::from_secs(2)),
            "disconnect did not close the link"
        );

        link.connect_demo_with(DemoProgram::Pocket)
            .expect("demo reconnect");
        assert!(
            wait_until(|| link.engine.is_ready(), Duration::from_secs(5)),
            "reconnect handshake did not complete"
        );
        link.disconnect();
    }

    #[test]
    fn a_rejected_serial_config_leaves_demo_reconnect_available() {
        let mut link = LiveLink::start_demo_with(DemoProgram::Pocket).expect("start demo engine");
        assert!(
            wait_until(|| link.engine.is_ready(), Duration::from_secs(5)),
            "demo handshake did not complete"
        );
        link.disconnect();
        assert!(
            wait_until(|| !link.engine.is_connected(), Duration::from_secs(2)),
            "disconnect did not close the link"
        );

        // An empty port name fails validation before any hardware is touched.
        let error = link
            .connect_serial(SerialConfig::new(""))
            .expect_err("an empty port must be rejected");
        assert!(!error.is_empty());
        assert!(!link.engine.is_connected());
        assert!(link.sim.is_none(), "a serial attempt drops the simulator");

        link.connect_demo_with(DemoProgram::Pocket)
            .expect("demo reconnect after a bad config");
        assert!(
            wait_until(|| link.engine.is_ready(), Duration::from_secs(5)),
            "demo handshake after a rejected config did not complete"
        );
        link.disconnect();
    }

    #[test]
    fn a_missing_serial_port_reports_an_error_and_stays_disconnected() {
        let mut link = LiveLink::start_demo_with(DemoProgram::Pocket).expect("start demo engine");
        assert!(
            wait_until(|| link.engine.is_ready(), Duration::from_secs(5)),
            "demo handshake did not complete"
        );
        link.disconnect();
        assert!(
            wait_until(|| !link.engine.is_connected(), Duration::from_secs(2)),
            "disconnect did not close the link"
        );

        // No hardware is opened: the path does not exist, so the engine must
        // return an error and leave the session disconnected.
        let error = link
            .connect_serial(SerialConfig::new("/dev/ghs-rs-missing-tty"))
            .expect_err("a missing serial port must fail");
        assert!(!error.is_empty());
        assert!(!link.engine.is_connected());

        link.connect_demo_with(DemoProgram::Pocket)
            .expect("demo reconnect after a failed port");
        assert!(
            wait_until(|| link.engine.is_ready(), Duration::from_secs(5)),
            "demo handshake after a failed serial port did not complete"
        );
        link.disconnect();
    }

    #[test]
    fn mode_descriptions_name_the_transport() {
        assert!(LinkMode::Demo.describe().contains("FlexiHAL"));
        let config = SerialConfig {
            baud: 250_000,
            ..SerialConfig::new("/dev/ttyACM0")
        };
        let description = LinkMode::Serial(config).describe();
        assert!(description.contains("/dev/ttyACM0"));
        assert!(description.contains("250000"));
        assert!(description.contains("8N1"));
    }

    #[test]
    fn demo_programs_parse_and_match_their_purpose() {
        let pocket =
            ghs_gcode::parse_program(DemoProgram::Pocket.source()).expect("pocket demo parses");
        assert_eq!(pocket.stats.tool_changes, 0);
        assert!(pocket.stats.arc_moves >= 1);
        assert_eq!(
            pocket.lines.last().map(|line| line.source.as_str()),
            Some("M30")
        );

        let tool = ghs_gcode::parse_program(DemoProgram::ToolChange.source())
            .expect("tool-change demo parses");
        assert_eq!(tool.stats.tool_changes, 1);
        assert!(
            tool.lines.iter().any(|line| line.source.contains("M6 T2")),
            "the tool-change demo must carry the M6 block"
        );
        assert_eq!(
            tool.lines.last().map(|line| line.source.as_str()),
            Some("M30")
        );
    }

    #[test]
    fn demo_link_preloads_the_program_and_starts_it() {
        let mut link = LiveLink::start_demo_with(DemoProgram::Pocket).expect("start demo engine");
        let mut state = AppState::new();
        state.begin_live("test transport");

        assert!(
            wait_until(
                || {
                    link.poll(&mut state);
                    state.job.has_program()
                },
                Duration::from_secs(10)
            ),
            "the demo program was not loaded after the handshake"
        );
        assert_eq!(state.job.name, DemoProgram::Pocket.name());
        assert_eq!(state.left_tab, LeftTab::Program);
        assert_eq!(state.job.total, 11);
        assert_eq!(state.job.status_label(), "LOADED");

        link.send(Command::StartProgram { from_line: None })
            .expect("start the loaded demo program");
        // The controller acknowledges a small program almost immediately, so
        // wait for the terminal state instead of sampling a RUNNING frame.
        assert!(
            wait_until(
                || {
                    link.poll(&mut state);
                    state.job.state == ghs_engine::JobState::Completed
                },
                Duration::from_secs(10)
            ),
            "the demo job did not complete"
        );
        assert_eq!(state.job.completed, 11);
        assert_eq!(state.job.current_line, 11);
        assert_eq!(state.job.fraction(), 1.0);
        assert!(
            state
                .console
                .iter()
                .any(|line| line.text.starts_with("Job completed")),
            "the completion event was not mapped to the console"
        );
        link.disconnect();
    }

    #[test]
    fn demo_link_pauses_at_program_stop_and_resumes() {
        let mut link =
            LiveLink::start_demo_with(DemoProgram::ProgramStop).expect("start demo engine");
        let mut state = AppState::new();
        state.begin_live("test transport");
        assert!(
            wait_until(
                || {
                    link.poll(&mut state);
                    state.job.has_program()
                },
                Duration::from_secs(10)
            ),
            "the program-stop demo was not loaded"
        );

        link.send(Command::StartProgram { from_line: None })
            .expect("start the program-stop demo");
        assert!(
            wait_until(
                || {
                    link.poll(&mut state);
                    state.job.pause_reason == Some(ghs_engine::JobPauseReason::ProgramStop)
                },
                Duration::from_secs(10)
            ),
            "the job did not pause at M0"
        );
        assert_eq!(state.job.current_line, 7);
        assert!(state.job.fraction() > 0.5 && state.job.fraction() < 1.0);
        assert_eq!(state.job.status_label(), "PAUSED");
        assert!(
            state
                .console
                .iter()
                .any(|line| line.text.starts_with("Job paused at line 7")),
            "the pause event was not mapped to the console"
        );

        link.send(Command::ResumeProgram).expect("resume after M0");
        assert!(
            wait_until(
                || {
                    link.poll(&mut state);
                    state.job.state == ghs_engine::JobState::Completed
                },
                Duration::from_secs(10)
            ),
            "the resumed job did not complete"
        );
        assert_eq!(state.job.fraction(), 1.0);
        link.disconnect();
    }

    #[test]
    fn demo_link_stops_at_a_tool_change_until_acknowledged_and_resumed() {
        let mut link =
            LiveLink::start_demo_with(DemoProgram::ToolChange).expect("start demo engine");
        let mut state = AppState::new();
        state.begin_live("test transport");
        assert!(
            wait_until(
                || {
                    link.poll(&mut state);
                    state.job.has_program()
                },
                Duration::from_secs(10)
            ),
            "the tool-change demo was not loaded"
        );

        link.send(Command::StartProgram { from_line: None })
            .expect("start the tool-change demo");
        assert!(
            wait_until(
                || {
                    link.poll(&mut state);
                    state.job.tool_change_pending()
                },
                Duration::from_secs(10)
            ),
            "the job did not stop for a tool change"
        );
        let change = state.job.tool_change.expect("pending tool change");
        assert_eq!(change.tool, 2);
        assert_eq!(change.line, 9);
        assert!(!change.acknowledged);
        assert_eq!(state.job.status_label(), "TOOL CHANGE");
        assert!(
            state
                .console
                .iter()
                .any(|line| line.text.starts_with("Tool T2 required")),
            "the tool-change event was not mapped to the console"
        );

        link.send(Command::AcknowledgeToolChange)
            .expect("acknowledge the tool change");
        // The shell marks this after the engine accepts the command; there is
        // no separate acknowledgement event.
        assert!(state.job.acknowledge_tool_change());
        link.send(Command::ResumeProgram)
            .expect("resume after the tool change");
        assert!(
            wait_until(
                || {
                    link.poll(&mut state);
                    state.job.state == ghs_engine::JobState::Completed
                },
                Duration::from_secs(10)
            ),
            "the job did not continue after the tool change"
        );
        assert_eq!(state.job.fraction(), 1.0);
        link.disconnect();
    }
}
