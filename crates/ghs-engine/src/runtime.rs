//! The engine thread: session state machine, protocol driving and the safety
//! gateway.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, TrySendError};
use ghs_core::{
    AlarmCode, ControllerInfo, JobState, MachineSnapshot, MachineState, Setting, SettingValue,
    ToolEntry,
};
use ghs_proto::{
    CodeTable, ExtendedCommand, JogCommand, LastEvents, LimitState, Offsets, ParserState,
    PinMapping, PinState, RealtimeCommand, ResetTarget, Response, ResponseParser,
    SettingDescription, SettingGroup, StartupLine, StatusReport, Streamer, StreamerError,
    SystemCommand, validate_line,
};
use ghs_serial::{SerialTransport, Transport, TransportError, TransportEvent};

use crate::command::{Command, ConnectTarget};
use crate::config::EngineConfig;
use crate::error::EngineError;
use crate::event::{EngineEvent, JobAbortReason, JobPauseReason, LogLevel};
use crate::handle::{EngineInput, MAX_PIN_MAPPINGS, MAX_STARTUP_LINES, SharedState};
use crate::job::JobRunner;
use crate::overrides::{feed_override_commands, rapid_override_command, spindle_override_commands};
use crate::telemetry::apply_status;

/// Maximum number of lines accepted in one [`Command::Mdi`].
const MAX_MDI_LINES: usize = 1_024;
/// Maximum total bytes accepted in one [`Command::Mdi`].
const MAX_MDI_BYTES: usize = 64 * 1_024;
/// Longest console text kept in one event.
const MAX_LOG_TEXT: usize = 512;
/// Maximum number of controller-reported `[T:...]` entries retained.
///
/// A grblHAL tool table is small (the reference build has none). The bound
/// protects the session from a malformed or hostile `$#` response that keeps
/// reporting new tool numbers.
const MAX_CONTROLLER_TOOLS: usize = 256;
/// Upper bound on a controller-reported serial receive buffer, in bytes.
///
/// A grblHAL controller reports a few hundred bytes to a few kilobytes. A
/// corrupt or hostile `OPT` field must not let the streamer send unbounded
/// bytes before an acknowledgement.
const MAX_RX_BUFFER_SIZE: usize = 64 * 1_024;

/// What wakes the engine loop.
enum Wake {
    Command(Result<EngineInput, crossbeam_channel::RecvError>),
    Tick(Result<Instant, crossbeam_channel::RecvError>),
    Transport(Result<TransportEvent, crossbeam_channel::RecvError>),
}

/// Connection and handshake phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Disconnected,
    Listening {
        until: Instant,
    },
    MpgPassive {
        next_poll_at: Instant,
    },
    StatusProbe {
        deadline: Instant,
        fallback_sent: bool,
    },
    Discovering,
    Ready,
}

/// One discovery command in the handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscoveryStep {
    Info,
    Settings,
    Offsets,
    ParserState,
    AlarmCodes,
    ErrorCodes,
    SettingDescriptions,
    SettingGroups,
}

impl DiscoveryStep {
    const fn command(self) -> &'static str {
        match self {
            DiscoveryStep::Info => "$I+",
            DiscoveryStep::Settings => "$$",
            DiscoveryStep::Offsets => "$#",
            DiscoveryStep::ParserState => "$G",
            DiscoveryStep::AlarmCodes => "$EA",
            DiscoveryStep::ErrorCodes => "$EE",
            DiscoveryStep::SettingDescriptions => "$ES",
            DiscoveryStep::SettingGroups => "$EG",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            DiscoveryStep::Info => "$I+",
            DiscoveryStep::Settings => "$$",
            DiscoveryStep::Offsets => "$#",
            DiscoveryStep::ParserState => "$G",
            DiscoveryStep::AlarmCodes => "$EA",
            DiscoveryStep::ErrorCodes => "$EE",
            DiscoveryStep::SettingDescriptions => "$ES",
            DiscoveryStep::SettingGroups => "$EG",
        }
    }
}

/// Pending capability discovery.
struct Discovery {
    steps: VecDeque<DiscoveryStep>,
    current: DiscoveryStep,
    deadline: Instant,
}

impl Discovery {
    fn new(now: Instant, timeout: Duration) -> Self {
        Discovery {
            steps: VecDeque::new(),
            current: DiscoveryStep::Info,
            deadline: deadline(now, timeout),
        }
    }
}

/// Why a line was queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinePurpose {
    Discovery(DiscoveryStep),
    MdiLine,
    Jog,
    System,
    SettingWrite,
    SettingRead,
    ClearError,
    JobLine { index: usize, line: u32 },
}

/// Stage of a settings write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteStage {
    AwaitAck,
    AwaitRead,
}

struct PendingWrite {
    number: u16,
    expected: SettingValue,
    stage: WriteStage,
}

/// The engine thread's owned state.
pub(crate) struct EngineRuntime {
    config: EngineConfig,
    commands: Receiver<EngineInput>,
    ticks: Receiver<Instant>,
    events: Sender<EngineEvent>,
    event_drop: Receiver<EngineEvent>,
    shared: Arc<SharedState>,
    never: Receiver<TransportEvent>,

    transport: Option<Box<dyn Transport>>,
    transport_events: Option<Receiver<TransportEvent>>,
    parser: ResponseParser,
    streamer: Streamer,

    phase: Phase,
    discovery: Option<Discovery>,
    current_purpose: Option<LinePurpose>,
    mdi_queue: VecDeque<String>,
    pending_write: Option<PendingWrite>,
    advance_after_clear: bool,
    error_latched: bool,
    settings_dump_pending: bool,
    job: JobRunner,

    snapshot: Arc<MachineSnapshot>,
    last_status: Option<Arc<StatusReport>>,
    last_status_at: Option<Instant>,
    last_alarm_emitted: Option<u16>,
    mpg_active: bool,
    watchdog_paused: bool,
    rt_plus: bool,
    sleeping: bool,
    sleep_on_ack: bool,
    wake_probe_pending: bool,

    info: ControllerInfo,
    settings: BTreeMap<u16, SettingValue>,
    setting_descriptions: BTreeMap<u16, SettingDescription>,
    setting_groups: BTreeMap<u16, SettingGroup>,
    controller_tools: BTreeMap<u32, ToolEntry>,
    startup_lines: BTreeMap<u8, StartupLine>,
    alarm_codes: CodeTable,
    error_codes: CodeTable,
    offsets: Offsets,
    parser_state: Option<ParserState>,
    pin_map: BTreeMap<u16, PinMapping>,
    pin_state: Option<Arc<PinState>>,
    limit_state: Option<Arc<LimitState>>,
    last_events: Option<Arc<LastEvents>>,

    next_poll_at: Instant,
    next_full_at: Instant,
}

impl EngineRuntime {
    pub(crate) fn new(
        config: EngineConfig,
        commands: Receiver<EngineInput>,
        ticks: Receiver<Instant>,
        events: Sender<EngineEvent>,
        event_drop: Receiver<EngineEvent>,
        shared: Arc<SharedState>,
        streamer: Streamer,
    ) -> Self {
        let now = Instant::now();
        EngineRuntime {
            config,
            commands,
            ticks,
            events,
            event_drop,
            shared,
            never: crossbeam_channel::never(),
            transport: None,
            transport_events: None,
            parser: ResponseParser::new(),
            streamer,
            phase: Phase::Disconnected,
            discovery: None,
            current_purpose: None,
            mdi_queue: VecDeque::new(),
            pending_write: None,
            advance_after_clear: false,
            error_latched: false,
            settings_dump_pending: false,
            job: JobRunner::new(),
            snapshot: Arc::new(MachineSnapshot::default()),
            last_status: None,
            last_status_at: None,
            last_alarm_emitted: None,
            mpg_active: false,
            watchdog_paused: false,
            rt_plus: false,
            sleeping: false,
            sleep_on_ack: false,
            wake_probe_pending: false,
            info: ControllerInfo::default(),
            settings: BTreeMap::new(),
            setting_descriptions: BTreeMap::new(),
            setting_groups: BTreeMap::new(),
            controller_tools: BTreeMap::new(),
            startup_lines: BTreeMap::new(),
            alarm_codes: CodeTable::new(),
            error_codes: CodeTable::new(),
            offsets: Offsets::default(),
            parser_state: None,
            pin_map: BTreeMap::new(),
            pin_state: None,
            limit_state: None,
            last_events: None,
            next_poll_at: now,
            next_full_at: now,
        }
    }

    /// Run the engine loop until shutdown.
    pub(crate) fn run(mut self) {
        loop {
            if self.shared.shutdown.load(Ordering::Acquire) {
                break;
            }

            self.on_tick(Instant::now());
            if self.shared.shutdown.load(Ordering::Acquire) {
                break;
            }

            // Clone the receivers so the selected values are owned before the
            // handler mutates `self`.
            let commands = self.commands.clone();
            let ticks = self.ticks.clone();
            let transport_events = self
                .transport_events
                .clone()
                .unwrap_or_else(|| self.never.clone());

            let wake = crossbeam_channel::select! {
                recv(commands) -> message => Wake::Command(message),
                recv(ticks) -> tick => Wake::Tick(tick),
                recv(transport_events) -> event => Wake::Transport(event),
            };

            match wake {
                Wake::Command(Ok(input)) => self.handle_input(input),
                Wake::Command(Err(_)) => {
                    self.shared.shutdown.store(true, Ordering::Release);
                    break;
                }
                Wake::Tick(Ok(_)) => {}
                Wake::Tick(Err(_)) => break,
                Wake::Transport(Ok(event)) => self.handle_transport_event(event),
                Wake::Transport(Err(_)) => self.close_link("transport event channel closed"),
            }
        }

        self.close_link("engine stopped");
    }

    // ---------------------------------------------------------------- timers

    fn on_tick(&mut self, now: Instant) {
        if self.shared.shutdown.load(Ordering::Acquire) {
            return;
        }
        self.retry_error_clear();

        match self.phase {
            Phase::Disconnected => {}
            Phase::Listening { until } => {
                if now >= until {
                    self.finish_listen(now);
                }
            }
            Phase::MpgPassive { next_poll_at } => {
                if now >= next_poll_at {
                    self.poll_pendant(now);
                }
            }
            Phase::StatusProbe {
                deadline: probe_deadline,
                fallback_sent,
            } => {
                if now >= probe_deadline {
                    self.on_status_probe_timeout(now, fallback_sent);
                }
            }
            Phase::Discovering => self.on_discovery_tick(now),
            Phase::Ready => self.on_ready_tick(now),
        }

        if self.transport.is_some()
            && let Err(error) = self.flush_streamer()
        {
            self.close_link(&format!("send failed: {error}"));
        }
    }

    fn finish_listen(&mut self, now: Instant) {
        if self.mpg_active {
            self.phase = Phase::MpgPassive { next_poll_at: now };
            self.log_warn("MPG pendant active before handshake; waiting for release");
            return;
        }
        self.send_status_probe(now);
    }

    fn send_status_probe(&mut self, now: Instant) {
        match self.write_bytes(&[0x87]) {
            Ok(()) => {
                self.log_command("0x87 full status report request");
                self.phase = Phase::StatusProbe {
                    deadline: deadline(now, self.config.handshake_response),
                    fallback_sent: false,
                };
            }
            Err(error) => self.close_link(&format!("status probe failed: {error}")),
        }
    }

    fn on_status_probe_timeout(&mut self, now: Instant, fallback_sent: bool) {
        if fallback_sent {
            self.fail_handshake("no status report from the controller");
            return;
        }
        match self.write_bytes(b"?") {
            Ok(()) => {
                self.log_warn("No status after 0x87; retrying with the printable '?'");
                self.phase = Phase::StatusProbe {
                    deadline: deadline(now, self.config.handshake_response),
                    fallback_sent: true,
                };
            }
            Err(error) => self.close_link(&format!("status request failed: {error}")),
        }
    }

    fn poll_pendant(&mut self, now: Instant) {
        if !self.mpg_active {
            self.log_info("Pendant released; starting controller discovery");
            self.start_discovery(now);
            return;
        }
        if let Some(last) = self.last_status_at
            && now.saturating_duration_since(last) >= self.config.watchdog_disconnect
        {
            self.close_link("no telemetry while waiting for the pendant");
            return;
        }
        if let Err(error) = self.send_status_request(false, false) {
            self.close_link(&format!("status request failed: {error}"));
            return;
        }
        self.phase = Phase::MpgPassive {
            next_poll_at: deadline(now, self.config.status_poll_idle),
        };
    }

    fn on_discovery_tick(&mut self, now: Instant) {
        let timed_out = self
            .discovery
            .as_ref()
            .is_none_or(|discovery| now >= discovery.deadline);
        if !timed_out {
            return;
        }
        let Some(step) = self.discovery.as_ref().map(|discovery| discovery.current) else {
            self.finish_discovery(now);
            return;
        };
        self.log_warn(format!(
            "Controller discovery step {} timed out",
            step.label()
        ));
        self.streamer.clear();
        self.streamer.resume();
        self.current_purpose = None;
        if step == DiscoveryStep::Offsets {
            // A timed-out `$#` may have delivered only part of the table.
            self.clear_controller_tools();
        }
        self.advance_discovery(step, now);
    }

    fn on_ready_tick(&mut self, now: Instant) {
        if self.sleeping {
            return;
        }
        if let Some((line, tool)) = self.job.tool_change_stalled(now) {
            let message = format!("controller did not enter the Tool state for T{tool}");
            self.job.abort(now, false);
            self.streamer.pause();
            self.sync_job_snapshot();
            self.emit_job_progress(now);
            self.emit(EngineEvent::JobError {
                line,
                code: 0,
                message,
            });
            self.emit(EngineEvent::JobAborted {
                line,
                reason: JobAbortReason::ProgramError,
            });
            return;
        }
        if let Some(last_status) = self.last_status_at {
            let elapsed = now.saturating_duration_since(last_status);
            if elapsed >= self.config.watchdog_disconnect {
                self.close_link("status watchdog timeout");
                return;
            }
            let job_running = self.job.is_running();
            if !self.watchdog_paused
                && elapsed >= self.config.watchdog_pause
                && (self.snapshot.state.is_motion() || job_running)
            {
                self.watchdog_paused = true;
                let command = if self.snapshot.state == MachineState::Jog {
                    RealtimeCommand::JogCancel
                } else {
                    RealtimeCommand::FeedHold
                };
                self.log_warn(
                    "Status watchdog: telemetry stopped during motion; sending a hold command",
                );
                if let Err(error) = self.send_realtime_command(command, true) {
                    self.close_link(&format!("watchdog hold failed: {error}"));
                    return;
                }
                if job_running {
                    self.pause_job(JobPauseReason::Watchdog);
                }
            }
        }

        if now >= self.next_full_at {
            self.next_full_at = deadline(now, self.config.full_status_interval);
            self.next_poll_at = deadline(now, self.poll_interval());
            if let Err(error) = self.send_status_request(true, false) {
                self.close_link(&format!("full status request failed: {error}"));
            }
        } else if now >= self.next_poll_at {
            self.next_poll_at = deadline(now, self.poll_interval());
            if let Err(error) = self.send_status_request(false, false) {
                self.close_link(&format!("status request failed: {error}"));
            }
        }
        self.pump_job();
    }

    fn poll_interval(&self) -> Duration {
        if self.snapshot.state.is_motion() {
            self.config.status_poll_active
        } else {
            self.config.status_poll_idle
        }
    }

    fn retry_error_clear(&mut self) {
        if self.error_latched
            && self.transport.is_some()
            && self.streamer.is_idle()
            && self.current_purpose.is_none()
            && let Err(error) = self.send_clear_line()
        {
            self.log_warn(format!("Could not clear the controller error: {error}"));
        }
    }

    // --------------------------------------------------------------- inputs

    fn handle_input(&mut self, input: EngineInput) {
        match input {
            EngineInput::Connect { target, reply } => {
                let result = self.start_connect(target);
                let _ = reply.send(result);
            }
            EngineInput::Command { command, reply } => {
                let result = self.execute_command(command);
                if let Err(error) = &result {
                    tracing::debug!(%error, "command rejected");
                    self.log_warn(format!("Command rejected: {error}"));
                }
                let _ = reply.send(result);
            }
            EngineInput::Disconnect => self.close_link("disconnect requested"),
            EngineInput::Shutdown => self.shared.shutdown.store(true, Ordering::Release),
        }
    }

    fn handle_transport_event(&mut self, event: TransportEvent) {
        match event {
            TransportEvent::Bytes(bytes) => self.handle_bytes(&bytes),
            TransportEvent::Error(text) => {
                self.log_error(format!("Transport error: {text}"));
                self.close_link(&text);
            }
            TransportEvent::Closed => self.close_link("transport closed"),
        }
    }

    fn handle_bytes(&mut self, bytes: &[u8]) {
        if self.transport.is_none() {
            return;
        }
        let responses = self.parser.push(bytes);
        for response in responses {
            if self.transport.is_none() || self.shared.shutdown.load(Ordering::Acquire) {
                break;
            }
            self.handle_response(response);
        }
        if self.transport.is_some()
            && let Err(error) = self.flush_streamer()
        {
            self.close_link(&format!("send failed: {error}"));
        }
    }

    // ------------------------------------------------------------ connection

    fn start_connect(&mut self, target: ConnectTarget) -> Result<(), EngineError> {
        if self.transport.is_some() {
            return Err(EngineError::AlreadyConnected);
        }

        let transport: Box<dyn Transport> = match target {
            ConnectTarget::Serial(config) => {
                config.validate()?;
                Box::new(SerialTransport::open(config)?)
            }
            ConnectTarget::Transport(transport) => transport,
        };
        if !transport.is_open() {
            return Err(EngineError::Transport(TransportError::Closed));
        }
        let transport_events = transport.events().clone();

        self.reset_session();
        self.transport = Some(transport);
        self.transport_events = Some(transport_events);
        self.phase = Phase::Listening {
            until: deadline(Instant::now(), self.config.handshake_listen),
        };
        self.shared.connected.store(true, Ordering::Release);
        self.shared.ready.store(false, Ordering::Release);
        self.log_info("Transport opened; listening for the controller");
        Ok(())
    }

    fn reset_session(&mut self) {
        let now = Instant::now();
        self.parser.reset();
        self.streamer.clear();
        self.streamer.resume();
        let _ = self
            .streamer
            .set_rx_buffer_size(self.config.default_rx_buffer);
        self.shared.reset();
        self.job.reset_toggles();

        self.phase = Phase::Disconnected;
        self.discovery = None;
        self.current_purpose = None;
        self.mdi_queue.clear();
        self.pending_write = None;
        self.advance_after_clear = false;
        self.error_latched = false;
        self.settings_dump_pending = false;
        self.sleeping = false;
        self.sleep_on_ack = false;
        self.wake_probe_pending = false;

        let mut snapshot = MachineSnapshot::default();
        self.job.apply_info(&mut snapshot.job);
        self.snapshot = Arc::new(snapshot);
        self.last_status = None;
        self.last_status_at = None;
        self.last_alarm_emitted = None;
        self.mpg_active = false;
        self.watchdog_paused = false;
        self.rt_plus = false;

        self.info = ControllerInfo::default();
        self.settings.clear();
        self.setting_descriptions.clear();
        self.setting_groups.clear();
        self.controller_tools.clear();
        self.clear_startup_lines();
        self.alarm_codes = CodeTable::new();
        self.error_codes = CodeTable::new();
        self.offsets = Offsets::default();
        self.parser_state = None;
        self.pin_map.clear();
        self.pin_state = None;
        self.limit_state = None;
        self.last_events = None;

        self.next_poll_at = now;
        self.next_full_at = now;
        self.shared.set_snapshot(Arc::clone(&self.snapshot), None);
    }

    fn close_link(&mut self, reason: &str) {
        let was_connected = self.transport.is_some();
        if let Some(mut transport) = self.transport.take() {
            transport.close();
        }
        self.transport_events = None;
        self.parser.reset();
        self.streamer.clear();
        self.streamer.resume();
        self.discovery = None;
        self.current_purpose = None;
        self.mdi_queue.clear();
        self.pending_write = None;
        self.advance_after_clear = false;
        self.error_latched = false;
        self.settings_dump_pending = false;
        self.sleeping = false;
        self.sleep_on_ack = false;
        self.wake_probe_pending = false;
        self.phase = Phase::Disconnected;
        self.rt_plus = false;
        self.mpg_active = false;
        self.watchdog_paused = false;
        self.last_status_at = None;
        // Live pin levels and events belong to the link that just closed. The
        // static `$PINS` mapping is retained until the next session starts.
        self.clear_live_pin_reports();
        self.shared.connected.store(false, Ordering::Release);
        self.shared.ready.store(false, Ordering::Release);
        // The virtual switches belong to the closed controller session.
        self.job.reset_toggles();
        self.shared.reset_toggles();

        if was_connected {
            let aborted = self
                .job
                .abort(Instant::now(), true)
                .map(|line| (line, JobAbortReason::Disconnected));
            let mut next = (*self.snapshot).clone();
            next.state = MachineState::Unknown;
            next.alarm = None;
            self.job.apply_info(&mut next.job);
            self.snapshot = Arc::new(next);
            self.publish_snapshot();
            if let Some((line, reason)) = aborted {
                self.emit(EngineEvent::JobAborted { line, reason });
            }
            self.emit(EngineEvent::Disconnected);
            self.log_warn(format!("Disconnected: {reason}"));
        }
    }

    fn fail_handshake(&mut self, reason: &str) {
        self.emit(EngineEvent::ConnectFailed(bounded_text(reason)));
        self.log_error(format!("Handshake failed: {reason}"));
        self.close_link(reason);
    }

    // --------------------------------------------------------------- commands

    fn execute_command(&mut self, command: Command) -> Result<(), EngineError> {
        match command {
            Command::Realtime(command) => self.execute_realtime(command),
            Command::System(command) => self.execute_system(command),
            Command::Jog(jog) => self.execute_jog(jog),
            Command::Mdi(text) => self.execute_mdi(&text),
            Command::SetFeedOverride(percent) => self.execute_feed_override(percent),
            Command::SetSpindleOverride(percent) => self.execute_spindle_override(percent),
            Command::SetRapidOverride(percent) => self.execute_rapid_override(percent),
            Command::SetSetting { number, value } => self.execute_set_setting(number, value),
            Command::Home => self.execute_system(SystemCommand::HomeAll),
            Command::Unlock => self.execute_system(SystemCommand::Unlock),
            Command::Reset => self.execute_realtime(RealtimeCommand::SoftReset),
            Command::Sleep => self.execute_system(SystemCommand::Sleep),
            Command::RequestStatus => {
                self.ensure_link()?;
                if self.sleeping {
                    self.wake_probe_pending = true;
                }
                self.send_status_request(false, true)
            }
            Command::ClearError => self.execute_clear_error(),
            Command::LoadProgram { name, source } => self.execute_load_program(name, &source),
            Command::StartProgram { from_line } => self.execute_start_program(from_line),
            Command::PauseProgram => self.execute_pause_program(),
            Command::ResumeProgram => self.execute_resume_program(),
            Command::StopProgram { reset } => self.execute_stop_program(reset),
            Command::AcknowledgeToolChange => self.execute_acknowledge_tool_change(),
            Command::SetOptionalStop(enabled) => self.execute_set_optional_stop(enabled),
            Command::SetSingleBlock(enabled) => self.execute_set_single_block(enabled),
            Command::SetBlockDelete(enabled) => self.execute_set_block_delete(enabled),
            Command::SetBreakpoint { line, enabled } => self.job.set_breakpoint(line, enabled),
            Command::StartCheckRun => Err(EngineError::Unsupported(
                "controller check mode ($C) is not wired end to end".into(),
            )),
        }
    }

    fn execute_realtime(&mut self, command: RealtimeCommand) -> Result<(), EngineError> {
        self.ensure_link()?;
        if self.mpg_active && !is_safe_realtime(command) {
            return Err(EngineError::PendantActive);
        }

        match command {
            RealtimeCommand::SoftReset => self.soft_reset(),
            RealtimeCommand::Stop => {
                self.abort_job(JobAbortReason::OperatorStop, false, true);
                self.abort_pending_lines("stop");
                self.send_realtime_command(command, true)
            }
            RealtimeCommand::SoftEStop => {
                self.abort_job(JobAbortReason::EmergencyStop, false, true);
                self.abort_pending_lines("soft E-stop");
                self.send_realtime_command(command, true)?;
                self.apply_local_alarm(AlarmCode::ESTOP.code());
                Ok(())
            }
            RealtimeCommand::StatusReport => {
                if self.sleeping {
                    self.wake_probe_pending = true;
                }
                self.send_status_request(false, true)
            }
            RealtimeCommand::FullStatusReport => {
                if self.sleeping {
                    self.wake_probe_pending = true;
                }
                self.send_status_request(true, true)
            }
            RealtimeCommand::Reboot => {
                self.ensure_ready()?;
                self.ensure_idle()?;
                self.send_realtime_command(command, true)
            }
            RealtimeCommand::Macro(_) => {
                self.ensure_ready()?;
                self.ensure_idle()?;
                self.send_realtime_command(command, true)
            }
            RealtimeCommand::CycleStart => {
                self.ensure_ready()?;
                self.ensure_motion_unlocked()?;
                if self.job.is_tool_change() && self.job.tool_change_acknowledged() {
                    self.ensure_resume_allowed()?;
                    self.job.finish_tool_change(Instant::now())?;
                    self.watchdog_paused = false;
                    self.streamer.resume();
                    self.send_realtime_command(command, true)?;
                    self.sync_job_snapshot();
                    self.emit(EngineEvent::JobResumed);
                    self.pump_job();
                    return Ok(());
                }
                if self.job.is_paused() && !self.job.is_tool_change() {
                    self.ensure_resume_allowed()?;
                    self.job.resume(Instant::now())?;
                    self.watchdog_paused = false;
                    self.streamer.resume();
                    self.send_realtime_command(command, true)?;
                    self.sync_job_snapshot();
                    self.emit(EngineEvent::JobResumed);
                    self.pump_job();
                    return Ok(());
                }
                self.watchdog_paused = false;
                self.streamer.resume();
                self.send_realtime_command(command, true)
            }
            RealtimeCommand::ToolChangeAck => {
                self.ensure_ready()?;
                self.send_realtime_command(command, true)?;
                if self.job.is_tool_change() {
                    self.job.acknowledge_tool_change();
                    self.log_info("Tool change acknowledged; waiting for cycle start");
                }
                Ok(())
            }
            RealtimeCommand::FeedHold => {
                self.ensure_ready()?;
                let running = self.job.is_running();
                self.send_realtime_command(command, true)?;
                if running {
                    self.pause_job(JobPauseReason::Operator);
                }
                Ok(())
            }
            _ => {
                self.ensure_ready()?;
                self.send_realtime_command(command, true)
            }
        }
    }

    fn execute_system(&mut self, command: SystemCommand) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        self.ensure_job_stream_available()?;

        match &command {
            SystemCommand::WriteSetting { .. } => {
                return Err(EngineError::InvalidCommand(
                    "use Command::SetSetting so the write is read back and verified".into(),
                ));
            }
            SystemCommand::HomeAll | SystemCommand::HomeAxis(_) => {
                self.ensure_home_allowed()?;
            }
            SystemCommand::Unlock => {}
            SystemCommand::Sleep | SystemCommand::CheckMode => self.ensure_idle()?,
            SystemCommand::SetBuildInfo(_)
            | SystemCommand::WriteStartupLine { .. }
            | SystemCommand::Reset(_)
            | SystemCommand::ToolLengthReference
            | SystemCommand::ProbeToolPlate => self.ensure_idle()?,
            SystemCommand::Extended(ExtendedCommand::Reboot | ExtendedCommand::Downgrade) => {
                self.ensure_idle()?;
            }
            _ => {}
        }

        if matches!(
            command,
            SystemCommand::Reset(_) | SystemCommand::Extended(ExtendedCommand::Reboot)
        ) {
            self.log_warn("Destructive controller command requested");
        }

        let is_dump = matches!(
            command,
            SystemCommand::Settings | SystemCommand::SettingsExtended
        );
        let is_sleep = matches!(command, SystemCommand::Sleep);
        let is_offsets_dump = matches!(command, SystemCommand::Offsets);
        let is_pins_dump = matches!(command, SystemCommand::Extended(ExtendedCommand::Pins));
        let line = command.encode()?;
        if is_dump {
            self.settings_dump_pending = true;
        }
        match self.enqueue_line(&line, LinePurpose::System) {
            Ok(()) => {
                if is_sleep {
                    self.sleep_on_ack = true;
                }
                if is_offsets_dump {
                    // A manual `$#` refresh replaces the controller table.
                    self.clear_controller_tools();
                }
                if is_pins_dump {
                    // Each `$PINS` is a complete controller report. Drop the
                    // previous map so entries the controller no longer sends do
                    // not survive a refresh.
                    self.clear_pin_map();
                }
                if matches!(
                    &command,
                    SystemCommand::Reset(ResetTarget::Offsets | ResetTarget::All)
                ) {
                    // `$RST=#` and `$RST=*` wipe the stored probe result with
                    // the offsets; drop the cached copy as soon as the command
                    // is accepted.
                    self.invalidate_probe_result();
                    // The controller's stored tool table belongs to the same
                    // non-volatile offset data; do not keep showing it after a
                    // reset. A later `$#` repopulates it from the controller.
                    self.clear_controller_tools();
                }
                Ok(())
            }
            Err(error) => {
                self.settings_dump_pending = false;
                self.sleep_on_ack = false;
                Err(error)
            }
        }
    }

    fn execute_jog(&mut self, jog: JogCommand) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        self.ensure_job_stream_available()?;
        self.ensure_jog_allowed()?;
        if self.job.is_running() {
            return Err(EngineError::JobState {
                action: "jog",
                state: self.job.job_state(),
            });
        }
        let line = jog.encode()?;
        self.enqueue_line(&line, LinePurpose::Jog)
    }

    fn execute_mdi(&mut self, text: &str) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        self.ensure_job_stream_available()?;
        self.ensure_mdi_allowed()?;
        if self.job.is_active() {
            return Err(EngineError::JobState {
                action: "run MDI",
                state: self.job.job_state(),
            });
        }
        if !self.streamer.is_idle() {
            return Err(EngineError::Busy);
        }

        let mut lines = Vec::new();
        let mut total_bytes = 0usize;
        for raw in text.split(['\n', '\r']) {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('$') {
                return Err(EngineError::InvalidCommand(
                    "MDI cannot contain '$' system commands".into(),
                ));
            }
            validate_line(line)?;
            let rx_buffer = self.streamer.config().rx_buffer_size;
            if line.len() + 1 > rx_buffer {
                return Err(EngineError::Streamer(StreamerError::LineTooLong {
                    len: line.len(),
                    limit: rx_buffer,
                }));
            }
            let max_line_len = self.streamer.config().max_line_len;
            if line.len() > max_line_len {
                return Err(EngineError::Streamer(StreamerError::ExceedsMaxLineLen {
                    len: line.len(),
                    limit: max_line_len,
                }));
            }
            total_bytes = total_bytes.saturating_add(line.len());
            if lines.len() >= MAX_MDI_LINES || total_bytes > MAX_MDI_BYTES {
                return Err(EngineError::InvalidCommand(
                    "MDI text exceeds the engine limit".into(),
                ));
            }
            lines.push(line.to_string());
        }
        if lines.is_empty() {
            return Err(EngineError::InvalidCommand("MDI text is empty".into()));
        }

        let mut lines = lines.into_iter();
        let Some(first) = lines.next() else {
            return Err(EngineError::InvalidCommand("MDI text is empty".into()));
        };
        self.mdi_queue = lines.collect();
        if let Err(error) = self.enqueue_line(&first, LinePurpose::MdiLine) {
            self.mdi_queue.clear();
            return Err(error);
        }
        Ok(())
    }

    fn send_next_mdi_line(&mut self) {
        let Some(line) = self.mdi_queue.pop_front() else {
            return;
        };
        let state = self.snapshot.state;
        if self.mpg_active
            || !matches!(
                state,
                MachineState::Idle | MachineState::Run | MachineState::Hold | MachineState::Jog
            )
        {
            let dropped = self.mdi_queue.len() + 1;
            self.mdi_queue.clear();
            self.log_warn(format!(
                "Stopped {dropped} pending MDI lines because the machine is in {state} state"
            ));
            return;
        }
        if let Err(error) = self.enqueue_line(&line, LinePurpose::MdiLine) {
            self.mdi_queue.clear();
            self.log_error(format!("MDI stopped: {error}"));
        }
    }

    fn execute_feed_override(&mut self, percent: u16) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        if !(10..=200).contains(&percent) {
            return Err(EngineError::InvalidCommand(
                "feed override must be between 10 and 200 percent".into(),
            ));
        }
        let previous = self.snapshot.overrides.feed;
        let commands = feed_override_commands(previous, percent);
        for command in commands {
            self.send_realtime_command(command, false)?;
        }
        if previous != percent {
            self.log_command(format!("feed override {previous}% -> {percent}%"));
        }
        let mut next = (*self.snapshot).clone();
        next.overrides.feed = percent;
        self.snapshot = Arc::new(next);
        self.publish_snapshot();
        Ok(())
    }

    fn execute_spindle_override(&mut self, percent: u16) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        if !(10..=200).contains(&percent) {
            return Err(EngineError::InvalidCommand(
                "spindle override must be between 10 and 200 percent".into(),
            ));
        }
        let previous = self.snapshot.overrides.spindle;
        let commands = spindle_override_commands(previous, percent);
        for command in commands {
            self.send_realtime_command(command, false)?;
        }
        if previous != percent {
            self.log_command(format!("spindle override {previous}% -> {percent}%"));
        }
        let mut next = (*self.snapshot).clone();
        next.overrides.spindle = percent;
        self.snapshot = Arc::new(next);
        self.publish_snapshot();
        Ok(())
    }

    fn execute_rapid_override(&mut self, percent: u16) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        let Some(command) = rapid_override_command(percent) else {
            return Err(EngineError::InvalidCommand(
                "rapid override must be 100, 50, 25 or 5 percent".into(),
            ));
        };
        let previous = self.snapshot.overrides.rapid;
        self.send_realtime_command(command, false)?;
        if previous != percent {
            self.log_command(format!("rapid override {previous}% -> {percent}%"));
        }
        let mut next = (*self.snapshot).clone();
        next.overrides.rapid = percent;
        self.snapshot = Arc::new(next);
        self.publish_snapshot();
        Ok(())
    }

    fn execute_set_setting(&mut self, number: u16, value: SettingValue) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        self.ensure_job_stream_available()?;
        self.ensure_idle()?;
        if self.pending_write.is_some() || !self.streamer.is_idle() {
            return Err(EngineError::Busy);
        }

        let command = SystemCommand::WriteSetting {
            number,
            value: value.clone(),
        };
        let line = command.encode()?;
        self.pending_write = Some(PendingWrite {
            number,
            expected: value,
            stage: WriteStage::AwaitAck,
        });
        if let Err(error) = self.enqueue_line(&line, LinePurpose::SettingWrite) {
            self.pending_write = None;
            return Err(error);
        }
        Ok(())
    }

    fn begin_readback(&mut self) {
        let Some(number) = (|| {
            let pending = self.pending_write.as_mut()?;
            if pending.stage != WriteStage::AwaitAck {
                return None;
            }
            pending.stage = WriteStage::AwaitRead;
            Some(pending.number)
        })() else {
            return;
        };

        let line = match SystemCommand::ReadSetting(number).encode() {
            Ok(line) => line,
            Err(error) => {
                self.fail_pending_read(&format!("could not encode the read-back: {error}"));
                return;
            }
        };
        if let Err(error) = self.enqueue_line(&line, LinePurpose::SettingRead) {
            self.fail_pending_read(&format!("could not queue the read-back: {error}"));
        }
    }

    fn fail_pending_read(&mut self, reason: &str) {
        let Some(pending) = self.pending_write.take() else {
            return;
        };
        self.log_error(format!(
            "Setting {} read-back failed: {reason}",
            pending.number
        ));
        self.emit(EngineEvent::SettingWritten {
            number: pending.number,
            value: pending.expected,
            verified: false,
        });
    }

    fn execute_clear_error(&mut self) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        self.ensure_job_stream_available()?;
        if !self.streamer.is_idle() {
            return Err(EngineError::Busy);
        }
        self.error_latched = true;
        self.send_clear_line()
    }

    // ------------------------------------------------------------- job control

    fn execute_load_program(&mut self, name: String, source: &str) -> Result<(), EngineError> {
        let program = self.job.load(name.clone(), source)?;
        let total_lines = self.job.total_lines();
        self.sync_job_snapshot();
        self.emit(EngineEvent::JobLoaded {
            name,
            total_lines,
            program,
        });
        Ok(())
    }

    fn execute_start_program(&mut self, from_line: Option<u32>) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        self.ensure_motion_unlocked()?;
        if !self.job.has_program() {
            return Err(EngineError::NoProgram);
        }
        if !self.streamer.is_idle() || self.current_purpose.is_some() {
            return Err(EngineError::Busy);
        }
        let state = self.snapshot.state;
        if !state.can_start_job() {
            return Err(EngineError::NotIdle { state });
        }
        if let Some(line) = from_line {
            if line == 0 {
                return Err(EngineError::InvalidCommand(
                    "start line must be 1 or greater".into(),
                ));
            }
            if line > 1 {
                if line > self.job.total_lines() {
                    return Err(EngineError::InvalidCommand(format!(
                        "start line {line} is outside the program"
                    )));
                }
                return Err(EngineError::Unsupported(
                    "start-from-line needs a modal preamble; only the first line is supported"
                        .into(),
                ));
            }
        }

        let now = Instant::now();
        let from = self.job.start(now)?;
        self.streamer.resume();
        self.sync_job_snapshot();
        self.emit(EngineEvent::JobStarted {
            from_line: from,
            total_lines: self.job.total_lines(),
        });
        self.emit_job_progress(now);
        self.pump_job();
        Ok(())
    }

    fn execute_pause_program(&mut self) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        if !self.job.is_running() {
            return Err(EngineError::JobState {
                action: "pause",
                state: self.job.job_state(),
            });
        }
        self.execute_realtime(RealtimeCommand::FeedHold)
    }

    fn execute_resume_program(&mut self) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        if self.job.is_tool_change() {
            self.ensure_resume_allowed()?;
            if !self.job.tool_change_acknowledged() {
                return Err(EngineError::JobState {
                    action: "continue after a tool change",
                    state: self.job.job_state(),
                });
            }
            let now = Instant::now();
            self.job.finish_tool_change(now)?;
            self.watchdog_paused = false;
            self.streamer.resume();
            self.send_realtime_command(RealtimeCommand::CycleStart, true)?;
            self.sync_job_snapshot();
            self.emit(EngineEvent::JobResumed);
            self.pump_job();
            return Ok(());
        }
        if self.job.is_paused() {
            return self.execute_realtime(RealtimeCommand::CycleStart);
        }
        Err(EngineError::JobState {
            action: "resume",
            state: self.job.job_state(),
        })
    }

    fn execute_stop_program(&mut self, reset: bool) -> Result<(), EngineError> {
        if !self.job.can_stop() {
            return Err(EngineError::JobState {
                action: "stop",
                state: self.job.job_state(),
            });
        }
        let now = Instant::now();
        let state = self.snapshot.state;
        let active = self.job.is_active();
        if active && self.transport.is_some() && !reset {
            if state == MachineState::Tool {
                self.send_realtime_command(RealtimeCommand::Stop, true)?;
                self.streamer.clear();
                self.streamer.resume();
                self.current_purpose = None;
            } else {
                self.send_realtime_command(RealtimeCommand::FeedHold, true)?;
            }
        }
        let line = self
            .job
            .stop(now)
            .unwrap_or_else(|| self.job.current_line().max(1));
        if active {
            if self.streamer.in_flight_lines() == 0 {
                self.streamer.clear();
                self.streamer.resume();
            } else {
                self.streamer.pause();
            }
        }
        self.sync_job_snapshot();
        self.emit_job_progress(now);
        self.emit(EngineEvent::JobAborted {
            line,
            reason: JobAbortReason::OperatorStop,
        });
        if reset && self.transport.is_some() {
            self.execute_realtime(RealtimeCommand::SoftReset)?;
        }
        Ok(())
    }

    fn execute_acknowledge_tool_change(&mut self) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        if !self.job.is_tool_change() {
            return Err(EngineError::JobState {
                action: "acknowledge a tool change",
                state: self.job.job_state(),
            });
        }
        if !self.job.tool_change_acknowledged() {
            self.send_realtime_command(RealtimeCommand::ToolChangeAck, true)?;
            self.job.acknowledge_tool_change();
            self.log_info("Tool change acknowledged; waiting for cycle start");
        }
        Ok(())
    }

    fn execute_set_optional_stop(&mut self, enabled: bool) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        if self.job.optional_stop() != enabled {
            self.send_realtime_command(RealtimeCommand::OptionalStopToggle, true)?;
            self.job.set_optional_stop(enabled);
            self.log_info(format!(
                "Optional stop {}",
                if enabled { "enabled" } else { "disabled" }
            ));
        }
        // The command is accepted: publish the authoritative value even when it
        // was already current, so a UI toggle can always re-read it.
        self.shared.set_optional_stop(enabled);
        Ok(())
    }

    fn execute_set_single_block(&mut self, enabled: bool) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        if self.job.single_block() != enabled {
            self.send_realtime_command(RealtimeCommand::SingleBlockToggle, true)?;
            self.job.set_single_block(enabled);
            self.log_info(format!(
                "Single block {}",
                if enabled { "enabled" } else { "disabled" }
            ));
        }
        self.shared.set_single_block(enabled);
        Ok(())
    }

    fn execute_set_block_delete(&mut self, enabled: bool) -> Result<(), EngineError> {
        self.ensure_ready()?;
        self.ensure_not_pendant()?;
        self.ensure_job_stream_available()?;
        if self.job.is_running() {
            return Err(EngineError::JobState {
                action: "change block delete",
                state: self.job.job_state(),
            });
        }
        if self.job.block_delete() == enabled {
            // Idempotent: a repeated value sends nothing and stays readable,
            // even while the previous `$B` line is still in flight.
            self.shared.set_block_delete(enabled);
            return Ok(());
        }
        if !self.streamer.is_idle() || self.current_purpose.is_some() {
            return Err(EngineError::Busy);
        }
        let line = SystemCommand::ToggleBlockDelete.encode()?;
        self.enqueue_line(&line, LinePurpose::System)?;
        self.job.set_block_delete(enabled);
        self.log_info(format!(
            "Block delete {}",
            if enabled { "enabled" } else { "disabled" }
        ));
        self.shared.set_block_delete(enabled);
        Ok(())
    }

    fn pause_job(&mut self, reason: JobPauseReason) {
        let now = Instant::now();
        if let Some((line, reason)) = self.job.pause(now, reason) {
            self.streamer.pause();
            self.sync_job_snapshot();
            self.emit_job_progress(now);
            self.emit(EngineEvent::JobPaused { line, reason });
        }
    }

    fn begin_tool_change(&mut self, tool: u32, line: u32) {
        let now = Instant::now();
        if let Some(pending) = self.job.begin_tool_change(now, tool, line) {
            // Keep the streamer running so jog and `$` lines can be sent while
            // the controller waits in Tool. The job phase, not the streamer,
            // prevents the next program line from being queued.
            self.streamer.resume();
            self.sync_job_snapshot();
            self.emit_job_progress(now);
            self.emit(EngineEvent::ToolChangeRequired {
                tool: pending.tool,
                line: pending.line,
            });
        }
    }

    fn abort_job(&mut self, reason: JobAbortReason, interrupt: bool, clear_stream: bool) {
        let now = Instant::now();
        if let Some(line) = self.job.abort(now, interrupt) {
            if clear_stream {
                self.streamer.clear();
                self.streamer.resume();
                self.current_purpose = None;
            } else {
                self.streamer.pause();
            }
            self.sync_job_snapshot();
            self.emit_job_progress(now);
            self.emit(EngineEvent::JobAborted { line, reason });
        }
    }

    fn complete_job(&mut self) {
        let now = Instant::now();
        let Some(elapsed) = self.job.complete(now) else {
            return;
        };
        self.streamer.resume();
        self.sync_job_snapshot();
        self.emit_job_progress(now);
        self.emit(EngineEvent::JobCompleted { elapsed });
    }

    fn fail_job_controller(&mut self, line: u32, code: u16) {
        let message = self.error_codes.error_label(code);
        let now = Instant::now();
        self.job.fail_in_flight(now, line);
        self.streamer.clear();
        self.streamer.resume();
        self.current_purpose = None;
        self.sync_job_snapshot();
        self.emit_job_progress(now);
        self.emit(EngineEvent::JobError {
            line,
            code,
            message,
        });
    }

    fn fail_job_stream(&mut self, line: u32, error: EngineError) {
        let now = Instant::now();
        self.job.fail_in_flight(now, line);
        self.streamer.clear();
        self.streamer.resume();
        self.current_purpose = None;
        self.sync_job_snapshot();
        self.emit_job_progress(now);
        self.emit(EngineEvent::JobError {
            line,
            code: 0,
            message: error.to_string(),
        });
        self.emit(EngineEvent::JobAborted {
            line,
            reason: JobAbortReason::ProgramError,
        });
    }

    fn on_job_ack(&mut self) {
        let now = Instant::now();
        let Some(ack) = self.job.acknowledge(now) else {
            self.sync_job_snapshot();
            return;
        };
        self.sync_job_snapshot();
        if ack.progress_due {
            self.emit_job_progress(now);
        }
        if ack.meta.is_tool_change() && !self.job.is_tool_change() {
            self.begin_tool_change(ack.meta.tool, ack.line);
            return;
        }
        if !self.job.is_running() {
            if self.streamer.is_idle()
                && matches!(
                    self.job.job_state(),
                    JobState::Aborted | JobState::Completed | JobState::Interrupted
                )
            {
                self.streamer.resume();
            }
            return;
        }
        if ack.meta.is_program_end() {
            self.complete_job();
            return;
        }
        if ack.meta.is_program_stop() {
            self.pause_job(JobPauseReason::ProgramStop);
            return;
        }
        if ack.meta.is_optional_stop() && self.job.optional_stop() {
            self.pause_job(JobPauseReason::OptionalStop);
            return;
        }
        if self.job.single_block() {
            self.pause_job(JobPauseReason::SingleBlock);
            return;
        }
        self.pump_job();
    }

    fn pump_job(&mut self) {
        if !self.job.is_running()
            || self.mpg_active
            || self.current_purpose.is_some()
            || !self.streamer.is_idle()
        {
            return;
        }
        if !matches!(self.snapshot.state, MachineState::Idle | MachineState::Run) {
            return;
        }
        loop {
            let index = self.job.cursor();
            if index >= self.job.total_lines() as usize {
                self.complete_job();
                return;
            }
            let Some((line_number, has_words, block_delete, source)) =
                self.job.line(index).map(|line| {
                    (
                        u32::try_from(line.line).unwrap_or(u32::MAX),
                        line.has_words(),
                        line.block_delete,
                        line.source.clone(),
                    )
                })
            else {
                self.complete_job();
                return;
            };
            if !has_words {
                self.job.advance_skipped();
                continue;
            }
            if self.job.block_delete() && block_delete {
                self.job.advance_skipped();
                continue;
            }
            if self.job.breakpoint_hit(line_number) {
                self.pause_job(JobPauseReason::Breakpoint);
                return;
            }
            match self.enqueue_line(
                &source,
                LinePurpose::JobLine {
                    index,
                    line: line_number,
                },
            ) {
                Ok(()) => {
                    self.job.mark_sent(index, line_number);
                    self.sync_job_snapshot();
                    return;
                }
                Err(error) => {
                    self.fail_job_stream(line_number, error);
                    return;
                }
            }
        }
    }

    fn sync_job_snapshot(&mut self) {
        let mut next = (*self.snapshot).clone();
        self.job.apply_info(&mut next.job);
        self.snapshot = Arc::new(next);
        self.publish_snapshot();
    }

    fn emit_job_progress(&mut self, now: Instant) {
        if !self.job.has_program() {
            return;
        }
        self.emit(EngineEvent::JobProgress {
            current_line: self.job.current_line().max(1),
            completed: self.job.completed_lines(),
            total: self.job.total_lines(),
            elapsed: self.job.elapsed(now),
        });
    }

    fn ensure_job_stream_available(&self) -> Result<(), EngineError> {
        if self.job.is_paused() && !self.job.is_tool_change() {
            return Err(EngineError::JobState {
                action: "send a line",
                state: self.job.job_state(),
            });
        }
        Ok(())
    }

    fn ensure_resume_allowed(&self) -> Result<(), EngineError> {
        let state = self.snapshot.state;
        match state {
            MachineState::Idle | MachineState::Hold | MachineState::Tool => Ok(()),
            _ if state.is_locked() => Err(EngineError::MotionBlocked { state }),
            _ => Err(EngineError::NotIdle { state }),
        }
    }

    fn soft_reset(&mut self) -> Result<(), EngineError> {
        self.abort_job(JobAbortReason::SoftReset, false, true);
        self.abort_pending_lines("soft reset");
        self.parser.reset();
        self.wake_probe_pending = false;
        self.error_latched = false;
        self.watchdog_paused = false;
        self.write_bytes(&[0x18])?;
        self.log_command("0x18 soft reset");
        // grblHAL clears its runtime `sys` block on a soft reset, so the
        // virtual switches return to false. A physical switch is still
        // reported by the controller in the status `Pn` field.
        self.job.reset_toggles();
        self.shared.reset_toggles();
        // The controller drops its stored probe result on a soft reset; the
        // cached copy must not survive and show a stale `PRB`.
        self.invalidate_probe_result();
        // Pin levels, limits and last events are instantaneous snapshots from
        // before the reset. The `$PINS` map is controller configuration and is
        // kept until the next session.
        self.clear_live_pin_reports();
        let now = Instant::now();
        self.next_poll_at = deadline(now, self.config.status_poll_idle);
        if self.sleeping {
            self.wake_from_sleep(now);
        }
        Ok(())
    }

    fn apply_local_alarm(&mut self, code: u16) {
        let alarm = AlarmCode::new(code);
        let mut next = (*self.snapshot).clone();
        next.state = MachineState::Alarm;
        next.alarm = Some(alarm);
        self.snapshot = Arc::new(next);
        self.publish_snapshot();
        if self.last_alarm_emitted != Some(code) {
            self.last_alarm_emitted = Some(code);
            self.emit(EngineEvent::Alarm(code));
        }
        self.log_error(self.alarm_codes.alarm_label(code));
    }

    fn enter_sleep(&mut self) {
        self.abort_job(JobAbortReason::ControllerSleep, false, true);
        self.sleeping = true;
        self.sleep_on_ack = false;
        self.wake_probe_pending = false;
        self.streamer.clear();
        self.streamer.resume();
        self.current_purpose = None;
        self.parser.reset();
        self.mdi_queue.clear();
        self.pending_write = None;
        self.settings_dump_pending = false;
        self.error_latched = false;
        self.shared.ready.store(false, Ordering::Release);
        let mut next = (*self.snapshot).clone();
        next.state = MachineState::Sleep;
        self.snapshot = Arc::new(next);
        self.publish_snapshot();
        self.log_info("Controller entered sleep; polling suspended until reset or status request");
    }

    fn wake_from_sleep(&mut self, now: Instant) {
        if !self.sleeping {
            return;
        }
        self.sleeping = false;
        self.wake_probe_pending = false;
        self.shared.ready.store(true, Ordering::Release);
        self.last_status_at = Some(now);
        self.next_poll_at = now;
        self.log_info("Controller woke from sleep; status polling resumed");
    }

    fn abort_pending_lines(&mut self, reason: &str) {
        let dropped = self.streamer.clear();
        self.streamer.resume();
        self.current_purpose = None;
        self.mdi_queue.clear();
        self.pending_write = None;
        self.advance_after_clear = false;
        self.settings_dump_pending = false;
        self.error_latched = false;
        self.sleep_on_ack = false;
        if dropped > 0 {
            tracing::debug!(dropped, reason, "dropped queued lines");
        }
    }

    // --------------------------------------------------------------- gateway

    fn ensure_link(&self) -> Result<(), EngineError> {
        if self.transport.is_some() {
            Ok(())
        } else {
            Err(EngineError::NotConnected)
        }
    }

    fn ensure_ready(&self) -> Result<(), EngineError> {
        self.ensure_link()?;
        if self.phase == Phase::Ready && !self.sleeping {
            Ok(())
        } else {
            Err(EngineError::NotReady)
        }
    }

    fn ensure_not_pendant(&self) -> Result<(), EngineError> {
        if self.mpg_active {
            Err(EngineError::PendantActive)
        } else {
            Ok(())
        }
    }

    fn ensure_motion_unlocked(&self) -> Result<(), EngineError> {
        let state = self.snapshot.state;
        if state.is_locked() {
            Err(EngineError::MotionBlocked { state })
        } else {
            Ok(())
        }
    }

    fn ensure_jog_allowed(&self) -> Result<(), EngineError> {
        let state = self.snapshot.state;
        match state {
            MachineState::Idle | MachineState::Hold | MachineState::Jog | MachineState::Tool => {
                Ok(())
            }
            _ => Err(EngineError::MotionBlocked { state }),
        }
    }

    fn ensure_mdi_allowed(&self) -> Result<(), EngineError> {
        let state = self.snapshot.state;
        if state.is_locked() {
            return Err(EngineError::MotionBlocked { state });
        }
        if state != MachineState::Idle {
            return Err(EngineError::NotIdle { state });
        }
        Ok(())
    }

    fn ensure_home_allowed(&self) -> Result<(), EngineError> {
        let state = self.snapshot.state;
        match state {
            MachineState::Idle | MachineState::Alarm | MachineState::Door => Ok(()),
            _ => Err(EngineError::MotionBlocked { state }),
        }
    }

    fn ensure_idle(&self) -> Result<(), EngineError> {
        let state = self.snapshot.state;
        if state == MachineState::Idle {
            Ok(())
        } else {
            Err(EngineError::NotIdle { state })
        }
    }

    // ---------------------------------------------------------------- output

    fn send_status_request(&mut self, full: bool, log: bool) -> Result<(), EngineError> {
        let command = if full {
            RealtimeCommand::FullStatusReport
        } else {
            RealtimeCommand::StatusReport
        };
        self.send_realtime_command(command, log)
    }

    fn send_realtime_command(
        &mut self,
        command: RealtimeCommand,
        log: bool,
    ) -> Result<(), EngineError> {
        self.ensure_link()?;
        let bytes = command.bytes(self.rt_plus);
        self.write_bytes(&bytes)?;
        if log {
            let byte = bytes.last().copied().unwrap_or(0);
            self.log_command(format!("0x{byte:02X} {}", command.name()));
        }
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), EngineError> {
        let result = match self.transport.as_mut() {
            Some(transport) => transport.send(bytes),
            None => return Err(EngineError::NotConnected),
        };
        match result {
            Ok(()) => Ok(()),
            Err(error) => {
                let reason = format!("transport send failed: {error}");
                self.close_link(&reason);
                Err(EngineError::Transport(error))
            }
        }
    }

    fn enqueue_line(&mut self, text: &str, purpose: LinePurpose) -> Result<(), EngineError> {
        self.ensure_link()?;
        if !self.streamer.is_idle() {
            return Err(EngineError::Busy);
        }
        self.streamer.enqueue(text)?;
        self.current_purpose = Some(purpose);
        if let Err(error) = self.flush_streamer() {
            self.current_purpose = None;
            return Err(error);
        }
        Ok(())
    }

    fn flush_streamer(&mut self) -> Result<(), EngineError> {
        let bytes = self.streamer.poll_output();
        if bytes.is_empty() {
            return Ok(());
        }
        self.write_bytes(&bytes)?;
        for line in bytes.split(|byte| *byte == b'\n') {
            if line.is_empty() {
                continue;
            }
            let text = String::from_utf8_lossy(line);
            self.log_command(text.as_ref());
        }
        Ok(())
    }

    fn send_clear_line(&mut self) -> Result<(), EngineError> {
        if !self.streamer.is_idle() {
            return Err(EngineError::Busy);
        }
        self.streamer.enqueue("")?;
        self.current_purpose = Some(LinePurpose::ClearError);
        if let Err(error) = self.flush_streamer() {
            self.current_purpose = None;
            return Err(error);
        }
        self.log_command("<blank>");
        Ok(())
    }

    // -------------------------------------------------------------- responses

    fn handle_response(&mut self, response: Response) {
        match response {
            Response::Status(report) => self.on_status(report),
            Response::Ok => self.on_ok(),
            Response::Error(code) => self.on_error(code),
            Response::Alarm(code) => self.on_alarm_line(code),
            Response::Message(text) => {
                let level = message_level(&text);
                self.log(level, format!("[MSG:{text}]"));
            }
            Response::Banner(text) => {
                self.log_info(text.clone());
                self.emit(EngineEvent::Banner(bounded_text(&text)));
            }
            Response::Feedback(text) => self.on_feedback(&text),
            Response::Setting { number, value } => self.on_setting(number, value),
            Response::SettingDescription(description) => {
                self.setting_descriptions
                    .insert(description.id, description);
                if self.phase != Phase::Discovering {
                    self.publish_settings(false);
                }
            }
            Response::SettingGroup(group) => {
                self.setting_groups.insert(group.id, group);
                if self.phase != Phase::Discovering {
                    self.publish_settings(false);
                }
            }
            Response::AlarmDescription(description) => {
                self.alarm_codes.insert(description);
                if self.phase != Phase::Discovering {
                    self.publish_code_tables_only();
                }
            }
            Response::ErrorDescription(description) => {
                self.error_codes.insert(description);
                if self.phase != Phase::Discovering {
                    self.publish_code_tables_only();
                }
            }
            Response::Offset(offset) => {
                self.log_info(format!("$# {offset}"));
                self.offsets.apply(offset);
                self.publish_offsets();
            }
            Response::ParserState(state) => {
                self.log_info(format!("$G {state}"));
                self.parser_state = Some(state);
                self.publish_parser_state();
            }
            Response::Probe(result) => self.on_probe(result),
            Response::ToolEntry(entry) => self.on_controller_tool(entry),
            Response::PinMapping(mapping) => self.on_pin_mapping(mapping),
            Response::PinState(state) => self.on_pin_state(state),
            Response::LimitState(state) => self.on_limit_state(state),
            Response::LastEvents(events) => self.on_last_events(events),
            Response::Help(text) => self.log_info(format!("[HLP:{text}]")),
            Response::Echo(text) => self.log_info(format!("[echo:{text}]")),
            Response::StartupLine(line) => self.on_startup_line(line),
            Response::StartupLineResult(result) => match result.result {
                Ok(()) => self.log_ok(format!("Startup line {}: ok", result.line)),
                Err(code) => {
                    let label = self.error_codes.error_label(code);
                    self.log_error(format!("Startup line {}: {label}", result.line));
                    self.emit(EngineEvent::Error(code));
                }
            },
            Response::Unknown(text) => self.on_unknown(&text),
        }
    }

    fn on_status(&mut self, report: StatusReport) {
        let now = Instant::now();
        if self.sleeping {
            if !self.wake_probe_pending {
                tracing::debug!("ignoring a status report while the controller sleeps");
                return;
            }
            self.wake_probe_pending = false;
            if report.state == MachineState::Sleep {
                // The controller answered the probe and is still asleep. Stay
                // passive instead of starting a polling loop that would trip
                // the watchdog.
                return;
            }
            self.wake_from_sleep(now);
        }
        let report = Arc::new(report);
        let previous_state = self.snapshot.state;
        let mut next = apply_status(&self.snapshot, &report);
        self.job.apply_info(&mut next.job);
        let state_changed = next.state != previous_state;

        self.snapshot = Arc::new(next);
        self.last_status = Some(Arc::clone(&report));
        self.last_status_at = Some(now);
        self.publish_snapshot();

        if let Some(buffer) = report.buffer {
            self.streamer.on_bf(buffer.planner_free, buffer.rx_free);
        }

        if let Some(alarm) = report.alarm {
            if self.last_alarm_emitted != Some(alarm.code()) {
                self.last_alarm_emitted = Some(alarm.code());
                self.emit(EngineEvent::Alarm(alarm.code()));
                self.log_error(self.alarm_codes.alarm_label(alarm.code()));
            }
        } else if report.state != MachineState::Alarm {
            self.last_alarm_emitted = None;
        }

        if state_changed
            && self.snapshot.state == MachineState::Alarm
            && previous_state != MachineState::Alarm
        {
            self.abort_pending_lines("alarm");
        }

        if state_changed {
            self.log_info(format!("State: {previous_state} -> {}", report.state));
            if matches!(
                self.snapshot.state,
                MachineState::Door | MachineState::Tool | MachineState::Home
            ) {
                let dropped = self.mdi_queue.len();
                self.mdi_queue.clear();
                if dropped > 0 {
                    self.log_warn(format!(
                        "Dropped {dropped} pending MDI lines after the machine entered {} state",
                        self.snapshot.state
                    ));
                }
            }
        }

        let machine_state = self.snapshot.state;
        if machine_state == MachineState::Alarm {
            self.abort_job(JobAbortReason::Alarm, true, true);
        } else if machine_state == MachineState::Door {
            self.abort_job(JobAbortReason::Door, false, true);
        } else if machine_state == MachineState::Tool {
            if self.job.is_tool_change() {
                self.job.note_tool_state();
            } else if let Some((tool, line)) = self.job.in_flight_tool_change() {
                self.begin_tool_change(tool, line);
                self.job.note_tool_state();
            } else if self.job.is_running() {
                let tool = report.tool.unwrap_or_else(|| self.job.active_tool());
                let line = self.job.current_line().max(1);
                self.begin_tool_change(tool, line);
                self.job.note_tool_state();
            }
        } else if machine_state == MachineState::Hold
            && self.job.is_running()
            && !self.job.in_flight_pauses_job()
        {
            let reason = if self.job.single_block() {
                JobPauseReason::SingleBlock
            } else {
                JobPauseReason::FeedHold
            };
            self.pause_job(reason);
        }

        if self.snapshot.state == MachineState::Sleep
            && !self.sleeping
            && self.phase == Phase::Ready
        {
            self.enter_sleep();
        }

        let previous_mpg = self.mpg_active;
        if let Some(mpg) = report.mpg {
            self.mpg_active = mpg;
        }
        if self.mpg_active != previous_mpg {
            if self.mpg_active {
                let dropped = self.mdi_queue.len();
                self.mdi_queue.clear();
                self.pending_write = None;
                self.advance_after_clear = false;
                self.settings_dump_pending = false;
                self.sleep_on_ack = false;
                if dropped > 0 {
                    self.log_warn(format!(
                        "Dropped {dropped} pending MDI lines after the pendant took control"
                    ));
                }
                self.log_warn("MPG/pendant took control; motion commands are blocked");
                if self.job.is_running() {
                    self.pause_job(JobPauseReason::FeedHold);
                }
            } else {
                self.log_info("MPG/pendant released control");
            }
        }

        self.update_phase_after_status(now);
        if self.phase == Phase::Ready && !self.sleeping {
            if self.next_poll_at > deadline(now, self.poll_interval()) {
                self.next_poll_at = deadline(now, self.poll_interval());
            }
            self.pump_job();
        }
    }

    fn update_phase_after_status(&mut self, now: Instant) {
        if self.mpg_active {
            match self.phase {
                Phase::StatusProbe { .. } => {
                    self.phase = Phase::MpgPassive {
                        next_poll_at: deadline(now, self.config.status_poll_idle),
                    };
                    self.log_warn("Pendant active; waiting for release before discovery");
                }
                Phase::Discovering => {
                    self.streamer.clear();
                    self.streamer.resume();
                    self.current_purpose = None;
                    self.discovery = None;
                    self.clear_controller_tools();
                    self.phase = Phase::MpgPassive {
                        next_poll_at: deadline(now, self.config.status_poll_idle),
                    };
                    self.log_warn("Pendant took control during discovery; pausing");
                }
                _ => {}
            }
        } else {
            match self.phase {
                Phase::StatusProbe { .. } => {
                    self.log_info("Controller responded; discovering capabilities");
                    self.start_discovery(now);
                }
                Phase::MpgPassive { .. } => {
                    self.log_info("Pendant released; discovering capabilities");
                    self.start_discovery(now);
                }
                _ => {}
            }
        }
    }

    fn on_ok(&mut self) {
        if self.streamer.on_ok().is_none() {
            tracing::debug!("ignoring unsolicited ok");
            return;
        }
        self.log_ok("ok");

        let purpose = self.current_purpose.take();
        match purpose {
            Some(LinePurpose::Discovery(step)) => {
                self.advance_after_clear = false;
                self.advance_discovery(step, Instant::now());
            }
            Some(LinePurpose::SettingWrite) => self.begin_readback(),
            Some(LinePurpose::SettingRead) => {
                if self.pending_write.is_some() {
                    self.fail_pending_read("the controller acknowledged the read without a value");
                }
            }
            Some(LinePurpose::MdiLine) => self.send_next_mdi_line(),
            Some(LinePurpose::JobLine { .. }) => self.on_job_ack(),
            Some(LinePurpose::ClearError) => {
                self.error_latched = false;
                if self.advance_after_clear {
                    self.advance_after_clear = false;
                    if let Some(discovery) = &self.discovery {
                        let step = discovery.current;
                        self.advance_discovery(step, Instant::now());
                    }
                }
            }
            Some(LinePurpose::System) => {
                if self.settings_dump_pending {
                    self.settings_dump_pending = false;
                    self.publish_settings(true);
                }
                if self.sleep_on_ack {
                    self.sleep_on_ack = false;
                    self.enter_sleep();
                }
            }
            Some(LinePurpose::Jog) | None => {}
        }
    }

    fn on_error(&mut self, code: u16) {
        let acknowledged = self.streamer.on_error();
        let purpose = if acknowledged.is_some() {
            self.current_purpose.take()
        } else {
            if self.current_purpose.is_some() {
                self.current_purpose = None;
                self.streamer.clear();
                self.streamer.resume();
            }
            None
        };

        let label = self.error_codes.error_label(code);
        self.log_error(label);
        self.emit(EngineEvent::Error(code));
        self.error_latched = true;

        match purpose {
            Some(LinePurpose::Discovery(step)) => {
                if step == DiscoveryStep::Offsets {
                    // The controller rejected `$#`; any entries already parsed
                    // belong to an incomplete report.
                    self.clear_controller_tools();
                }
                self.advance_after_clear = true;
            }
            Some(LinePurpose::MdiLine) => {
                let dropped = self.mdi_queue.len();
                self.mdi_queue.clear();
                if dropped > 0 {
                    self.log_warn(format!(
                        "Dropped {dropped} pending MDI lines after the error"
                    ));
                }
            }
            Some(LinePurpose::SettingWrite) => {
                self.pending_write = None;
            }
            Some(LinePurpose::SettingRead) => {
                self.fail_pending_read("the controller rejected the read-back");
            }
            Some(LinePurpose::ClearError) => {
                self.log_error("The controller rejected the error-clear line");
                return;
            }
            Some(LinePurpose::System) => {
                self.settings_dump_pending = false;
                self.sleep_on_ack = false;
            }
            Some(LinePurpose::JobLine { line, .. }) => {
                self.fail_job_controller(line, code);
            }
            Some(LinePurpose::Jog) | None => {
                if self.job.is_running() {
                    let line = self.job.current_line().max(1);
                    self.fail_job_controller(line, code);
                }
            }
        }

        if let Err(error) = self.send_clear_line() {
            self.log_warn(format!("Could not send the error-clear line: {error}"));
        }
    }

    fn on_alarm_line(&mut self, code: u16) {
        self.abort_pending_lines("ALARM");
        let alarm = AlarmCode::new(code);
        let mut next = (*self.snapshot).clone();
        next.state = MachineState::Alarm;
        next.alarm = Some(alarm);
        self.job.apply_info(&mut next.job);
        self.snapshot = Arc::new(next);
        self.publish_snapshot();
        self.abort_job(JobAbortReason::Alarm, true, true);
        if self.last_alarm_emitted != Some(code) {
            self.last_alarm_emitted = Some(code);
            self.emit(EngineEvent::Alarm(code));
        }
        self.log_error(self.alarm_codes.alarm_label(code));
    }

    fn on_probe(&mut self, result: ghs_core::ProbeResult) {
        self.offsets.probe = Some(result);
        self.publish_offsets();
        let mut next = (*self.snapshot).clone();
        next.probe = Some(result);
        self.snapshot = Arc::new(next);
        self.publish_snapshot();
        self.emit(EngineEvent::Probe(result));
        self.log_info(format!("Probe: {result}"));
    }

    /// Record one controller-reported `[T:...]` entry.
    ///
    /// The controller table is read-only and separate from the application's
    /// table. A repeated tool number replaces the earlier entry, and new
    /// numbers beyond [`MAX_CONTROLLER_TOOLS`] are ignored so a malformed
    /// response cannot grow memory without bound.
    fn on_controller_tool(&mut self, entry: ToolEntry) {
        self.log_info(format!("$# {entry}"));
        let number = entry.number;
        if self.controller_tools.get(&number) == Some(&entry) {
            return;
        }
        if !self.controller_tools.contains_key(&number)
            && self.controller_tools.len() >= MAX_CONTROLLER_TOOLS
        {
            self.log_warn(format!(
                "Ignoring controller tool T{number}: at most {MAX_CONTROLLER_TOOLS} entries are kept"
            ));
            return;
        }
        self.controller_tools.insert(number, entry);
        self.publish_controller_tools();
    }

    /// Record one controller-reported `$N<n>=<line>` startup line.
    ///
    /// A repeated index replaces the earlier value and the store stays sorted by
    /// index because it is a `BTreeMap`. Indexes above [`MAX_STARTUP_LINES`] are
    /// ignored so a malformed `$N` report cannot grow the cache without bound.
    /// The line is published on every value response, including an unchanged
    /// read-back, so the app can observe that a verification read completed.
    fn on_startup_line(&mut self, line: StartupLine) {
        self.log_info(format!("$N{}={}", line.index, line.line));
        if usize::from(line.index) >= MAX_STARTUP_LINES {
            self.log_warn(format!(
                "Ignoring startup line $N{}: at most {MAX_STARTUP_LINES} entries are kept",
                line.index
            ));
            return;
        }
        self.startup_lines.insert(line.index, line);
        self.publish_startup_lines();
    }

    /// Record one controller-reported `[PIN:...]` mapping entry.
    ///
    /// The map is replaced when a `$PINS` request is accepted, so entries are
    /// accumulated for the current report only. A repeated id replaces the
    /// earlier entry and ids beyond [`MAX_PIN_MAPPINGS`] are ignored, keeping a
    /// malformed report from growing the cache without bound.
    fn on_pin_mapping(&mut self, mapping: PinMapping) {
        let direction = mapping
            .direction
            .map_or("unknown", |direction| direction.as_str());
        self.log_info(format!(
            "$PINS {} {} ({direction})",
            mapping.id, mapping.name
        ));
        let id = mapping.id;
        if self.pin_map.get(&id) == Some(&mapping) {
            return;
        }
        if !self.pin_map.contains_key(&id) && self.pin_map.len() >= MAX_PIN_MAPPINGS {
            self.log_warn(format!(
                "Ignoring pin {id}: at most {MAX_PIN_MAPPINGS} entries are kept"
            ));
            return;
        }
        self.pin_map.insert(id, mapping);
        self.publish_pin_map();
    }

    fn on_pin_state(&mut self, state: PinState) {
        self.log_info(format!(
            "$PINSTATE mask {} inputs {} outputs {}",
            state.mask,
            state.inputs.len(),
            state.outputs.len()
        ));
        let state = Arc::new(state);
        self.pin_state = Some(Arc::clone(&state));
        self.shared.set_pin_state(Some(Arc::clone(&state)));
        self.emit(EngineEvent::PinState(state));
    }

    fn on_limit_state(&mut self, state: LimitState) {
        self.log_info(format!("$LIM mask {} axes {}", state.mask, state.axes));
        let state = Arc::new(state);
        self.limit_state = Some(Arc::clone(&state));
        self.shared.set_limit_state(Some(Arc::clone(&state)));
        self.emit(EngineEvent::LimitState(state));
    }

    fn on_last_events(&mut self, events: LastEvents) {
        self.log_info(format!("$LEV {} {}", events.count, events.signals));
        let events = Arc::new(events);
        self.last_events = Some(Arc::clone(&events));
        self.shared.set_limit_events(Some(Arc::clone(&events)));
        self.emit(EngineEvent::LastEvents(events));
    }

    /// Forget the cached `PRB` result after the controller invalidates it.
    ///
    /// A soft reset and `$RST=#`/`$RST=*` clear the controller's stored probe
    /// result. The engine's copy must go too, or the offsets and probe panels
    /// keep showing a `PRB` the controller no longer has. Work offsets, `G92`,
    /// TLO, tool data, settings and code tables stay cached: only the probe
    /// result expires.
    fn invalidate_probe_result(&mut self) {
        let had_result = self.offsets.probe.is_some()
            || self.snapshot.probe.is_some()
            || self
                .last_status
                .as_ref()
                .is_some_and(|status| status.probe.is_some());
        self.offsets.probe = None;
        self.publish_offsets();

        let mut next = (*self.snapshot).clone();
        next.probe = None;
        self.snapshot = Arc::new(next);
        // `last_status` is republished alongside the snapshot; drop the stale
        // field there too so re-reading the pair cannot restore the result.
        if let Some(status) = self.last_status.take() {
            let mut status = (*status).clone();
            status.probe = None;
            self.last_status = Some(Arc::new(status));
        }
        self.publish_snapshot();

        if had_result {
            tracing::debug!("cleared the cached probe result after a controller reset");
        }
    }

    fn on_setting(&mut self, number: u16, value: SettingValue) {
        self.log_info(format!("${number}={value}"));
        let awaiting_read = matches!(
            &self.pending_write,
            Some(pending) if pending.stage == WriteStage::AwaitRead && pending.number == number
        );

        if awaiting_read {
            let Some(pending) = self.pending_write.take() else {
                return;
            };
            self.settings.insert(number, value.clone());
            let verified = pending.expected.to_string() == value.to_string();
            if verified {
                self.log_ok(format!("Setting {number} verified"));
            } else {
                self.log_warn(format!(
                    "Setting {number} read back as {value}, expected {}",
                    pending.expected
                ));
            }
            let list = Arc::new(self.build_settings());
            self.shared.set_settings(Arc::clone(&list));
            self.emit(EngineEvent::SettingWritten {
                number,
                value,
                verified,
            });
            self.emit(EngineEvent::Settings(list));
        } else {
            self.settings.insert(number, value);
            if self.phase != Phase::Discovering {
                self.publish_settings(false);
            }
        }
    }

    fn on_feedback(&mut self, text: &str) {
        self.log_info(text);
        let discovering_info = self.phase == Phase::Discovering
            && self
                .discovery
                .as_ref()
                .is_some_and(|discovery| discovery.current == DiscoveryStep::Info);
        if discovering_info {
            self.info.apply_line(text);
        } else if self.info.apply_line(text) {
            self.shared.set_info(Some(Arc::new(self.info.clone())));
        }
    }

    fn on_unknown(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.log_info(text);
        let discovering_info = self.phase == Phase::Discovering
            && self
                .discovery
                .as_ref()
                .is_some_and(|discovery| discovery.current == DiscoveryStep::Info);
        if discovering_info {
            self.info.apply_line(text);
        } else if self.info.apply_line(text) {
            self.shared.set_info(Some(Arc::new(self.info.clone())));
        }
    }

    // ------------------------------------------------------------- discovery

    fn start_discovery(&mut self, now: Instant) {
        if self.phase == Phase::Discovering {
            return;
        }
        self.streamer.clear();
        self.streamer.resume();
        self.current_purpose = None;
        self.parser.reset();
        self.reset_discovery_state();
        self.phase = Phase::Discovering;
        self.discovery = Some(Discovery::new(now, self.config.discovery_timeout));
        if let Err(error) = self.enqueue_line(
            DiscoveryStep::Info.command(),
            LinePurpose::Discovery(DiscoveryStep::Info),
        ) {
            self.fail_handshake(&format!("could not start discovery: {error}"));
        }
    }

    fn reset_discovery_state(&mut self) {
        self.info = ControllerInfo::default();
        self.settings.clear();
        self.setting_descriptions.clear();
        self.setting_groups.clear();
        self.clear_controller_tools();
        self.alarm_codes = CodeTable::new();
        self.error_codes = CodeTable::new();
        self.offsets = Offsets::default();
        self.parser_state = None;
        self.rt_plus = false;
        self.settings_dump_pending = false;
        self.error_latched = false;
        self.sleep_on_ack = false;
        self.wake_probe_pending = false;
        self.clear_live_pin_reports();

        self.shared.set_info(None);
        self.shared.set_settings(Arc::new(Vec::new()));
        self.shared.set_offsets(Arc::new(Offsets::default()));
        self.shared.set_parser_state(None);
        self.shared
            .set_code_tables(Arc::new(CodeTable::new()), Arc::new(CodeTable::new()));
    }

    fn advance_discovery(&mut self, completed: DiscoveryStep, now: Instant) {
        if self.discovery.is_none() {
            return;
        }

        if completed == DiscoveryStep::Info {
            self.apply_controller_info();
            let mut steps = VecDeque::from([
                DiscoveryStep::Settings,
                DiscoveryStep::Offsets,
                DiscoveryStep::ParserState,
            ]);
            if self.info.has_new_option("ENUMS") {
                steps.push_back(DiscoveryStep::AlarmCodes);
                steps.push_back(DiscoveryStep::ErrorCodes);
                steps.push_back(DiscoveryStep::SettingDescriptions);
                steps.push_back(DiscoveryStep::SettingGroups);
            }
            if let Some(discovery) = self.discovery.as_mut() {
                discovery.steps = steps;
            }
        }

        let next = self
            .discovery
            .as_mut()
            .and_then(|discovery| discovery.steps.pop_front());
        let Some(next) = next else {
            self.finish_discovery(now);
            return;
        };

        if let Some(discovery) = self.discovery.as_mut() {
            discovery.current = next;
            discovery.deadline = deadline(now, self.config.discovery_timeout);
        }

        if next == DiscoveryStep::Offsets {
            // Each `$#` is a complete controller report: rebuild the tool
            // table instead of keeping entries the controller no longer sends.
            self.clear_controller_tools();
        }

        if let Err(error) = self.enqueue_line(next.command(), LinePurpose::Discovery(next)) {
            self.log_warn(format!(
                "Skipping discovery step {} after an enqueue error: {error}",
                next.label()
            ));
            self.streamer.clear();
            self.streamer.resume();
            self.current_purpose = None;
            self.advance_discovery(next, now);
        }
    }

    fn apply_controller_info(&mut self) {
        let info = self.info.clone();
        self.rt_plus = info.rt_plus();
        let reported_rx = info.rx_buffer_size_or(self.config.default_rx_buffer);
        let rx_buffer = reported_rx.min(MAX_RX_BUFFER_SIZE);
        if reported_rx > rx_buffer {
            self.log_warn(format!(
                "Clamping the reported RX buffer size {reported_rx} to {rx_buffer} bytes"
            ));
        }
        if let Err(error) = self.streamer.set_rx_buffer_size(rx_buffer) {
            self.log_warn(format!(
                "Ignoring the reported RX buffer size {rx_buffer}: {error}"
            ));
        }
        let firmware = info.firmware.as_deref().unwrap_or("unknown firmware");
        let version = if info.version.is_empty() {
            "unknown version"
        } else {
            info.version.as_str()
        };
        self.log_info(format!(
            "Controller: {firmware} {version}, {} axes, RX buffer {rx_buffer} bytes",
            info.axis_count()
        ));
        self.shared.set_info(Some(Arc::new(info)));
    }

    fn finish_discovery(&mut self, now: Instant) {
        if self.phase != Phase::Discovering {
            return;
        }
        self.apply_controller_info();
        self.phase = Phase::Ready;
        self.discovery = None;
        self.current_purpose = None;
        self.advance_after_clear = false;
        self.error_latched = false;
        self.sleeping = false;
        self.sleep_on_ack = false;
        self.wake_probe_pending = false;
        self.last_status_at = Some(now);
        self.next_poll_at = now;
        self.next_full_at = deadline(now, self.config.full_status_interval);

        let info = Arc::new(self.info.clone());
        let alarms = Arc::new(self.alarm_codes.clone());
        let errors = Arc::new(self.error_codes.clone());
        self.shared.set_info(Some(Arc::clone(&info)));
        self.shared
            .set_code_tables(Arc::clone(&alarms), Arc::clone(&errors));
        self.publish_offsets();
        self.publish_parser_state();
        self.shared.ready.store(true, Ordering::Release);

        self.emit(EngineEvent::Connected);
        self.emit(EngineEvent::ControllerInfo(info));
        // Publish values together with the `$ES`/`$EG` metadata recorded
        // during discovery; the settings UI needs both in one ready event.
        self.publish_settings(true);
        self.emit(EngineEvent::CodeTables { alarms, errors });
        self.log_info("Controller ready; status polling active");
    }

    // -------------------------------------------------------------- telemetry

    fn publish_snapshot(&mut self) {
        let snapshot = Arc::clone(&self.snapshot);
        let last_status = self.last_status.clone();
        self.shared.set_snapshot(snapshot, last_status);
    }

    fn publish_settings(&mut self, emit: bool) {
        let descriptions = Arc::new(
            self.setting_descriptions
                .values()
                .cloned()
                .collect::<Vec<_>>(),
        );
        let groups = Arc::new(self.setting_groups.values().cloned().collect::<Vec<_>>());
        self.shared
            .set_setting_metadata(Arc::clone(&descriptions), Arc::clone(&groups));

        let list = Arc::new(self.build_settings());
        self.shared.set_settings(Arc::clone(&list));
        if emit {
            self.emit(EngineEvent::SettingMetadata {
                descriptions,
                groups,
            });
            self.emit(EngineEvent::Settings(list));
        }
    }

    fn publish_code_tables_only(&mut self) {
        self.shared.set_code_tables(
            Arc::new(self.alarm_codes.clone()),
            Arc::new(self.error_codes.clone()),
        );
    }

    fn build_settings(&self) -> Vec<Setting> {
        let mut settings = Vec::with_capacity(self.settings.len());
        for (number, value) in &self.settings {
            let mut setting = Setting::new(*number, value.clone());
            if let Some(description) = self.setting_descriptions.get(number) {
                setting.description = Some(description.name.clone());
                setting.unit = description.unit.clone();
                if let Some(group_id) = description.group {
                    setting.group = self
                        .setting_groups
                        .get(&group_id)
                        .map(|group| group.name.clone())
                        .or_else(|| Some(group_id.to_string()));
                }
            }
            settings.push(setting);
        }
        settings
    }

    fn publish_offsets(&mut self) {
        self.shared.set_offsets(Arc::new(self.offsets.clone()));
    }

    fn publish_controller_tools(&mut self) {
        let tools = Arc::new(self.controller_tools.values().cloned().collect::<Vec<_>>());
        self.shared.set_controller_tools(Arc::clone(&tools));
        self.emit(EngineEvent::ControllerTools(tools));
    }

    fn publish_startup_lines(&mut self) {
        let lines = Arc::new(self.startup_lines.values().cloned().collect::<Vec<_>>());
        self.shared.set_startup_lines(Arc::clone(&lines));
        self.emit(EngineEvent::StartupLines(lines));
    }

    fn publish_pin_map(&mut self) {
        let map = Arc::new(self.pin_map.values().cloned().collect::<Vec<_>>());
        self.shared.set_pin_map(Arc::clone(&map));
        self.emit(EngineEvent::PinMap(map));
    }

    /// Drop the cached `$PINS` map before a fresh controller report.
    ///
    /// Called when a `$PINS` request is accepted. The map is controller
    /// configuration, so a soft reset and a disconnect keep it; a new session
    /// clears it through [`EngineRuntime::reset_session`].
    fn clear_pin_map(&mut self) {
        if self.pin_map.is_empty() {
            self.shared.set_pin_map(Arc::new(Vec::new()));
            return;
        }
        self.pin_map.clear();
        self.publish_pin_map();
    }

    /// Drop the live pin levels, limit state and last events.
    ///
    /// These describe one instant and belong to the current link. The static
    /// `$PINS` map is deliberately kept.
    fn clear_live_pin_reports(&mut self) {
        self.pin_state = None;
        self.limit_state = None;
        self.last_events = None;
        self.shared.clear_live_pin_reports();
    }

    /// Drop the cached startup lines at the start of a new session.
    ///
    /// Called from [`EngineRuntime::reset_session`]; a soft reset is
    /// deliberately not included because the controller keeps its stored lines
    /// across `0x18`.
    fn clear_startup_lines(&mut self) {
        if self.startup_lines.is_empty() {
            self.shared.set_startup_lines(Arc::new(Vec::new()));
            return;
        }
        self.startup_lines.clear();
        self.publish_startup_lines();
    }

    /// Drop the cached controller tool table.
    ///
    /// Called before a fresh `$#` report and when a session or discovery
    /// restarts. A soft reset is deliberately not included: the controller
    /// keeps its stored tool table across `0x18`.
    fn clear_controller_tools(&mut self) {
        if self.controller_tools.is_empty() {
            self.shared.set_controller_tools(Arc::new(Vec::new()));
            return;
        }
        self.controller_tools.clear();
        self.publish_controller_tools();
    }

    fn publish_parser_state(&mut self) {
        self.shared
            .set_parser_state(self.parser_state.clone().map(Arc::new));
    }

    // --------------------------------------------------------------- logging

    fn emit(&mut self, event: EngineEvent) {
        match self.events.try_send(event) {
            Ok(()) => {}
            Err(TrySendError::Full(event)) => {
                let _ = self.event_drop.try_recv();
                let _ = self.events.try_send(event);
            }
            Err(TrySendError::Disconnected(_)) => {}
        }
    }

    fn log(&mut self, level: LogLevel, text: impl Into<String>) {
        let text = bounded_text(&text.into());
        match level {
            LogLevel::Error => tracing::error!(text, "engine log"),
            LogLevel::Warn => tracing::warn!(text, "engine log"),
            _ => tracing::debug!(text, "engine log"),
        }
        self.emit(EngineEvent::Log { level, text });
    }

    fn log_info(&mut self, text: impl Into<String>) {
        self.log(LogLevel::Info, text);
    }

    fn log_command(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.log(LogLevel::Command, format!("> {text}"));
    }

    fn log_ok(&mut self, text: impl Into<String>) {
        self.log(LogLevel::Ok, text);
    }

    fn log_warn(&mut self, text: impl Into<String>) {
        self.log(LogLevel::Warn, text);
    }

    fn log_error(&mut self, text: impl Into<String>) {
        self.log(LogLevel::Error, text);
    }
}

fn deadline(now: Instant, after: Duration) -> Instant {
    now.checked_add(after).unwrap_or(now)
}

fn message_level(text: &str) -> LogLevel {
    let lower = text.trim().to_ascii_lowercase();
    if lower.starts_with("error") || lower.contains(": error") {
        LogLevel::Error
    } else if lower.starts_with("warning")
        || lower.starts_with("warn")
        || lower.contains(": warning")
    {
        LogLevel::Warn
    } else {
        LogLevel::Info
    }
}

fn is_safe_realtime(command: RealtimeCommand) -> bool {
    matches!(
        command,
        RealtimeCommand::StatusReport
            | RealtimeCommand::FullStatusReport
            | RealtimeCommand::SoftReset
            | RealtimeCommand::Stop
            | RealtimeCommand::SoftEStop
            | RealtimeCommand::FeedHold
            | RealtimeCommand::JogCancel
            | RealtimeCommand::SafetyDoor
            | RealtimeCommand::PidReport
    )
}

fn bounded_text(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .take(MAX_LOG_TEXT)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_levels_follow_the_msg_prefix() {
        assert_eq!(message_level("Info: hello"), LogLevel::Info);
        assert_eq!(message_level("Warning: low coolant"), LogLevel::Warn);
        assert_eq!(message_level("Error: probe not connected"), LogLevel::Error);
        assert_eq!(message_level("something: error here"), LogLevel::Error);
        assert_eq!(message_level("Prepared"), LogLevel::Info);
    }

    #[test]
    fn safe_realtime_commands_are_the_safety_set() {
        for command in [
            RealtimeCommand::StatusReport,
            RealtimeCommand::FullStatusReport,
            RealtimeCommand::SoftReset,
            RealtimeCommand::Stop,
            RealtimeCommand::SoftEStop,
            RealtimeCommand::FeedHold,
            RealtimeCommand::JogCancel,
            RealtimeCommand::SafetyDoor,
        ] {
            assert!(is_safe_realtime(command), "{command:?}");
        }
        for command in [
            RealtimeCommand::CycleStart,
            RealtimeCommand::FeedOverrideReset,
            RealtimeCommand::SpindleOverrideReset,
            RealtimeCommand::ToolChangeAck,
            RealtimeCommand::Macro(0),
        ] {
            assert!(!is_safe_realtime(command), "{command:?}");
        }
    }

    #[test]
    fn bounded_text_removes_control_characters_and_caps_length() {
        let text = format!("a\nb\r{}", "x".repeat(MAX_LOG_TEXT + 10));
        let bounded = bounded_text(&text);
        assert!(!bounded.contains('\n'));
        assert!(!bounded.contains('\r'));
        assert_eq!(bounded.chars().count(), MAX_LOG_TEXT);
    }

    #[test]
    fn discovery_steps_cover_the_protocol_handshake() {
        assert_eq!(DiscoveryStep::Info.command(), "$I+");
        assert_eq!(DiscoveryStep::Settings.command(), "$$");
        assert_eq!(DiscoveryStep::Offsets.command(), "$#");
        assert_eq!(DiscoveryStep::ParserState.command(), "$G");
        assert_eq!(DiscoveryStep::AlarmCodes.command(), "$EA");
        assert_eq!(DiscoveryStep::ErrorCodes.command(), "$EE");
        assert_eq!(DiscoveryStep::SettingDescriptions.command(), "$ES");
        assert_eq!(DiscoveryStep::SettingGroups.command(), "$EG");
    }
}
