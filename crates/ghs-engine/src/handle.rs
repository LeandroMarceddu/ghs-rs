//! Engine handle, shared telemetry and thread spawning.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};
use ghs_core::{ControllerInfo, MachineSnapshot, Setting, ToolEntry};
use ghs_proto::{
    CodeTable as ProtoCodeTable, Offsets, ParserState, SettingDescription, SettingGroup,
    StatusReport, Streamer, StreamerConfig,
};

use crate::command::{Command, ConnectTarget};
use crate::config::EngineConfig;
use crate::error::EngineError;
use crate::event::EngineEvent;
use crate::runtime::EngineRuntime;

/// Latest telemetry shared with the UI plus session bookkeeping.
#[derive(Debug, Default)]
pub(crate) struct Telemetry {
    pub(crate) snapshot: Arc<MachineSnapshot>,
    pub(crate) last_status: Option<Arc<StatusReport>>,
    pub(crate) info: Option<Arc<ControllerInfo>>,
    pub(crate) settings: Arc<Vec<Setting>>,
    pub(crate) setting_descriptions: Arc<Vec<SettingDescription>>,
    pub(crate) setting_groups: Arc<Vec<SettingGroup>>,
    pub(crate) controller_tools: Arc<Vec<ToolEntry>>,
    pub(crate) offsets: Arc<Offsets>,
    pub(crate) parser_state: Option<Arc<ParserState>>,
    pub(crate) alarm_codes: Arc<ProtoCodeTable>,
    pub(crate) error_codes: Arc<ProtoCodeTable>,
}

/// State shared between the engine thread and the handle.
#[derive(Debug, Default)]
pub(crate) struct SharedState {
    telemetry: Mutex<Telemetry>,
    pub(crate) connected: AtomicBool,
    pub(crate) ready: AtomicBool,
    pub(crate) shutdown: AtomicBool,
}

impl SharedState {
    fn telemetry(&self) -> MutexGuard<'_, Telemetry> {
        match self.telemetry.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub(crate) fn snapshot(&self) -> Arc<MachineSnapshot> {
        Arc::clone(&self.telemetry().snapshot)
    }

    pub(crate) fn last_status(&self) -> Option<Arc<StatusReport>> {
        self.telemetry().last_status.clone()
    }

    pub(crate) fn info(&self) -> Option<Arc<ControllerInfo>> {
        self.telemetry().info.clone()
    }

    pub(crate) fn settings(&self) -> Arc<Vec<Setting>> {
        Arc::clone(&self.telemetry().settings)
    }

    pub(crate) fn setting_descriptions(&self) -> Arc<Vec<SettingDescription>> {
        Arc::clone(&self.telemetry().setting_descriptions)
    }

    pub(crate) fn setting_groups(&self) -> Arc<Vec<SettingGroup>> {
        Arc::clone(&self.telemetry().setting_groups)
    }

    pub(crate) fn set_setting_metadata(
        &self,
        descriptions: Arc<Vec<SettingDescription>>,
        groups: Arc<Vec<SettingGroup>>,
    ) {
        let mut telemetry = self.telemetry();
        telemetry.setting_descriptions = descriptions;
        telemetry.setting_groups = groups;
    }

    pub(crate) fn controller_tools(&self) -> Arc<Vec<ToolEntry>> {
        Arc::clone(&self.telemetry().controller_tools)
    }

    pub(crate) fn set_controller_tools(&self, tools: Arc<Vec<ToolEntry>>) {
        self.telemetry().controller_tools = tools;
    }

    pub(crate) fn offsets(&self) -> Arc<Offsets> {
        Arc::clone(&self.telemetry().offsets)
    }

    pub(crate) fn parser_state(&self) -> Option<Arc<ParserState>> {
        self.telemetry().parser_state.clone()
    }

    pub(crate) fn code_tables(&self) -> (Arc<ProtoCodeTable>, Arc<ProtoCodeTable>) {
        let telemetry = self.telemetry();
        (
            Arc::clone(&telemetry.alarm_codes),
            Arc::clone(&telemetry.error_codes),
        )
    }

    pub(crate) fn set_snapshot(
        &self,
        snapshot: Arc<MachineSnapshot>,
        last_status: Option<Arc<StatusReport>>,
    ) {
        let mut telemetry = self.telemetry();
        telemetry.snapshot = snapshot;
        telemetry.last_status = last_status;
    }

    pub(crate) fn set_info(&self, info: Option<Arc<ControllerInfo>>) {
        self.telemetry().info = info;
    }

    pub(crate) fn set_settings(&self, settings: Arc<Vec<Setting>>) {
        self.telemetry().settings = settings;
    }

    pub(crate) fn set_offsets(&self, offsets: Arc<Offsets>) {
        self.telemetry().offsets = offsets;
    }

    pub(crate) fn set_parser_state(&self, parser_state: Option<Arc<ParserState>>) {
        self.telemetry().parser_state = parser_state;
    }

    pub(crate) fn set_code_tables(&self, alarms: Arc<ProtoCodeTable>, errors: Arc<ProtoCodeTable>) {
        let mut telemetry = self.telemetry();
        telemetry.alarm_codes = alarms;
        telemetry.error_codes = errors;
    }

    /// Reset every piece of session telemetry and mark the session
    /// disconnected.
    pub(crate) fn reset(&self) {
        *self.telemetry() = Telemetry::default();
        self.connected.store(false, Ordering::Release);
        self.ready.store(false, Ordering::Release);
    }
}

/// A message from the handle to the engine thread.
pub(crate) enum EngineInput {
    Connect {
        target: ConnectTarget,
        reply: Sender<Result<(), EngineError>>,
    },
    Command {
        command: Command,
        reply: Sender<Result<(), EngineError>>,
    },
    Disconnect,
    Shutdown,
}

/// Handle to a running engine thread.
///
/// The handle is `Send` and exposes a cheap latest-value snapshot plus a
/// bounded event receiver. Dropping it stops and joins the engine thread.
pub struct EngineHandle {
    config: EngineConfig,
    commands: Sender<EngineInput>,
    events: Receiver<EngineEvent>,
    shared: Arc<SharedState>,
    join: Option<JoinHandle<()>>,
}

impl EngineHandle {
    /// Open a transport and start the handshake.
    ///
    /// The call returns once the engine thread has opened the transport; the
    /// handshake continues asynchronously. Wait for [`EngineEvent::Connected`]
    /// (and [`EngineHandle::is_ready`]) before sending control commands.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::AlreadyConnected`] when a transport is already
    /// open, [`EngineError::InvalidConfig`] for a bad serial configuration,
    /// [`EngineError::CommandQueueFull`] or [`EngineError::EngineStopped`] when
    /// the engine cannot accept the request, and [`EngineError::Transport`]
    /// when the port cannot be opened.
    pub fn connect(&self, target: ConnectTarget) -> Result<(), EngineError> {
        if self.is_connected() {
            return Err(EngineError::AlreadyConnected);
        }
        if let ConnectTarget::Serial(config) = &target {
            config
                .validate()
                .map_err(|error| EngineError::InvalidConfig(error.to_string()))?;
        }
        let (reply_tx, reply_rx) = bounded(1);
        enqueue_input(
            &self.commands,
            EngineInput::Connect {
                target,
                reply: reply_tx,
            },
        )?;
        match reply_rx.recv_timeout(self.config.command_timeout) {
            Ok(result) => result,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => Err(EngineError::Timeout {
                operation: "connect",
            }),
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                Err(EngineError::EngineStopped)
            }
        }
    }

    /// Close the transport and return to the disconnected state.
    ///
    /// The engine thread stays alive, so [`EngineHandle::connect`] can be
    /// called again. A closed link never resumes a job or restarts motion.
    pub fn disconnect(&self) {
        let timeout = self.config.command_timeout.min(Duration::from_millis(250));
        let _ = self.commands.send_timeout(EngineInput::Disconnect, timeout);
    }

    /// Send a typed command through the safety gateway.
    ///
    /// The call returns after the engine has validated the command and either
    /// written or queued it. It never waits for the controller's `ok`; that
    /// arrives as an [`EngineEvent`]. Motion, jog and MDI are refused in locked
    /// states, settings writes are idle-only and read back, and a pendant
    /// blocks control commands.
    ///
    /// # Errors
    ///
    /// Returns a gateway error such as [`EngineError::MotionBlocked`],
    /// [`EngineError::NotIdle`], [`EngineError::PendantActive`] or
    /// [`EngineError::Busy`], a queue error, or a transport error.
    pub fn send(&self, command: Command) -> Result<(), EngineError> {
        let (reply_tx, reply_rx) = bounded(1);
        enqueue_input(
            &self.commands,
            EngineInput::Command {
                command,
                reply: reply_tx,
            },
        )?;
        match reply_rx.recv_timeout(self.config.command_timeout) {
            Ok(result) => result,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => Err(EngineError::Timeout {
                operation: "accept the command",
            }),
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                Err(EngineError::EngineStopped)
            }
        }
    }

    /// Latest machine snapshot. The call is non-blocking in practice and only
    /// clones an `Arc`.
    #[must_use]
    pub fn snapshot(&self) -> Arc<MachineSnapshot> {
        self.shared.snapshot()
    }

    /// Bounded event receiver. The UI should drain it every frame.
    #[must_use]
    pub fn events(&self) -> &Receiver<EngineEvent> {
        &self.events
    }

    /// Returns `true` while a transport is open, including during the
    /// handshake.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.shared.connected.load(Ordering::Acquire)
    }

    /// Returns `true` after the handshake completed and polling is active.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.shared.ready.load(Ordering::Acquire)
    }

    /// Last raw status report, including fields that are not part of
    /// [`MachineSnapshot`] such as `DTG`, `T`, `TLR` and `SD`.
    #[must_use]
    pub fn last_status(&self) -> Option<Arc<StatusReport>> {
        self.shared.last_status()
    }

    /// Latest controller identity and capabilities from `$I+`.
    #[must_use]
    pub fn controller_info(&self) -> Option<Arc<ControllerInfo>> {
        self.shared.info()
    }

    /// Latest settings snapshot, sorted by setting number.
    #[must_use]
    pub fn settings(&self) -> Arc<Vec<Setting>> {
        self.shared.settings()
    }

    /// Setting type/range metadata from `$ES`, when the controller reports it.
    #[must_use]
    pub fn setting_descriptions(&self) -> Arc<Vec<SettingDescription>> {
        self.shared.setting_descriptions()
    }

    /// Setting groups from `$EG`, when the controller reports them.
    #[must_use]
    pub fn setting_groups(&self) -> Arc<Vec<SettingGroup>> {
        self.shared.setting_groups()
    }

    /// Controller-reported `[T:...]` tool table from `$#`.
    ///
    /// Entries are sorted by tool number and are read-only: the application
    /// owns its own editable table and must not merge the two. The list is
    /// replaced by each controller report and cleared at the start of a new
    /// discovery; it never contains entries the controller did not send.
    #[must_use]
    pub fn controller_tools(&self) -> Arc<Vec<ToolEntry>> {
        self.shared.controller_tools()
    }

    /// Latest `$#` offsets.
    #[must_use]
    pub fn offsets(&self) -> Arc<Offsets> {
        self.shared.offsets()
    }

    /// Latest `$G` parser state.
    #[must_use]
    pub fn parser_state(&self) -> Option<Arc<ParserState>> {
        self.shared.parser_state()
    }

    /// Runtime alarm and error descriptions from `$EA`/`$EE`.
    #[must_use]
    pub fn code_tables(&self) -> (Arc<ProtoCodeTable>, Arc<ProtoCodeTable>) {
        self.shared.code_tables()
    }

    /// Stop the engine thread and wait for it to exit.
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        let _ = self.commands.try_send(EngineInput::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Entry point for creating an engine session.
#[derive(Debug, Clone, Copy, Default)]
pub struct Engine;

impl Engine {
    /// Spawn the engine thread.
    ///
    /// # Errors
    ///
    /// Returns [`EngineError::InvalidConfig`] when the configuration is not
    /// usable, or [`EngineError::ThreadSpawn`] when the thread cannot start.
    pub fn spawn(config: EngineConfig) -> Result<EngineHandle, EngineError> {
        config.validate()?;

        let (command_tx, command_rx) = bounded(config.command_queue_capacity);
        let (event_tx, event_rx) = bounded(config.event_queue_capacity);
        let event_drop_rx = event_rx.clone();
        let shared = Arc::new(SharedState::default());
        let ticks = crossbeam_channel::tick(config.tick_interval);

        let streamer = Streamer::new(StreamerConfig {
            rx_buffer_size: config.default_rx_buffer,
            max_line_len: config.max_command_line,
            use_bf_topup: true,
        });
        let runtime = EngineRuntime::new(
            config.clone(),
            command_rx,
            ticks,
            event_tx,
            event_drop_rx,
            Arc::clone(&shared),
            streamer,
        );

        let join = std::thread::Builder::new()
            .name("ghs-engine".into())
            .spawn(move || runtime.run())
            .map_err(EngineError::ThreadSpawn)?;

        Ok(EngineHandle {
            config,
            commands: command_tx,
            events: event_rx,
            shared,
            join: Some(join),
        })
    }
}

fn enqueue_input(sender: &Sender<EngineInput>, input: EngineInput) -> Result<(), EngineError> {
    match sender.try_send(input) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_)) => Err(EngineError::CommandQueueFull),
        Err(TrySendError::Disconnected(_)) => Err(EngineError::EngineStopped),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_input_queue_reports_full_instead_of_blocking() {
        let (tx, _rx) = bounded(1);
        enqueue_input(&tx, EngineInput::Shutdown).expect("first message fits");
        assert!(matches!(
            enqueue_input(&tx, EngineInput::Shutdown),
            Err(EngineError::CommandQueueFull)
        ));
    }

    #[test]
    fn a_closed_engine_reports_stopped() {
        let (tx, rx) = bounded(1);
        drop(rx);
        assert!(matches!(
            enqueue_input(&tx, EngineInput::Disconnect),
            Err(EngineError::EngineStopped)
        ));
    }

    #[test]
    fn shared_snapshot_is_cheap_to_clone_and_reset() {
        let shared = SharedState::default();
        let first = shared.snapshot();
        let second = shared.snapshot();
        assert!(Arc::ptr_eq(&first, &second));
        shared.reset();
        assert!(!shared.connected.load(Ordering::Acquire));
        assert!(!shared.ready.load(Ordering::Acquire));
    }
}
