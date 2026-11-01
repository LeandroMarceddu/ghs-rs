//! View-model state for the touch shell.
//!
//! The shell starts against the in-process demo controller and maps engine
//! telemetry into this plain data model. `--mock` keeps the original mock
//! mutations for layout review. `Profile` is a plain data value: panels and
//! axis rows are derived from `Capabilities`, never from a board name or a
//! compile-time feature. The reference machine is one profile, not a code
//! path; a generic or extended controller can supply a different value without
//! touching this module's consumers.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ghs_core::Units as CoreUnits;
use ghs_engine::{
    Axis as CoreAxis, ControllerInfo, EngineEvent, JobAbortReason, JobInfo, JobPauseReason,
    JobState, MachineSnapshot, Program, ProgramLine as EngineProgramLine, Signals as CoreSignals,
    StatusReport,
};

/// Connection state shown in the top-bar status pill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    /// No engine; the shell mutates mock data only (`--mock`).
    Mock,
    /// A transport is open but the engine handshake is still running.
    Connecting,
    /// The handshake completed and telemetry is live.
    Ready,
    /// The link closed; the last snapshot stays visible but stale.
    Disconnected,
    /// Starting the engine or connecting failed.
    Failed(String),
}

impl ConnectionState {
    /// Short label for the status pill.
    pub fn label(&self) -> &'static str {
        match self {
            ConnectionState::Mock => "MOCK",
            ConnectionState::Connecting => "CONNECTING",
            ConnectionState::Ready => "READY",
            ConnectionState::Disconnected => "DISCONNECTED",
            ConnectionState::Failed(_) => "ERROR",
        }
    }

    /// True when the UI may issue controller commands.
    pub fn is_ready_like(&self) -> bool {
        matches!(self, ConnectionState::Mock | ConnectionState::Ready)
    }
}

/// Top-level workspace selected in the mode bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Job,
    Probe,
    Offset,
    Utility,
    Tools,
    Settings,
}

impl Mode {
    pub const ALL: [Mode; 6] = [
        Mode::Job,
        Mode::Probe,
        Mode::Offset,
        Mode::Utility,
        Mode::Tools,
        Mode::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Job => "Job",
            Mode::Probe => "Probe",
            Mode::Offset => "Offset",
            Mode::Utility => "Utility",
            Mode::Tools => "Tools",
            Mode::Settings => "Settings",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Mode::Job => "JOB",
            Mode::Probe => "PROBING",
            Mode::Offset => "WORK OFFSETS",
            Mode::Utility => "UTILITIES",
            Mode::Tools => "TOOL TABLE",
            Mode::Settings => "SETTINGS",
        }
    }
}

/// Left rail tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeftTab {
    Console,
    Program,
}

impl LeftTab {
    pub fn label(self) -> &'static str {
        match self {
            LeftTab::Console => "Console",
            LeftTab::Program => "Program",
        }
    }
}

/// Axis identity. The domain is 9-axis capable; this mock profile reports XYZ.
/// A capability can report additional axes by constructing `Axis::from_index`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Axis(u8);

impl Axis {
    pub const X: Self = Self(0);
    pub const Y: Self = Self(1);
    pub const Z: Self = Self(2);

    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn label(self) -> &'static str {
        const LABELS: [&str; 9] = ["X", "Y", "Z", "A", "B", "C", "U", "V", "W"];
        LABELS.get(self.index()).copied().unwrap_or("?")
    }

    /// Extension point for profiles that report more than three axes.
    pub fn from_index(index: usize) -> Self {
        Self(index.min(8) as u8)
    }
}

/// Display/protocol unit mode. mm stays canonical; inch is a presentation edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    Millimetre,
    Inch,
}

impl Units {
    pub fn suffix(self) -> &'static str {
        match self {
            Units::Millimetre => "mm",
            Units::Inch => "in",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Units::Millimetre => "Millimetres (G21)",
            Units::Inch => "Inches (G20)",
        }
    }

    /// Convert a millimetre value into this display unit.
    pub fn display(self, value_mm: f64) -> f64 {
        match self {
            Units::Millimetre => value_mm,
            Units::Inch => value_mm / ghs_core::Units::MM_PER_INCH,
        }
    }

    /// Convert a value in this display unit back to millimetres.
    pub fn to_mm(self, value: f64) -> f64 {
        match self {
            Units::Millimetre => value,
            Units::Inch => value * ghs_core::Units::MM_PER_INCH,
        }
    }

    /// Map the engine's unit enum into the display enum.
    pub fn from_core(units: CoreUnits) -> Self {
        match units {
            CoreUnits::Mm => Units::Millimetre,
            CoreUnits::Inch => Units::Inch,
        }
    }
}

/// Controller capabilities. Every field is data; no panel branches on a board.
#[derive(Clone, Debug)]
pub struct Capabilities {
    pub axes: Vec<Axis>,
    pub units: Units,
    pub has_probe: bool,
    pub has_tool_table: bool,
    pub tool_table_entries: usize,
    pub has_sd: bool,
    pub has_atc: bool,
    pub has_mpg: bool,
    pub multi_spindle: bool,
    pub coolant_mist: bool,
    pub coolant_flood: bool,
    pub coolant_fan: bool,
    pub limit_pins: bool,
    pub has_estop: bool,
    pub has_door: bool,
}

impl Capabilities {
    /// The reference FlexiHAL-shaped mock. Values are data, not identity.
    pub fn mock_reference() -> Self {
        Self {
            axes: (0..3).map(Axis::from_index).collect(),
            units: Units::Millimetre,
            has_probe: true,
            has_tool_table: true,
            tool_table_entries: 0,
            has_sd: false,
            has_atc: false,
            has_mpg: false,
            multi_spindle: false,
            coolant_mist: true,
            coolant_flood: true,
            coolant_fan: true,
            limit_pins: true,
            has_estop: true,
            has_door: true,
        }
    }

    /// Neutral capability set used while the handshake is still running.
    pub fn live_pending() -> Self {
        Self {
            axes: (0..3).map(Axis::from_index).collect(),
            units: Units::Millimetre,
            has_probe: false,
            has_tool_table: false,
            tool_table_entries: 0,
            has_sd: false,
            has_atc: false,
            has_mpg: false,
            multi_spindle: false,
            // grblHAL has no capability flag for the three classic coolant
            // outputs; keep them visible and let the `A:` field report state.
            coolant_mist: true,
            coolant_flood: true,
            coolant_fan: true,
            limit_pins: false,
            has_estop: false,
            has_door: false,
        }
    }

    /// Build capabilities from an engine `$I+` record.
    ///
    /// Every decision is made from reported signals and options. The board
    /// name is never inspected.
    pub fn from_info(info: &ControllerInfo, units: Units) -> Self {
        let axes: Vec<Axis> = if info.axes.is_empty() {
            (0..3).map(Axis::from_index).collect()
        } else {
            info.axis_list()
                .into_iter()
                .map(|axis| Axis::from_index(axis.index()))
                .collect()
        };
        let limit_pins = info.has_signal(CoreSignals::X_LIMIT)
            || info.has_signal(CoreSignals::Y_LIMIT)
            || info.has_signal(CoreSignals::Z_LIMIT);
        let has_probe = info.has_signal(CoreSignals::PROBE)
            || info.has_signal(CoreSignals::PROBE_PRIMARY)
            || info.has_signal(CoreSignals::TOOL_SETTER)
            || info.has_signal(CoreSignals::PROBE_SECONDARY)
            || info.has_probe_connected_signal();
        let multi_spindle = info
            .new_options
            .iter()
            .find_map(|option| option.strip_prefix("SPINDLES="))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .is_some_and(|count| count > 1);
        let tool_table_entries = info.tool_table_entries.unwrap_or(0);
        Self {
            axes,
            units,
            has_probe,
            has_tool_table: tool_table_entries > 0,
            tool_table_entries,
            has_sd: info.has_sd(),
            has_atc: info.has_atc(),
            has_mpg: info.has_mpg(),
            multi_spindle,
            coolant_mist: true,
            coolant_flood: true,
            coolant_fan: true,
            limit_pins,
            has_estop: info.has_signal(CoreSignals::ESTOP),
            has_door: info.has_signal(CoreSignals::DOOR),
        }
    }

    /// Coolant channels exposed by this capability set, with their UI slot.
    pub fn coolant_channels(&self) -> Vec<(&'static str, usize)> {
        let mut out = Vec::new();
        if self.coolant_mist {
            out.push(("Mist", 0));
        }
        if self.coolant_flood {
            out.push(("Flood", 1));
        }
        if self.coolant_fan {
            out.push(("Fan", 2));
        }
        out
    }

    /// Post-v1.0 panels exposed by this profile. The touch shell only reports
    /// them; each becomes a real view behind its own capability at M7+.
    pub fn accessory_panels(&self) -> [(&'static str, bool); 3] {
        [
            ("SD card", self.has_sd),
            ("ATC", self.has_atc),
            ("MPG", self.has_mpg),
        ]
    }
}

/// One controller identity plus its capabilities.
#[derive(Clone, Debug)]
pub struct Profile {
    pub name: String,
    pub capabilities: Capabilities,
}

impl Profile {
    pub fn mock_reference() -> Self {
        Self {
            name: "Mock 3-axis mill".to_string(),
            capabilities: Capabilities::mock_reference(),
        }
    }

    pub fn live_pending() -> Self {
        Self {
            name: "Controller (connecting)".to_string(),
            capabilities: Capabilities::live_pending(),
        }
    }

    pub fn live(info: &ControllerInfo, units: Units) -> Self {
        let board = info
            .board
            .clone()
            .or_else(|| info.firmware.clone())
            .unwrap_or_else(|| "grblHAL controller".to_string());
        let version = if info.version.is_empty() {
            String::new()
        } else {
            format!(" {}", info.version)
        };
        Self {
            name: format!("{board}{version}"),
            capabilities: Capabilities::from_info(info, units),
        }
    }
}

/// Keypad owner. Tapping a DRO value, the MDI field, or RPM opens one sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeypadTarget {
    Dro(Axis),
    Mdi,
    Rpm,
}

impl KeypadTarget {
    pub fn title(self) -> &'static str {
        match self {
            KeypadTarget::Dro(axis) => match axis.index() {
                0 => "SET X",
                1 => "SET Y",
                2 => "SET Z",
                _ => "SET AXIS",
            },
            KeypadTarget::Mdi => "MDI TEXT",
            KeypadTarget::Rpm => "SPINDLE RPM",
        }
    }
}

/// One keypad key. Keeping the action in state lets the overlay stay dumb.
#[derive(Clone, Copy, Debug)]
pub enum KeypadAction {
    Digit(char),
    Dot,
    Minus,
    Backspace,
    Clear,
    Token(&'static str),
    Submit,
    Cancel,
}

#[derive(Clone, Debug)]
pub struct KeypadState {
    pub target: KeypadTarget,
    pub buffer: String,
    /// The first digit replaces the current value instead of appending.
    pub fresh: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleLevel {
    Info,
    Command,
    Ok,
    Warn,
    Error,
}

#[derive(Clone, Debug)]
pub struct ConsoleLine {
    pub time: String,
    pub level: ConsoleLevel,
    pub text: String,
}

impl ConsoleLine {
    fn new(level: ConsoleLevel, text: impl Into<String>) -> Self {
        Self {
            time: now_hhmm(),
            level,
            text: text.into(),
        }
    }
}

/// Wall-clock `HH:MM` for a console line.
///
/// The shell has no clock service yet. UTC from the system clock is enough for
/// ordering console output and avoids a timezone dependency in the binary.
fn now_hhmm() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() % 86_400)
        .unwrap_or(0);
    format!("{:02}:{:02}", seconds / 3_600, (seconds % 3_600) / 60)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramKind {
    Comment,
    Setup,
    Rapid,
    Cut,
    Plunge,
}

#[derive(Clone, Debug)]
pub struct ProgramLine {
    pub number: usize,
    pub text: &'static str,
    pub kind: ProgramKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroAction {
    ZeroXy,
    ZeroZ,
    SafeZ,
    Home,
    Unlock,
    Park,
    ProbeZ,
    Coolant,
}

#[derive(Clone, Copy, Debug)]
pub struct Macro {
    pub label: &'static str,
    pub action: MacroAction,
}

fn mac(label: &'static str, action: MacroAction) -> Macro {
    Macro { label, action }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalState {
    Off,
    Ok,
    Warn,
    Alert,
}

#[derive(Clone, Debug)]
pub struct Signal {
    pub label: String,
    pub state: SignalState,
}

const WCS: [&str; 9] = [
    "G54", "G55", "G56", "G57", "G58", "G59", "G59.1", "G59.2", "G59.3",
];
const PROGRAM: [(&str, ProgramKind); 30] = [
    ("(Shop sign - 6 mm pocket)", ProgramKind::Comment),
    ("G21 G90 G54", ProgramKind::Setup),
    ("G17 G94", ProgramKind::Setup),
    ("M3 S12000", ProgramKind::Setup),
    ("G0 Z5.000", ProgramKind::Rapid),
    ("G0 X18.000 Y18.000", ProgramKind::Rapid),
    ("G1 Z-1.500 F300", ProgramKind::Plunge),
    ("G1 X98.000 F1200", ProgramKind::Cut),
    ("G3 X112.000 Y32.000 I0 J14.000", ProgramKind::Cut),
    ("G1 Y78.000", ProgramKind::Cut),
    ("G3 X98.000 Y92.000 I-14.000 J0", ProgramKind::Cut),
    ("G1 X42.000", ProgramKind::Cut),
    ("G2 X28.000 Y78.000 I0 J-14.000", ProgramKind::Cut),
    ("G1 Y46.000", ProgramKind::Cut),
    ("G2 X42.000 Y32.000 I14.000 J0", ProgramKind::Cut),
    ("G1 X18.000", ProgramKind::Cut),
    ("G0 Z5.000", ProgramKind::Rapid),
    ("G0 X62.000 Y62.000", ProgramKind::Rapid),
    ("G1 Z-1.200 F250", ProgramKind::Plunge),
    ("G2 X62.000 Y62.000 I6.000 J0 F900", ProgramKind::Cut),
    ("G0 Z5.000", ProgramKind::Rapid),
    ("G0 X18.000 Y18.000", ProgramKind::Rapid),
    ("M5", ProgramKind::Setup),
    ("G0 Z25.000", ProgramKind::Rapid),
    ("G0 X0 Y0", ProgramKind::Rapid),
    ("M30", ProgramKind::Setup),
    ("(Stock 130 x 110 x 12 mm)", ProgramKind::Comment),
    ("(Origin: front-left, top of stock)", ProgramKind::Comment),
    ("(Tool 3: 6 mm flat end mill)", ProgramKind::Comment),
    ("(Line 13 is being streamed)", ProgramKind::Comment),
];

/// A pending manual tool change shown in the modal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolChangeView {
    /// Requested tool number.
    pub tool: u32,
    /// 1-based source line carrying the `M6`.
    pub line: u32,
    /// Whether `0xA3` was requested. Resume stays locked until this is true.
    pub acknowledged: bool,
}

/// Live job-runner view model.
///
/// Plain data updated from [`EngineEvent`] job events and
/// [`ghs_engine::MachineSnapshot::job`]. It holds the parsed program so the
/// Program tab can render real source lines without copying them. The shell
/// never derives motion from this type: every action is a typed
/// [`ghs_engine::Command`].
#[derive(Clone, Debug, Default)]
pub struct JobView {
    program: Option<Arc<Program>>,
    /// Program name supplied with `LoadProgram`.
    pub name: String,
    /// Job lifecycle state.
    pub state: JobState,
    /// Current 1-based source line; zero before the first line is resolved.
    pub current_line: u32,
    /// Source lines resolved so far (acknowledged or skipped).
    pub completed: u32,
    /// Source lines in this run, from the engine's parsed program.
    pub total: u32,
    /// Active job time; paused intervals are excluded.
    pub elapsed: Duration,
    /// Why the job paused, when the engine reported a reason.
    pub pause_reason: Option<JobPauseReason>,
    /// Pending manual tool change.
    pub tool_change: Option<ToolChangeView>,
}

impl JobView {
    /// Parsed program installed by `JobLoaded`.
    pub fn program(&self) -> Option<&Arc<Program>> {
        self.program.as_ref()
    }

    /// Whether a program is installed.
    pub fn has_program(&self) -> bool {
        self.program.is_some()
    }

    /// Number of parsed source lines, including lines after `M30`.
    pub fn line_count(&self) -> usize {
        self.program
            .as_ref()
            .map_or(0, |program| program.lines.len())
    }

    /// One parsed source line by zero-based index.
    pub fn line(&self, index: usize) -> Option<&EngineProgramLine> {
        self.program.as_ref()?.lines.get(index)
    }

    /// Source text for a 1-based line number.
    pub fn line_source(&self, line: u32) -> Option<&str> {
        let index = usize::try_from(line).ok()?.checked_sub(1)?;
        self.line(index).map(|line| line.source.as_str())
    }

    /// Resolved fraction of the streamed lines, clamped to `0.0..=1.0`.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        (self.completed.min(self.total) as f32 / self.total as f32).clamp(0.0, 1.0)
    }

    /// Whether the runner is feeding or ready to feed lines.
    pub fn is_active(&self) -> bool {
        matches!(self.state, JobState::Running | JobState::Paused)
    }

    /// Whether the runner is feeding lines.
    pub fn is_running(&self) -> bool {
        self.state == JobState::Running
    }

    /// Whether the runner is paused, including a pending tool change.
    pub fn is_paused(&self) -> bool {
        self.state == JobState::Paused
    }

    /// Whether a tool change awaits operator action.
    pub fn tool_change_pending(&self) -> bool {
        self.tool_change.is_some()
    }

    /// Whether the tool change was acknowledged with `0xA3`.
    pub fn tool_change_acknowledged(&self) -> bool {
        self.tool_change.is_some_and(|change| change.acknowledged)
    }

    /// Short status label for the job toolbar.
    pub fn status_label(&self) -> &'static str {
        if self.tool_change.is_some() {
            return "TOOL CHANGE";
        }
        if self.state == JobState::Idle {
            return if self.has_program() {
                "LOADED"
            } else {
                "NO PROGRAM"
            };
        }
        job_state_label(self.state)
    }

    /// Installs a parsed program from `JobLoaded`.
    pub fn loaded(&mut self, name: String, program: Arc<Program>, total: u32) {
        self.program = Some(program);
        self.name = name;
        self.state = JobState::Idle;
        self.current_line = 0;
        self.completed = 0;
        self.total = total;
        self.elapsed = Duration::ZERO;
        self.pause_reason = None;
        self.tool_change = None;
    }

    /// Records `JobStarted`.
    pub fn started(&mut self, from_line: u32, total: u32) {
        self.state = JobState::Running;
        self.current_line = from_line;
        self.completed = 0;
        if total > 0 {
            self.total = total;
        }
        self.elapsed = Duration::ZERO;
        self.pause_reason = None;
        self.tool_change = None;
    }

    /// Records `JobProgress`.
    pub fn progress(&mut self, current_line: u32, completed: u32, total: u32, elapsed: Duration) {
        self.current_line = current_line;
        self.completed = completed;
        if total > 0 {
            self.total = total;
        }
        self.elapsed = elapsed;
    }

    /// Records `JobPaused`.
    pub fn paused(&mut self, line: u32, reason: JobPauseReason) {
        self.state = JobState::Paused;
        if line > 0 {
            self.current_line = line;
        }
        self.pause_reason = Some(reason);
        self.tool_change = None;
    }

    /// Records `JobResumed`.
    pub fn resumed(&mut self) {
        self.state = JobState::Running;
        self.pause_reason = None;
        self.tool_change = None;
    }

    /// Records `JobCompleted`.
    pub fn completed(&mut self, elapsed: Duration) {
        self.state = JobState::Completed;
        self.elapsed = elapsed;
        self.completed = self.total;
        if self.total > 0 {
            self.current_line = self.total;
        }
        self.pause_reason = None;
        self.tool_change = None;
    }

    /// Records `JobAborted`.
    pub fn aborted(&mut self, line: u32, _reason: JobAbortReason) {
        self.state = JobState::Aborted;
        if line > 0 {
            self.current_line = line;
        }
        self.pause_reason = None;
        self.tool_change = None;
    }

    /// Records `JobError`. The controller rejected the line, so the runner is
    /// aborted even before the next snapshot confirms it.
    pub fn job_error(&mut self, line: u32) {
        if line > 0 {
            self.current_line = line;
        }
        self.state = JobState::Aborted;
        self.pause_reason = None;
        self.tool_change = None;
    }

    /// Marks the job interrupted after the link closed.
    ///
    /// The engine normally emits `JobAborted { reason: Disconnected }` first;
    /// this also clears a tool-change sheet if that event was dropped.
    pub fn interrupted(&mut self) {
        if self.has_program() {
            self.state = JobState::Interrupted;
            self.pause_reason = None;
            self.tool_change = None;
        }
    }

    /// Records `ToolChangeRequired`.
    pub fn tool_change_required(&mut self, tool: u32, line: u32) {
        self.state = JobState::Paused;
        if line > 0 {
            self.current_line = line;
        }
        self.pause_reason = None;
        self.tool_change = Some(ToolChangeView {
            tool,
            line,
            acknowledged: false,
        });
    }

    /// Marks the pending tool change acknowledged after `0xA3` was accepted.
    ///
    /// Returns `false` when no tool change is pending.
    pub fn acknowledge_tool_change(&mut self) -> bool {
        let Some(change) = self.tool_change.as_mut() else {
            return false;
        };
        change.acknowledged = true;
        true
    }

    /// Merges `MachineSnapshot.job`.
    ///
    /// Events carry the authoritative pause reason and tool change; the
    /// snapshot is a cross-check that also recovers a missed event. A snapshot
    /// that says the job is no longer paused clears a stale tool change.
    pub fn apply_snapshot(&mut self, info: &JobInfo) -> bool {
        let mut changed = false;
        if let Some(name) = &info.name
            && *name != self.name
        {
            self.name.clone_from(name);
            changed = true;
        }
        if info.total_lines > 0 {
            if info.total_lines != self.total {
                self.total = info.total_lines;
                changed = true;
            }
            if info.completed_lines != self.completed {
                self.completed = info.completed_lines;
                changed = true;
            }
            if let Some(line) = info.line
                && line != self.current_line
            {
                self.current_line = line;
                changed = true;
            }
        }
        if info.state != self.state {
            self.state = info.state;
            changed = true;
        }
        if !matches!(info.state, JobState::Paused) {
            if self.pause_reason.take().is_some() {
                changed = true;
            }
            if self.tool_change.take().is_some() {
                changed = true;
            }
        }
        changed
    }

    /// Applies one engine event. Non-job variants leave the view unchanged.
    ///
    /// Kept next to the setters so the job state machine is testable without
    /// an engine thread. Console text stays in `live.rs`.
    pub fn apply_event(&mut self, event: &EngineEvent) -> bool {
        match event {
            EngineEvent::JobLoaded {
                name,
                total_lines,
                program,
            } => {
                self.loaded(name.clone(), Arc::clone(program), *total_lines);
                true
            }
            EngineEvent::JobStarted {
                from_line,
                total_lines,
            } => {
                self.started(*from_line, *total_lines);
                true
            }
            EngineEvent::JobProgress {
                current_line,
                completed,
                total,
                elapsed,
            } => {
                self.progress(*current_line, *completed, *total, *elapsed);
                true
            }
            EngineEvent::JobPaused { line, reason } => {
                self.paused(*line, *reason);
                true
            }
            EngineEvent::JobResumed => {
                self.resumed();
                true
            }
            EngineEvent::JobCompleted { elapsed } => {
                self.completed(*elapsed);
                true
            }
            EngineEvent::JobAborted { line, reason } => {
                self.aborted(*line, *reason);
                true
            }
            EngineEvent::JobError { line, .. } => {
                self.job_error(*line);
                true
            }
            EngineEvent::ToolChangeRequired { tool, line } => {
                self.tool_change_required(*tool, *line);
                true
            }
            _ => false,
        }
    }
}

/// Short label for a job lifecycle state.
pub fn job_state_label(state: JobState) -> &'static str {
    match state {
        JobState::Idle => "IDLE",
        JobState::Running => "RUNNING",
        JobState::Paused => "PAUSED",
        JobState::Completed => "COMPLETED",
        JobState::Aborted => "ABORTED",
        JobState::Interrupted => "INTERRUPTED",
    }
}

/// Short label for a pause reason.
pub fn pause_reason_label(reason: JobPauseReason) -> &'static str {
    match reason {
        JobPauseReason::Operator => "operator",
        JobPauseReason::FeedHold => "feed hold",
        JobPauseReason::ProgramStop => "program stop (M0)",
        JobPauseReason::OptionalStop => "optional stop (M1)",
        JobPauseReason::SingleBlock => "single block",
        JobPauseReason::Breakpoint => "breakpoint",
        JobPauseReason::Watchdog => "status watchdog",
    }
}

/// Short label for an abort reason.
pub fn abort_reason_label(reason: JobAbortReason) -> &'static str {
    match reason {
        JobAbortReason::OperatorStop => "operator stop",
        JobAbortReason::Alarm => "alarm",
        JobAbortReason::Door => "safety door",
        JobAbortReason::Disconnected => "disconnected",
        JobAbortReason::SoftReset => "soft reset",
        JobAbortReason::EmergencyStop => "E-stop",
        JobAbortReason::ProgramError => "program error",
        JobAbortReason::ControllerSleep => "controller sleep",
    }
}

/// `MM:SS` (or `H:MM:SS` past an hour) for job time.
pub fn format_elapsed(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// Everything the shell draws. Plain data, no GPUI types.
pub struct AppState {
    pub profile: Profile,
    pub mode: Mode,
    pub left_tab: LeftTab,
    pub dark: bool,
    pub connection: ConnectionState,
    pub live: bool,
    pub machine_state: &'static str,
    /// Engine job runner state. Empty in `--mock` mode.
    pub job: JobView,
    pub dro: Vec<f64>,
    pub mpos: Vec<f64>,
    pub wco: Vec<f64>,
    pub dtg: Vec<f64>,
    pub wcs_index: usize,
    pub jog_axis: Axis,
    pub jog_step: f64,
    pub continuous: bool,
    pub feed_override: u16,
    pub spindle_override: u16,
    pub rapid_override: u16,
    pub feed_override_active: bool,
    pub spindle_override_active: bool,
    pub spindle_on: bool,
    pub spindle_cw: bool,
    pub rpm: u32,
    pub spindle_actual_rpm: f64,
    pub tool: Option<u32>,
    pub tlr: Option<bool>,
    pub sd: Option<String>,
    pub coolant: [bool; 3],
    pub console: Vec<ConsoleLine>,
    pub program: Vec<ProgramLine>,
    pub active_line: usize,
    pub progress: f32,
    pub keypad: Option<KeypadState>,
    pub mdi: String,
    pub macros: Vec<Macro>,
    pub signals: Vec<Signal>,
    pub signal_caps: String,
    pub feed_rate: u32,
}

impl AppState {
    pub fn new() -> Self {
        let profile = Profile::mock_reference();
        let dro = initial_dro(&profile.capabilities.axes);
        let signals = build_signals(&profile.capabilities, "Idle", CoreSignals::NONE, false);
        let axes = profile.capabilities.axes.len();
        Self {
            profile,
            mode: Mode::Job,
            left_tab: LeftTab::Console,
            dark: true,
            connection: ConnectionState::Mock,
            live: false,
            machine_state: "Idle",
            job: JobView::default(),
            dro: dro.clone(),
            mpos: dro.clone(),
            wco: vec![0.0; axes],
            dtg: vec![0.0; axes],
            wcs_index: 0,
            jog_axis: Axis::X,
            jog_step: 1.0,
            continuous: false,
            feed_override: 100,
            spindle_override: 100,
            rapid_override: 100,
            feed_override_active: false,
            spindle_override_active: false,
            spindle_on: true,
            spindle_cw: true,
            rpm: 12_000,
            spindle_actual_rpm: 0.0,
            tool: Some(3),
            tlr: None,
            sd: None,
            coolant: [false, true, false],
            console: seed_console(),
            program: PROGRAM
                .iter()
                .enumerate()
                .map(|(i, (text, kind))| ProgramLine {
                    number: i + 1,
                    text,
                    kind: *kind,
                })
                .collect(),
            active_line: 13,
            progress: 0.42,
            keypad: None,
            mdi: "G0 X10 Y10".to_string(),
            macros: vec![
                mac("Zero XY", MacroAction::ZeroXy),
                mac("Zero Z", MacroAction::ZeroZ),
                mac("Safe Z", MacroAction::SafeZ),
                mac("Home", MacroAction::Home),
                mac("Unlock", MacroAction::Unlock),
                mac("Park", MacroAction::Park),
                mac("Probe Z", MacroAction::ProbeZ),
                mac("M8/M9", MacroAction::Coolant),
            ],
            signals,
            signal_caps: "HSDEP".to_string(),
            feed_rate: 1200,
        }
    }

    /// Switch to the live engine view model.
    ///
    /// The first snapshot replaces every value below; clearing the seeded mock
    /// console makes the controller handshake and real acknowledgements the
    /// only console history. `message` names the transport that is opening.
    pub fn begin_live(&mut self, message: impl Into<String>) {
        self.live = true;
        self.connection = ConnectionState::Connecting;
        self.profile = Profile::live_pending();
        self.machine_state = "Unknown";
        // A fresh session starts without a program. Demo mode reloads its
        // bundled source once the controller is ready; a serial reconnect has
        // to send its program again (there is no program-read command).
        self.job = JobView::default();
        self.active_line = 0;
        self.progress = 0.0;
        self.console.clear();
        self.mpos = vec![0.0; self.profile.capabilities.axes.len()];
        self.dro = vec![0.0; self.profile.capabilities.axes.len()];
        self.wco = vec![0.0; self.profile.capabilities.axes.len()];
        self.dtg = vec![0.0; self.profile.capabilities.axes.len()];
        self.wcs_index = 0;
        self.feed_override = 100;
        self.spindle_override = 100;
        self.rapid_override = 100;
        self.feed_override_active = false;
        self.spindle_override_active = false;
        self.spindle_on = false;
        self.spindle_cw = true;
        self.rpm = 0;
        self.spindle_actual_rpm = 0.0;
        self.tool = None;
        self.tlr = None;
        self.sd = None;
        self.coolant = [false; 3];
        self.feed_rate = 0;
        self.signals.clear();
        self.signal_caps.clear();
        self.push_console(ConsoleLevel::Info, message);
    }

    /// Rebuild capabilities and axis rows from the engine's `$I+` record.
    pub fn apply_controller_info(&mut self, info: &ControllerInfo) {
        let units = self.profile.capabilities.units;
        self.profile = Profile::live(info, units);
        let axis_count = self.profile.capabilities.axes.len();
        self.mpos.resize(axis_count, 0.0);
        self.dro.resize(axis_count, 0.0);
        self.wco.resize(axis_count, 0.0);
        self.dtg.resize(axis_count, 0.0);
        if !self.profile.capabilities.axes.contains(&self.jog_axis) {
            self.jog_axis = self
                .profile
                .capabilities
                .axes
                .first()
                .copied()
                .unwrap_or(Axis::X);
        }
        self.signal_caps = info.signals.to_string();
        self.signals = build_signals(
            &self.profile.capabilities,
            self.machine_state,
            CoreSignals::NONE,
            true,
        );
    }

    /// Apply one engine snapshot plus the raw status fields that are not in
    /// [`MachineSnapshot`].
    ///
    /// Positions arrive in millimetres and are converted at this display edge.
    pub fn apply_snapshot(&mut self, snapshot: &MachineSnapshot, status: Option<&StatusReport>) {
        let units = self.profile.capabilities.units;
        let axis_count = self.profile.capabilities.axes.len();
        self.mpos.resize(axis_count, 0.0);
        self.dro.resize(axis_count, 0.0);
        self.wco.resize(axis_count, 0.0);
        self.dtg.resize(axis_count, 0.0);

        for (index, app_axis) in self.profile.capabilities.axes.iter().enumerate() {
            let Some(core_axis) = CoreAxis::from_index(app_axis.index()) else {
                continue;
            };
            let work = if snapshot.wpos.axes().contains(core_axis) {
                snapshot.wpos.get(core_axis)
            } else {
                snapshot.mpos.get(core_axis) - snapshot.wco.get(core_axis)
            };
            let machine = snapshot.mpos.get(core_axis);
            let offset = snapshot.wco.get(core_axis);
            if let Some(slot) = self.dro.get_mut(index) {
                *slot = units.display(work);
            }
            if let Some(slot) = self.mpos.get_mut(index) {
                *slot = units.display(machine);
            }
            if let Some(slot) = self.wco.get_mut(index) {
                *slot = units.display(offset);
            }
        }

        self.machine_state = snapshot.state.as_str();
        self.wcs_index = snapshot.wcs.index();
        if !self.feed_override_active {
            self.feed_override = snapshot.overrides.feed;
        }
        if !self.spindle_override_active {
            self.spindle_override = snapshot.overrides.spindle;
        }
        self.rapid_override = snapshot.overrides.rapid;
        self.feed_rate = snapshot.feed_spindle.feed.max(0.0).round() as u32;
        self.spindle_on = snapshot.feed_spindle.spindle_on;
        self.spindle_cw = snapshot.feed_spindle.clockwise;
        if snapshot.feed_spindle.spindle_rpm > 0.0 || snapshot.feed_spindle.spindle_on {
            self.rpm = snapshot.feed_spindle.spindle_rpm.max(0.0).round() as u32;
        }
        self.signals = build_signals(
            &self.profile.capabilities,
            self.machine_state,
            snapshot.signals,
            true,
        );

        if let Some(status) = status {
            if let Some(accessory) = &status.accessory {
                self.coolant[0] = accessory.mist;
                self.coolant[1] = accessory.flood;
            }
            if let Some(tool) = status.tool {
                self.tool = Some(tool);
            }
            if let Some(tlr) = status.tlo_reference_set {
                self.tlr = Some(tlr);
            }
            if let Some(actual) = status.spindle_actual_rpm {
                self.spindle_actual_rpm = actual;
            }
            if let Some(dtg) = status.dtg {
                for (index, app_axis) in self.profile.capabilities.axes.iter().enumerate() {
                    if let Some(core_axis) = CoreAxis::from_index(app_axis.index())
                        && let Some(slot) = self.dtg.get_mut(index)
                    {
                        *slot = units.display(dtg.get(core_axis));
                    }
                }
            }
            if let Some(sd) = &status.sd {
                self.sd = Some(format!("{sd:?}"));
            }
        } else {
            self.dtg.fill(0.0);
        }

        self.job.apply_snapshot(&snapshot.job);
    }

    /// Program name shown in the viewer header.
    pub fn job_title(&self) -> &str {
        if !self.job.name.is_empty() {
            self.job.name.as_str()
        } else if self.live {
            "No program loaded"
        } else {
            "shop-sign-pocket.nc"
        }
    }

    /// Lines parsed in the Program tab: the engine program or the mock list.
    pub fn display_program_lines(&self) -> usize {
        if self.job.has_program() {
            self.job.line_count()
        } else if self.live {
            0
        } else {
            self.program.len()
        }
    }

    /// Current 1-based line for the Program tab and header counter.
    pub fn display_line(&self) -> u32 {
        if self.job.has_program() {
            self.job.current_line
        } else if self.live {
            0
        } else {
            self.active_line as u32
        }
    }

    /// Streamed line count for the header counter.
    pub fn display_total(&self) -> u32 {
        if self.job.has_program() {
            self.job.total
        } else if self.live {
            0
        } else {
            self.program.len() as u32
        }
    }

    /// Progress fraction for the header bar.
    pub fn display_progress(&self) -> f32 {
        if self.job.has_program() {
            self.job.fraction()
        } else if self.live {
            0.0
        } else {
            self.progress
        }
    }

    /// Job status label, including the mock program's "loaded" state.
    pub fn display_job_status(&self) -> &'static str {
        if !self.live && !self.job.has_program() && !self.program.is_empty() {
            return match self.job.state {
                JobState::Idle => "LOADED",
                JobState::Running => "RUNNING",
                JobState::Paused => "PAUSED",
                other => job_state_label(other),
            };
        }
        self.job.status_label()
    }

    pub fn axes(&self) -> &[Axis] {
        &self.profile.capabilities.axes
    }

    pub fn unit(&self) -> &'static str {
        self.profile.capabilities.units.suffix()
    }

    pub fn value(&self, axis: Axis) -> f64 {
        self.dro.get(axis.index()).copied().unwrap_or(0.0)
    }

    /// Work-coordinate offset for one axis, in display units.
    pub fn offset(&self, axis: Axis) -> f64 {
        self.wco.get(axis.index()).copied().unwrap_or(0.0)
    }

    /// One-based `P` word for the active WCS (`G54` -> `P1`).
    pub fn wcs_program_number(&self) -> usize {
        self.wcs_index + 1
    }

    /// Apply a unit-mode change from the parser state.
    pub fn set_units(&mut self, units: CoreUnits) {
        self.profile.capabilities.units = Units::from_core(units);
    }

    fn set_value(&mut self, axis: Axis, value: f64) {
        if let Some(slot) = self.dro.get_mut(axis.index()) {
            *slot = value.clamp(-9999.0, 9999.0);
        }
    }

    pub fn wcs(&self) -> &'static str {
        WCS[self.wcs_index.min(WCS.len() - 1)]
    }

    pub fn wcs_at(index: usize) -> &'static str {
        WCS[index.min(WCS.len() - 1)]
    }

    pub fn wcs_count() -> usize {
        WCS.len()
    }

    pub fn zero_axis(&mut self, axis: Axis) {
        self.set_value(axis, 0.0);
        self.push_console(
            ConsoleLevel::Command,
            format!(
                "G10 L20 P{} {}0.000 (mock)",
                self.wcs_program_number(),
                axis.label()
            ),
        );
    }

    pub fn jog(&mut self, sign: f64) {
        let distance = if self.continuous {
            self.jog_step.max(1.0)
        } else {
            self.jog_step
        };
        let axis = self.jog_axis;
        let next = self.value(axis) + sign * distance;
        self.set_value(axis, next);
        self.push_console(
            ConsoleLevel::Command,
            format!(
                "$J=G91 {}{:.3} F{:.0} (mock)",
                axis.label(),
                sign * distance,
                self.feed_rate
            ),
        );
    }

    pub fn toggle_coolant(&mut self, index: usize) {
        let Some(value) = self.coolant.get_mut(index) else {
            return;
        };
        *value = !*value;
        let enabled = *value;
        let name = ["Mist", "Flood", "Fan"]
            .get(index)
            .copied()
            .unwrap_or("AUX");
        self.push_console(
            ConsoleLevel::Command,
            format!("{} {} (mock)", name, if enabled { "on" } else { "off" }),
        );
    }

    pub fn run_macro(&mut self, index: usize) {
        let Some(mac) = self.macros.get(index).copied() else {
            return;
        };
        self.push_console(ConsoleLevel::Command, format!("Macro: {}", mac.label));
        match mac.action {
            MacroAction::ZeroXy => {
                self.set_value(Axis::X, 0.0);
                self.set_value(Axis::Y, 0.0);
            }
            MacroAction::ZeroZ => self.set_value(Axis::Z, 0.0),
            MacroAction::SafeZ => self.set_value(Axis::Z, 25.0),
            MacroAction::Home => {
                for axis in self.profile.capabilities.axes.clone() {
                    self.set_value(axis, 0.0);
                }
                self.machine_state = "Idle";
            }
            MacroAction::Unlock => {
                self.machine_state = "Idle";
                self.push_console(ConsoleLevel::Ok, "ok (unlock is visual only)");
            }
            MacroAction::Park => {
                self.set_value(Axis::X, -1043.33);
                self.set_value(Axis::Y, -801.0);
                self.set_value(Axis::Z, -2.0);
                self.machine_state = "Idle";
            }
            MacroAction::ProbeZ => {
                self.machine_state = "Probe";
                self.push_console(
                    ConsoleLevel::Warn,
                    "probe cycle is not wired in the M0 shell",
                );
            }
            MacroAction::Coolant => {
                self.toggle_coolant(1);
                self.spindle_on = true;
            }
        }
        self.push_console(ConsoleLevel::Ok, "ok (mock)");
    }

    pub fn send_mdi(&mut self) {
        let text = self.mdi.trim().to_string();
        if text.is_empty() {
            self.push_console(ConsoleLevel::Warn, "MDI is empty");
            return;
        }
        self.push_console(ConsoleLevel::Command, format!("> {text}"));
        self.push_console(ConsoleLevel::Ok, "ok (mock, nothing was sent)");
        self.machine_state = "MDI";
    }

    pub fn push_console(&mut self, level: ConsoleLevel, text: impl Into<String>) {
        self.console.push(ConsoleLine::new(level, text));
        if self.console.len() > 200 {
            self.console.remove(0);
        }
    }

    pub fn open_keypad(&mut self, target: KeypadTarget) {
        let (buffer, fresh) = match target {
            KeypadTarget::Dro(axis) => (format!("{:.3}", self.value(axis)), true),
            KeypadTarget::Mdi => (self.mdi.clone(), false),
            KeypadTarget::Rpm => (self.rpm.to_string(), true),
        };
        self.keypad = Some(KeypadState {
            target,
            buffer,
            fresh,
        });
    }

    pub fn keypad_action(&mut self, action: KeypadAction) {
        if matches!(action, KeypadAction::Submit) {
            if let Some(keypad) = self.keypad.take() {
                self.commit_keypad(keypad);
            }
            return;
        }
        if matches!(action, KeypadAction::Cancel) {
            self.keypad = None;
            return;
        }

        let Some(keypad) = self.keypad.as_mut() else {
            return;
        };
        match action {
            KeypadAction::Digit(digit) => {
                if keypad.fresh {
                    keypad.buffer.clear();
                    keypad.fresh = false;
                }
                if keypad.buffer.len() < 32 {
                    keypad.buffer.push(digit);
                }
            }
            KeypadAction::Dot => {
                if keypad.fresh {
                    keypad.buffer.clear();
                    keypad.fresh = false;
                }
                if !keypad.buffer.contains('.') && keypad.buffer.len() < 32 {
                    keypad.buffer.push('.');
                }
            }
            KeypadAction::Minus => {
                if keypad.fresh {
                    keypad.buffer.clear();
                    keypad.fresh = false;
                }
                if matches!(keypad.target, KeypadTarget::Mdi) {
                    keypad.buffer.push('-');
                } else if keypad.buffer.starts_with('-') {
                    keypad.buffer.remove(0);
                } else {
                    keypad.buffer.insert(0, '-');
                }
            }
            KeypadAction::Backspace => {
                keypad.buffer.pop();
                keypad.fresh = false;
            }
            KeypadAction::Clear => {
                keypad.buffer.clear();
                keypad.fresh = false;
            }
            KeypadAction::Token(token) => {
                if keypad.fresh {
                    keypad.buffer.clear();
                    keypad.fresh = false;
                }
                if token == "SP" {
                    keypad.buffer.push(' ');
                } else {
                    if !keypad.buffer.is_empty() && !keypad.buffer.ends_with(' ') {
                        keypad.buffer.push(' ');
                    }
                    keypad.buffer.push_str(token);
                }
            }
            KeypadAction::Submit | KeypadAction::Cancel => {}
        }
    }

    fn commit_keypad(&mut self, keypad: KeypadState) {
        match keypad.target {
            KeypadTarget::Mdi => {
                self.mdi = keypad.buffer.trim().to_string();
                if !self.live {
                    self.push_console(ConsoleLevel::Info, "MDI text updated with keypad");
                }
            }
            KeypadTarget::Dro(axis) => match keypad.buffer.trim().parse::<f64>() {
                Ok(value) => {
                    if self.live {
                        // The shell sends G10 L20 and waits for the next
                        // snapshot, so a rejected command never moves the DRO.
                    } else {
                        self.set_value(axis, value);
                        self.push_console(
                            ConsoleLevel::Command,
                            format!("{} = {:.3} mm (mock keypad)", axis.label(), value),
                        );
                    }
                }
                Err(_) => self.push_console(
                    ConsoleLevel::Warn,
                    format!("'{}' is not a number", keypad.buffer),
                ),
            },
            KeypadTarget::Rpm => match keypad.buffer.trim().parse::<f64>() {
                Ok(value) => {
                    self.rpm = value.clamp(0.0, 24_000.0) as u32;
                    if !self.live {
                        self.push_console(
                            ConsoleLevel::Command,
                            format!("S{} (mock keypad)", self.rpm),
                        );
                    }
                }
                Err(_) => self.push_console(ConsoleLevel::Warn, "RPM must be numeric"),
            },
        }
    }
}

fn initial_dro(axes: &[Axis]) -> Vec<f64> {
    let seed = [12.34, -48.12, -1.5, 0.0, 0.0, 0.0];
    axes.iter()
        .map(|axis| seed.get(axis.index()).copied().unwrap_or(0.0))
        .collect()
}

fn build_signals(
    capabilities: &Capabilities,
    machine_state: &str,
    signals: CoreSignals,
    live: bool,
) -> Vec<Signal> {
    let mut out = Vec::new();
    if capabilities.limit_pins {
        for axis in capabilities.axes.iter().take(3) {
            let asserted = CoreAxis::from_index(axis.index())
                .is_some_and(|core_axis| signals.contains_axis_limit(core_axis));
            out.push(Signal {
                label: format!("{} LIMIT", axis.label()),
                state: if asserted {
                    SignalState::Alert
                } else {
                    SignalState::Ok
                },
            });
        }
    }
    if capabilities.has_probe {
        let triggered = signals.contains(CoreSignals::PROBE)
            || signals.contains(CoreSignals::PROBE_PRIMARY)
            || signals.contains(CoreSignals::TOOL_SETTER)
            || signals.contains(CoreSignals::PROBE_SECONDARY);
        out.push(Signal {
            label: "PROBE".into(),
            state: if triggered {
                SignalState::Warn
            } else if live {
                SignalState::Off
            } else {
                // The M0 mock keeps the probe amber so the design review shows
                // every signal colour.
                SignalState::Warn
            },
        });
    }
    if capabilities.has_estop {
        out.push(Signal {
            label: "E-STOP".into(),
            state: if signals.contains(CoreSignals::ESTOP) || machine_state == "Alarm" {
                SignalState::Alert
            } else {
                SignalState::Ok
            },
        });
    }
    if capabilities.has_door {
        out.push(Signal {
            label: "DOOR".into(),
            state: if signals.contains(CoreSignals::DOOR) {
                SignalState::Alert
            } else {
                SignalState::Off
            },
        });
    }
    out
}

fn seed_console() -> Vec<ConsoleLine> {
    const SEED: [(ConsoleLevel, &str); 18] = [
        (ConsoleLevel::Info, "Grbl 1.1f ['$' for help]"),
        (ConsoleLevel::Info, "[VER:1.1f.20240604:]"),
        (ConsoleLevel::Info, "[OPT:VNMSL+2,64,1024,3,0]"),
        (ConsoleLevel::Info, "[AXS:3:XYZ]"),
        (ConsoleLevel::Info, "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]"),
        (ConsoleLevel::Info, "[SIGNALS:HSDEP]"),
        (ConsoleLevel::Info, "[SPINDLE:Huanyang v1]"),
        (ConsoleLevel::Info, "[AUX IO:4,4,0,0]"),
        (ConsoleLevel::Command, "> $I+"),
        (ConsoleLevel::Command, "> G21 G90 G54"),
        (ConsoleLevel::Ok, "ok"),
        (ConsoleLevel::Command, "> G0 X18.000 Y18.000"),
        (ConsoleLevel::Ok, "ok"),
        (
            ConsoleLevel::Info,
            "<Idle|MPos:12.340,-48.120,-1.500|FS:0,0>",
        ),
        (ConsoleLevel::Command, "> G1 Z-1.500 F300"),
        (ConsoleLevel::Ok, "ok"),
        (
            ConsoleLevel::Info,
            "<Run|MPos:12.340,-48.120,-1.500|FS:1200,12000>",
        ),
        (
            ConsoleLevel::Info,
            "[MSG:Mock session - no controller attached]",
        ),
    ];
    SEED.iter()
        .map(|(level, text)| {
            let mut line = ConsoleLine::new(*level, *text);
            line.time = "10:42".to_string();
            line
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ghs_core::{
        FeedSpindle, MachineState, Overrides, Position, Signals, Wcs, parse_controller_info,
    };
    use ghs_engine::StatusReport;

    use super::*;

    const FLEXIHAL: &[&str] = &[
        "[VER:1.1f.20240604:]",
        "[OPT:VNMSL+2,64,1024,3,0]",
        "[AXS:3:XYZ]",
        "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]",
        "[FIRMWARE:grblHAL]",
        "[SIGNALS:HSDEP]",
        "[BOARD:FlexiHAL_20240604_210624]",
    ];

    #[test]
    fn units_round_trip_between_millimetres_and_inch() {
        assert_eq!(Units::Millimetre.to_mm(12.5), 12.5);
        assert!((Units::Inch.to_mm(1.0) - 25.4).abs() < 1.0e-9);
        for value in [0.0, 1.0, -3.75, 120.125] {
            let shown = Units::Inch.display(value);
            assert!((Units::Inch.to_mm(shown) - value).abs() < 1.0e-9);
        }
    }

    #[test]
    fn live_telemetry_maps_to_the_touch_view_model() {
        let info = parse_controller_info(FLEXIHAL);
        let mut state = AppState::new();
        state.begin_live("test transport");
        state.apply_controller_info(&info);

        assert_eq!(state.axes().len(), 3);
        assert!(state.profile.capabilities.has_probe);
        assert!(state.profile.capabilities.has_estop);
        assert!(state.profile.capabilities.has_door);
        assert!(!state.profile.capabilities.limit_pins);
        assert!(!state.profile.capabilities.has_tool_table);
        assert!(state.profile.name.contains("FlexiHAL"));

        let snapshot = MachineSnapshot {
            state: MachineState::Idle,
            mpos: Position::xyz(110.0, 220.0, 330.0),
            wpos: Position::xyz(10.0, 20.0, 30.0),
            wco: Position::xyz(100.0, 200.0, 300.0),
            wcs: Wcs::G56,
            overrides: Overrides::new(125, 50, 90),
            feed_spindle: FeedSpindle::new(800.0, 12_000.0, true, false),
            signals: Signals::PROBE | Signals::ESTOP | Signals::DOOR,
            ..MachineSnapshot::default()
        };
        let mut status = StatusReport {
            tool: Some(4),
            tlo_reference_set: Some(true),
            dtg: Some(Position::xyz(-1.0, -2.0, -3.0)),
            ..StatusReport::default()
        };
        state.apply_snapshot(&snapshot, Some(&status));
        assert_eq!(state.dro, vec![10.0, 20.0, 30.0]);
        assert_eq!(state.mpos, vec![110.0, 220.0, 330.0]);
        assert_eq!(state.wco, vec![100.0, 200.0, 300.0]);
        assert_eq!(state.wcs_index, Wcs::G56.index());
        assert_eq!(state.feed_override, 125);
        assert_eq!(state.rapid_override, 50);
        assert_eq!(state.spindle_override, 90);
        assert_eq!(state.feed_rate, 800);
        assert_eq!(state.rpm, 12_000);
        assert!(state.spindle_on);
        assert!(!state.spindle_cw);
        assert_eq!(state.tool, Some(4));
        assert_eq!(state.tlr, Some(true));
        assert_eq!(state.dtg, vec![-1.0, -2.0, -3.0]);
        assert_eq!(
            state
                .signals
                .iter()
                .find(|signal| signal.label == "PROBE")
                .map(|signal| signal.state),
            Some(SignalState::Warn)
        );
        assert_eq!(
            state
                .signals
                .iter()
                .find(|signal| signal.label == "E-STOP")
                .map(|signal| signal.state),
            Some(SignalState::Alert)
        );
        assert_eq!(
            state
                .signals
                .iter()
                .find(|signal| signal.label == "DOOR")
                .map(|signal| signal.state),
            Some(SignalState::Alert)
        );

        status.tool = Some(5);
        state.apply_snapshot(&snapshot, Some(&status));
        assert_eq!(state.tool, Some(5));
    }

    fn sample_program() -> Arc<Program> {
        Arc::new(
            ghs_gcode::parse_program("G21\nG0 X10\nM6 T2\nM30\n").expect("sample program parses"),
        )
    }

    #[test]
    fn job_view_maps_program_lines_and_progress() {
        let program = sample_program();
        let mut job = JobView::default();
        assert!(job.apply_event(&EngineEvent::JobLoaded {
            name: "sample.nc".into(),
            total_lines: 4,
            program: Arc::clone(&program),
        }));
        assert_eq!(job.status_label(), "LOADED");
        assert_eq!(job.line_count(), 4);
        assert_eq!(job.line_source(2), Some("G0 X10"));
        assert_eq!(job.line_source(0), None);
        assert_eq!(job.line_source(5), None);
        assert_eq!(job.current_line, 0);

        assert!(job.apply_event(&EngineEvent::JobStarted {
            from_line: 1,
            total_lines: 4,
        }));
        assert!(job.is_running());
        assert!(job.apply_event(&EngineEvent::JobProgress {
            current_line: 2,
            completed: 1,
            total: 4,
            elapsed: Duration::from_secs(3),
        }));
        assert_eq!(job.current_line, 2);
        assert!((job.fraction() - 0.25).abs() < f32::EPSILON);

        assert!(job.apply_event(&EngineEvent::JobCompleted {
            elapsed: Duration::from_secs(9),
        }));
        assert_eq!(job.state, JobState::Completed);
        assert_eq!(job.completed, 4);
        assert_eq!(job.current_line, 4);
        assert_eq!(job.fraction(), 1.0);
        assert_eq!(format_elapsed(job.elapsed), "00:09");
    }

    #[test]
    fn job_view_tracks_pause_resume_and_the_tool_change_handshake() {
        let mut job = JobView::default();
        job.loaded("sample.nc".into(), sample_program(), 4);
        job.started(1, 4);

        job.paused(3, JobPauseReason::ProgramStop);
        assert!(job.is_paused());
        assert_eq!(job.status_label(), "PAUSED");
        assert_eq!(job.pause_reason, Some(JobPauseReason::ProgramStop));
        assert_eq!(
            pause_reason_label(JobPauseReason::ProgramStop),
            "program stop (M0)"
        );

        job.resumed();
        assert!(job.is_running());
        assert!(job.pause_reason.is_none());

        job.tool_change_required(2, 3);
        assert_eq!(job.status_label(), "TOOL CHANGE");
        assert!(job.tool_change_pending());
        assert!(!job.tool_change_acknowledged());
        assert_eq!(
            job.tool_change,
            Some(ToolChangeView {
                tool: 2,
                line: 3,
                acknowledged: false,
            })
        );
        assert!(job.acknowledge_tool_change());
        assert!(job.tool_change_acknowledged());

        job.resumed();
        assert!(job.tool_change.is_none());
        assert!(job.is_running());

        job.job_error(3);
        assert_eq!(job.state, JobState::Aborted);
        assert_eq!(job.status_label(), "ABORTED");

        job.loaded("sample.nc".into(), sample_program(), 4);
        job.tool_change_required(2, 3);
        job.interrupted();
        assert_eq!(job.state, JobState::Interrupted);
        assert!(job.tool_change.is_none(), "a link loss clears the sheet");
    }

    #[test]
    fn job_snapshot_recovers_progress_and_clears_a_finished_tool_change() {
        let mut job = JobView::default();
        job.loaded("sample.nc".into(), sample_program(), 4);
        job.tool_change_required(2, 3);

        let mut info = JobInfo {
            name: Some("sample.nc".into()),
            state: JobState::Paused,
            line: Some(3),
            total_lines: 4,
            completed_lines: 2,
            ..JobInfo::default()
        };
        info.update_progress();
        assert!(job.apply_snapshot(&info));
        assert_eq!(job.total, 4);
        assert_eq!(job.completed, 2);
        assert_eq!(job.current_line, 3);
        assert!(
            job.tool_change_pending(),
            "a paused snapshot keeps the prompt"
        );

        info.state = JobState::Running;
        info.line = Some(4);
        info.completed_lines = 3;
        info.update_progress();
        assert!(job.apply_snapshot(&info));
        assert!(job.tool_change.is_none(), "a running snapshot clears it");
        assert!(job.pause_reason.is_none());
        assert_eq!(job.fraction(), 0.75);
    }
}
