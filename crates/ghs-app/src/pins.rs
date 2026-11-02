//! Read-only controller pin, limit and event diagnostics.
//!
//! grblHAL answers `$PINS`, `$PINSTATE`, `$LIM` and `$LEV` with read-only
//! reports. This module owns the app-side request state machine and the mirrors
//! of the typed engine data. It never parses console text and never builds a
//! motion, MDI or setting command: the only commands it can produce are the
//! four read-only system commands below.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ghs_engine::{
    Command, EngineError, ExtendedCommand, LastEvents, LimitState, PinDirection, PinMapping,
    PinState, Signals, SystemCommand,
};

/// Backoff between `Busy` retries for one diagnostics request.
///
/// The shell polls at 50 ms. A larger delay keeps a busy streamer from being
/// hammered while still finishing a refresh quickly once the controller frees
/// up.
pub const PIN_REFRESH_BUSY_RETRY: Duration = Duration::from_millis(150);

/// Upper bound of input/output levels rendered into the panel.
///
/// The protocol parser already bounds a reported level list; this second bound
/// keeps the rendered text finite even if a future caller supplies a larger
/// vector directly.
pub const MAX_RENDERED_PIN_BITS: usize = 256;

/// One read-only report requested by the diagnostics panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PinsRequest {
    /// `$PINS`: controller pin map.
    PinMap,
    /// `$PINSTATE`: auxiliary pin levels.
    PinState,
    /// `$LIM`: current limit-switch state.
    Limits,
    /// `$LEV`: last control and limit events.
    Events,
}

impl PinsRequest {
    /// Every request sent by one refresh, in wire order.
    pub const ALL: [Self; 4] = [Self::PinMap, Self::PinState, Self::Limits, Self::Events];

    /// Wire label shown in the panel and used by tests.
    pub const fn label(self) -> &'static str {
        match self {
            Self::PinMap => "$PINS",
            Self::PinState => "$PINSTATE",
            Self::Limits => "$LIM",
            Self::Events => "$LEV",
        }
    }

    /// The typed, guarded command for this report.
    pub fn command(self) -> Command {
        let extended = match self {
            Self::PinMap => ExtendedCommand::Pins,
            Self::PinState => ExtendedCommand::PinState,
            Self::Limits => ExtendedCommand::Limits,
            Self::Events => ExtendedCommand::Events,
        };
        Command::System(SystemCommand::Extended(extended))
    }
}

/// Why the panel may not start a refresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinsRefusal {
    /// No engine is attached (`--mock`).
    Mock,
    /// The handshake has not completed or the link closed.
    NotReady,
    /// The machine is in `Alarm` or `Door`.
    MachineLocked,
    /// A job is running or paused; the streamer owns the line queue.
    JobActive,
}

impl PinsRefusal {
    /// Operator-facing reason shown in the panel.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Mock => "Mock mode: controller diagnostics need a live controller",
            Self::NotReady => "The controller link is not ready",
            Self::MachineLocked => "Refresh is blocked while the machine is in Alarm or Door",
            Self::JobActive => "Refresh is blocked while a job is running or paused",
        }
    }
}

/// Gate inputs mirrored from the shell's connection and machine state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinsGate {
    /// A live engine session is active.
    pub live: bool,
    /// The engine handshake completed.
    pub ready: bool,
    /// The machine is in `Alarm` or `Door`.
    pub locked: bool,
    /// A job is running or paused.
    pub job_active: bool,
}

impl PinsGate {
    /// First reason this gate blocks a refresh, if any.
    pub fn refusal(self) -> Option<PinsRefusal> {
        if !self.live {
            Some(PinsRefusal::Mock)
        } else if !self.ready {
            Some(PinsRefusal::NotReady)
        } else if self.locked {
            Some(PinsRefusal::MachineLocked)
        } else if self.job_active {
            Some(PinsRefusal::JobActive)
        } else {
            None
        }
    }
}

impl Default for PinsGate {
    fn default() -> Self {
        Self {
            live: true,
            ready: true,
            locked: false,
            job_active: false,
        }
    }
}

/// Request lifecycle shown in the panel header.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum PinsStatus {
    /// Opening the panel is not a request.
    #[default]
    Idle,
    /// Requests are queued or have been accepted and reports are arriving.
    Refreshing,
    /// The engine is busy; the same request is retried after a backoff.
    Busy,
    /// All four requests were accepted by the engine.
    Requested,
    /// The refresh stopped; the text is the connection or engine reason.
    Stopped(String),
}

impl PinsStatus {
    /// Short uppercase label for the panel header.
    pub fn label(&self) -> &str {
        match self {
            Self::Idle => "NOT REQUESTED",
            Self::Refreshing => "READING CONTROLLER",
            Self::Busy => "CONTROLLER BUSY - RETRYING",
            Self::Requested => "REPORTS REQUESTED",
            Self::Stopped(reason) => reason.as_str(),
        }
    }
}

/// Whether one report has arrived and whether it carried any entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportState {
    /// Nothing has arrived since the session started.
    Missing,
    /// A report arrived but explicitly carries no entries.
    Empty,
    /// A report arrived with data.
    Reported,
}

/// Plain-data model for the read-only controller diagnostics panel.
#[derive(Debug, Default)]
pub struct PinsModel {
    open: bool,
    queue: VecDeque<PinsRequest>,
    pending: Option<PinsRequest>,
    retry_at: Option<Instant>,
    accepted: u8,
    pin_map_requested: bool,
    status: PinsStatus,
    pin_map: Option<Arc<Vec<PinMapping>>>,
    pin_state: Option<Arc<PinState>>,
    limit_state: Option<Arc<LimitState>>,
    last_events: Option<Arc<LastEvents>>,
}

impl PinsModel {
    /// Empty panel state for a new shell.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the sheet is visible. Opening it sends nothing by itself.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Mark the sheet open without sending a command.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Mark the sheet closed and drop requests that have not been sent.
    ///
    /// A request already accepted by the engine still completes; closing the
    /// panel never touches the machine.
    pub fn close(&mut self) {
        self.open = false;
        self.queue.clear();
        self.pending = None;
        self.retry_at = None;
        self.status = PinsStatus::Idle;
    }

    /// Current request lifecycle.
    pub fn status(&self) -> &PinsStatus {
        &self.status
    }

    /// Whether the poll loop should call [`Self::pump`].
    pub fn needs_pump(&self) -> bool {
        self.pending.is_some() || !self.queue.is_empty()
    }

    /// Start one refresh and queue the four read-only reports.
    ///
    /// Returns the gate refusal without queuing anything when the machine is
    /// locked, a job is active, or the link is not ready.
    pub fn begin_refresh(&mut self, gate: PinsGate) -> Result<(), PinsRefusal> {
        if let Some(refusal) = gate.refusal() {
            self.stop(refusal.message().to_string());
            return Err(refusal);
        }
        self.queue = PinsRequest::ALL.into_iter().collect();
        self.pending = None;
        self.retry_at = None;
        self.accepted = 0;
        self.status = PinsStatus::Refreshing;
        Ok(())
    }

    /// Send queued requests through the caller's guarded engine path.
    ///
    /// A `Busy` engine leaves the current request pending and records a
    /// backoff; the next pump before [`Instant::now`] reaches that deadline
    /// sends nothing. Any other error stops the refresh and keeps its text for
    /// the panel.
    ///
    /// Returns `true` when the status, queue, or mirrors changed.
    pub fn pump(
        &mut self,
        now: Instant,
        gate: PinsGate,
        mut send: impl FnMut(Command) -> Result<(), EngineError>,
    ) -> bool {
        if !self.open || !self.needs_pump() {
            return false;
        }
        if let Some(refusal) = gate.refusal() {
            self.stop(refusal.message().to_string());
            return true;
        }
        if self.retry_at.is_some_and(|deadline| now < deadline) {
            return false;
        }
        self.retry_at = None;

        let mut changed = false;
        loop {
            let request = match self.pending {
                Some(request) => request,
                None => match self.queue.pop_front() {
                    Some(request) => {
                        self.pending = Some(request);
                        request
                    }
                    None => break,
                },
            };
            match send(request.command()) {
                Ok(()) => {
                    if request == PinsRequest::PinMap {
                        self.pin_map_requested = true;
                    }
                    self.pending = None;
                    self.accepted = self.accepted.saturating_add(1);
                    self.status = if usize::from(self.accepted) >= PinsRequest::ALL.len() {
                        PinsStatus::Requested
                    } else {
                        PinsStatus::Refreshing
                    };
                    changed = true;
                }
                Err(EngineError::Busy) => {
                    self.retry_at = Some(now + PIN_REFRESH_BUSY_RETRY);
                    self.status = PinsStatus::Busy;
                    changed = true;
                    break;
                }
                Err(error) => {
                    self.pending = None;
                    self.queue.clear();
                    self.retry_at = None;
                    self.status = PinsStatus::Stopped(error.to_string());
                    changed = true;
                    break;
                }
            }
        }
        changed
    }

    /// Stop a refresh and keep the reason for the panel.
    pub fn stop(&mut self, reason: impl Into<String>) {
        self.queue.clear();
        self.pending = None;
        self.retry_at = None;
        self.status = PinsStatus::Stopped(reason.into());
    }

    /// Drop live reports for a closed link, matching the engine.
    ///
    /// The static `$PINS` map is controller configuration and survives a
    /// disconnect, exactly like [`ghs_engine::EngineHandle::pin_map`].
    pub fn on_disconnected(&mut self) {
        self.queue.clear();
        self.pending = None;
        self.retry_at = None;
        self.pin_state = None;
        self.limit_state = None;
        self.last_events = None;
        self.status = if self.open {
            PinsStatus::Stopped("Disconnected; the last diagnostics are stale".to_string())
        } else {
            PinsStatus::Idle
        };
    }

    /// Stop a refresh because the controller raised an alarm.
    pub fn on_alarm(&mut self, code: u16) {
        self.stop(format!("ALARM:{code}"));
    }

    /// Clear every mirror at the start of a new controller session.
    ///
    /// The sheet may stay open; it shows the explicit "not requested" state
    /// until the operator refreshes again.
    pub fn reset_session(&mut self) {
        self.queue.clear();
        self.pending = None;
        self.retry_at = None;
        self.accepted = 0;
        self.pin_map_requested = false;
        self.status = PinsStatus::Idle;
        self.pin_map = None;
        self.pin_state = None;
        self.limit_state = None;
        self.last_events = None;
    }

    /// Replace the pin map; `Some(empty)` means an explicit empty report.
    pub fn apply_pin_map(&mut self, map: Arc<Vec<PinMapping>>) -> bool {
        replace_arc(&mut self.pin_map, Some(map))
    }

    /// Replace or clear the auxiliary pin levels.
    pub fn apply_pin_state(&mut self, state: Option<Arc<PinState>>) -> bool {
        replace_arc(&mut self.pin_state, state)
    }

    /// Replace or clear the limit-switch state.
    pub fn apply_limit_state(&mut self, state: Option<Arc<LimitState>>) -> bool {
        replace_arc(&mut self.limit_state, state)
    }

    /// Replace or clear the last control/limit events.
    pub fn apply_last_events(&mut self, events: Option<Arc<LastEvents>>) -> bool {
        replace_arc(&mut self.last_events, events)
    }

    /// Whether `$PINS` has been accepted in this session.
    ///
    /// The shell uses this to poll the engine's map accessor after a request;
    /// before the first request the accessor is empty for a different reason
    /// (no report was asked for) and must not look like an empty report.
    pub fn pin_map_requested(&self) -> bool {
        self.pin_map_requested
    }

    /// Current pin map, when a report arrived.
    pub fn pin_map(&self) -> Option<&[PinMapping]> {
        self.pin_map.as_deref().map(Vec::as_slice)
    }

    /// Current auxiliary pin levels, when a report arrived.
    pub fn pin_state(&self) -> Option<&PinState> {
        self.pin_state.as_deref()
    }

    /// Current limit-switch state, when a report arrived.
    pub fn limit_state(&self) -> Option<&LimitState> {
        self.limit_state.as_deref()
    }

    /// Current last control/limit events, when a report arrived.
    pub fn last_events(&self) -> Option<&LastEvents> {
        self.last_events.as_deref()
    }

    /// Report state for the pin map.
    pub fn pin_map_report(&self) -> ReportState {
        match self.pin_map() {
            None => ReportState::Missing,
            Some([]) => ReportState::Empty,
            Some(_) => ReportState::Reported,
        }
    }

    /// Report state for the auxiliary pin levels.
    pub fn pin_state_report(&self) -> ReportState {
        match self.pin_state.as_deref() {
            None => ReportState::Missing,
            Some(state)
                if state.mask.is_empty() && state.inputs.is_empty() && state.outputs.is_empty() =>
            {
                ReportState::Empty
            }
            Some(_) => ReportState::Reported,
        }
    }

    /// Report state for the limit-switch state.
    pub fn limits_report(&self) -> ReportState {
        match self.limit_state {
            None => ReportState::Missing,
            Some(_) => ReportState::Reported,
        }
    }

    /// Report state for the last control/limit events.
    pub fn last_events_report(&self) -> ReportState {
        match self.last_events.as_deref() {
            None => ReportState::Missing,
            Some(events) if events.count == 0 && events.signals.is_empty() => ReportState::Empty,
            Some(_) => ReportState::Reported,
        }
    }

    /// Explicit missing/empty text for the pin map.
    pub fn pin_map_note(&self) -> &'static str {
        match self.pin_map_report() {
            ReportState::Missing => "No $PINS report has arrived in this session.",
            ReportState::Empty => "The controller reported no pin entries.",
            ReportState::Reported => "",
        }
    }

    /// Explicit missing/empty text for the auxiliary pin levels.
    pub fn pin_state_note(&self) -> &'static str {
        match self.pin_state_report() {
            ReportState::Missing => "No $PINSTATE report has arrived in this session.",
            ReportState::Empty => "The controller reported no auxiliary pin levels.",
            ReportState::Reported => "",
        }
    }

    /// Explicit missing text for the limit-switch state.
    pub fn limits_note(&self) -> &'static str {
        match self.limits_report() {
            ReportState::Missing => "No $LIM report has arrived in this session.",
            _ => "",
        }
    }

    /// Explicit missing/empty text for the last events.
    pub fn last_events_note(&self) -> &'static str {
        match self.last_events_report() {
            ReportState::Missing => "No $LEV report has arrived in this session.",
            ReportState::Empty => "The controller reports no control or limit events yet.",
            ReportState::Reported => "",
        }
    }
}

/// Replace an `Arc` slot only when the value actually changed.
fn replace_arc<T>(slot: &mut Option<Arc<T>>, next: Option<Arc<T>>) -> bool {
    match (slot.as_ref(), next.as_ref()) {
        (None, None) => false,
        (Some(current), Some(next)) if Arc::ptr_eq(current, next) => false,
        _ => {
            *slot = next;
            true
        }
    }
}

/// Operator-facing direction token.
pub fn pin_direction_label(direction: Option<PinDirection>) -> &'static str {
    match direction {
        Some(PinDirection::Input) => "INPUT",
        Some(PinDirection::Output) => "OUTPUT",
        None => "UNKNOWN",
    }
}

/// Raw signal mask plus its `Pn`-style letters.
pub fn signal_mask_text(mask: Signals) -> String {
    if mask.is_empty() {
        "0x00000000 (none)".to_string()
    } else {
        format!("0x{:08X} ({mask})", mask.bits())
    }
}

/// Bounded bit string for one input/output level list.
///
/// Bits are grouped in bytes and wrapped every 32 bits so a wide report never
/// forces the panel to scroll horizontally.
pub fn pin_bits_text(levels: &[bool]) -> String {
    if levels.is_empty() {
        return "(none)".to_string();
    }
    let shown = levels.len().min(MAX_RENDERED_PIN_BITS);
    let mut text = String::with_capacity(shown + shown / 8 + 32);
    for (index, level) in levels.iter().take(shown).enumerate() {
        if index > 0 {
            if index % 32 == 0 {
                text.push('\n');
            } else if index % 8 == 0 {
                text.push(' ');
            }
        }
        text.push(if *level { '1' } else { '0' });
    }
    if levels.len() > shown {
        text.push_str("\n+ more levels not shown");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghs_engine::Axes;

    use crate::state::AppState;

    fn ready_gate() -> PinsGate {
        PinsGate {
            live: true,
            ready: true,
            locked: false,
            job_active: false,
        }
    }

    fn command_label(command: &Command) -> &'static str {
        match command {
            Command::System(SystemCommand::Extended(ExtendedCommand::Pins)) => "$PINS",
            Command::System(SystemCommand::Extended(ExtendedCommand::PinState)) => "$PINSTATE",
            Command::System(SystemCommand::Extended(ExtendedCommand::Limits)) => "$LIM",
            Command::System(SystemCommand::Extended(ExtendedCommand::Events)) => "$LEV",
            other => panic!("unexpected diagnostics command: {other:?}"),
        }
    }

    fn sample_map() -> Arc<Vec<PinMapping>> {
        Arc::new(vec![
            PinMapping {
                id: 0,
                name: "X limit".into(),
                direction: Some(PinDirection::Input),
            },
            PinMapping {
                id: 12,
                name: "Aux output 3".into(),
                direction: Some(PinDirection::Output),
            },
        ])
    }

    fn sample_pin_state() -> Arc<PinState> {
        Arc::new(PinState {
            mask: Signals::X_LIMIT | Signals::DOOR,
            inputs: vec![true, false, true, false],
            outputs: vec![false, true],
        })
    }

    fn sample_limits() -> Arc<LimitState> {
        Arc::new(LimitState {
            mask: 0b101,
            axes: Axes::from_bits(0b101),
        })
    }

    fn sample_events() -> Arc<LastEvents> {
        Arc::new(LastEvents {
            count: 17,
            signals: Signals::X_LIMIT | Signals::FEED_HOLD,
        })
    }

    #[test]
    fn refresh_sends_exactly_the_four_read_only_commands() {
        let mut model = PinsModel::new();
        model.open();
        model.begin_refresh(ready_gate()).expect("ready gate");

        let mut commands = Vec::new();
        assert!(model.pump(Instant::now(), ready_gate(), |command| {
            commands.push(command);
            Ok(())
        }));

        assert_eq!(commands.len(), 4);
        assert_eq!(
            commands.iter().map(command_label).collect::<Vec<_>>(),
            vec!["$PINS", "$PINSTATE", "$LIM", "$LEV"]
        );
        assert_eq!(model.status(), &PinsStatus::Requested);
        assert!(!model.needs_pump());
        assert!(model.pin_map_requested());
    }

    #[test]
    fn busy_retry_waits_for_the_backoff_and_never_duplicates_accepted_requests() {
        let mut model = PinsModel::new();
        model.open();
        model.begin_refresh(ready_gate()).expect("ready gate");

        let start = Instant::now();
        let mut attempts = Vec::new();
        let mut accepted = Vec::new();
        let mut fail_once = true;

        assert!(model.pump(start, ready_gate(), |command| {
            let label = command_label(&command);
            attempts.push(label);
            if fail_once {
                fail_once = false;
                Err(EngineError::Busy)
            } else {
                accepted.push(label);
                Ok(())
            }
        }));
        assert_eq!(attempts, vec!["$PINS"]);
        assert_eq!(model.status(), &PinsStatus::Busy);
        assert!(model.needs_pump());

        // Before the backoff expires the same request is not re-sent.
        assert!(
            !model.pump(start + Duration::from_millis(10), ready_gate(), |command| {
                attempts.push(command_label(&command));
                Ok(())
            })
        );
        assert_eq!(attempts, vec!["$PINS"]);

        // After the backoff the pending request is retried exactly once and
        // the remaining three follow.
        assert!(
            model.pump(start + PIN_REFRESH_BUSY_RETRY, ready_gate(), |command| {
                let label = command_label(&command);
                attempts.push(label);
                accepted.push(label);
                Ok(())
            })
        );
        assert_eq!(
            attempts,
            vec!["$PINS", "$PINS", "$PINSTATE", "$LIM", "$LEV"]
        );
        assert_eq!(accepted, vec!["$PINS", "$PINSTATE", "$LIM", "$LEV"]);
        assert_eq!(model.status(), &PinsStatus::Requested);
        assert!(!model.needs_pump());
    }

    #[test]
    fn pin_reports_copy_into_the_app_state_mirrors() {
        let map = sample_map();
        let pin_state = sample_pin_state();
        let limits = sample_limits();
        let events = sample_events();

        let mut state = AppState::new();
        state.begin_live("test transport");
        assert!(state.apply_pin_map(Arc::clone(&map)));
        assert!(state.apply_pin_state(Some(Arc::clone(&pin_state))));
        assert!(state.apply_limit_state(Some(Arc::clone(&limits))));
        assert!(state.apply_last_events(Some(Arc::clone(&events))));

        assert_eq!(state.pins.pin_map().map(<[PinMapping]>::len), Some(2));
        assert_eq!(
            state.pins.pin_map().and_then(|map| map.first()),
            Some(&map[0])
        );
        assert_eq!(
            state.pins.pin_state().map(|state| state.inputs.clone()),
            Some(vec![true, false, true, false])
        );
        assert_eq!(
            state.pins.limit_state().map(|state| state.mask),
            Some(0b101)
        );
        assert_eq!(
            state.pins.last_events().map(|events| events.count),
            Some(17)
        );
        assert_eq!(state.pins.pin_map_report(), ReportState::Reported);
        assert_eq!(state.pins.pin_state_report(), ReportState::Reported);
        assert_eq!(state.pins.limits_report(), ReportState::Reported);
        assert_eq!(state.pins.last_events_report(), ReportState::Reported);

        // A repeated identical value is not reported as a change.
        assert!(!state.apply_pin_map(Arc::clone(&map)));
        assert!(!state.apply_pin_state(Some(Arc::clone(&pin_state))));
        assert!(!state.apply_limit_state(Some(Arc::clone(&limits))));
        assert!(!state.apply_last_events(Some(Arc::clone(&events))));
    }

    #[test]
    fn missing_and_empty_reports_have_explicit_states() {
        let mut model = PinsModel::new();
        assert_eq!(model.pin_map_report(), ReportState::Missing);
        assert_eq!(model.pin_state_report(), ReportState::Missing);
        assert_eq!(model.limits_report(), ReportState::Missing);
        assert_eq!(model.last_events_report(), ReportState::Missing);
        assert!(model.pin_map_note().contains("No $PINS"));
        assert!(model.pin_state_note().contains("No $PINSTATE"));
        assert!(model.limits_note().contains("No $LIM"));
        assert!(model.last_events_note().contains("No $LEV"));

        assert!(model.apply_pin_map(Arc::new(Vec::new())));
        assert!(model.apply_pin_state(Some(Arc::new(PinState::default()))));
        assert!(model.apply_limit_state(Some(Arc::new(LimitState::default()))));
        assert!(model.apply_last_events(Some(Arc::new(LastEvents::default()))));

        assert_eq!(model.pin_map_report(), ReportState::Empty);
        assert_eq!(model.pin_state_report(), ReportState::Empty);
        // A zero `$LIM` mask is a valid all-clear report, not a missing one.
        assert_eq!(model.limits_report(), ReportState::Reported);
        assert_eq!(model.last_events_report(), ReportState::Empty);
        assert_eq!(
            model.pin_map_note(),
            "The controller reported no pin entries."
        );
        assert_eq!(
            model.pin_state_note(),
            "The controller reported no auxiliary pin levels."
        );
        assert_eq!(
            model.last_events_note(),
            "The controller reports no control or limit events yet."
        );
    }

    #[test]
    fn disconnect_clears_live_reports_and_keeps_the_static_map() {
        let mut model = PinsModel::new();
        model.open();
        model.apply_pin_map(sample_map());
        model.apply_pin_state(Some(sample_pin_state()));
        model.apply_limit_state(Some(sample_limits()));
        model.apply_last_events(Some(sample_events()));

        model.on_disconnected();
        assert_eq!(model.pin_map_report(), ReportState::Reported);
        assert_eq!(model.pin_state_report(), ReportState::Missing);
        assert_eq!(model.limits_report(), ReportState::Missing);
        assert_eq!(model.last_events_report(), ReportState::Missing);
        assert!(matches!(model.status(), PinsStatus::Stopped(_)));

        // A new session clears the static map too.
        model.reset_session();
        assert_eq!(model.pin_map_report(), ReportState::Missing);
        assert_eq!(model.status(), &PinsStatus::Idle);
    }

    #[test]
    fn diagnostics_never_produce_writes_or_motion() {
        let mut model = PinsModel::new();
        model.open();
        model.begin_refresh(ready_gate()).expect("ready gate");
        let mut commands = Vec::new();
        model.pump(Instant::now(), ready_gate(), |command| {
            commands.push(command);
            Ok(())
        });

        for command in &commands {
            assert!(
                matches!(
                    command,
                    Command::System(SystemCommand::Extended(
                        ExtendedCommand::Pins
                            | ExtendedCommand::PinState
                            | ExtendedCommand::Limits
                            | ExtendedCommand::Events
                    ))
                ),
                "diagnostics sent a non-read-only command: {command:?}"
            );
            assert!(!matches!(
                command,
                Command::Mdi(_)
                    | Command::Jog(_)
                    | Command::SetSetting { .. }
                    | Command::Home
                    | Command::Unlock
                    | Command::Reset
                    | Command::Sleep
                    | Command::SetFeedOverride(_)
                    | Command::SetSpindleOverride(_)
                    | Command::SetRapidOverride(_)
                    | Command::SetOptionalStop(_)
                    | Command::SetSingleBlock(_)
                    | Command::SetBlockDelete(_)
            ));
        }
    }

    #[test]
    fn opening_and_closing_the_panel_send_nothing_until_refresh() {
        let mut model = PinsModel::new();
        let mut sent = 0;

        model.open();
        model.pump(Instant::now(), ready_gate(), |_| {
            sent += 1;
            Ok(())
        });
        assert_eq!(sent, 0, "opening the sheet must not send a command");

        // Queue a refresh, then close before the pump. Closing drops the queue.
        model.begin_refresh(ready_gate()).expect("ready gate");
        model.close();
        model.pump(Instant::now(), ready_gate(), |_| {
            sent += 1;
            Ok(())
        });
        assert_eq!(sent, 0, "closing the sheet must not send a command");

        // Only the explicit REFRESH action sends the four reports.
        model.open();
        model.begin_refresh(ready_gate()).expect("ready gate");
        model.pump(Instant::now(), ready_gate(), |_| {
            sent += 1;
            Ok(())
        });
        assert_eq!(sent, 4);
    }

    #[test]
    fn a_running_job_refuses_refresh_without_sending() {
        let gate = PinsGate {
            job_active: true,
            ..ready_gate()
        };
        let mut model = PinsModel::new();
        model.open();
        assert_eq!(model.begin_refresh(gate), Err(PinsRefusal::JobActive));
        assert!(matches!(model.status(), PinsStatus::Stopped(_)));

        let mut sent = 0;
        model.pump(Instant::now(), gate, |_| {
            sent += 1;
            Ok(())
        });
        assert_eq!(sent, 0);
    }

    #[test]
    fn alarm_stops_a_pending_refresh_and_keeps_the_reason() {
        let mut model = PinsModel::new();
        model.open();
        model.begin_refresh(ready_gate()).expect("ready gate");
        let start = Instant::now();
        model.pump(start, ready_gate(), |_| Err(EngineError::Busy));
        assert_eq!(model.status(), &PinsStatus::Busy);

        model.on_alarm(10);
        assert_eq!(model.status(), &PinsStatus::Stopped("ALARM:10".to_string()));
        assert!(!model.needs_pump());

        let mut sent = 0;
        model.pump(start + PIN_REFRESH_BUSY_RETRY, ready_gate(), |_| {
            sent += 1;
            Ok(())
        });
        assert_eq!(sent, 0);
    }

    #[test]
    fn rendered_pin_levels_are_bounded_and_grouped() {
        let levels = vec![true; MAX_RENDERED_PIN_BITS + 32];
        let text = pin_bits_text(&levels);
        assert!(text.contains("not shown"));
        assert_eq!(text.lines().count(), MAX_RENDERED_PIN_BITS / 32 + 1);
        assert!(text.lines().next().is_some_and(|line| line.len() <= 32 + 3));
        assert_eq!(pin_bits_text(&[]), "(none)");
    }
}
