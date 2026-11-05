//! View-model state for the touch shell.
//!
//! The shell starts against the in-process demo controller and maps engine
//! telemetry into this plain data model. `--mock` keeps the original mock
//! mutations for layout review. `Profile` is a plain data value: panels and
//! axis rows are derived from `Capabilities`, never from a board name or a
//! compile-time feature. The reference machine is one profile, not a code
//! path; a generic or extended controller can supply a different value without
//! touching this module's consumers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ghs_config::{HeightMapRecord, Macro as ConfigMacro, ProbeOffsetRecord};
use ghs_core::Units as CoreUnits;
use ghs_engine::{
    Axis as CoreAxis, Command, ControllerInfo, EngineEvent, JobAbortReason, JobInfo,
    JobPauseReason, JobState, LastEvents, LimitState, MachineSnapshot, MachineState, Offsets,
    PinMapping, PinState, Position, Program, ProgramLine as EngineProgramLine, SettingValue,
    Signals as CoreSignals, StartupLine, StatusReport, ToolEntry, Wcs,
};
use ghs_i18n::t;
use ghs_probe::XyPoint;

use crate::config::ConfigModel;
use crate::console_log::{ConsoleLogError, ConsoleLogExport, ConsoleLogStatus, write_console_log};
use crate::controller_reset::ControllerReset;
use crate::height_compensation::HeightMapCompensation;
use crate::height_map_library::{HeightMapLibrary, LoadedSavedMap, record_from_run};
use crate::macros::{MacroModel, MacroRunGate, MacroRunRequest};
use crate::offsets::OffsetModel;
use crate::offsets_export::{OffsetsBackupModel, OffsetsExport};
use crate::pins::PinsModel;
use crate::probe::{ProbeField, ProbeModel, ToolLengthAction, ToolLengthGate, ToolLengthWrite};
use crate::settings::{SettingKind, SettingsModel};
use crate::setup::{SafetyItem, SetupModel, SetupStep};
use crate::startup::{StartupLinesModel, StartupWriteGate};
use crate::tools::{ControllerToolTable, ToolField, ToolTableModel};
use crate::utilities::{
    CalibrationAxis, CalibrationField, CalibrationGate, CalibrationWrite, GantryField,
    ProgramLimits, TravelSettings, UtilityField, UtilityModel,
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
    /// The controller advertises a probe-connected (`PC`) signal.
    pub has_probe_connected: bool,
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
            has_probe_connected: false,
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
            has_probe_connected: false,
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
            has_probe_connected: info.has_probe_connected_signal(),
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

/// Keypad owner. Tapping a DRO value, the MDI field, the RPM field, or a
/// generated settings value opens the same sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeypadTarget {
    Dro(Axis),
    Mdi,
    Rpm,
    /// A numeric setting row; the edit stays local until Apply.
    Setting(u16),
    /// One work-offset axis in a specific coordinate system.
    WorkOffset {
        /// Zero-based WCS index (`G54` is 0).
        wcs: usize,
        /// Axis to set.
        axis: Axis,
    },
    /// One numeric field in the tool sheet.
    Tool(ToolField),
    /// One numeric probe preflight field.
    ProbeField(ProbeField),
    /// One numeric utility (surfacing or transform) field.
    Utility(UtilityField),
    /// One stepper-calibration distance field.
    Calibration(CalibrationField),
    /// One gantry-squaring measurement field.
    Gantry(GantryField),
}

impl KeypadTarget {
    pub fn title(self) -> String {
        match self {
            KeypadTarget::Dro(axis) => match axis.index() {
                0 => t("SET X").to_string(),
                1 => t("SET Y").to_string(),
                2 => t("SET Z").to_string(),
                _ => t("SET AXIS").to_string(),
            },
            KeypadTarget::Mdi => t("MDI TEXT").to_string(),
            KeypadTarget::Rpm => t("SPINDLE RPM").to_string(),
            KeypadTarget::Setting(number) => format!("{} ${number}", t("SETTING")),
            KeypadTarget::WorkOffset { wcs, axis } => {
                format!("{} {} {}", AppState::wcs_at(wcs), axis.label(), t("OFFSET"))
            }
            KeypadTarget::Tool(field) => t(field.title()).to_string(),
            KeypadTarget::ProbeField(field) => t(field.label()).to_string(),
            KeypadTarget::Utility(field) => t(field.label()).to_string(),
            KeypadTarget::Calibration(field) => t(field.label()).to_string(),
            KeypadTarget::Gantry(field) => t(field.label()).to_string(),
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
    /// Optional range/unit hint for a settings value.
    pub hint: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleLevel {
    Info,
    Command,
    Ok,
    Warn,
    Error,
}

impl ConsoleLevel {
    /// Short uppercase label used by the filter chips and the exported log.
    pub fn label(self) -> &'static str {
        match self {
            ConsoleLevel::Info => "INFO",
            ConsoleLevel::Command => "CMD",
            ConsoleLevel::Ok => "OK",
            ConsoleLevel::Warn => "WARN",
            ConsoleLevel::Error => "ERROR",
        }
    }
}

/// View-only Console filter.
///
/// The ring buffer keeps every line regardless of this value; only the rendered
/// list and the `SAVE LOG` scope change. The active filter survives normal
/// telemetry polling and resets to [`ConsoleFilter::All`] when a new controller
/// session begins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConsoleFilter {
    /// Every line in the ring buffer.
    #[default]
    All,
    /// Commands sent to the controller.
    Command,
    /// Informational protocol and status lines.
    Info,
    /// Controller acknowledgements.
    Ok,
    /// Warnings.
    Warn,
    /// Errors and alarms.
    Error,
}

impl ConsoleFilter {
    /// Chip order, left to right.
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::Command,
        Self::Info,
        Self::Ok,
        Self::Warn,
        Self::Error,
    ];

    /// Uppercase chip and export label.
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Command => "CMD",
            Self::Info => "INFO",
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    /// Stable element-ID suffix for the chip.
    pub fn id(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Command => "cmd",
            Self::Info => "info",
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    /// Parse a screenshot/CLI value such as `warn` or `command`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "all" => Some(Self::All),
            "cmd" | "command" => Some(Self::Command),
            "info" => Some(Self::Info),
            "ok" => Some(Self::Ok),
            "warn" | "warning" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    /// Whether one line belongs to this view.
    pub fn matches(self, level: ConsoleLevel) -> bool {
        match self {
            Self::All => true,
            Self::Command => level == ConsoleLevel::Command,
            Self::Info => level == ConsoleLevel::Info,
            Self::Ok => level == ConsoleLevel::Ok,
            Self::Warn => level == ConsoleLevel::Warn,
            Self::Error => level == ConsoleLevel::Error,
        }
    }
}

/// Per-level line counts for the Console chips.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConsoleCounts {
    /// Every line in the ring buffer.
    pub all: usize,
    pub command: usize,
    pub info: usize,
    pub ok: usize,
    pub warn: usize,
    pub error: usize,
}

impl ConsoleCounts {
    /// Count one ring buffer in a single pass.
    pub fn from_lines(lines: &[ConsoleLine]) -> Self {
        let mut counts = Self::default();
        for line in lines {
            counts.all += 1;
            match line.level {
                ConsoleLevel::Info => counts.info += 1,
                ConsoleLevel::Command => counts.command += 1,
                ConsoleLevel::Ok => counts.ok += 1,
                ConsoleLevel::Warn => counts.warn += 1,
                ConsoleLevel::Error => counts.error += 1,
            }
        }
        counts
    }

    /// Count represented by one chip.
    pub fn count(&self, filter: ConsoleFilter) -> usize {
        match filter {
            ConsoleFilter::All => self.all,
            ConsoleFilter::Command => self.command,
            ConsoleFilter::Info => self.info,
            ConsoleFilter::Ok => self.ok,
            ConsoleFilter::Warn => self.warn,
            ConsoleFilter::Error => self.error,
        }
    }
}

/// Explicit bound of the in-app Console ring buffer.
///
/// The console keeps the newest [`CONSOLE_LINE_LIMIT`] lines; older lines are
/// dropped. Export writes a snapshot of what remains.
pub const CONSOLE_LINE_LIMIT: usize = 200;

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

/// Program-tab search mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProgramSearchMode {
    /// Show only matching lines, keeping their original line numbers.
    #[default]
    Filter,
    /// Show every line and mark the matching ones.
    Highlight,
}

impl ProgramSearchMode {
    /// Short label for the mode toggle.
    pub fn label(self) -> &'static str {
        match self {
            ProgramSearchMode::Filter => "FILTER",
            ProgramSearchMode::Highlight => "HIGHLIGHT",
        }
    }
}

/// Source lines scanned by [`ProgramSearch`].
///
/// Kept as a trait so the search stays independent of the engine and can be
/// exercised against the mock design-review program in tests.
pub(crate) trait ProgramLineSource {
    fn line_count(&self) -> usize;
    fn line_number(&self, index: usize) -> u32;
    fn line_text(&self, index: usize) -> &str;
}

impl ProgramLineSource for Program {
    fn line_count(&self) -> usize {
        self.lines.len()
    }

    fn line_number(&self, index: usize) -> u32 {
        self.lines
            .get(index)
            .map_or(0, |line| u32::try_from(line.line).unwrap_or(u32::MAX))
    }

    fn line_text(&self, index: usize) -> &str {
        self.lines
            .get(index)
            .map_or("", |line| line.source.as_str())
    }
}

impl ProgramLineSource for [ProgramLine] {
    fn line_count(&self) -> usize {
        self.len()
    }

    fn line_number(&self, index: usize) -> u32 {
        self.get(index)
            .map_or(0, |line| u32::try_from(line.number).unwrap_or(u32::MAX))
    }

    fn line_text(&self, index: usize) -> &str {
        self.get(index).map_or("", |line| line.text)
    }
}

/// Local search state for the Program tab.
///
/// The model stores only a sorted list of matching source indices; it never
/// copies or rebuilds the parsed program. Filter mode maps a visible list index
/// through those matches, while highlight mode keeps the identity mapping and
/// marks matches inside the lazily rendered rows.
#[derive(Clone, Debug, Default)]
pub struct ProgramSearch {
    query: String,
    mode: ProgramSearchMode,
    /// Zero-based indices into the program's source lines, ascending.
    matches: Arc<[usize]>,
    /// Index into `matches`, not into the program.
    current: Option<usize>,
    /// What the current match list was compiled from.
    compiled_query: Option<String>,
    compiled_key: Option<usize>,
}

impl ProgramSearch {
    /// Search text as typed by the operator.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Active display mode.
    pub fn mode(&self) -> ProgramSearchMode {
        self.mode
    }

    /// Set the display mode. Matches are unaffected.
    pub fn set_mode(&mut self, mode: ProgramSearchMode) {
        self.mode = mode;
    }

    /// Replace the search text. A blank query is the cleared state.
    ///
    /// The caller must refresh the model (normally through
    /// [`AppState::set_program_search_query`]) before reading the match count.
    pub fn set_query(&mut self, query: impl Into<String>) {
        let query = query.into();
        if query != self.query {
            self.query = query;
            self.current = None;
        }
    }

    /// Clear the search and every cached match.
    pub fn clear(&mut self) {
        self.query.clear();
        self.matches = Arc::from(Vec::new());
        self.current = None;
        self.compiled_query = None;
        self.compiled_key = None;
    }

    /// Whether a non-blank query is active.
    pub fn is_active(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// Number of matching source lines.
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// One-based current match position, when any line matches.
    pub fn current_position(&self) -> Option<usize> {
        self.current.map(|index| index + 1)
    }

    /// Zero-based source index of the current match.
    pub fn current_source_index(&self) -> Option<usize> {
        self.current
            .and_then(|index| self.matches.get(index).copied())
    }

    /// Whether one source index is a match.
    pub fn is_match(&self, source_index: usize) -> bool {
        self.matches.binary_search(&source_index).is_ok()
    }

    /// Advance to the next match, wrapping to the first.
    ///
    /// Returns `false` when the query has no matches.
    pub fn next_match(&mut self) -> bool {
        let count = self.matches.len();
        if count == 0 {
            self.current = None;
            return false;
        }
        let next = self.current.map_or(0, |current| (current + 1) % count);
        self.current = Some(next);
        true
    }

    /// Move to the previous match, wrapping to the last.
    ///
    /// Returns `false` when the query has no matches.
    pub fn previous_match(&mut self) -> bool {
        let count = self.matches.len();
        if count == 0 {
            self.current = None;
            return false;
        }
        let previous = self
            .current
            .map_or(count - 1, |current| (current + count - 1) % count);
        self.current = Some(previous);
        true
    }

    /// Number of rows the Program tab should render.
    ///
    /// Filter mode renders only the matches; highlight mode, and the cleared
    /// state, render every source line.
    pub fn visible_count(&self, line_count: usize) -> usize {
        if self.is_active() && self.mode == ProgramSearchMode::Filter {
            self.matches.len()
        } else {
            line_count
        }
    }

    /// Map one visible row to its source index.
    pub fn visible_source_index(&self, visible: usize, line_count: usize) -> Option<usize> {
        if self.is_active() && self.mode == ProgramSearchMode::Filter {
            self.matches.get(visible).copied()
        } else if visible < line_count {
            Some(visible)
        } else {
            None
        }
    }

    /// Map one source index to its visible row.
    pub fn source_visible_index(&self, source: usize, line_count: usize) -> Option<usize> {
        if self.is_active() && self.mode == ProgramSearchMode::Filter {
            self.matches.binary_search(&source).ok()
        } else if source < line_count {
            Some(source)
        } else {
            None
        }
    }

    /// Visible row for the current match, used by the scroll handle.
    pub fn current_visible_index(&self, line_count: usize) -> Option<usize> {
        let source = self.current_source_index()?;
        self.source_visible_index(source, line_count)
    }

    /// Whether the cached matches were built for this query and program.
    pub(crate) fn needs_refresh(&self, program_key: usize) -> bool {
        self.compiled_key != Some(program_key)
            || self.compiled_query.as_deref() != Some(self.query.as_str())
    }

    /// Recompute the match list for one program.
    ///
    /// Called only when the query or program identity changes, so typing does
    /// not repeatedly scan a large program and rendering never does.
    pub(crate) fn refresh(
        &mut self,
        source: &(impl ProgramLineSource + ?Sized),
        program_key: usize,
    ) {
        let raw_query = self.query.clone();
        let query = raw_query.trim();
        let reset = self.compiled_query.as_deref() != Some(raw_query.as_str());
        let mut matches = Vec::new();
        if !query.is_empty() {
            if let Some(line_number) = program_line_query(query) {
                for index in 0..source.line_count() {
                    if source.line_number(index) == line_number {
                        matches.push(index);
                    }
                }
            } else {
                let needle = query.to_ascii_lowercase();
                for index in 0..source.line_count() {
                    if contains_ignore_ascii_case(source.line_text(index), &needle) {
                        matches.push(index);
                    }
                }
            }
        }
        self.matches = Arc::from(matches);
        self.compiled_query = Some(raw_query);
        self.compiled_key = Some(program_key);
        let count = self.matches.len();
        if count == 0 {
            self.current = None;
        } else if reset || self.current.is_none() {
            self.current = Some(0);
        } else {
            let current = self.current.map_or(0, |index| index.min(count - 1));
            self.current = Some(current);
        }
    }
}

/// Parse `120` or `N120` as an exact source line number.
///
/// Any other query is text. Zero is not a valid source line.
fn program_line_query(query: &str) -> Option<u32> {
    let digits = query
        .strip_prefix('N')
        .or_else(|| query.strip_prefix('n'))
        .unwrap_or(query);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u32>().ok().filter(|line| *line > 0)
}

/// ASCII case-insensitive substring test without allocating per line.
fn contains_ignore_ascii_case(haystack: &str, needle_lower: &str) -> bool {
    let needle = needle_lower.as_bytes();
    if needle.is_empty() {
        return true;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroAction {
    ZeroXy,
    ZeroZ,
    SafeZ,
    Home,
    Unlock,
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

/// One pending run-to-line confirmation.
///
/// Captured when the operator opens the sheet. Confirming builds commands from
/// this captured line and source, never from a later selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunToConfirmation {
    /// 1-based source line where the job should pause.
    pub line: u32,
    /// Source text shown in the confirmation sheet.
    pub source: String,
}

/// App-side mirror of the engine's breakpoint set for the loaded program.
///
/// The engine owns the authoritative breakpoints. This model only remembers
/// which lines the app enabled so the Program list can mark and count them,
/// plus the selected row and the pending run-to confirmation. It is cleared by
/// [`JobView::loaded`] because the engine clears its own set on every load.
#[derive(Clone, Debug, Default)]
pub struct ProgramBreakpoints {
    lines: BTreeSet<u32>,
    selected: Option<u32>,
    pending_run_to: Option<RunToConfirmation>,
    /// Last refusal or engine rejection shown in the run-to sheet.
    pub error: Option<String>,
}

impl ProgramBreakpoints {
    /// Whether one 1-based source line has an enabled breakpoint.
    pub fn contains(&self, line: u32) -> bool {
        self.lines.contains(&line)
    }

    /// Number of enabled breakpoints.
    pub fn count(&self) -> usize {
        self.lines.len()
    }

    /// Enabled breakpoints before `line`, in ascending order.
    ///
    /// Used by the run-to sheet so an operator knows earlier breakpoints will
    /// pause the job before it reaches the requested line.
    #[must_use]
    pub fn earlier_than(&self, line: u32) -> Vec<u32> {
        self.lines.range(..line).copied().collect()
    }

    /// Selected 1-based source line, when one row is selected.
    pub fn selected(&self) -> Option<u32> {
        self.selected
    }

    /// Select one source line, or clear the selection.
    pub fn select(&mut self, line: Option<u32>) {
        self.selected = line;
    }

    /// Pending run-to confirmation, when the sheet is open.
    pub fn pending_run_to(&self) -> Option<&RunToConfirmation> {
        self.pending_run_to.as_ref()
    }

    /// Record one engine-accepted breakpoint change.
    ///
    /// Returns `true` when the set changed.
    pub fn set_line(&mut self, line: u32, enabled: bool) -> bool {
        if enabled {
            self.lines.insert(line)
        } else {
            self.lines.remove(&line)
        }
    }

    /// Clear every breakpoint and transient selection state.
    ///
    /// Called for a new program, where the engine also clears its own set.
    fn clear_for_program(&mut self) {
        self.lines.clear();
        self.selected = None;
        self.pending_run_to = None;
        self.error = None;
    }
}

/// Why a breakpoint or run-to action is unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakpointRefusal {
    /// `--mock` has no engine to receive `SetBreakpoint`.
    Mock,
    /// The link is connecting, disconnected, or failed.
    NotReady,
    /// The machine is in `Alarm` or `Door`.
    MachineLocked,
    /// No program is loaded in the engine.
    NoProgram,
    /// The line is zero or beyond the streamed program.
    LineOutOfRange,
    /// The job is already paused at this line; re-arming it would pause again.
    PausedAtLine,
}

impl BreakpointRefusal {
    /// Operator-facing refusal text.
    pub fn message(self) -> &'static str {
        match self {
            BreakpointRefusal::Mock => "Mock mode: breakpoints need a live controller",
            BreakpointRefusal::NotReady => "The controller link is not ready",
            BreakpointRefusal::MachineLocked => "The machine is in Alarm or Door",
            BreakpointRefusal::NoProgram => "No program is loaded",
            BreakpointRefusal::LineOutOfRange => "That line is outside the streamed program",
            BreakpointRefusal::PausedAtLine => {
                "The job is paused at this line; resume or stop before re-arming it"
            }
        }
    }
}

/// Why run-to-line is unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunToRefusal {
    /// `--mock` has no engine to receive the breakpoint or start commands.
    Mock,
    /// The link is connecting, disconnected, or failed.
    NotReady,
    /// The machine is in `Alarm` or `Door`.
    MachineLocked,
    /// No program is loaded in the engine.
    NoProgram,
    /// No row is selected in the Program list.
    NoSelection,
    /// The selected line is zero or beyond the streamed program.
    LineOutOfRange,
    /// The job is already paused at this breakpoint line.
    PausedAtLine,
    /// A generated probe program owns the job; re-running it is unsafe.
    ProbeProgram,
    /// No job is active, but the machine is not Idle, so a start would fail.
    MachineNotIdle,
    /// The confirmation sheet is not open.
    NoPending,
}

impl RunToRefusal {
    /// Operator-facing refusal text.
    pub fn message(self) -> &'static str {
        match self {
            RunToRefusal::Mock => "Mock mode: run-to needs a live controller",
            RunToRefusal::NotReady => "The controller link is not ready",
            RunToRefusal::MachineLocked => "Run-to is blocked in Alarm or Door",
            RunToRefusal::NoProgram => "Load a program before selecting a run-to line",
            RunToRefusal::NoSelection => "Select a program line first",
            RunToRefusal::LineOutOfRange => "That line is outside the streamed program",
            RunToRefusal::PausedAtLine => "The job is already paused at this line",
            RunToRefusal::ProbeProgram => "A generated probe program cannot be started from run-to",
            RunToRefusal::MachineNotIdle => "The machine must be Idle to start the job",
            RunToRefusal::NoPending => "No run-to confirmation is open",
        }
    }
}

impl From<BreakpointRefusal> for RunToRefusal {
    fn from(refusal: BreakpointRefusal) -> Self {
        match refusal {
            BreakpointRefusal::Mock => RunToRefusal::Mock,
            BreakpointRefusal::NotReady => RunToRefusal::NotReady,
            BreakpointRefusal::MachineLocked => RunToRefusal::MachineLocked,
            BreakpointRefusal::NoProgram => RunToRefusal::NoProgram,
            BreakpointRefusal::LineOutOfRange => RunToRefusal::LineOutOfRange,
            BreakpointRefusal::PausedAtLine => RunToRefusal::PausedAtLine,
        }
    }
}

/// Result of one breakpoint toggle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BreakpointOutcome {
    /// The engine accepted the command.
    Sent {
        /// 1-based source line.
        line: u32,
        /// New breakpoint state.
        enabled: bool,
    },
    /// The gate refused before any command was built.
    Refused(BreakpointRefusal),
    /// The engine rejected the command; the local set is unchanged.
    Rejected(String),
}

/// Result of one confirmed run-to action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunToOutcome {
    /// The breakpoint and the job start were both accepted.
    Started {
        /// 1-based source line where the job pauses.
        line: u32,
    },
    /// Only the breakpoint was set because a job was already active.
    BreakpointOnly {
        /// 1-based source line where the job will pause.
        line: u32,
    },
    /// The breakpoint was accepted but the start was rejected.
    StartRejected {
        /// 1-based source line whose breakpoint is now enabled.
        line: u32,
        /// Engine rejection text.
        message: String,
    },
    /// The breakpoint was accepted and the start is held behind the
    /// over-travel confirmation. A later confirm sends exactly one
    /// `StartProgram`; cancelling leaves the breakpoint enabled.
    StartHeld {
        /// 1-based source line where the job would pause.
        line: u32,
    },
    /// The gate refused before any command was built.
    Refused(RunToRefusal),
    /// The breakpoint command was rejected; nothing else was sent.
    Rejected(String),
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
    /// Breakpoint mirror, selected row, and pending run-to confirmation.
    pub breakpoints: ProgramBreakpoints,
    /// The loaded program's current or most recent run is a controller dry run.
    ///
    /// Mirrors [`ghs_engine::EngineHandle::check_run`] through the live poll so
    /// the label survives event loss, a reconnect, or a fresh snapshot. It is
    /// cleared by a new load and by a normal start, never by completion.
    check: bool,
    /// [`ghs_engine::Command::StartCheckRun`] was accepted; `CheckStarted` has
    /// not arrived yet. The controller is still entering `$C`.
    check_pending: bool,
    /// `CheckStarted` was seen for the run that is about to emit `JobStarted`.
    check_started_event: bool,
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

    /// Rebuild the loaded source text, separated by `\n`.
    ///
    /// The engine stores parsed lines rather than the raw file, so this is the
    /// program text the app can transform. Blank lines and comments are kept;
    /// the original file's line terminator style is not retained.
    pub fn source_text(&self) -> Option<String> {
        let program = self.program.as_ref()?;
        Some(
            program
                .lines
                .iter()
                .map(|line| line.source.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    /// Whether a 1-based line is inside the engine's streamed program.
    ///
    /// The engine stops its line count at `M2`/`M30`; lines parsed after that
    /// are not streamed and cannot carry a breakpoint or run-to target.
    pub fn has_runnable_line(&self, line: u32) -> bool {
        line > 0 && line <= self.total && self.line_source(line).is_some()
    }

    /// Resolved fraction of the streamed lines, clamped to `0.0..=1.0`.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        (self.completed.min(self.total) as f32 / self.total as f32).clamp(0.0, 1.0)
    }

    /// Estimated remaining active time for a running or paused job.
    ///
    /// Uses the job's tracked active elapsed time and confirmed progress; it
    /// never guesses from line counts alone. Idle, finished, and not-yet-
    /// started jobs return `None`.
    pub fn eta(&self) -> Option<Duration> {
        if !self.is_active() {
            return None;
        }
        estimate_remaining(self.elapsed, self.fraction())
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

    /// Whether the loaded program's current or most recent run is a dry run.
    pub fn is_check(&self) -> bool {
        self.check
    }

    /// Whether a check start was accepted but `CheckStarted` has not arrived.
    ///
    /// While this is set the controller may be entering `$C`; no second check
    /// and no normal start may be sent.
    pub fn check_pending(&self) -> bool {
        self.check_pending
    }

    /// Whether a check is being streamed (running or paused in check mode).
    pub fn checking(&self) -> bool {
        self.check && self.is_active()
    }

    /// Whether a check run reached the end.
    pub fn check_complete(&self) -> bool {
        self.check && self.state == JobState::Completed
    }

    /// Whether a check transition owns the runner right now.
    pub fn check_busy(&self) -> bool {
        self.check_pending || self.checking()
    }

    /// Records that `StartCheckRun` was accepted. Nothing is streamed yet.
    pub fn begin_check(&mut self) {
        self.check_pending = true;
    }

    /// Records `CheckStarted`.
    pub fn check_started(&mut self, total: u32) {
        self.check = true;
        self.check_pending = false;
        self.check_started_event = true;
        if total > 0 {
            self.total = total;
        }
    }

    /// Records `CheckCompleted`; the dry-run label and elapsed time stay.
    pub fn check_completed(&mut self, elapsed: Duration) {
        self.check = true;
        self.check_pending = false;
        self.check_started_event = false;
        self.elapsed = elapsed;
    }

    /// Clears the dry-run label after a normal `StartProgram`.
    pub fn clear_check(&mut self) {
        self.check = false;
        self.check_pending = false;
        self.check_started_event = false;
    }

    /// Mirrors the engine's dry-run accessor on every poll.
    ///
    /// Returns `true` when the displayed check state changed. A `true`
    /// accessor also recovers a `CheckStarted` event that was drained before
    /// the UI existed; a `false` accessor clears the label after a normal
    /// start or a new load.
    pub fn sync_check_run(&mut self, engine_check: bool) -> bool {
        let mut changed = false;
        if engine_check {
            if !self.check {
                self.check = true;
                changed = true;
            }
            if self.check_pending {
                self.check_pending = false;
                changed = true;
            }
        } else if self.check {
            self.check = false;
            changed = true;
        }
        changed
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
        if self.check_pending || self.checking() {
            return "CHECKING";
        }
        if self.check {
            if self.check_complete() {
                return "CHECK COMPLETE";
            }
            match self.state {
                JobState::Aborted | JobState::Interrupted => return "CHECK STOPPED",
                JobState::Idle | JobState::Running | JobState::Paused | JobState::Completed => {
                    return "CHECKING";
                }
            }
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
    ///
    /// A new program invalidates every app-side breakpoint, the selected row,
    /// and any open run-to confirmation. The engine still owns the real set and
    /// clears it in the same load (verified in `JobRunner::load`).
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
        self.clear_check();
        self.breakpoints.clear_for_program();
    }

    /// Records `JobStarted`.
    ///
    /// A preceding `CheckStarted` owns the dry-run label. Any other start is a
    /// normal `StartProgram` and clears a stale dry-run label immediately,
    /// including the run-to and probe start paths that do not go through
    /// [`AppState::dispatch_start`].
    pub fn started(&mut self, from_line: u32, total: u32) {
        if self.check_started_event {
            self.check_started_event = false;
        } else {
            self.clear_check();
        }
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
        self.check_pending = false;
        self.check_started_event = false;
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
        self.check_pending = false;
        self.check_started_event = false;
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
            self.check_pending = false;
            self.check_started_event = false;
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
            EngineEvent::CheckStarted { total_lines } => {
                self.check_started(*total_lines);
                true
            }
            EngineEvent::CheckCompleted { elapsed } => {
                self.check_completed(*elapsed);
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

/// One confirmed utility program ready for the shell to send.
///
/// The command is always [`Command::LoadProgram`]; the utility load path never
/// constructs a start, resume, offset, or compensation command.
#[derive(Clone, Debug, PartialEq)]
pub struct UtilityLoad {
    /// Program name shown in the Job workspace.
    pub name: String,
    /// The one command the shell must send.
    pub command: Command,
}

/// Why the app refuses to replace the loaded program.
///
/// The engine also rejects a load while its runner is active or the link is
/// down; this gate makes the UI explain the refusal before the file is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramLoadRefusal {
    /// `--mock` has no engine; browsing is allowed, loading is not.
    Mock,
    /// The transport is connecting, disconnected, or failed.
    NotReady,
    /// A program is running or paused; stop it before choosing another file.
    JobActive,
}

impl ProgramLoadRefusal {
    /// Operator-facing reason shown in the file browser and the console.
    pub fn message(self) -> &'static str {
        match self {
            ProgramLoadRefusal::Mock => "MOCK MODE: browsing only; loading needs a live controller",
            ProgramLoadRefusal::NotReady => {
                "LINK NOT READY: finish the connection handshake before loading a program"
            }
            ProgramLoadRefusal::JobActive => {
                "JOB ACTIVE: stop the running job before loading another program"
            }
        }
    }
}

/// Why the app refuses to start a controller dry run.
///
/// The engine repeats these checks; the gate exists so the CHECK action is
/// disabled before a command is built and the operator gets a reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckRefusal {
    /// `--mock` has no engine; a dry run needs a controller.
    Mock,
    /// The transport is connecting, disconnected, or failed.
    NotReady,
    /// The controller reports Alarm or Door; clear it before checking.
    MachineLocked,
    /// No program is loaded in the engine.
    NoProgram,
    /// A real job is running or paused.
    JobActive,
    /// The controller is not Idle.
    MachineNotIdle,
    /// A check start or dry run owns the runner.
    CheckActive,
    /// A generated probe plan cannot be dry-run from the job toolbar.
    ProbeProgram,
}

impl CheckRefusal {
    /// Operator-facing reason shown in the console and the check sheet.
    pub fn message(self) -> &'static str {
        match self {
            CheckRefusal::Mock => "MOCK MODE: a check run needs a live controller",
            CheckRefusal::NotReady => {
                "LINK NOT READY: finish the connection handshake before a check run"
            }
            CheckRefusal::MachineLocked => "MACHINE LOCKED: clear Alarm or Door before a check run",
            CheckRefusal::NoProgram => "NO PROGRAM: load a program before a check run",
            CheckRefusal::JobActive => "JOB ACTIVE: stop the running job before a check run",
            CheckRefusal::MachineNotIdle => "MACHINE NOT IDLE: a check run starts only from IDLE",
            CheckRefusal::CheckActive => {
                "CHECK RUN ACTIVE: wait for the controller to finish check mode"
            }
            CheckRefusal::ProbeProgram => {
                "PROBE PROGRAM: a generated probe plan cannot be checked from the job toolbar"
            }
        }
    }
}

/// Result of one confirmed `StartCheckRun` action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckOutcome {
    /// The sheet was confirmed and exactly one `StartCheckRun` was accepted.
    Started,
    /// The gate refused after confirmation; nothing was sent.
    Refused(CheckRefusal),
    /// The engine rejected the command; nothing was started.
    Rejected(String),
    /// No confirmation was open; nothing was sent.
    NotConfirmed,
}

/// Result of one normal `StartProgram` dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartOutcome {
    /// Exactly one `StartProgram { from_line: None }` was accepted.
    Sent,
    /// The program exceeds reported travel; the confirmation sheet opened and
    /// nothing was sent.
    ConfirmationRequired,
    /// A check transition owns the runner; nothing was sent.
    Refused(CheckRefusal),
    /// The engine rejected the command.
    Rejected(String),
    /// There was no open confirmation; nothing was sent.
    NotConfirmed,
    /// The open confirmation belonged to a changed program, link, machine
    /// state, or limits report; it was discarded and nothing was sent.
    Stale,
}

/// Program and travel snapshot captured by an over-travel START confirmation.
///
/// The exact parsed program is retained so a later confirmation can refuse to
/// fire after the controller loaded or reloaded a program. Creating this value
/// sends nothing; only [`AppState::confirm_start`] sends `StartProgram`.
#[derive(Clone, Debug)]
pub struct StartConfirmation {
    limits: ProgramLimits,
    program: Arc<Program>,
    run_to_line: Option<u32>,
}

impl StartConfirmation {
    /// Limits report shown by the confirmation sheet.
    #[must_use]
    pub fn limits(&self) -> &ProgramLimits {
        &self.limits
    }

    /// Run-to line whose breakpoint is already set, when this held start came
    /// from the run-to sheet rather than a normal START.
    #[must_use]
    pub fn run_to_line(&self) -> Option<u32> {
        self.run_to_line
    }
}

/// Short scope label for a successful console-log export.
fn console_log_scope(export: &ConsoleLogExport) -> String {
    let mut scope = if export.filtered {
        format!("{} LINES ({} VIEW)", export.lines, export.filter.label())
    } else {
        format!("{} LINES (FULL BUFFER)", export.lines)
    };
    if export.skipped > 0 {
        scope.push_str(&format!(", {} SKIPPED AT SIZE LIMIT", export.skipped));
    }
    scope
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

/// Longest remaining time the Job ETA strip will display.
///
/// A job that has just started has a tiny progress sample, so an unclamped
/// estimate can be days or years. A day is already far beyond any job this UI
/// is meant to run; showing a cap is more honest than an absurd number.
pub const MAX_JOB_ETA: Duration = Duration::from_secs(24 * 60 * 60);

/// Estimate the remaining active job time from elapsed time and progress.
///
/// `progress` is the fraction of the program already confirmed, in `(0, 1)`.
/// Zero, one, negative, or non-finite progress has no meaningful remaining
/// time, and neither has a job with no elapsed time yet, so all of those return
/// `None`. The result is rounded up to whole seconds and clamped to
/// [`MAX_JOB_ETA`], which keeps `NaN`, negative values, and absurd estimates
/// out of the header.
pub fn estimate_remaining(elapsed: Duration, progress: f32) -> Option<Duration> {
    if !progress.is_finite() || progress <= 0.0 || progress >= 1.0 {
        return None;
    }
    let elapsed_s = elapsed.as_secs_f64();
    if !elapsed_s.is_finite() || elapsed_s <= 0.0 {
        return None;
    }
    let remaining_s = elapsed_s * ((1.0 - f64::from(progress)) / f64::from(progress));
    if !remaining_s.is_finite() {
        return None;
    }
    // Round up: an ETA of 00:00 while work remains reads as "already done".
    let seconds = remaining_s.ceil().max(1.0);
    Duration::try_from_secs_f64(seconds)
        .ok()
        .map(|remaining| remaining.min(MAX_JOB_ETA))
}

/// Rapid override levels offered by the compact touch control, in tap order.
pub const RAPID_OVERRIDE_CYCLE: [u16; 3] = [100, 50, 25];

/// Next rapid override level in the touch cycle.
///
/// The controller also documents a 5 percent extra-low level. It is not part
/// of this compact cycle; any reported value outside the cycle (including 5)
/// returns to 100 percent, because reset is always valid and is the safest
/// place to restart from.
pub fn next_rapid_override(current: u16) -> u16 {
    RAPID_OVERRIDE_CYCLE
        .iter()
        .position(|level| *level == current)
        .map_or(100, |index| {
            RAPID_OVERRIDE_CYCLE[(index + 1) % RAPID_OVERRIDE_CYCLE.len()]
        })
}

/// Why the rapid-override control cannot send a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RapidOverrideRefusal {
    /// `--mock` has no engine to send a command to.
    Mock,
    /// The transport is connecting, disconnected, or failed.
    NotReady,
}

impl RapidOverrideRefusal {
    /// Operator-facing reason shown in the console.
    pub fn message(self) -> &'static str {
        match self {
            RapidOverrideRefusal::Mock => "MOCK MODE: rapid override needs a live controller",
            RapidOverrideRefusal::NotReady => {
                "LINK NOT READY: rapid override is disabled until the handshake completes"
            }
        }
    }
}

/// Smallest feed override percentage the controller accepts.
pub const FEED_OVERRIDE_MIN: u16 = 10;
/// Largest feed override percentage the controller accepts.
pub const FEED_OVERRIDE_MAX: u16 = 200;
/// Feed override percentage restored by `RESET 100%`.
pub const FEED_OVERRIDE_RESET: u16 = 100;
/// Bounded session history of lines sent through the MDI field.
pub const MDI_HISTORY_LIMIT: usize = 10;

/// Why a feed-override control cannot send a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedOverrideRefusal {
    /// `--mock` has no engine to send a command to.
    Mock,
    /// The transport is connecting, disconnected, or failed.
    NotReady,
}

impl FeedOverrideRefusal {
    /// Operator-facing reason shown in the console.
    pub fn message(self) -> &'static str {
        match self {
            FeedOverrideRefusal::Mock => "MOCK MODE: feed override needs a live controller",
            FeedOverrideRefusal::NotReady => {
                "LINK NOT READY: feed override is disabled until the handshake completes"
            }
        }
    }
}

/// Why the controller cannot be put to sleep right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepRefusal {
    /// `--mock` has no engine to send a command to.
    Mock,
    /// The transport is connecting, disconnected, or failed.
    NotReady,
    /// The controller reports a state other than `Idle`.
    NotIdle,
    /// A job is running or paused; sleep would interrupt it.
    JobActive,
}

impl SleepRefusal {
    /// Operator-facing reason shown in the console and the utility card.
    pub fn message(self) -> &'static str {
        match self {
            SleepRefusal::Mock => "MOCK MODE: sleep needs a live controller",
            SleepRefusal::NotReady => {
                "LINK NOT READY: sleep is disabled until the handshake completes"
            }
            SleepRefusal::NotIdle => {
                "NOT IDLE: sleep is available only while the controller reports Idle"
            }
            SleepRefusal::JobActive => "JOB ACTIVE: stop or finish the job before sleep",
        }
    }
}

/// Result of confirming the sleep sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SleepOutcome {
    /// Exactly one `Command::Sleep` was handed to the sender.
    Sent,
    /// The sender rejected the one command; nothing is claimed.
    Rejected(String),
    /// The gate refused the confirmation; nothing was sent.
    Refused(SleepRefusal),
    /// No confirmation sheet was open; nothing was sent.
    NotConfirmed,
}

/// One controller run switch exposed by the Job `RUN OPTIONS` sheet.
///
/// The displayed value is always the engine accessor copied into [`AppState`];
/// this enum only names the switch and its command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOption {
    /// Virtual optional stop, `M1`.
    OptionalStop,
    /// Virtual single block.
    SingleBlock,
    /// Block delete, `$B`; idle-only on the engine.
    BlockDelete,
}

impl RunOption {
    /// Sheet order. All three switches are independent.
    pub const ALL: [Self; 3] = [Self::OptionalStop, Self::SingleBlock, Self::BlockDelete];

    /// Uppercase label in the sheet.
    pub const fn label(self) -> &'static str {
        match self {
            Self::OptionalStop => "OPTIONAL STOP",
            Self::SingleBlock => "SINGLE BLOCK",
            Self::BlockDelete => "BLOCK DELETE",
        }
    }

    /// Short explanation shown under the label while the toggle is enabled.
    pub const fn detail(self) -> &'static str {
        match self {
            Self::OptionalStop => "M1 optional stops are honored",
            Self::SingleBlock => "Hold after each block",
            Self::BlockDelete => "Skip blocks marked with /",
        }
    }
}

/// Why a run-option toggle cannot send a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunOptionRefusal {
    /// `--mock` has no engine.
    Mock,
    /// The transport is connecting, disconnected, or failed.
    NotReady,
    /// Block delete is idle-only; a running or paused job locks it.
    JobActive,
}

impl RunOptionRefusal {
    /// Operator-facing reason shown in the sheet and console.
    pub fn message(self) -> &'static str {
        match self {
            Self::Mock => "MOCK MODE: run options need a live controller",
            Self::NotReady => {
                "LINK NOT READY: run options are disabled until the handshake completes"
            }
            Self::JobActive => "BLOCK DELETE IS LOCKED WHILE A JOB IS RUNNING OR PAUSED",
        }
    }

    /// One-line reason used inside a disabled toggle row.
    pub fn hint(self) -> &'static str {
        match self {
            Self::Mock => "Mock mode: no controller",
            Self::NotReady => "Link not ready",
            Self::JobActive => "Locked while a job runs or pauses",
        }
    }
}

/// Result of asking the engine to flip one run switch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunOptionOutcome {
    /// Exactly one command was handed to the sender.
    Sent,
    /// The sender rejected that one command. The displayed value is unchanged.
    Rejected(String),
    /// No command was built or sent.
    Refused(RunOptionRefusal),
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
    /// Authoritative virtual optional-stop flag, copied from the engine.
    pub optional_stop: bool,
    /// Authoritative virtual single-block flag, copied from the engine.
    pub single_block: bool,
    /// Authoritative block-delete flag, copied from the engine.
    pub block_delete: bool,
    /// Whether the Job `RUN OPTIONS` sheet is open. Opening sends nothing.
    pub run_options_open: bool,
    /// Last refusal or engine rejection shown inside the run-options sheet.
    pub run_options_error: Option<String>,
    /// The check-run confirmation sheet is open. Opening sends nothing.
    pub check_confirm_open: bool,
    /// Over-travel START confirmation, with the exact program and limits it
    /// was opened for. `Some` opens the sheet; opening sends nothing.
    pub start_confirm: Option<StartConfirmation>,
    /// The sleep confirmation sheet is open. Opening sends nothing.
    pub sleep_confirm_open: bool,
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
    /// View-only level filter. The ring buffer keeps every line.
    pub console_filter: ConsoleFilter,
    /// Last `SAVE LOG` result shown under the filter chips.
    pub console_log_status: Option<ConsoleLogStatus>,
    pub program: Vec<ProgramLine>,
    pub active_line: usize,
    pub progress: f32,
    /// Program-tab search/filter and jump state.
    pub program_search: ProgramSearch,
    pub keypad: Option<KeypadState>,
    pub mdi: String,
    /// Bounded session history of MDI lines that were accepted for sending.
    /// Oldest entry first; this is never persisted.
    pub mdi_history: Vec<String>,
    /// Index of the entry currently recalled into [`Self::mdi`], if any.
    /// `None` means the field is a fresh draft, not a history entry.
    pub mdi_history_index: Option<usize>,
    pub macros: Vec<Macro>,
    pub signals: Vec<Signal>,
    pub signal_caps: String,
    pub feed_rate: u32,
    /// Generated settings editor, filled from `$ES`/`$EG` and `$$`.
    pub settings: SettingsModel,
    /// Live `$#` offsets, formatted for the offsets workspace.
    pub offsets: OffsetModel,
    /// Local `offsets.nc` export result and the validated restore preview.
    pub offsets_backup: OffsetsBackupModel,
    /// Probe preflight, run binding and apply model.
    pub probe: ProbeModel,
    /// Original source and applied state for the explicit height-map workflow.
    pub height_map: HeightMapCompensation,
    /// Saved per-controller maps and the current read-only selection.
    pub height_map_library: HeightMapLibrary,
    /// Asserted `Pn` input bits from the last snapshot, for probe gating.
    pub signals_raw: CoreSignals,
    /// `MPG:1`: a pendant owns the controller input stream.
    pub mpg_active: bool,
    /// Utility destructive `$RST` selection, confirmation, and result.
    pub controller_reset: ControllerReset,
    /// Application-owned tool table and its profile state.
    pub tools: ToolTableModel,
    /// Read-only controller `[T:...]` report; never merged into `tools`.
    pub controller_tools: ControllerToolTable,
    /// `$N0`/`$N1` read state and the local edit/confirmation model.
    pub startup_lines: StartupLinesModel,
    /// Read-only `$PINS`/`$PINSTATE`/`$LIM`/`$LEV` diagnostics mirrors.
    pub pins: PinsModel,
    /// User-defined macro execution: pending confirmation and last error.
    ///
    /// The macro records themselves live in the active controller profile
    /// (`ConfigModel`); this model never stores a second copy.
    pub user_macros: MacroModel,
    /// Material surfacing wizard, program-limits state, and source transforms.
    pub utilities: UtilityModel,
    /// First-run safety/setup wizard. Nothing in it can send a command.
    pub setup: SetupModel,
    /// Local app preferences, controller profile, and backup helpers.
    pub config: ConfigModel,
    /// Whether this app run already offered the first-run wizard once.
    setup_auto_prompted: bool,
}

impl AppState {
    pub fn new() -> Self {
        let config = ConfigModel::platform();
        let mut profile = Profile::mock_reference();
        profile.capabilities.units = Units::from_core(config.app().units);
        let dro = initial_dro(&profile.capabilities.axes);
        let signals = build_signals(&profile.capabilities, "Idle", CoreSignals::NONE, false);
        let axes = profile.capabilities.axes.len();
        let mut state = Self {
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
            optional_stop: false,
            single_block: false,
            block_delete: false,
            run_options_open: false,
            run_options_error: None,
            check_confirm_open: false,
            start_confirm: None,
            sleep_confirm_open: false,
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
            console_filter: ConsoleFilter::All,
            console_log_status: None,
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
            program_search: ProgramSearch::default(),
            keypad: None,
            mdi: "G0 X10 Y10".to_string(),
            mdi_history: Vec::new(),
            mdi_history_index: None,
            macros: vec![
                mac("Zero XY", MacroAction::ZeroXy),
                mac("Zero Z", MacroAction::ZeroZ),
                mac("Safe Z", MacroAction::SafeZ),
                mac("Home", MacroAction::Home),
                mac("Unlock", MacroAction::Unlock),
                mac("Probe Z", MacroAction::ProbeZ),
                mac("M8/M9", MacroAction::Coolant),
            ],
            signals,
            signal_caps: "HSDEP".to_string(),
            feed_rate: 1200,
            settings: SettingsModel::default(),
            offsets: OffsetModel::default(),
            offsets_backup: OffsetsBackupModel::default(),
            probe: ProbeModel::default(),
            height_map: HeightMapCompensation::default(),
            height_map_library: HeightMapLibrary::default(),
            signals_raw: CoreSignals::NONE,
            mpg_active: false,
            controller_reset: ControllerReset::default(),
            tools: ToolTableModel::new(),
            controller_tools: ControllerToolTable::new(),
            startup_lines: StartupLinesModel::new(),
            pins: PinsModel::new(),
            user_macros: MacroModel::new(),
            utilities: UtilityModel::default(),
            setup: SetupModel::new(config.app().units),
            config,
            setup_auto_prompted: false,
        };
        state.sync_offset_context();
        state
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
        // A fresh session starts with the full view and no previous export
        // result. Telemetry polling never changes the filter.
        self.console_filter = ConsoleFilter::All;
        self.console_log_status = None;
        self.mpos = vec![0.0; self.profile.capabilities.axes.len()];
        self.dro = vec![0.0; self.profile.capabilities.axes.len()];
        self.wco = vec![0.0; self.profile.capabilities.axes.len()];
        self.dtg = vec![0.0; self.profile.capabilities.axes.len()];
        self.wcs_index = 0;
        self.feed_override = 100;
        self.spindle_override = 100;
        self.rapid_override = 100;
        // Runtime switches belong to the controller session and clear on a
        // new one; the next poll copies the engine accessors again.
        self.optional_stop = false;
        self.single_block = false;
        self.block_delete = false;
        self.run_options_open = false;
        self.run_options_error = None;
        self.check_confirm_open = false;
        self.start_confirm = None;
        self.sleep_confirm_open = false;
        self.mdi_history_index = None;
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
        self.signals_raw = CoreSignals::NONE;
        self.signal_caps.clear();
        self.mpg_active = false;
        self.controller_reset.reset_session();
        // Metadata belongs to the controller, never to the previous session.
        self.settings = SettingsModel::default();
        // Offsets and the tool table are reloaded after `$I+`; never show the
        // previous machine's data as if it belonged to this one.
        self.offsets = OffsetModel::default();
        self.offsets_backup.close_preview();
        self.sync_offset_context();
        self.tools = ToolTableModel::new();
        // Controller-reported data belongs to this session only.
        self.controller_tools = ControllerToolTable::new();
        self.startup_lines.reset();
        // Pin/limit diagnostics are read-only controller reports; the engine
        // clears them for a new session and the mirrors follow it.
        self.pins.reset_session();
        self.user_macros.reset();
        self.utilities.reset_session();
        self.probe = ProbeModel::default();
        self.height_map.reset();
        self.height_map_library.reset();
        self.config.begin_connection();
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
        self.settings.set_axis_labels(
            self.profile
                .capabilities
                .axes
                .iter()
                .map(|axis| axis.label().to_string())
                .collect(),
        );
        self.sync_offset_context();
        if let Some(load) = self.config.on_controller(info) {
            // A newly loaded profile owns a different set of saved maps and
            // macros; an unconfirmed run from the previous controller dies here.
            self.height_map_library.reset();
            self.user_macros.reset();
            let unverified = load.unverified;
            let corrupt = self.config.corrupt_profile();
            let status = if corrupt {
                "PROFILE CORRUPT".to_string()
            } else if unverified {
                "UNVERIFIED - CONFIRM IN UTILITY".to_string()
            } else {
                format!("{} TOOLS, SAVED", self.config.data().tools.len())
            };
            if let Some(tools) = load.tools {
                self.tools.adopt(tools, load.path, status);
            } else {
                self.tools.set_path(load.path);
                self.tools.set_status(status);
            }
            let level = if load.unverified || corrupt {
                ConsoleLevel::Warn
            } else {
                ConsoleLevel::Info
            };
            self.push_console(level, load.notice);
            if let Some(error) = self.config.profile_error()
                && !load.unverified
                && !corrupt
            {
                self.push_console(ConsoleLevel::Warn, error.to_string());
            }
            // The profile owns the physical mount offset and the shared probe
            // defaults. Neither is sent to the controller.
            self.load_probe_offset();
            self.load_probe_defaults();
        }
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
        self.signals_raw = snapshot.signals;
        let previous_wcs = self.wcs_index;
        self.wcs_index = snapshot.wcs.index();
        if previous_wcs != self.wcs_index {
            self.sync_offset_context();
        }
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
            if let Some(mpg) = status.mpg {
                self.mpg_active = mpg;
            }
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

    /// Why a program file may not be loaded right now, or `None` when allowed.
    ///
    /// Browsing is deliberately independent of this gate: the file picker can
    /// stay open and useful in mock mode even though selecting a file cannot
    /// send a command.
    pub fn program_load_refusal(&self) -> Option<ProgramLoadRefusal> {
        if !self.live {
            Some(ProgramLoadRefusal::Mock)
        } else if !self.connection.is_ready_like() {
            Some(ProgramLoadRefusal::NotReady)
        } else if self.job.is_active() {
            Some(ProgramLoadRefusal::JobActive)
        } else {
            None
        }
    }

    /// Next rapid-override command for the compact touch control.
    ///
    /// The target comes from the value the controller last reported. This
    /// method never changes that value: only the next snapshot may, so a
    /// rejected or lost command can never leave the chip claiming a level the
    /// controller does not have.
    ///
    /// # Errors
    ///
    /// Returns the refusal reason in mock mode or while the link is not ready.
    pub fn rapid_override_step(&self) -> Result<Command, RapidOverrideRefusal> {
        if !self.live {
            return Err(RapidOverrideRefusal::Mock);
        }
        if !self.connection.is_ready_like() {
            return Err(RapidOverrideRefusal::NotReady);
        }
        Ok(Command::SetRapidOverride(next_rapid_override(
            self.rapid_override,
        )))
    }

    /// Feed override percentage produced by a fine or coarse step.
    ///
    /// The result is always inside the controller's `10..=200` range; the
    /// command itself is built by [`Self::feed_override_step`].
    pub fn feed_override_target(&self, delta: i16) -> u16 {
        let target = i32::from(self.feed_override) + i32::from(delta);
        target.clamp(i32::from(FEED_OVERRIDE_MIN), i32::from(FEED_OVERRIDE_MAX)) as u16
    }

    /// One fine (`-1`/`+1`) or coarse (`-10`/`+10`) feed-override command.
    ///
    /// This never changes [`Self::feed_override`]: the shell may update its
    /// optimistic display only when no slider drag owns the value, and the
    /// next snapshot remains authoritative.
    ///
    /// # Errors
    ///
    /// Returns the refusal reason in mock mode or while the link is not ready.
    pub fn feed_override_step(&self, delta: i16) -> Result<Command, FeedOverrideRefusal> {
        self.feed_override_command(self.feed_override_target(delta))
    }

    /// The exact `RESET 100%` feed-override command.
    ///
    /// # Errors
    ///
    /// Returns the refusal reason in mock mode or while the link is not ready.
    pub fn feed_override_reset(&self) -> Result<Command, FeedOverrideRefusal> {
        self.feed_override_command(FEED_OVERRIDE_RESET)
    }

    fn feed_override_command(&self, percent: u16) -> Result<Command, FeedOverrideRefusal> {
        if !self.live {
            return Err(FeedOverrideRefusal::Mock);
        }
        if !self.connection.is_ready_like() {
            return Err(FeedOverrideRefusal::NotReady);
        }
        Ok(Command::SetFeedOverride(
            percent.clamp(FEED_OVERRIDE_MIN, FEED_OVERRIDE_MAX),
        ))
    }

    /// Why the controller cannot sleep right now, or `None` when it can.
    ///
    /// The engine refuses `Sleep` unless the controller reports `Idle`; this
    /// mirror also blocks a paused job so the sheet is never a trap.
    pub fn sleep_refusal(&self) -> Option<SleepRefusal> {
        if !self.live {
            Some(SleepRefusal::Mock)
        } else if !self.connection.is_ready_like() {
            Some(SleepRefusal::NotReady)
        } else if self.job.is_active() {
            Some(SleepRefusal::JobActive)
        } else if self.machine_state != "Idle" {
            Some(SleepRefusal::NotIdle)
        } else {
            None
        }
    }

    /// The one command sent after the sleep sheet is confirmed.
    ///
    /// # Errors
    ///
    /// Returns the gate refusal when sleep is not available.
    pub fn sleep_command(&self) -> Result<Command, SleepRefusal> {
        match self.sleep_refusal() {
            Some(refusal) => Err(refusal),
            None => Ok(Command::Sleep),
        }
    }

    /// Open the sleep confirmation sheet. Opening sends nothing.
    pub fn open_sleep_confirm(&mut self) {
        self.sleep_confirm_open = true;
    }

    /// Close the sleep confirmation sheet without sending anything.
    pub fn cancel_sleep_confirm(&mut self) {
        self.sleep_confirm_open = false;
    }

    /// Send exactly one confirmed `Sleep` through `send`.
    ///
    /// The sheet must already be open. It is consumed before the gate is
    /// checked, so a refusal or rejection closes the sheet and changes no
    /// displayed value. Without an open confirmation nothing is sent.
    pub fn dispatch_sleep(
        &mut self,
        send: impl FnOnce(Command) -> Result<(), String>,
    ) -> SleepOutcome {
        if !self.sleep_confirm_open {
            return SleepOutcome::NotConfirmed;
        }
        self.sleep_confirm_open = false;
        let command = match self.sleep_command() {
            Ok(command) => command,
            Err(refusal) => return SleepOutcome::Refused(refusal),
        };
        match send(command) {
            Ok(()) => SleepOutcome::Sent,
            Err(detail) => SleepOutcome::Rejected(detail),
        }
    }

    // ------------------------------------------------------- setup wizard

    /// Open the first-run safety/setup wizard at SAFETY.
    ///
    /// The checklist starts unchecked and the unit preference is prefilled from
    /// the current app config. Opening the overlay sends nothing to the
    /// controller.
    pub fn open_setup(&mut self) {
        let units = self.config.app().units;
        self.setup.open(units);
    }

    /// Offer the first-run wizard once in this app run.
    ///
    /// Returns `true` only when the overlay was opened. A completed setup, a
    /// demo flag that already selects the initial screen, or a second call in
    /// the same run all return `false`. This is what keeps "REMIND ME LATER"
    /// from re-opening the wizard until the next launch.
    pub fn auto_open_setup(&mut self, demo_initial_screen: bool) -> bool {
        if self.setup_auto_prompted {
            return false;
        }
        self.setup_auto_prompted = true;
        if demo_initial_screen || self.config.setup_complete() {
            return false;
        }
        self.open_setup();
        true
    }

    /// Demo/screenshot aid: open UNITS with the checklist acknowledged.
    pub fn open_setup_units_for_demo(&mut self) {
        let units = self.config.app().units;
        self.setup.open_units_for_demo(units);
    }

    /// Demo/screenshot aid: open REVIEW with the checklist acknowledged.
    pub fn open_setup_review_for_demo(&mut self) {
        let units = self.config.app().units;
        self.setup.open_review_for_demo(units);
    }

    /// Toggle one required safety acknowledgement.
    pub fn setup_toggle_safety(&mut self, item: SafetyItem) {
        self.setup.toggle_safety(item);
    }

    /// Advance one wizard page, honoring the SAFETY gate.
    pub fn setup_next(&mut self) -> bool {
        self.setup.next()
    }

    /// Go back one wizard page, preserving acknowledgements and units.
    pub fn setup_back(&mut self) -> bool {
        self.setup.back()
    }

    /// Select the app display-unit preference. This never sends `G20`/`G21`.
    pub fn setup_set_units(&mut self, units: CoreUnits) {
        self.setup.set_units(units);
    }

    /// Close the wizard without completing setup. The saved flag is untouched.
    pub fn remind_later_setup(&mut self) {
        self.setup.close();
    }

    /// Complete the wizard: persist the unit preference, then the flag.
    ///
    /// The overlay closes only when [`ConfigModel::set_setup_complete`]
    /// succeeds. A save failure keeps it open, records the reason, and leaves
    /// `setup_complete` false. No controller command is built or sent.
    ///
    /// # Errors
    ///
    /// Returns the save error when the app config cannot be written, or a local
    /// gate message when the operator is not on the reviewed summary.
    pub fn finish_setup(&mut self) -> Result<PathBuf, String> {
        if self.setup.step() != SetupStep::Review || !self.setup.all_safety_checked() {
            let message =
                "every safety item must be acknowledged on the review page before setup can finish";
            self.setup.set_error(message);
            return Err(message.to_string());
        }
        let units = self.setup.units();
        self.config.set_units(units);
        match self.config.set_setup_complete(true) {
            Ok(path) => {
                self.setup.close();
                Ok(path)
            }
            Err(error) => {
                self.setup
                    .set_error(format!("Setup was not saved: {error}"));
                Err(error)
            }
        }
    }

    /// Copy the engine's run-switch accessors into the view model.
    ///
    /// This is the only writer of the three displayed values after startup.
    /// There is no optimistic target: a tap sends a command, and the row
    /// changes only when a later poll reads the accepted accessor back.
    ///
    /// Returns `true` when at least one displayed value changed.
    pub fn sync_run_options(
        &mut self,
        optional_stop: bool,
        single_block: bool,
        block_delete: bool,
    ) -> bool {
        let mut changed = false;
        if self.optional_stop != optional_stop {
            self.optional_stop = optional_stop;
            changed = true;
        }
        if self.single_block != single_block {
            self.single_block = single_block;
            changed = true;
        }
        if self.block_delete != block_delete {
            self.block_delete = block_delete;
            changed = true;
        }
        changed
    }

    /// Authoritative value displayed for one run switch.
    pub fn run_option_value(&self, option: RunOption) -> bool {
        match option {
            RunOption::OptionalStop => self.optional_stop,
            RunOption::SingleBlock => self.single_block,
            RunOption::BlockDelete => self.block_delete,
        }
    }

    /// Whether one run switch can be changed right now.
    ///
    /// Optional stop and single block are real-time switches the engine accepts
    /// whenever the link is ready, including while a job runs. Block delete is
    /// idle-only, so a running or paused job locks it before the command is
    /// built.
    pub fn run_option_refusal(&self, option: RunOption) -> Option<RunOptionRefusal> {
        if !self.live {
            return Some(RunOptionRefusal::Mock);
        }
        if !self.connection.is_ready_like() {
            return Some(RunOptionRefusal::NotReady);
        }
        if option == RunOption::BlockDelete && self.job.is_active() {
            return Some(RunOptionRefusal::JobActive);
        }
        None
    }

    /// The single command that flips one run switch.
    ///
    /// # Errors
    ///
    /// Returns the refusal reason when the switch is locked in this state.
    pub fn run_option_command(&self, option: RunOption) -> Result<Command, RunOptionRefusal> {
        if let Some(refusal) = self.run_option_refusal(option) {
            return Err(refusal);
        }
        let enabled = !self.run_option_value(option);
        Ok(match option {
            RunOption::OptionalStop => Command::SetOptionalStop(enabled),
            RunOption::SingleBlock => Command::SetSingleBlock(enabled),
            RunOption::BlockDelete => Command::SetBlockDelete(enabled),
        })
    }

    /// Send one run-switch toggle through `send`, exactly once.
    ///
    /// The displayed value is never changed here. A gate refusal sends nothing;
    /// a sender rejection keeps the previous value and records the reason for
    /// the open sheet.
    pub fn dispatch_run_option(
        &mut self,
        option: RunOption,
        send: impl FnOnce(Command) -> Result<(), String>,
    ) -> RunOptionOutcome {
        let command = match self.run_option_command(option) {
            Ok(command) => command,
            Err(refusal) => {
                self.run_options_error =
                    Some(format!("{} BLOCKED: {}", option.label(), refusal.message()));
                return RunOptionOutcome::Refused(refusal);
            }
        };
        match send(command) {
            Ok(()) => {
                self.run_options_error = None;
                RunOptionOutcome::Sent
            }
            Err(detail) => {
                let message = format!("{} NOT CHANGED: {detail}", option.label());
                self.run_options_error = Some(message.clone());
                RunOptionOutcome::Rejected(message)
            }
        }
    }

    /// Reason line shown under one run-option row.
    pub fn run_option_hint(&self, option: RunOption) -> &'static str {
        self.run_option_refusal(option)
            .map_or_else(|| option.detail(), RunOptionRefusal::hint)
    }

    /// Sheet-wide lock notice, or `None` when every switch is available.
    pub fn run_options_lock_reason(&self) -> Option<&'static str> {
        if !self.live {
            Some(RunOptionRefusal::Mock.message())
        } else if !self.connection.is_ready_like() {
            Some(RunOptionRefusal::NotReady.message())
        } else if self.job.is_active() {
            Some(RunOptionRefusal::JobActive.message())
        } else {
            None
        }
    }

    // ------------------------------------------- program breakpoints / run-to

    /// Shared gate for breakpoint and run-to actions.
    ///
    /// Breakpoints can be changed while a job runs or pauses, but never in
    /// mock mode, on a stale link, or while the machine is locked.
    pub fn breakpoint_refusal(&self) -> Option<BreakpointRefusal> {
        if !self.live {
            return Some(BreakpointRefusal::Mock);
        }
        if !self.connection.is_ready_like() {
            return Some(BreakpointRefusal::NotReady);
        }
        if matches!(self.machine_state, "Alarm" | "Door") {
            return Some(BreakpointRefusal::MachineLocked);
        }
        if !self.job.has_program() {
            return Some(BreakpointRefusal::NoProgram);
        }
        None
    }

    /// The one command that flips one breakpoint.
    ///
    /// The command always carries the resulting `enabled` state, never a target
    /// that the shell has to infer. Re-arming the line where the job is already
    /// paused is refused so RESUME cannot immediately pause there again.
    ///
    /// # Errors
    ///
    /// Returns the gate or line refusal before any command exists.
    pub fn breakpoint_command(&self, line: u32) -> Result<Command, BreakpointRefusal> {
        if let Some(refusal) = self.breakpoint_refusal() {
            return Err(refusal);
        }
        if !self.job.has_runnable_line(line) {
            return Err(BreakpointRefusal::LineOutOfRange);
        }
        let enabled = !self.job.breakpoints.contains(line);
        if enabled
            && self.job.pause_reason == Some(JobPauseReason::Breakpoint)
            && self.job.current_line == line
        {
            return Err(BreakpointRefusal::PausedAtLine);
        }
        Ok(Command::SetBreakpoint { line, enabled })
    }

    /// Send one breakpoint toggle through `send`, exactly once.
    ///
    /// The local mirror changes only after the engine accepts the command. A
    /// gate refusal sends nothing; a sender rejection leaves the marker and
    /// count unchanged.
    pub fn dispatch_toggle_breakpoint(
        &mut self,
        line: u32,
        send: impl FnOnce(Command) -> Result<(), String>,
    ) -> BreakpointOutcome {
        let command = match self.breakpoint_command(line) {
            Ok(command) => command,
            Err(refusal) => {
                let message = format!("Breakpoint line {line}: {}", refusal.message());
                self.job.breakpoints.error = Some(message);
                return BreakpointOutcome::Refused(refusal);
            }
        };
        let enabled = matches!(command, Command::SetBreakpoint { enabled: true, .. });
        match send(command) {
            Ok(()) => {
                self.job.breakpoints.set_line(line, enabled);
                self.job.breakpoints.error = None;
                BreakpointOutcome::Sent { line, enabled }
            }
            Err(detail) => {
                let message = format!("Breakpoint at line {line} not changed: {detail}");
                self.job.breakpoints.error = Some(message.clone());
                BreakpointOutcome::Rejected(message)
            }
        }
    }

    /// Select one Program-list row, or clear the selection.
    pub fn select_program_line(&mut self, line: Option<u32>) {
        self.job.breakpoints.select(line);
    }

    /// Why run-to is unavailable for one line.
    ///
    /// An active running or paused job is not a refusal: the confirmation then
    /// only sets the breakpoint. A start needs a ready link, an unlocked
    /// machine, an idle machine, and a startable job state.
    pub fn run_to_refusal(&self, line: Option<u32>) -> Option<RunToRefusal> {
        if let Some(refusal) = self.breakpoint_refusal() {
            return Some(refusal.into());
        }
        if self.probe.owns_program(&self.job.name) {
            return Some(RunToRefusal::ProbeProgram);
        }
        let Some(line) = line else {
            return Some(RunToRefusal::NoSelection);
        };
        if !self.job.has_runnable_line(line) {
            return Some(RunToRefusal::LineOutOfRange);
        }
        if self.job.pause_reason == Some(JobPauseReason::Breakpoint)
            && self.job.current_line == line
        {
            return Some(RunToRefusal::PausedAtLine);
        }
        if !self.job.is_active() {
            if !matches!(
                self.job.state,
                JobState::Idle | JobState::Completed | JobState::Aborted | JobState::Interrupted
            ) {
                return Some(RunToRefusal::MachineNotIdle);
            }
            if self.machine_state != "Idle" {
                return Some(RunToRefusal::MachineNotIdle);
            }
        }
        None
    }

    /// Open the run-to confirmation for one line. Nothing is sent.
    ///
    /// # Errors
    ///
    /// Returns the gate or line refusal; no sheet opens then.
    pub fn request_run_to(&mut self, line: u32) -> Result<(), RunToRefusal> {
        if let Some(refusal) = self.run_to_refusal(Some(line)) {
            self.job.breakpoints.error = Some(refusal.message().to_string());
            return Err(refusal);
        }
        let source = self.job.line_source(line).unwrap_or_default().to_string();
        self.job.breakpoints.select(Some(line));
        self.job.breakpoints.pending_run_to = Some(RunToConfirmation { line, source });
        self.job.breakpoints.error = None;
        Ok(())
    }

    /// Open run-to for the selected row. Nothing is sent.
    ///
    /// # Errors
    ///
    /// Returns [`RunToRefusal::NoSelection`] when no row is selected.
    pub fn request_run_to_selected(&mut self) -> Result<(), RunToRefusal> {
        let Some(line) = self.job.breakpoints.selected() else {
            self.job.breakpoints.error = Some(RunToRefusal::NoSelection.message().to_string());
            return Err(RunToRefusal::NoSelection);
        };
        self.request_run_to(line)
    }

    /// Close the run-to confirmation. Nothing is sent.
    pub fn cancel_run_to(&mut self) -> bool {
        let had = self.job.breakpoints.pending_run_to.take().is_some();
        self.job.breakpoints.error = None;
        had
    }

    /// Whether confirming the open sheet would also start the job.
    pub fn run_to_will_start(&self) -> bool {
        let Some(pending) = self.job.breakpoints.pending_run_to() else {
            return false;
        };
        self.run_to_refusal(Some(pending.line)).is_none() && !self.job.is_active()
    }

    /// Confirm the open run-to sheet.
    ///
    /// Always sends exactly one `SetBreakpoint { enabled: true }`. When no job
    /// is active it then uses the same normal-start gate as START: within
    /// travel or unchecked sends exactly one `StartProgram { from_line: None }`,
    /// while an over-travel report holds the start behind the confirmation
    /// sheet. The engine intentionally rejects `from_line > 1`, so this never
    /// claims start-from-line.
    pub fn confirm_run_to(
        &mut self,
        mut send: impl FnMut(Command) -> Result<(), String>,
    ) -> RunToOutcome {
        let Some(pending) = self.job.breakpoints.pending_run_to().cloned() else {
            return RunToOutcome::Refused(RunToRefusal::NoPending);
        };
        let line = pending.line;
        if let Some(refusal) = self.run_to_refusal(Some(line)) {
            self.job.breakpoints.error = Some(refusal.message().to_string());
            return RunToOutcome::Refused(refusal);
        }
        // An active job (running or paused) keeps its position: set the
        // breakpoint only, never start or resume it.
        let will_start = !self.job.is_active();

        if let Err(detail) = send(Command::SetBreakpoint {
            line,
            enabled: true,
        }) {
            let message = format!("Breakpoint at line {line} not set: {detail}");
            self.job.breakpoints.error = Some(message.clone());
            return RunToOutcome::Rejected(message);
        }
        self.job.breakpoints.set_line(line, true);
        self.job.breakpoints.pending_run_to = None;
        self.job.breakpoints.error = None;

        if !will_start {
            return RunToOutcome::BreakpointOnly { line };
        }
        // A run-to start is a real start, so it goes through the same gate as
        // START instead of sending `StartProgram` directly. The breakpoint is
        // already enabled when the gate holds the start.
        match self.dispatch_start_after_run_to(Some(line), send) {
            StartOutcome::Sent => RunToOutcome::Started { line },
            StartOutcome::ConfirmationRequired => RunToOutcome::StartHeld { line },
            StartOutcome::Rejected(detail) => {
                let message = format!("Breakpoint set at line {line}; start rejected: {detail}");
                self.job.breakpoints.error = Some(message.clone());
                RunToOutcome::StartRejected { line, message }
            }
            StartOutcome::Refused(refusal) => RunToOutcome::StartRejected {
                line,
                message: refusal.message().to_string(),
            },
            // Neither outcome is reachable from a fresh dispatch, but treat
            // one defensively as a refused start rather than claiming sent.
            StartOutcome::NotConfirmed | StartOutcome::Stale => RunToOutcome::StartRejected {
                line,
                message: "the start gate no longer matches the loaded program".to_string(),
            },
        }
    }

    /// Detail line for the current pause, including the breakpoint line.
    pub fn pause_detail(&self) -> Option<String> {
        match self.job.pause_reason {
            Some(JobPauseReason::Breakpoint) => {
                Some(format!("BREAKPOINT AT LINE {}", self.job.current_line))
            }
            Some(reason) => Some(pause_reason_label(reason).to_string()),
            None => None,
        }
    }

    /// Extra toolbar detail while a check run or its entry transition is live.
    pub fn check_detail(&self) -> Option<&'static str> {
        if self.job.check_pending() {
            Some("WAITING FOR CONTROLLER CHECK MODE ($C)")
        } else if self.job.checking() {
            Some("DRY RUN - NO MOTION")
        } else {
            None
        }
    }

    /// Open the `RUN OPTIONS` sheet. Nothing is sent.
    pub fn open_run_options(&mut self) {
        self.run_options_open = true;
        self.run_options_error = None;
    }

    /// Close the `RUN OPTIONS` sheet. Nothing is sent.
    pub fn close_run_options(&mut self) {
        self.run_options_open = false;
        self.run_options_error = None;
    }

    /// Estimated remaining job time for the Job header, when meaningful.
    pub fn display_eta(&self) -> Option<Duration> {
        self.job.eta()
    }

    /// Why a controller check run is unavailable right now.
    ///
    /// This gate mirrors the engine's own requirements: a live ready link, a
    /// loaded program, an idle controller, and no real job or check transition
    /// active. It never sends a command.
    pub fn check_refusal(&self) -> Option<CheckRefusal> {
        if !self.live {
            return Some(CheckRefusal::Mock);
        }
        if !self.connection.is_ready_like() {
            return Some(CheckRefusal::NotReady);
        }
        if matches!(self.machine_state, "Alarm" | "Door") {
            return Some(CheckRefusal::MachineLocked);
        }
        if !self.job.has_program() {
            return Some(CheckRefusal::NoProgram);
        }
        if self.probe.owns_program(&self.job.name) {
            return Some(CheckRefusal::ProbeProgram);
        }
        if self.job.check_busy() {
            return Some(CheckRefusal::CheckActive);
        }
        if self.job.is_active() {
            return Some(CheckRefusal::JobActive);
        }
        if !matches!(
            self.job.state,
            JobState::Idle | JobState::Completed | JobState::Aborted | JobState::Interrupted
        ) {
            return Some(CheckRefusal::JobActive);
        }
        if self.machine_state != "Idle" {
            return Some(CheckRefusal::MachineNotIdle);
        }
        None
    }

    /// Open the check confirmation sheet. Nothing is sent.
    ///
    /// # Errors
    ///
    /// Returns the gate refusal; no sheet opens then.
    pub fn request_check(&mut self) -> Result<(), CheckRefusal> {
        if let Some(refusal) = self.check_refusal() {
            return Err(refusal);
        }
        self.check_confirm_open = true;
        Ok(())
    }

    /// Close the check confirmation sheet without sending anything.
    pub fn cancel_check_confirm(&mut self) -> bool {
        std::mem::take(&mut self.check_confirm_open)
    }

    /// Confirm the open check sheet with exactly one `StartCheckRun`.
    ///
    /// The sheet is consumed before the gate is checked, so a refusal or
    /// rejection closes it and leaves the job view unchanged. Without an open
    /// confirmation nothing is sent.
    pub fn confirm_check(
        &mut self,
        send: impl FnOnce(Command) -> Result<(), String>,
    ) -> CheckOutcome {
        if !std::mem::take(&mut self.check_confirm_open) {
            return CheckOutcome::NotConfirmed;
        }
        if let Some(refusal) = self.check_refusal() {
            return CheckOutcome::Refused(refusal);
        }
        match send(Command::StartCheckRun) {
            Ok(()) => {
                self.job.begin_check();
                CheckOutcome::Started
            }
            Err(detail) => CheckOutcome::Rejected(detail),
        }
    }

    /// Whether an over-travel START confirmation is open.
    #[must_use]
    pub fn start_confirm_open(&self) -> bool {
        self.start_confirm.is_some()
    }

    /// Limits report captured by the open START confirmation.
    #[must_use]
    pub fn pending_start_limits(&self) -> Option<&ProgramLimits> {
        self.start_confirm.as_ref().map(StartConfirmation::limits)
    }

    /// Run-to line whose breakpoint is already set for the open START
    /// confirmation, when the held start came from run-to.
    #[must_use]
    pub fn pending_start_run_to_line(&self) -> Option<u32> {
        self.start_confirm
            .as_ref()
            .and_then(StartConfirmation::run_to_line)
    }

    /// Close the START confirmation without sending anything.
    ///
    /// Returns `true` when a sheet was open.
    pub fn cancel_start_confirm(&mut self) -> bool {
        self.start_confirm.take().is_some()
    }

    /// Send exactly one normal `StartProgram { from_line: None }`.
    ///
    /// Check mode has its own `StartCheckRun`; this path never selects it. A
    /// pending or active check refuses the start without sending, and an
    /// accepted normal start clears the dry-run label at once (the engine's
    /// `check_run` accessor confirms it on the next poll).
    ///
    /// When `ProgramLimits` reports any overrun this opens the confirmation
    /// sheet instead of sending. A program whose travel comparison was not
    /// checked keeps the previous direct-start behavior; the comparison is
    /// informational and never a controller soft-limit enforcement.
    pub fn dispatch_start(
        &mut self,
        send: impl FnOnce(Command) -> Result<(), String>,
    ) -> StartOutcome {
        self.dispatch_start_after_run_to(None, send)
    }

    /// Shared normal-start path for a direct START and a run-to start.
    ///
    /// `run_to_line` is `Some` only after `confirm_run_to` has set that line's
    /// breakpoint. It is carried into the confirmation so the sheet can say
    /// which breakpoint is already armed; it changes no command.
    fn dispatch_start_after_run_to(
        &mut self,
        run_to_line: Option<u32>,
        send: impl FnOnce(Command) -> Result<(), String>,
    ) -> StartOutcome {
        if self.job.check_busy() {
            return StartOutcome::Refused(CheckRefusal::CheckActive);
        }
        // A stale sheet must not hold a start after the link, program, or
        // machine state changed. Discard it, then evaluate the current program.
        self.sync_start_confirm();
        if self.start_confirm.is_some() {
            return StartOutcome::ConfirmationRequired;
        }
        let limits = self.program_limits();
        if limits.is_over_travel()
            && let Some(program) = self.job.program().cloned()
        {
            self.start_confirm = Some(StartConfirmation {
                limits,
                program,
                run_to_line,
            });
            return StartOutcome::ConfirmationRequired;
        }
        self.send_normal_start(send)
    }

    /// Confirm the open over-travel sheet with exactly one `StartProgram`.
    ///
    /// The sheet is consumed before the gate is checked. A program, link,
    /// machine-state, or limits change since the sheet opened returns
    /// [`StartOutcome::Stale`] without sending, so a stale start can never fire
    /// later. Cancelling and confirming with no open sheet send nothing.
    pub fn confirm_start(
        &mut self,
        send: impl FnOnce(Command) -> Result<(), String>,
    ) -> StartOutcome {
        let Some(pending) = self.start_confirm.take() else {
            return StartOutcome::NotConfirmed;
        };
        if self.job.check_busy() {
            return StartOutcome::Refused(CheckRefusal::CheckActive);
        }
        if !self.start_confirmation_is_current(&pending) {
            return StartOutcome::Stale;
        }
        self.send_normal_start(send)
    }

    /// Clear the START confirmation when it no longer matches the live state.
    ///
    /// The shell calls this every poll so a program change, a dropped link, a
    /// machine that leaves `Idle`, or a changed limits report closes the sheet
    /// even when the operator never presses a button. Returns `true` when a
    /// stale sheet was discarded.
    pub fn sync_start_confirm(&mut self) -> bool {
        let stale = self
            .start_confirm
            .as_ref()
            .is_some_and(|pending| !self.start_confirmation_is_current(pending));
        if !stale {
            return false;
        }
        self.start_confirm = None;
        true
    }

    fn start_confirmation_is_current(&self, pending: &StartConfirmation) -> bool {
        self.live
            && self.connection.is_ready_like()
            && self.machine_state == "Idle"
            && !self.job.is_active()
            && self
                .job
                .program()
                .is_some_and(|program| Arc::ptr_eq(program, &pending.program))
            && self.program_limits() == pending.limits
    }

    fn send_normal_start(
        &mut self,
        send: impl FnOnce(Command) -> Result<(), String>,
    ) -> StartOutcome {
        let command = Command::StartProgram { from_line: None };
        match send(command) {
            Ok(()) => {
                self.job.clear_check();
                StartOutcome::Sent
            }
            Err(detail) => StartOutcome::Rejected(detail),
        }
    }

    /// The one stop command for the current job.
    ///
    /// A real job keeps the soft reset so buffered motion cannot execute
    /// afterwards. A dry run has no motion to discard; it stops feeding and
    /// lets the engine restore `Idle` without a reset.
    pub fn stop_program_command(&self) -> Command {
        Command::StopProgram {
            reset: !self.job.is_check(),
        }
    }

    /// Register the exact program source and build the one load command.
    ///
    /// The source is remembered before the engine parses it, because
    /// `JobLoaded` only carries the parsed program. This is what keeps the
    /// height-map Apply/Revert workflow on the exact bytes that were sent.
    /// The command loads the program for review; it never starts, applies, or
    /// changes offsets or settings.
    ///
    /// # Errors
    ///
    /// Returns the refusal reason when mock, not ready, or a job is active.
    pub fn program_load(
        &mut self,
        name: &str,
        source: &str,
    ) -> Result<Command, ProgramLoadRefusal> {
        if let Some(refusal) = self.program_load_refusal() {
            return Err(refusal);
        }
        self.height_map.remember_exact_source(name, source);
        Ok(Command::LoadProgram {
            name: name.to_string(),
            source: source.to_string(),
        })
    }

    /// Build the one load command for a confirmed surfacing preview.
    ///
    /// # Errors
    ///
    /// Returns a message when no preview exists, the operator has not
    /// acknowledged the generated-program warning, or the normal program-load
    /// gate refuses (mock, not ready, or a job is active).
    pub fn take_surfacing_load(&mut self) -> Result<UtilityLoad, String> {
        if !self.utilities.surfacing_confirmed() {
            return Err(
                "confirm that the program is generated, not checked against the machine"
                    .to_string(),
            );
        }
        let Some(preview) = self.utilities.surfacing_preview() else {
            return Err("generate the surfacing program before loading it".to_string());
        };
        let name = preview.name.clone();
        let source = preview.source.clone();
        let command = self
            .program_load(&name, &source)
            .map_err(|refusal| refusal.message().to_string())?;
        Ok(UtilityLoad { name, command })
    }

    /// Build the one load command for a confirmed transform preview.
    ///
    /// This loads the previewed source and never starts or applies
    /// compensation.
    ///
    /// # Errors
    ///
    /// Returns a message when no preview exists, the operator has not
    /// acknowledged the transformed-program warning, or the normal program-load
    /// gate refuses (mock, not ready, or a job is active).
    pub fn take_transform_load(&mut self) -> Result<UtilityLoad, String> {
        if !self.utilities.transform_confirmed() {
            return Err("confirm the transformed source before loading it".to_string());
        }
        let Some(preview) = self.utilities.transform_preview() else {
            return Err("generate a transform preview before loading it".to_string());
        };
        let name = preview.name.clone();
        let source = preview.source.clone();
        let command = self
            .program_load(&name, &source)
            .map_err(|refusal| refusal.message().to_string())?;
        Ok(UtilityLoad { name, command })
    }

    /// Reported steps/mm for one calibration axis.
    ///
    /// Only a finite value inside the wizard's accepted range is returned, so a
    /// malformed `$100`/`$101`/`$102` report becomes "not reported" instead of
    /// an unsafe baseline.
    #[must_use]
    pub fn calibration_current_steps(&self, axis: CalibrationAxis) -> Option<f64> {
        self.settings
            .row(axis.setting_number())
            .and_then(|row| row.current.as_f64())
            .filter(|value| {
                value.is_finite()
                    && *value >= crate::utilities::CALIBRATION_MIN_STEPS_PER_MM
                    && *value <= crate::utilities::CALIBRATION_MAX_STEPS_PER_MM
            })
    }

    /// Machine/connection gate for a `$100`/`$101`/`$102` calibration write.
    #[must_use]
    pub fn calibration_gate(&self) -> CalibrationGate {
        CalibrationGate {
            live: self.live,
            ready: self.connection.is_ready_like(),
            idle: self.machine_state == "Idle",
            alarm_or_door: matches!(self.machine_state, "Alarm" | "Door"),
            job_active: self.job.is_active(),
        }
    }

    /// Recalculate the calibration proposal from the reported `$10x` value.
    ///
    /// # Errors
    ///
    /// Returns the wizard's field or range message.
    pub fn calculate_calibration(&mut self) -> Result<(), String> {
        let axis = self.utilities.calibration().axis();
        let current = self.calibration_current_steps(axis);
        self.utilities.calibration_mut().calculate(current)
    }

    /// Recalculate the gantry-squaring result.
    ///
    /// This is pure local arithmetic: it returns no command and never moves,
    /// writes, or adjusts the machine. Opening the calculator and pressing
    /// CALCULATE are both send-free.
    ///
    /// # Errors
    ///
    /// Returns the wizard's field or measurement message.
    pub fn calculate_gantry_squaring(&mut self) -> Result<(), String> {
        self.utilities.gantry_mut().calculate()
    }

    /// Build the one confirmed calibration write after every safety gate.
    ///
    /// The returned command is always `Command::SetSetting`; it never jogs,
    /// moves, or starts a program. The engine writes the value and verifies the
    /// read-back before the model shows `APPLIED`.
    ///
    /// # Errors
    ///
    /// Returns the gate refusal, a missing/invalid `$10x` value, missing
    /// confirmation, or a changed current value.
    pub fn take_calibration_write(&mut self) -> Result<CalibrationWrite, String> {
        if let Some(reason) = self.calibration_gate().blocked_reason() {
            return Err(reason.to_string());
        }
        if self.settings.inflight_number().is_some() || self.settings.has_queued_write() {
            return Err("a settings write is already in progress".to_string());
        }
        let axis = self.utilities.calibration().axis();
        let current = self.calibration_current_steps(axis);
        self.utilities.calibration_mut().take_write(current)
    }

    /// Read-only program limits for the Utility card.
    ///
    /// Travel values come from `$20` and `$130`/`$131`/`$132` as last reported
    /// by the controller. The machine estimate adds the active WCS plus `G92`;
    /// it does not add TLO. When any of those values is unavailable the result
    /// says NOT CHECKED rather than implying that the program is safe.
    #[must_use]
    pub fn program_limits(&self) -> ProgramLimits {
        let travel = TravelSettings {
            soft_limits: self.setting_bool(20),
            travel_mm: [
                self.setting_f64(130),
                self.setting_f64(131),
                self.setting_f64(132),
            ],
        };
        ProgramLimits::evaluate(
            self.job.program().map(Arc::as_ref),
            travel,
            self.travel_work_offset_mm(),
        )
    }

    fn setting_f64(&self, number: u16) -> Option<f64> {
        self.settings
            .row(number)
            .and_then(|row| row.current.as_f64())
            .filter(|value| value.is_finite())
    }

    fn setting_bool(&self, number: u16) -> Option<bool> {
        self.settings
            .row(number)
            .and_then(|row| match row.current.as_bool() {
                Some(value) => Some(value),
                None => row.current.as_f64().map(|value| value != 0.0),
            })
    }

    /// Active WCS plus `G92`, in millimetres, when `$#` was reported.
    fn travel_work_offset_mm(&self) -> Option<[f64; 3]> {
        if !self.offsets.reported() {
            return None;
        }
        let wcs = self.offsets.wcs_position(self.offsets.active_wcs())?;
        let g92 = self.offsets.g92_position().unwrap_or_else(Position::zero);
        let sum = |axis: CoreAxis| wcs.get(axis) + g92.get(axis);
        Some([sum(CoreAxis::X), sum(CoreAxis::Y), sum(CoreAxis::Z)])
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

    // ------------------------------------------------------- program search

    /// Replace the Program-tab search text and rebuild its match list.
    ///
    /// A numeric query (`120` or `N120`) matches that exact source line; any
    /// other query is a case-insensitive substring of the source text.
    pub fn set_program_search_query(&mut self, query: impl Into<String>) {
        self.program_search.set_query(query);
        self.sync_program_search();
    }

    /// Switch between filtered and highlighted Program-tab results.
    pub fn set_program_search_mode(&mut self, mode: ProgramSearchMode) {
        self.program_search.set_mode(mode);
        self.sync_program_search();
    }

    /// Clear the search, restoring the full program list.
    pub fn clear_program_search(&mut self) {
        self.program_search.clear();
        self.sync_program_search();
    }

    /// Rebuild the search matches when the query or program changed.
    ///
    /// This is deliberately cheap when nothing changed: it only compares the
    /// query and the program identity. The parsed program is never re-parsed.
    ///
    /// Returns `true` when the match list was recomputed.
    pub fn sync_program_search(&mut self) -> bool {
        if let Some(program) = self.job.program() {
            let key = Arc::as_ptr(program) as usize;
            if !self.program_search.needs_refresh(key) {
                return false;
            }
            self.program_search.refresh(program.as_ref(), key);
            return true;
        }
        if self.live {
            let key = 0;
            if !self.program_search.needs_refresh(key) {
                return false;
            }
            let empty: &[ProgramLine] = &[];
            self.program_search.refresh(empty, key);
            return true;
        }
        let key = 1;
        if !self.program_search.needs_refresh(key) {
            return false;
        }
        self.program_search.refresh(&self.program[..], key);
        true
    }

    /// Rows the Program tab should render for the current search.
    pub fn program_search_visible_count(&self) -> usize {
        self.program_search
            .visible_count(self.display_program_lines())
    }

    /// Visible row of the current match, for the list's scroll handle.
    pub fn program_search_scroll_index(&self) -> Option<usize> {
        self.program_search
            .current_visible_index(self.display_program_lines())
    }

    /// Job status label, including the mock program's "loaded" state.
    pub fn display_job_status(&self) -> &'static str {
        if self.probe.owns_program(&self.job.name) {
            return "PROBE PLAN";
        }
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

    /// Whether the loaded job is a height-map compensated program.
    ///
    /// The job toolbar uses this to show the `HEIGHT MAP / COMPENSATED` marker;
    /// a normal program with the same text in its name is never marked.
    pub fn job_compensated(&self) -> bool {
        self.height_map.is_applied_for(&self.job.name)
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
        self.sync_offset_context();
        self.config.set_units(units);
    }

    /// Install the latest engine `$#` snapshot.
    ///
    /// A confirmed TLO write is verified against the reported `TLO` row here.
    /// The old value is never replaced optimistically: `APPLIED` appears only
    /// when the read-back matches the target.
    pub fn apply_offsets(&mut self, offsets: &Offsets) {
        self.sync_offset_context();
        self.offsets.apply_engine(offsets);
        if self
            .probe
            .on_offsets_reported(self.offsets.reported_tlo_z_mm())
        {
            let notice = self
                .probe
                .tool_length_write()
                .and_then(|write| match &write.status {
                    crate::probe::ToolLengthWriteStatus::Applied { reported_tlo_mm } => Some((
                        ConsoleLevel::Ok,
                        format!(
                            "TLO applied: {} verified by `$#` at {} mm",
                            write.line,
                            crate::probe::format_mm(*reported_tlo_mm)
                        ),
                    )),
                    crate::probe::ToolLengthWriteStatus::Failed { message } => Some((
                        ConsoleLevel::Error,
                        format!("TLO write FAILED: {message}; the old TLO is retained"),
                    )),
                    crate::probe::ToolLengthWriteStatus::AwaitingReadBack => None,
                });
            if let Some((level, message)) = notice {
                self.push_console(level, message);
            }
        }
    }

    /// Controller gate for the tool-length apply/clear actions.
    #[must_use]
    pub fn probe_tool_length_gate(&self) -> ToolLengthGate {
        ToolLengthGate {
            live: self.live,
            ready: self.connection.is_ready_like(),
            machine_state: MachineState::parse(self.machine_state),
            job_active: self.job.is_active(),
            offsets_pending: self.offsets.status().is_pending(),
        }
    }

    /// Open the inline confirmation for a TLO action. Nothing is sent.
    ///
    /// # Errors
    ///
    /// Returns the gate refusal or the probe model's build/completion error.
    pub fn request_probe_tool_length(&mut self, action: ToolLengthAction) -> Result<(), String> {
        self.probe_tool_length_gate().check()?;
        self.probe.request_tool_length_action(action)
    }

    /// Confirm and send one TLO write through the caller's sender.
    ///
    /// The model sends exactly one `Command::Mdi` line followed by one
    /// `Command::System(SystemCommand::Offsets)` request. No other command is
    /// produced by this path.
    ///
    /// # Errors
    ///
    /// Returns the gate refusal, a missing confirmation, a build error, or the
    /// sender rejection. The model records a failed write on a sender error.
    pub fn confirm_probe_tool_length(
        &mut self,
        send: impl FnMut(Command) -> Result<(), String>,
    ) -> Result<ToolLengthWrite, String> {
        let gate = self.probe_tool_length_gate();
        gate.check()?;
        self.probe.confirm_tool_length_action(gate, send)
    }

    /// Install the controller-reported, read-only `[T:...]` tool table.
    ///
    /// This never touches [`AppState::tools`]; the app table is editable and
    /// persisted, the controller table is evidence.
    pub fn apply_controller_tools(&mut self, entries: &[ToolEntry]) -> bool {
        self.controller_tools.apply(entries)
    }

    /// Install the controller startup lines from `$N`/`$N<n>`.
    ///
    /// A report for an index with a write in flight is treated as the read-back
    /// and compared with the requested text.
    pub fn apply_startup_lines(&mut self, lines: &[StartupLine]) -> bool {
        self.startup_lines.apply_reported(lines)
    }

    /// Install a `$PINS` map from the engine event or accessor.
    pub fn apply_pin_map(&mut self, map: Arc<Vec<PinMapping>>) -> bool {
        self.pins.apply_pin_map(map)
    }

    /// Install or clear the `$PINSTATE` auxiliary pin levels.
    pub fn apply_pin_state(&mut self, state: Option<Arc<PinState>>) -> bool {
        self.pins.apply_pin_state(state)
    }

    /// Install or clear the `$LIM` limit-switch state.
    pub fn apply_limit_state(&mut self, state: Option<Arc<LimitState>>) -> bool {
        self.pins.apply_limit_state(state)
    }

    /// Install or clear the `$LEV` last control and limit events.
    pub fn apply_last_events(&mut self, events: Option<Arc<LastEvents>>) -> bool {
        self.pins.apply_last_events(events)
    }

    /// Machine/connection gate for a `$N<n>` write.
    #[must_use]
    pub fn startup_write_gate(&self) -> StartupWriteGate {
        StartupWriteGate {
            live: self.live,
            ready: self.connection.is_ready_like(),
            idle: self.machine_state == "Idle",
            alarm_or_door: matches!(self.machine_state, "Alarm" | "Door"),
            job_active: self.job.is_active(),
        }
    }

    // ------------------------------------------------------------ user macros

    /// User-defined macros stored in the active controller profile.
    #[must_use]
    pub fn user_macro_list(&self) -> &[ConfigMacro] {
        self.config.macros()
    }

    /// Why a macro cannot be written to the active profile, if any.
    #[must_use]
    pub fn macro_save_blocker(&self) -> Option<String> {
        self.config.macro_save_blocker()
    }

    /// Connection/machine gate for running a user macro.
    ///
    /// This mirrors the engine's MDI rules: live and ready, not Alarm/Door, no
    /// active job, and the controller must report Idle.
    #[must_use]
    pub fn macro_gate(&self) -> MacroRunGate {
        MacroRunGate {
            live: self.live,
            ready: self.connection.is_ready_like(),
            machine_state: self.machine_state,
            job_active: self.job.is_active(),
        }
    }

    /// Add or update one macro and persist the full list in the profile.
    ///
    /// `original` is the stored name when editing. Names are trimmed and unique
    /// per profile. Nothing is sent to the controller.
    ///
    /// # Errors
    ///
    /// Returns a validation message, a missing profile, the corrupt/unverified
    /// save gate, or a write error.
    pub fn save_macro(
        &mut self,
        original: Option<&str>,
        name: &str,
        command: &str,
        confirm: bool,
    ) -> Result<PathBuf, String> {
        let mut macros = self.config.macros().to_vec();
        let record = crate::macros::build_macro(name, command, confirm, original, &macros)?;
        if let Some(original) = original {
            let original = original.trim();
            let Some(index) = macros.iter().position(|mac| mac.name.trim() == original) else {
                return Err(format!(
                    "the macro '{original}' no longer exists in this profile"
                ));
            };
            macros[index] = record;
        } else {
            macros.push(record);
        }
        self.config.save_macros(macros)
    }

    /// Delete one macro from the active profile by stored name.
    ///
    /// # Errors
    ///
    /// Returns a message when the name is not stored, the profile cannot be
    /// written, or the write fails.
    pub fn delete_macro(&mut self, name: &str) -> Result<PathBuf, String> {
        let name = name.trim();
        let mut macros = self.config.macros().to_vec();
        let before = macros.len();
        macros.retain(|mac| mac.name.trim() != name);
        if macros.len() == before {
            return Err(format!("no macro named '{name}' is stored in this profile"));
        }
        if self
            .user_macros
            .pending()
            .is_some_and(|pending| pending.name.trim() == name)
        {
            self.user_macros.cancel();
        }
        self.config.save_macros(macros)
    }

    /// Handle one explicit RUN click.
    ///
    /// A macro marked `confirm` returns [`MacroRunRequest::Confirm`] and no
    /// command is produced until [`AppState::confirm_macro_run`]. A direct
    /// macro returns [`MacroRunRequest::Send`] once.
    ///
    /// # Errors
    ///
    /// Returns the gate reason (mock, not connected, Alarm/Door, job active,
    /// not Idle), an unknown name, or a record that fails current validation.
    pub fn request_macro_run(&mut self, name: &str) -> Result<MacroRunRequest, String> {
        let gate = self.macro_gate();
        self.user_macros
            .request_run(self.config.macros(), name, gate)
    }

    /// Produce the one MDI command for a captured macro confirmation.
    ///
    /// # Errors
    ///
    /// Returns a message when no run is waiting or the gate closed.
    pub fn confirm_macro_run(&mut self) -> Result<Command, String> {
        let gate = self.macro_gate();
        self.user_macros.confirm(gate)
    }

    /// Close the macro confirmation sheet without producing a command.
    pub fn cancel_macro_run(&mut self) -> bool {
        self.user_macros.cancel().is_some()
    }

    /// Macro waiting for confirmation, if any.
    #[must_use]
    pub fn macro_run_pending(&self) -> Option<&ConfigMacro> {
        self.user_macros.pending()
    }

    /// Last macro gate refusal or send error.
    #[must_use]
    pub fn macro_last_error(&self) -> Option<&str> {
        self.user_macros.last_error()
    }

    /// Record an engine rejection. The caller must not retry.
    pub fn macro_send_failed(&mut self, message: impl Into<String>) {
        self.user_macros.send_failed(message);
    }

    /// Clear the last error after the engine accepted a macro.
    pub fn macro_sent(&mut self) {
        self.user_macros.sent();
    }

    /// Drop a pending macro confirmation and its error.
    ///
    /// Used after a profile import or an explicit "use local data" action.
    pub fn reset_macro_runs(&mut self) {
        self.user_macros.reset();
    }

    /// Update the active WCS used by the offsets workspace.
    pub fn set_wcs_index(&mut self, index: usize) {
        self.wcs_index = index.min(Wcs::ALL.len().saturating_sub(1));
        self.sync_offset_context();
    }

    /// Keep the offsets view's units, axes, and active WCS in sync.
    pub fn sync_offset_context(&mut self) {
        let units = self.profile.capabilities.units;
        let axes = self.profile.capabilities.axes.clone();
        self.offsets.set_context(units, &axes, self.wcs_index);
    }

    /// Settings snapshot for a local profile or backup.
    ///
    /// Password and unknown-type values are omitted; `ghs-config` documents
    /// that the caller must filter secrets before constructing a snapshot.
    #[must_use]
    pub fn settings_snapshot(&self) -> BTreeMap<u16, SettingValue> {
        self.settings
            .rows()
            .iter()
            .filter(|row| !matches!(row.kind, SettingKind::Password | SettingKind::Unknown(_)))
            .map(|row| (row.number, row.current.clone()))
            .collect()
    }

    /// Offsets for a local save or backup.
    ///
    /// Before `$#` has answered, the stored profile's snapshot is kept rather
    /// than replacing it with an empty one.
    #[must_use]
    pub fn profile_offset_snapshot(&self) -> ghs_config::OffsetSnapshot {
        if self.offsets.reported() {
            self.offsets.snapshot()
        } else {
            self.config.data().offsets.clone()
        }
    }

    /// Write the fixed `exports/offsets.nc` from the live `$#` snapshot.
    ///
    /// Local only: this never sends a command and never starts a job. A missing
    /// configuration root, a missing `$#` report, or an invalid value is
    /// refused by [`OffsetsBackupModel::export`].
    ///
    /// # Errors
    ///
    /// Returns the validation, directory, or write message.
    pub fn export_offsets_nc(&mut self) -> Result<OffsetsExport, String> {
        let root = self.config.store_root().map(std::path::Path::to_path_buf);
        self.offsets_backup.export(&self.offsets, root.as_deref())
    }

    /// Validate the fixed offsets file and open the preview sheet.
    ///
    /// Nothing is loaded, sent, or started. The file content is held for the
    /// explicit `LOAD AS PROGRAM` action.
    ///
    /// # Errors
    ///
    /// Returns a path-qualified I/O or validation message.
    pub fn open_offsets_preview(&mut self) -> Result<(), String> {
        let root = self.config.store_root().map(std::path::Path::to_path_buf);
        self.offsets_backup.open_preview(root.as_deref())
    }

    /// Close the offsets preview after CANCEL. Nothing is loaded or sent.
    pub fn cancel_offsets_preview(&mut self) {
        self.offsets_backup.cancel();
    }

    /// Re-validate the previewed file and return the one load command.
    ///
    /// The command is always `Command::LoadProgram`; it never starts the job.
    /// The shell sends it only after the operator pressed `LOAD AS PROGRAM`.
    ///
    /// # Errors
    ///
    /// Returns a message when no preview is open or the file is no longer
    /// valid.
    pub fn take_offsets_program(&mut self) -> Result<Command, String> {
        self.offsets_backup.load_program()
    }

    /// Settings for a local save or backup.
    ///
    /// A controller that has not reported settings yet keeps the stored
    /// snapshot instead of clearing it.
    #[must_use]
    pub fn profile_settings_snapshot(&self) -> BTreeMap<u16, SettingValue> {
        let live = self.settings_snapshot();
        if live.is_empty() {
            self.config.data().settings.clone()
        } else {
            live
        }
    }

    /// Export the fixed local backup bundle and update the reset card.
    ///
    /// This writes only `<config>/exports/ghs-backup.json`. It never sends a
    /// command, never touches the local controller profile, and does not
    /// change any saved height map. A successful export acknowledges the
    /// backup requirement for a destructive `$RST` in this session.
    ///
    /// # Errors
    ///
    /// Returns the configuration error when no profile is connected, the
    /// store is unavailable, or the write fails.
    pub fn export_backup_for_reset(&mut self) -> Result<PathBuf, String> {
        self.controller_reset.begin_backup_export();
        let tools = self.tools.table().clone();
        let offsets = self.profile_offset_snapshot();
        let settings = self.profile_settings_snapshot();
        let result = self.config.export_backup(&tools, &offsets, settings);
        match &result {
            Ok(path) => self
                .controller_reset
                .backup_export_succeeded(path.display().to_string()),
            Err(message) => self.controller_reset.backup_export_failed(message.clone()),
        }
        result
    }

    /// Copy the profile's probe mount offset into the editor fields.
    ///
    /// A missing profile or an old record loads as zero. This is setup data
    /// only; nothing is sent to the controller.
    pub fn load_probe_offset(&mut self) {
        let offset = self.config.probe_offset();
        let _ = self
            .probe
            .set_probe_offset(XyPoint::new(offset.x_mm, offset.y_mm));
    }

    /// Copy the profile's shared probe defaults into the editor fields.
    ///
    /// Only safe Z, search, feed, retract, diameter, edge offset, and plate
    /// thickness change. An active probe run is never overwritten. This is
    /// local setup data only; nothing is sent to the controller.
    pub fn load_probe_defaults(&mut self) {
        let defaults = self.config.probe_defaults();
        let _ = self.probe.apply_defaults(&defaults);
    }

    /// Whether the `--demo-probe-defaults` switch may press SAVE DEFAULTS.
    ///
    /// The controller profile must have loaded and no probe run may be active.
    #[must_use]
    pub fn probe_defaults_demo_ready(&self) -> bool {
        self.config.key().is_some() && !self.probe.is_running()
    }

    /// Save the editor's shared probe defaults into the active profile.
    ///
    /// This writes local setup data only. It never sends a command, setting,
    /// offset, or profile replacement to the controller.
    ///
    /// # Errors
    ///
    /// Returns a message when no controller profile is active, a probe run is
    /// active, a shared field is invalid, or the profile write fails. A failed
    /// save keeps the previous stored record.
    pub fn save_probe_defaults(&mut self) -> Result<PathBuf, String> {
        if self.config.key().is_none() {
            return Err(
                "no controller profile is active; connect and finish the handshake".to_string(),
            );
        }
        if self.probe.is_running() {
            return Err(
                "a probe run is active; finish or stop it before saving probe defaults".to_string(),
            );
        }
        let record = self.probe.defaults_record()?;
        self.config.save_probe_defaults(record)
    }

    /// Save the editor's probe mount offset into the active profile.
    ///
    /// The value is local setup data (`probe tip - controlled point`). This
    /// never writes an offset, setting, or command to the controller.
    ///
    /// # Errors
    ///
    /// Returns the field/plan validation message or the profile save error.
    pub fn save_probe_offset(&mut self) -> Result<PathBuf, String> {
        let offset = self.probe.probe_offset()?;
        self.config.save_probe_offset(ProbeOffsetRecord {
            x_mm: offset.x_mm,
            y_mm: offset.y_mm,
        })
    }

    /// Maps stored in the active controller profile.
    #[must_use]
    pub fn saved_height_maps(&self) -> &[HeightMapRecord] {
        self.config.height_maps()
    }

    /// Saved map currently shown read-only in the height-map workspace.
    #[must_use]
    pub fn selected_saved_map(&self) -> Option<&LoadedSavedMap> {
        self.height_map_library.selected()
    }

    /// Select one saved map for display.
    ///
    /// The loaded map only builds a view model. It never applies compensation,
    /// loads a program, or starts a job. The current part program is adopted as
    /// the future Apply/Revert original, which is bookkeeping only.
    ///
    /// # Errors
    ///
    /// Returns a message when a probe run is active, the name is not stored, or
    /// the record cannot be rebuilt.
    pub fn select_saved_height_map(&mut self, name: &str) -> Result<(), String> {
        if self.probe.is_running() {
            return Err(
                "a probe run is active; finish or stop it before opening a saved map".into(),
            );
        }
        let record = self
            .saved_height_maps()
            .iter()
            .find(|record| record.name == name)
            .cloned()
            .ok_or_else(|| format!("no saved map named '{name}'"))?;
        self.height_map_library.select(record)?;
        // A preview from the previous map must never survive the view change.
        self.height_map.cancel_pending();
        // Bookkeeping for a later explicit Apply. This does not apply or start.
        self.height_map.adopt_current_program_for_map();
        Ok(())
    }

    /// Clear the read-only saved-map selection.
    pub fn clear_saved_map_selection(&mut self) {
        // Leaving the browser also discards an unconfirmed rewrite.
        self.height_map.cancel_pending();
        self.height_map_library.clear_selection();
    }

    /// Save the current measured map into the active controller profile.
    ///
    /// The run may be complete or partial, but it must have at least one valid
    /// cell. Nothing is sent to the controller.
    ///
    /// # Errors
    ///
    /// Returns a message when there is no measured map, no valid cell, the
    /// profile cannot be written, or the record is invalid.
    pub fn save_current_height_map(
        &mut self,
        name: impl Into<String>,
        created_at_unix_s: u64,
    ) -> Result<PathBuf, String> {
        let name = name.into();
        let result = (|| {
            let run = self
                .probe
                .run()
                .ok_or_else(|| "no probe run to save".to_string())?;
            let record = record_from_run(run, &name, Some(created_at_unix_s), self.tool)?;
            self.config.save_height_map(record)
        })();
        match &result {
            Ok(path) => self
                .height_map_library
                .mark_saved(name.trim().to_string(), path.clone()),
            Err(message) => self.height_map_library.mark_error(message.clone()),
        }
        result
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
            MacroAction::ProbeZ => {
                self.mode = Mode::Probe;
                self.push_console(
                    ConsoleLevel::Info,
                    "Opened Probe > Z touch plate (mock; no probe starts)",
                );
            }
            MacroAction::Coolant => {
                self.toggle_coolant(1);
                self.spindle_on = true;
            }
        }
        self.push_console(ConsoleLevel::Ok, "ok (mock)");
    }

    /// Remember one accepted MDI line for UP/DOWN recall.
    ///
    /// Empty text and a duplicate of the newest entry are ignored. The newest
    /// [`MDI_HISTORY_LIMIT`] entries are kept; the oldest is dropped first.
    /// Recording never sends and never changes [`Self::mdi`].
    pub fn remember_mdi(&mut self, text: impl AsRef<str>) -> bool {
        let text = text.as_ref().trim();
        if text.is_empty() {
            return false;
        }
        self.mdi_history_index = None;
        if self.mdi_history.last().is_some_and(|last| last == text) {
            return false;
        }
        if self.mdi_history.len() == MDI_HISTORY_LIMIT {
            self.mdi_history.remove(0);
        }
        self.mdi_history.push(text.to_string());
        true
    }

    /// Recall the previous MDI history entry into the field.
    ///
    /// The first call recalls the newest entry. Returns `false` when the
    /// history is empty. This never sends and never touches the console.
    pub fn mdi_history_previous(&mut self) -> bool {
        if self.mdi_history.is_empty() {
            return false;
        }
        let index = match self.mdi_history_index {
            Some(index) => index.saturating_sub(1),
            None => self.mdi_history.len() - 1,
        };
        self.mdi_history_index = Some(index);
        self.mdi = self.mdi_history[index].clone();
        true
    }

    /// Recall the next MDI history entry, or return to an empty draft.
    ///
    /// Returns `false` when no entry is currently recalled. Moving past the
    /// newest entry clears the field; nothing is sent either way.
    pub fn mdi_history_next(&mut self) -> bool {
        let Some(index) = self.mdi_history_index else {
            return false;
        };
        if index + 1 < self.mdi_history.len() {
            self.mdi_history_index = Some(index + 1);
            self.mdi = self.mdi_history[index + 1].clone();
        } else {
            self.mdi_history_index = None;
            self.mdi.clear();
        }
        true
    }

    pub fn send_mdi(&mut self) {
        let text = self.mdi.trim().to_string();
        if text.is_empty() {
            self.push_console(ConsoleLevel::Warn, "MDI is empty");
            return;
        }
        self.remember_mdi(&text);
        self.push_console(ConsoleLevel::Command, format!("> {text}"));
        self.push_console(ConsoleLevel::Ok, "ok (mock, nothing was sent)");
        self.machine_state = "MDI";
    }

    pub fn push_console(&mut self, level: ConsoleLevel, text: impl Into<String>) {
        self.console.push(ConsoleLine::new(level, text));
        if self.console.len() > CONSOLE_LINE_LIMIT {
            let excess = self.console.len() - CONSOLE_LINE_LIMIT;
            self.console.drain(..excess);
        }
    }

    // ------------------------------------------------------------- console view

    /// Select the Console level filter. This changes the view only.
    pub fn set_console_filter(&mut self, filter: ConsoleFilter) {
        self.console_filter = filter;
    }

    /// Counts for every filter chip, computed from the whole ring buffer.
    pub fn console_counts(&self) -> ConsoleCounts {
        ConsoleCounts::from_lines(&self.console)
    }

    /// Number of lines the active filter would show.
    pub fn console_visible_count(&self) -> usize {
        self.console
            .iter()
            .filter(|line| self.console_filter.matches(line.level))
            .count()
    }

    /// Lines the Console should render for the active filter.
    pub fn console_visible_lines(&self) -> impl Iterator<Item = &ConsoleLine> {
        self.console
            .iter()
            .filter(|line| self.console_filter.matches(line.level))
    }

    /// Write one snapshot of the ring buffer through the active filter.
    ///
    /// The export is local only and never sends a command. The result is kept
    /// in [`Self::console_log_status`] for the Console toolbar and is also
    /// appended to the Console as an `OK` or `ERROR` line.
    ///
    /// # Errors
    ///
    /// Returns the refusal or write error after recording it in the status
    /// line. A missing configuration root writes nothing.
    pub fn save_console_log(
        &mut self,
        now_unix_s: u64,
    ) -> Result<ConsoleLogExport, ConsoleLogError> {
        let root = self.config.store_root().map(std::path::Path::to_path_buf);
        let result = write_console_log(
            &self.console,
            self.console_filter,
            root.as_deref(),
            now_unix_s,
        );
        match &result {
            Ok(export) => {
                let scope = console_log_scope(export);
                self.console_log_status = Some(ConsoleLogStatus {
                    ok: true,
                    text: format!("SAVED {scope}"),
                    path: Some(export.path.clone()),
                });
                self.push_console(
                    ConsoleLevel::Ok,
                    format!("Console log saved: {} ({scope})", export.path.display()),
                );
            }
            Err(error) => {
                self.console_log_status = Some(ConsoleLogStatus {
                    ok: false,
                    text: format!("NOT SAVED: {error}"),
                    path: error.path().map(std::path::Path::to_path_buf),
                });
                self.push_console(
                    ConsoleLevel::Error,
                    format!("Console log not saved: {error}"),
                );
            }
        }
        result
    }

    pub fn open_keypad(&mut self, target: KeypadTarget) {
        let (buffer, fresh, hint) = match target {
            KeypadTarget::Dro(axis) => (format!("{:.3}", self.value(axis)), true, None),
            KeypadTarget::Mdi => (self.mdi.clone(), false, None),
            KeypadTarget::Rpm => (self.rpm.to_string(), true, Some("0 - 24000".to_string())),
            KeypadTarget::Setting(number) => {
                let Some(row) = self.settings.row(number) else {
                    return;
                };
                let mut buffer = row.number_text().to_string();
                if buffer.is_empty() {
                    buffer = row.display_value();
                }
                let mut hint = row.range_text();
                if let Some(unit) = &row.unit {
                    hint.push_str(" | ");
                    hint.push_str(unit);
                }
                (buffer, true, Some(hint))
            }
            KeypadTarget::WorkOffset { wcs, axis } => {
                let value = self
                    .offsets
                    .wcs_row(wcs)
                    .and_then(|row| row.values.get(axis.index()).copied().flatten())
                    .map_or_else(String::new, |value| self.offsets.format_value(value));
                (
                    value,
                    true,
                    Some("G10 L20 changes the offset; it does not move an axis".to_string()),
                )
            }
            KeypadTarget::Tool(field) => (String::new(), true, Some(field.title().to_string())),
            KeypadTarget::ProbeField(field) => {
                let value = self.probe.field_text(field).to_string();
                let hint = if field.unit().is_empty() {
                    field.label().to_string()
                } else {
                    format!("{} ({})", field.label(), field.unit())
                };
                (value, true, Some(hint))
            }
            KeypadTarget::Utility(field) => {
                let value = self.utilities.field_text(field).to_string();
                let hint = if field.unit().is_empty() {
                    field.label().to_string()
                } else {
                    format!("{} ({})", field.label(), field.unit())
                };
                (value, true, Some(hint))
            }
            KeypadTarget::Calibration(field) => {
                let value = self.utilities.calibration().field_text(field).to_string();
                (
                    value,
                    true,
                    Some(format!("{} ({})", field.label(), field.unit())),
                )
            }
            KeypadTarget::Gantry(field) => {
                let value = self.utilities.gantry().field_text(field).to_string();
                (
                    value,
                    true,
                    Some(format!("{} ({})", field.label(), field.unit())),
                )
            }
        };
        self.keypad = Some(KeypadState {
            target,
            buffer,
            fresh,
            hint,
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
            KeypadTarget::Setting(number) => {
                if let Err(message) = self.settings.set_number_text(number, keypad.buffer.clone()) {
                    self.push_console(ConsoleLevel::Warn, format!("Setting ${number}: {message}"));
                }
            }
            KeypadTarget::WorkOffset { wcs, axis } => {
                if self.live {
                    // The shell sends the G10 line and waits for `$#`; a
                    // rejected command must not move the displayed value.
                } else if let Ok(value) = keypad.buffer.trim().parse::<f64>() {
                    self.push_console(
                        ConsoleLevel::Warn,
                        format!(
                            "{} {} offset edit requires a live controller (mock: {value})",
                            AppState::wcs_at(wcs),
                            axis.label()
                        ),
                    );
                }
            }
            KeypadTarget::Tool(_) => {
                // The tool sheet owns this draft; the shell applies it.
            }
            KeypadTarget::ProbeField(field) => {
                if let Err(message) = self.probe.set_field_text(field, keypad.buffer.clone()) {
                    self.push_console(ConsoleLevel::Warn, message);
                }
            }
            KeypadTarget::Utility(field) => {
                if let Err(message) = self.utilities.set_field_text(field, keypad.buffer.clone()) {
                    self.push_console(ConsoleLevel::Warn, message);
                }
            }
            KeypadTarget::Calibration(field) => {
                if let Err(message) = self
                    .utilities
                    .calibration_mut()
                    .set_field_text(field, keypad.buffer.clone())
                {
                    self.push_console(ConsoleLevel::Warn, message);
                }
            }
            KeypadTarget::Gantry(field) => {
                if let Err(message) = self
                    .utilities
                    .gantry_mut()
                    .set_field_text(field, keypad.buffer.clone())
                {
                    self.push_console(ConsoleLevel::Warn, message);
                }
            }
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
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use ghs_config::{
        ConfigStore, ControllerData, HeightMapFrame, HeightMapRecord, ProbeDefaultsRecord,
        ProbeOffsetRecord, ProfileKey,
    };
    use ghs_core::{
        FeedSpindle, MachineState, Overrides, Position, Signals, Wcs, parse_controller_info,
    };
    use ghs_engine::{ResetTarget, StatusReport, SystemCommand};

    use super::*;
    use crate::controller_reset::{
        RESET_TARGETS, ResetGate, ResetOutcome, ResetRefusal, ResetResult, command_text,
        refresh_commands, reset_command,
    };
    use crate::probe::{ProbeCard, ProbeContext};
    use crate::setup::{SafetyItem, SetupStep};
    use crate::utilities::LimitsStatus;

    const FLEXIHAL: &[&str] = &[
        "[VER:1.1f.20240604:]",
        "[OPT:VNMSL+2,64,1024,3,0]",
        "[AXS:3:XYZ]",
        "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]",
        "[FIRMWARE:grblHAL]",
        "[SIGNALS:HSDEP]",
        "[BOARD:FlexiHAL_20240604_210624]",
    ];

    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!(
                "ghs-app-state-{label}-{}-{nanos}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn saved_record(name: &str, seed: f64) -> HeightMapRecord {
        HeightMapRecord {
            name: name.to_string(),
            tool_number: Some(3),
            created_at_unix_s: Some(1_700_000_000),
            origin_x_mm: 0.0,
            origin_y_mm: 0.0,
            step_x_mm: 10.0,
            step_y_mm: 10.0,
            columns: 2,
            rows: 2,
            surface_z_mm: -1.0,
            z_offset_mm: None,
            samples: (0..4).map(|index| Some(seed + f64::from(index))).collect(),
            frame: HeightMapFrame {
                wcs: Wcs::G54,
                wcs_offset: Position::xyz(1.0, 2.0, 3.0),
                g92: Position::zero(),
                tool_length_offset_z_mm: 0.0,
            },
        }
    }

    fn state_with_profile(label: &str) -> (AppState, TempRoot, ControllerInfo) {
        let dir = TempRoot::new(label);
        let store = ConfigStore::new(&dir.path);
        let info = parse_controller_info(FLEXIHAL);
        let key = ProfileKey::from_controller(&info);
        let mut data = ControllerData::new();
        data.height_maps = vec![saved_record("top", 1.0), saved_record("bottom", 2.0)];
        store.save_controller_data(&key, &data).expect("profile");
        let mut state = AppState::new();
        state.config = ConfigModel::from_store(store);
        state.begin_live("test transport");
        state.apply_controller_info(&info);
        (state, dir, info)
    }

    fn profile_probe_defaults() -> ProbeDefaultsRecord {
        ProbeDefaultsRecord {
            safe_z_mm: 22.5,
            search_distance_mm: 6.5,
            feed_mm_min: 220.25,
            retract_mm: 1.5,
            probe_diameter_mm: 2.0,
            edge_offset_mm: 0.25,
            plate_thickness_mm: 9.5,
        }
    }

    fn second_probe_defaults() -> ProbeDefaultsRecord {
        ProbeDefaultsRecord {
            safe_z_mm: 30.0,
            search_distance_mm: 8.0,
            feed_mm_min: 150.0,
            retract_mm: 2.5,
            probe_diameter_mm: 4.0,
            edge_offset_mm: 1.5,
            plate_thickness_mm: 12.0,
        }
    }

    fn probe_default_context() -> ProbeContext {
        ProbeContext {
            live: true,
            ready: true,
            machine_state: MachineState::Idle,
            job_active: false,
            machine_pos_mm: Position::xyz(0.0, 0.0, 0.0),
            work_pos_mm: Position::xyz(0.0, 0.0, 0.0),
            wcs: Wcs::G54,
            wcs_value_mm: Position::zero(),
            g92_mm: Position::zero(),
            tlo_z_mm: 0.0,
            spindle_on: false,
            coolant_on: false,
            has_probe: true,
            has_probe_connected_signal: false,
            probe_disconnected: false,
            probe_triggered: false,
            offsets_reported: true,
            units: Units::Millimetre,
        }
    }

    fn shared_probe_text(state: &AppState) -> Vec<String> {
        ProbeField::PROFILE_DEFAULTS
            .iter()
            .map(|field| state.probe.field_text(*field).to_string())
            .collect()
    }

    fn state_with_probe_defaults(
        label: &str,
        defaults: ProbeDefaultsRecord,
    ) -> (AppState, TempRoot, ControllerInfo, ConfigStore) {
        let dir = TempRoot::new(label);
        let store = ConfigStore::new(&dir.path);
        let info = parse_controller_info(FLEXIHAL);
        let key = ProfileKey::from_controller(&info);
        let mut data = ControllerData::new();
        data.probe_defaults = defaults;
        store.save_controller_data(&key, &data).expect("profile");
        let mut state = AppState::new();
        state.config = ConfigModel::from_store(store.clone());
        state.begin_live("test transport");
        state.apply_controller_info(&info);
        (state, dir, info, store)
    }

    fn ready_state_with_macro(label: &str, confirm: bool) -> (AppState, TempRoot, ControllerInfo) {
        let (mut state, dir, info) = state_with_profile(label);
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        state
            .save_macro(None, "Safe Z", "G53 G0 Z0", confirm)
            .expect("save macro");
        (state, dir, info)
    }

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
    fn surfacing_load_requires_confirmation_and_builds_one_load_program() {
        let mut state = AppState::new();
        state.begin_live("test transport");
        state.connection = ConnectionState::Ready;
        state
            .utilities
            .generate_surfacing(Wcs::G54)
            .expect("preview");
        assert!(
            state.take_surfacing_load().is_err(),
            "confirmation is required"
        );
        state.utilities.set_surfacing_confirmed(true);
        let load = state.take_surfacing_load().expect("confirmed load");
        assert_eq!(load.name, "surfacing-100x80-2deep.nc");
        let Command::LoadProgram { name, source } = &load.command else {
            panic!("utility load must be LoadProgram, got {:?}", load.command);
        };
        assert_eq!(name, &load.name);
        assert!(source.starts_with("G21 G90 G54 G17 G94"));
        assert!(!state.job.is_active(), "utility load never starts the job");
    }

    #[test]
    fn transform_load_is_load_program_and_never_start() {
        let mut state = AppState::new();
        state.begin_live("test transport");
        state.connection = ConnectionState::Ready;
        let program = Arc::new(
            ghs_gcode::parse_program("G21\nG0 X0 (park)\nG1 X10 F100 ; cut\nM30\n")
                .expect("program parses"),
        );
        state.job.loaded("part.nc".into(), Arc::clone(&program), 4);
        let name = state.job.name.clone();
        let source = state.job.source_text().expect("loaded source");
        state
            .utilities
            .generate_transform(
                crate::utilities::TransformKind::StripComments,
                &name,
                &source,
            )
            .expect("transform preview");
        assert!(state.take_transform_load().is_err());
        state.utilities.set_transform_confirmed(true);
        let load = state.take_transform_load().expect("confirmed load");
        assert_eq!(load.name, "part.nc [comments stripped]");
        let Command::LoadProgram { name, source } = &load.command else {
            panic!("transform load must be LoadProgram, got {:?}", load.command);
        };
        assert_eq!(name, &load.name);
        assert_eq!(source, "G21\nG0 X0\nG1 X10 F100\nM30");
        assert!(!state.job.is_active());
        assert!(Arc::ptr_eq(
            state.job.program().expect("program remains loaded"),
            &program
        ));
    }

    fn calibration_ready_state() -> AppState {
        let mut state = AppState::new();
        state.begin_live("test transport");
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        state.settings.apply_settings(&[
            ghs_engine::Setting::new(100, SettingValue::Float(250.0)),
            ghs_engine::Setting::new(101, SettingValue::Float(250.0)),
            ghs_engine::Setting::new(102, SettingValue::Float(250.0)),
        ]);
        {
            let calibration = state.utilities.calibration_mut();
            assert!(calibration.set_axis(CalibrationAxis::X));
            calibration
                .set_field_text(CalibrationField::CommandedMm, "100")
                .expect("commanded distance");
            calibration
                .set_field_text(CalibrationField::MeasuredMm, "98.5")
                .expect("measured distance");
        }
        state.calculate_calibration().expect("calibration proposal");
        state.utilities.calibration_mut().set_confirmed(true);
        state
    }

    #[test]
    fn calibration_refuses_unsafe_gates_and_sends_only_set_setting() {
        let mut mock = AppState::new();
        mock.settings
            .apply_settings(&[ghs_engine::Setting::new(100, SettingValue::Float(250.0))]);
        {
            let calibration = mock.utilities.calibration_mut();
            calibration
                .set_field_text(CalibrationField::CommandedMm, "100")
                .expect("commanded");
            calibration
                .set_field_text(CalibrationField::MeasuredMm, "98.5")
                .expect("measured");
        }
        mock.calculate_calibration().expect("proposal");
        mock.utilities.calibration_mut().set_confirmed(true);
        assert_eq!(
            mock.take_calibration_write().expect_err("mock mode"),
            "mock mode: no controller"
        );

        let mut state = calibration_ready_state();
        state.connection = ConnectionState::Connecting;
        assert_eq!(
            state.take_calibration_write().expect_err("not ready"),
            "not connected"
        );

        state.connection = ConnectionState::Ready;
        state.machine_state = "Alarm";
        assert_eq!(
            state.take_calibration_write().expect_err("alarm"),
            "alarm or door"
        );

        state.machine_state = "Door";
        assert_eq!(
            state.take_calibration_write().expect_err("door"),
            "alarm or door"
        );

        state.machine_state = "Idle";
        let program = Arc::new(ghs_gcode::parse_program("G21\nM30\n").expect("program"));
        state.job.loaded("job.nc".into(), program, 2);
        state.job.started(1, 2);
        assert_eq!(
            state.take_calibration_write().expect_err("job active"),
            "job active"
        );

        state.job.completed(Duration::from_secs(1));
        state.machine_state = "Run";
        assert_eq!(
            state.take_calibration_write().expect_err("not idle"),
            "controller not Idle"
        );

        state.machine_state = "Idle";
        let write = state.take_calibration_write().expect("idle write");
        assert_eq!(write.setting_number, 100);
        assert_eq!(
            write.command,
            Command::SetSetting {
                number: 100,
                value: SettingValue::Float(253.807),
            }
        );
        assert!(!matches!(
            write.command,
            Command::Jog(_)
                | Command::Mdi(_)
                | Command::LoadProgram { .. }
                | Command::StartProgram { .. }
                | Command::Home
                | Command::Unlock
                | Command::Reset
                | Command::Sleep
        ));
    }

    #[test]
    fn calibration_read_back_refreshes_applied_and_failed_state() {
        let mut state = calibration_ready_state();
        let write = state.take_calibration_write().expect("idle write");
        let value = match &write.command {
            Command::SetSetting { value, .. } => value.clone(),
            other => panic!("calibration must send SetSetting, got {other:?}"),
        };
        state
            .utilities
            .calibration_mut()
            .written(write.setting_number, &value, true);
        assert_eq!(
            state.utilities.calibration().stage(),
            crate::utilities::CalibrationStage::Applied
        );
        assert!(
            state
                .utilities
                .calibration()
                .last_result()
                .is_some_and(|text| text.contains("APPLIED $100"))
        );

        // The engine emits a fresh Settings snapshot after a verified write.
        state
            .settings
            .apply_settings(&[ghs_engine::Setting::new(100, SettingValue::Float(253.807))]);
        assert_eq!(
            state.calibration_current_steps(CalibrationAxis::X),
            Some(253.807)
        );

        let mut failed = calibration_ready_state();
        let write = failed.take_calibration_write().expect("idle write");
        failed.utilities.calibration_mut().written(
            write.setting_number,
            &SettingValue::Float(250.5),
            false,
        );
        assert_eq!(
            failed.utilities.calibration().stage(),
            crate::utilities::CalibrationStage::Failed
        );
        assert!(
            failed
                .utilities
                .calibration()
                .last_result()
                .is_some_and(|text| text.contains("previous 250.000"))
        );
        assert_eq!(
            failed.calibration_current_steps(CalibrationAxis::X),
            Some(250.0)
        );
    }

    #[test]
    fn opening_and_running_the_gantry_calculator_sends_nothing() {
        let mut state = AppState::new();
        let mut sent = Vec::new();
        let send = recorded_send(&mut sent);

        // Opening a gantry field is the wizard's entry point. It only fills the
        // shared keypad and never dispatches a command.
        state.open_keypad(KeypadTarget::Gantry(GantryField::Span));
        assert!(state.keypad.is_some());
        state.keypad_action(KeypadAction::Clear);
        for digit in "400".chars() {
            state.keypad_action(KeypadAction::Digit(digit));
        }
        state.keypad_action(KeypadAction::Submit);
        assert_eq!(
            state.utilities.gantry().field_text(GantryField::Span),
            "400"
        );

        state.utilities.gantry_mut().load_demo();
        state
            .calculate_gantry_squaring()
            .expect("gantry result is local");
        let result = state.utilities.gantry().result().expect("result");
        assert_eq!(
            result.correction_text(),
            "RIGHT SIDE IS 0.250 mm FORWARD; MOVE IT BACK 0.250 mm OR SHIM THE LEFT SIDE"
        );

        // The calculator exposes no send path. The recorder stays empty; any
        // future dispatch wired through it would fail this assertion.
        drop(send);
        assert!(sent.is_empty(), "gantry calculator sent {sent:?}");
        assert!(!state.job.is_active());
    }

    #[test]
    fn program_limits_use_reported_settings_offsets_and_warn() {
        let mut state = AppState::new();
        state.begin_live("test transport");
        state.connection = ConnectionState::Ready;
        state.settings.apply_settings(&[
            ghs_engine::Setting::new(20, SettingValue::Bool(true)),
            ghs_engine::Setting::new(130, SettingValue::Int(100)),
            ghs_engine::Setting::new(131, SettingValue::Int(200)),
            ghs_engine::Setting::new(132, SettingValue::Int(100)),
        ]);
        let mut offsets = Offsets::default();
        offsets.wcs[Wcs::G54.index()] = Some(Position::xyz(10.0, 0.0, 10.0));
        state.apply_offsets(&offsets);
        let program = Arc::new(
            ghs_gcode::parse_program("G21\nG0 X10 Y0 Z0\nG1 X100 F100\nM30\n")
                .expect("program parses"),
        );
        state.job.loaded("limits.nc".into(), program, 4);
        let limits = state.program_limits();
        assert!(limits.is_over_travel());
        assert_eq!(
            limits.overruns(),
            vec!["X MAX 110.000 > $130 100.000".to_string()]
        );
    }

    #[test]
    fn rapid_override_cycle_maps_every_documented_level() {
        assert_eq!(next_rapid_override(100), 50);
        assert_eq!(next_rapid_override(50), 25);
        assert_eq!(next_rapid_override(25), 100);
        // 5 % is a controller level but not part of the compact cycle.
        assert_eq!(next_rapid_override(5), 100);
        assert_eq!(next_rapid_override(0), 100);
        assert_eq!(next_rapid_override(33), 100);
    }

    #[test]
    fn rapid_override_step_is_the_exact_command() {
        let mut state = AppState::new();
        state.begin_live("test transport");
        state.connection = ConnectionState::Ready;

        state.rapid_override = 100;
        assert_eq!(
            state.rapid_override_step(),
            Ok(Command::SetRapidOverride(50))
        );
        state.rapid_override = 50;
        assert_eq!(
            state.rapid_override_step(),
            Ok(Command::SetRapidOverride(25))
        );
        state.rapid_override = 25;
        assert_eq!(
            state.rapid_override_step(),
            Ok(Command::SetRapidOverride(100))
        );

        // Asking for the next step never changes the displayed value.
        assert_eq!(state.rapid_override, 25);
    }

    #[test]
    fn rapid_override_snapshot_is_the_only_value_source() {
        let mut state = AppState::new();
        state.begin_live("test transport");
        state.apply_controller_info(&parse_controller_info(FLEXIHAL));
        assert_eq!(state.rapid_override, 100);

        let snapshot = MachineSnapshot {
            overrides: Overrides::new(100, 25, 100),
            ..MachineSnapshot::default()
        };
        state.apply_snapshot(&snapshot, None);
        assert_eq!(state.rapid_override, 25);

        let snapshot = MachineSnapshot {
            overrides: Overrides::new(100, 50, 100),
            ..MachineSnapshot::default()
        };
        state.apply_snapshot(&snapshot, None);
        assert_eq!(state.rapid_override, 50);
    }

    #[test]
    fn rapid_override_is_refused_when_mock_or_not_ready() {
        let mut state = AppState::new();
        assert_eq!(state.rapid_override_step(), Err(RapidOverrideRefusal::Mock));

        state.begin_live("test transport");
        assert_eq!(state.connection, ConnectionState::Connecting);
        assert_eq!(
            state.rapid_override_step(),
            Err(RapidOverrideRefusal::NotReady)
        );

        state.connection = ConnectionState::Disconnected;
        assert_eq!(
            state.rapid_override_step(),
            Err(RapidOverrideRefusal::NotReady)
        );

        state.connection = ConnectionState::Ready;
        assert_eq!(
            state.rapid_override_step(),
            Ok(Command::SetRapidOverride(50))
        );
    }

    #[test]
    fn mdi_history_recalls_without_sending_or_touching_the_console() {
        let mut state = AppState::new();
        assert!(state.remember_mdi("G0 X1"));
        assert!(state.remember_mdi("G1 Z-1 F200"));
        let console_len = state.console.len();
        let machine = state.machine_state;
        state.mdi.clear();

        assert!(state.mdi_history_previous());
        assert_eq!(state.mdi, "G1 Z-1 F200");
        assert!(state.mdi_history_previous());
        assert_eq!(state.mdi, "G0 X1");
        assert!(state.mdi_history_next());
        assert_eq!(state.mdi, "G1 Z-1 F200");
        assert!(state.mdi_history_next());
        assert!(
            state.mdi.is_empty(),
            "past the newest entry is an empty draft"
        );
        assert!(!state.mdi_history_next());

        assert_eq!(state.console.len(), console_len, "recall must not append");
        assert_eq!(state.machine_state, machine, "recall must not send");
    }

    #[test]
    fn mdi_history_ignores_empty_duplicates_and_caps_the_length() {
        let mut state = AppState::new();
        assert!(!state.remember_mdi(""));
        assert!(!state.remember_mdi("   "));
        for index in 0..12 {
            assert!(state.remember_mdi(format!("G0 X{index}")));
        }
        assert_eq!(state.mdi_history.len(), MDI_HISTORY_LIMIT);
        assert_eq!(state.mdi_history.first().map(String::as_str), Some("G0 X2"));
        assert_eq!(state.mdi_history.last().map(String::as_str), Some("G0 X11"));

        assert!(
            !state.remember_mdi("G0 X11"),
            "a consecutive duplicate is ignored"
        );
        assert_eq!(state.mdi_history.len(), MDI_HISTORY_LIMIT);

        assert!(state.remember_mdi("G0 X0"));
        assert_eq!(state.mdi_history.len(), MDI_HISTORY_LIMIT);
        assert_eq!(
            state.mdi_history.first().map(String::as_str),
            Some("G0 X3"),
            "the oldest entry is dropped at the cap"
        );
        assert_eq!(state.mdi_history.last().map(String::as_str), Some("G0 X0"));
    }

    // ------------------------------------------------------ console filter/log

    #[test]
    fn console_filter_shows_only_the_selected_level() {
        let mut state = AppState::new();
        state.console.clear();
        state.push_console(ConsoleLevel::Info, "info one");
        state.push_console(ConsoleLevel::Command, "> G0 X1");
        state.push_console(ConsoleLevel::Warn, "warn one");
        state.push_console(ConsoleLevel::Error, "error one");
        state.push_console(ConsoleLevel::Ok, "ok one");

        state.set_console_filter(ConsoleFilter::Warn);
        let visible: Vec<&str> = state
            .console_visible_lines()
            .map(|line| line.text.as_str())
            .collect();
        assert_eq!(visible, vec!["warn one"]);
        assert_eq!(state.console_visible_count(), 1);
        assert_eq!(
            state.console.len(),
            5,
            "filtering changes the view, not the ring buffer"
        );
        assert_eq!(state.console_counts().count(ConsoleFilter::All), 5);

        state.set_console_filter(ConsoleFilter::Command);
        assert_eq!(state.console_visible_count(), 1);
        assert_eq!(
            state
                .console_visible_lines()
                .next()
                .map(|line| line.text.as_str()),
            Some("> G0 X1")
        );
    }

    #[test]
    fn console_counts_cover_each_level_and_the_total() {
        let mut state = AppState::new();
        state.console.clear();
        state.push_console(ConsoleLevel::Info, "info one");
        state.push_console(ConsoleLevel::Info, "info two");
        state.push_console(ConsoleLevel::Command, "> $I+");
        state.push_console(ConsoleLevel::Ok, "ok");
        state.push_console(ConsoleLevel::Warn, "warning");
        state.push_console(ConsoleLevel::Error, "error");
        let counts = state.console_counts();
        assert_eq!(counts.all, 6);
        assert_eq!(counts.info, 2);
        assert_eq!(counts.command, 1);
        assert_eq!(counts.ok, 1);
        assert_eq!(counts.warn, 1);
        assert_eq!(counts.error, 1);
        for filter in ConsoleFilter::ALL {
            assert_eq!(
                counts.count(filter),
                state
                    .console
                    .iter()
                    .filter(|line| filter.matches(line.level))
                    .count(),
                "chip count matches the view for {}",
                filter.label()
            );
        }
    }

    #[test]
    fn selecting_all_restores_the_full_console() {
        let mut state = AppState::new();
        state.console.clear();
        state.push_console(ConsoleLevel::Info, "info one");
        state.push_console(ConsoleLevel::Warn, "warn one");
        state.push_console(ConsoleLevel::Error, "error one");

        state.set_console_filter(ConsoleFilter::Error);
        assert_eq!(state.console_visible_count(), 1);

        state.set_console_filter(ConsoleFilter::All);
        assert_eq!(state.console_visible_count(), 3);
        let visible: Vec<&str> = state
            .console_visible_lines()
            .map(|line| line.text.as_str())
            .collect();
        assert_eq!(visible, vec!["info one", "warn one", "error one"]);
    }

    #[test]
    fn a_new_session_resets_the_console_filter_and_export_status() {
        let mut state = AppState::new();
        state.set_console_filter(ConsoleFilter::Error);
        state.console_log_status = Some(ConsoleLogStatus {
            ok: true,
            text: "SAVED".to_string(),
            path: None,
        });

        state.begin_live("test transport");

        assert_eq!(state.console_filter, ConsoleFilter::All);
        assert!(state.console_log_status.is_none());
        assert_eq!(state.console.len(), 1);
        assert_eq!(state.console[0].text, "test transport");
    }

    #[test]
    fn console_ring_buffer_is_bounded_while_filtering_is_view_only() {
        let mut state = AppState::new();
        state.console.clear();
        for index in 0..CONSOLE_LINE_LIMIT + 7 {
            state.push_console(ConsoleLevel::Info, format!("line {index}"));
        }
        assert_eq!(state.console.len(), CONSOLE_LINE_LIMIT);
        assert_eq!(
            state.console.first().map(|line| line.text.as_str()),
            Some("line 7"),
            "the oldest lines are dropped at the explicit cap"
        );

        state.set_console_filter(ConsoleFilter::Info);
        assert_eq!(state.console_visible_count(), CONSOLE_LINE_LIMIT);
        assert_eq!(state.console_counts().all, CONSOLE_LINE_LIMIT);
    }

    #[test]
    fn save_console_log_writes_the_fixed_path_and_reports_the_filtered_scope() {
        let dir = TempRoot::new("console-log");
        let mut state = AppState::new();
        state.config = ConfigModel::from_store(ConfigStore::new(&dir.path));
        state.console.clear();
        state.push_console(ConsoleLevel::Info, "hello");
        state.push_console(ConsoleLevel::Warn, "careful");
        state.set_console_filter(ConsoleFilter::Warn);

        let export = state.save_console_log(1_700_000_000).expect("save log");
        let expected = dir.path.join("exports").join("console.log");
        assert_eq!(export.path, expected);
        assert!(export.filtered);
        assert_eq!(export.lines, 1);
        assert_eq!(export.ring_lines, 2);

        let text = fs::read_to_string(&expected).expect("read log");
        assert!(text.contains("Generated: 2023-11-14 22:13:20 UTC"));
        assert!(text.contains("Filter: WARN (filtered view; ring buffer has 2 lines)"));
        assert!(text.contains("careful"));
        assert!(!text.contains("hello"), "the filtered export omits INFO");

        let status = state.console_log_status.as_ref().expect("status");
        assert!(status.ok);
        assert_eq!(status.path.as_ref(), Some(&expected));
        assert!(
            status.text.contains("1 LINES (WARN VIEW)"),
            "{}",
            status.text
        );
        assert!(
            state
                .console
                .iter()
                .any(|line| line.level == ConsoleLevel::Ok && line.text.contains("saved")),
            "the result is also appended to the Console"
        );
    }

    #[test]
    fn save_console_log_labels_a_full_buffer_export() {
        let dir = TempRoot::new("console-log-full");
        let mut state = AppState::new();
        state.config = ConfigModel::from_store(ConfigStore::new(&dir.path));
        state.console.clear();
        state.push_console(ConsoleLevel::Info, "hello");
        state.push_console(ConsoleLevel::Warn, "careful");

        let export = state.save_console_log(0).expect("save log");
        assert!(!export.filtered);
        assert_eq!(export.lines, 2);
        let text = fs::read_to_string(&export.path).expect("read log");
        assert!(text.contains("Filter: ALL (full ring buffer)"));
        assert!(text.contains("hello"));
        assert!(text.contains("careful"));
        let status = state.console_log_status.as_ref().expect("status");
        assert!(
            status.text.contains("2 LINES (FULL BUFFER)"),
            "{}",
            status.text
        );
    }

    #[test]
    fn save_console_log_reports_a_write_failure_and_keeps_the_previous_file() {
        let dir = TempRoot::new("console-log-failure");
        // A regular file where the exports directory belongs forces the write
        // to fail without relying on permissions.
        fs::write(dir.path.join("exports"), b"blocking file").expect("blocking file");
        let mut state = AppState::new();
        state.config = ConfigModel::from_store(ConfigStore::new(&dir.path));

        let error = state.save_console_log(0).expect_err("write must fail");
        let status = state.console_log_status.as_ref().expect("status");
        assert!(!status.ok);
        assert!(status.text.contains("NOT SAVED"), "{}", status.text);
        assert!(
            status
                .path
                .as_ref()
                .is_some_and(|path| path.ends_with("exports/console.log")),
            "the full target path is reported"
        );
        assert!(
            error.to_string().contains("console.log"),
            "the error names the target: {error}"
        );
        assert_eq!(
            fs::read(dir.path.join("exports")).expect("blocking file"),
            b"blocking file"
        );
    }

    #[test]
    fn save_console_log_is_refused_without_a_configuration_root() {
        let mut state = AppState::new();
        // Unit tests never open the developer's real config directory.
        assert!(state.config.store_root().is_none());
        let error = state.save_console_log(0).expect_err("no root");
        assert_eq!(error, ConsoleLogError::NoConfigRoot);
        let status = state.console_log_status.as_ref().expect("status");
        assert!(!status.ok);
        assert!(
            status
                .text
                .contains("configuration directory is unavailable")
        );
        assert!(status.path.is_none());
        assert!(
            state
                .console
                .iter()
                .any(|line| line.level == ConsoleLevel::Error && line.text.contains("not saved")),
            "the refusal is surfaced in the Console"
        );
    }

    #[test]
    fn mock_mdi_send_records_one_history_entry() {
        let mut state = AppState::new();
        state.mdi = "G0 X1".to_string();
        state.send_mdi();
        assert_eq!(state.mdi_history, vec!["G0 X1".to_string()]);
        state.send_mdi();
        assert_eq!(state.mdi_history.len(), 1, "resend is a duplicate");
    }

    #[test]
    fn feed_fine_step_and_reset_are_exact_and_clamped() {
        let mut state = AppState::new();
        state.begin_live("test transport");
        state.connection = ConnectionState::Ready;

        state.feed_override = 100;
        assert_eq!(
            state.feed_override_step(1),
            Ok(Command::SetFeedOverride(101))
        );
        assert_eq!(
            state.feed_override_step(-1),
            Ok(Command::SetFeedOverride(99))
        );
        state.feed_override = 200;
        assert_eq!(
            state.feed_override_step(1),
            Ok(Command::SetFeedOverride(200))
        );
        state.feed_override = 10;
        assert_eq!(
            state.feed_override_step(-1),
            Ok(Command::SetFeedOverride(10))
        );

        state.feed_override = 37;
        assert_eq!(
            state.feed_override_reset(),
            Ok(Command::SetFeedOverride(100))
        );
        assert_eq!(
            state.feed_override, 37,
            "building a command does not claim it"
        );
    }

    #[test]
    fn feed_override_is_refused_when_mock_or_not_ready() {
        let mut state = AppState::new();
        assert_eq!(state.feed_override_step(1), Err(FeedOverrideRefusal::Mock));

        state.begin_live("test transport");
        assert_eq!(
            state.feed_override_step(1),
            Err(FeedOverrideRefusal::NotReady)
        );
        assert_eq!(
            state.feed_override_reset(),
            Err(FeedOverrideRefusal::NotReady)
        );

        state.connection = ConnectionState::Disconnected;
        assert_eq!(
            state.feed_override_step(1),
            Err(FeedOverrideRefusal::NotReady)
        );

        state.connection = ConnectionState::Ready;
        assert_eq!(
            state.feed_override_step(1),
            Ok(Command::SetFeedOverride(101))
        );
    }

    #[test]
    fn feed_commands_leave_spindle_and_rapid_overrides_alone() {
        let mut state = AppState::new();
        state.begin_live("test transport");
        state.connection = ConnectionState::Ready;
        state.feed_override = 100;
        state.spindle_override = 55;
        state.rapid_override = 25;

        let _ = state.feed_override_step(1);
        let _ = state.feed_override_reset();

        assert_eq!(state.feed_override, 100);
        assert_eq!(state.spindle_override, 55);
        assert_eq!(state.rapid_override, 25);
    }

    #[test]
    fn sleep_requires_confirmation_and_sends_exactly_once() {
        let mut state = AppState::new();
        state.begin_live("test transport");
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";

        let mut sends = Vec::new();
        assert_eq!(
            state.dispatch_sleep(|command| {
                sends.push(command);
                Ok(())
            }),
            SleepOutcome::NotConfirmed
        );
        assert!(sends.is_empty(), "no confirmation means no send");

        state.open_sleep_confirm();
        assert!(state.sleep_confirm_open);
        assert_eq!(
            state.dispatch_sleep(|command| {
                sends.push(command);
                Ok(())
            }),
            SleepOutcome::Sent
        );
        assert_eq!(sends, vec![Command::Sleep]);
        assert!(!state.sleep_confirm_open, "the sheet is consumed");

        assert_eq!(
            state.dispatch_sleep(|command| {
                sends.push(command);
                Ok(())
            }),
            SleepOutcome::NotConfirmed
        );
        assert_eq!(sends.len(), 1, "a second confirmation sends nothing");
    }

    #[test]
    fn sleep_cancel_sends_nothing() {
        let mut state = AppState::new();
        state.begin_live("test transport");
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        state.open_sleep_confirm();
        state.cancel_sleep_confirm();
        assert!(!state.sleep_confirm_open);

        let mut sent = false;
        assert_eq!(
            state.dispatch_sleep(|_| {
                sent = true;
                Ok(())
            }),
            SleepOutcome::NotConfirmed
        );
        assert!(!sent);
    }

    #[test]
    fn sleep_is_refused_when_not_idle_ready_or_job_free() {
        let mut state = AppState::new();
        state.open_sleep_confirm();
        let mut sends = 0;
        assert_eq!(
            state.dispatch_sleep(|_| {
                sends += 1;
                Ok(())
            }),
            SleepOutcome::Refused(SleepRefusal::Mock)
        );
        assert_eq!(sends, 0);

        state.begin_live("test transport");
        state.open_sleep_confirm();
        assert_eq!(
            state.dispatch_sleep(|_| {
                sends += 1;
                Ok(())
            }),
            SleepOutcome::Refused(SleepRefusal::NotReady)
        );
        assert_eq!(sends, 0);

        state.connection = ConnectionState::Ready;
        state.machine_state = "Run";
        state.open_sleep_confirm();
        assert_eq!(
            state.dispatch_sleep(|_| {
                sends += 1;
                Ok(())
            }),
            SleepOutcome::Refused(SleepRefusal::NotIdle)
        );
        assert_eq!(sends, 0);

        let mut job = JobView::default();
        job.loaded("sample.nc".into(), sample_program(), 10);
        job.started(1, 10);
        state.job = job;
        state.machine_state = "Idle";
        state.open_sleep_confirm();
        assert_eq!(
            state.dispatch_sleep(|_| {
                sends += 1;
                Ok(())
            }),
            SleepOutcome::Refused(SleepRefusal::JobActive)
        );
        assert_eq!(sends, 0);

        state.job = JobView::default();
        state.open_sleep_confirm();
        assert_eq!(state.sleep_command(), Ok(Command::Sleep));
    }

    // ------------------------------------------------------- controller reset

    fn ready_reset_state() -> AppState {
        let mut state = AppState::new();
        state.begin_live("reset test");
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        state
    }

    fn snapshot_tree(root: &std::path::Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(dir: &std::path::Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, files);
                } else if let Ok(bytes) = fs::read(&path) {
                    files.insert(path, bytes);
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(root, &mut files);
        files
    }

    #[test]
    fn controller_reset_requires_confirmation_and_cancel_sends_nothing() {
        let mut state = ready_reset_state();
        state.controller_reset.set_loss_accepted(true);
        let mut sent = Vec::new();
        assert_eq!(
            state
                .controller_reset
                .confirm(ResetGate::READY_IDLE, recorded_send(&mut sent)),
            ResetOutcome::NotConfirmed
        );
        assert!(sent.is_empty());

        state.controller_reset.select(ResetTarget::Offsets);
        state
            .controller_reset
            .request_confirm(ResetGate::READY_IDLE)
            .expect("confirmation opens");
        assert!(state.controller_reset.is_confirm_open());
        state.controller_reset.cancel_confirm();
        assert!(!state.controller_reset.is_confirm_open());
        assert_eq!(
            state
                .controller_reset
                .confirm(ResetGate::READY_IDLE, recorded_send(&mut sent)),
            ResetOutcome::NotConfirmed
        );
        assert!(sent.is_empty(), "opening or cancelling never sends");
    }

    #[test]
    fn controller_reset_without_a_backup_acknowledgement_sends_nothing() {
        let mut state = ready_reset_state();
        assert_eq!(
            state
                .controller_reset
                .request_confirm(ResetGate::READY_IDLE),
            Err(ResetRefusal::BackupNotAcknowledged)
        );
        assert!(!state.controller_reset.is_confirm_open());
        let mut sent = Vec::new();
        assert_eq!(
            state
                .controller_reset
                .confirm(ResetGate::READY_IDLE, recorded_send(&mut sent)),
            ResetOutcome::NotConfirmed
        );
        assert!(sent.is_empty());
        assert_eq!(
            state.controller_reset.last_result(),
            Some(ResetResult::Refused)
        );
    }

    #[test]
    fn controller_reset_export_acknowledges_the_backup() {
        let (mut state, _dir, _info) = state_with_profile("reset-export-ack");
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        assert!(!state.controller_reset.backup_acknowledged());

        let path = state.export_backup_for_reset().expect("backup export");
        assert_eq!(path, state.config.backup_path().expect("fixed backup path"));
        assert!(state.controller_reset.export_succeeded_this_session());
        assert!(state.controller_reset.backup_acknowledged());
        assert!(
            state
                .controller_reset
                .export_result()
                .is_some_and(|result| result.ok())
        );
        assert!(
            state
                .controller_reset
                .request_confirm(ResetGate::READY_IDLE)
                .is_ok()
        );
    }

    #[test]
    fn controller_reset_is_refused_while_the_backup_export_is_in_flight() {
        let mut state = ready_reset_state();
        state.controller_reset.set_loss_accepted(true);
        state.controller_reset.begin_backup_export();
        assert_eq!(
            state
                .controller_reset
                .request_confirm(ResetGate::READY_IDLE),
            Err(ResetRefusal::BackupExportInFlight)
        );
        assert!(!state.controller_reset.is_confirm_open());
        assert!(state.controller_reset.backup_export_in_flight());
    }

    #[test]
    fn controller_reset_refuses_a_pendant_reported_by_the_status_stream() {
        let mut state = ready_reset_state();
        state.controller_reset.set_loss_accepted(true);
        let status = StatusReport {
            mpg: Some(true),
            ..StatusReport::default()
        };
        state.apply_snapshot(
            &MachineSnapshot {
                state: MachineState::Idle,
                ..MachineSnapshot::default()
            },
            Some(&status),
        );
        assert!(state.mpg_active);
        assert_eq!(
            state.controller_reset.request_confirm(ResetGate {
                pendant_active: state.mpg_active,
                ..ResetGate::READY_IDLE
            }),
            Err(ResetRefusal::PendantActive)
        );
        assert!(!state.controller_reset.is_confirm_open());

        state.begin_live("fresh reset session");
        assert!(!state.mpg_active);
        assert!(!state.controller_reset.backup_acknowledged());
    }

    #[test]
    fn controller_reset_sends_one_command_and_only_the_allowed_refresh() {
        for target in RESET_TARGETS {
            let mut state = ready_reset_state();
            state.controller_reset.set_loss_accepted(true);
            state.controller_reset.select(target);
            state
                .controller_reset
                .request_confirm(ResetGate::READY_IDLE)
                .expect("confirmation opens");
            let mut sent = Vec::new();
            let outcome = state
                .controller_reset
                .confirm(ResetGate::READY_IDLE, recorded_send(&mut sent));
            assert_eq!(outcome, ResetOutcome::Sent { target });
            assert_eq!(sent, vec![reset_command(target)]);

            sent.extend(refresh_commands(target));
            assert_eq!(
                sent.iter()
                    .filter(|command| matches!(command, Command::System(SystemCommand::Reset(_))))
                    .count(),
                1,
                "{target:?} must send exactly one reset"
            );
            assert_eq!(
                sent.iter()
                    .filter(|command| matches!(command, Command::System(SystemCommand::Offsets)))
                    .count(),
                usize::from(matches!(target, ResetTarget::Offsets | ResetTarget::All))
            );
            assert_eq!(
                sent.iter()
                    .filter(|command| matches!(command, Command::System(SystemCommand::Settings)))
                    .count(),
                usize::from(matches!(target, ResetTarget::Settings | ResetTarget::All))
            );
        }
    }

    #[test]
    fn controller_reset_never_emits_motion_start_load_g10_or_setting_writes() {
        for target in RESET_TARGETS {
            let mut commands = vec![reset_command(target)];
            commands.extend(refresh_commands(target));
            for command in commands {
                match command {
                    Command::System(SystemCommand::Reset(sent_target)) => {
                        assert_eq!(sent_target, target);
                    }
                    Command::System(SystemCommand::Offsets)
                    | Command::System(SystemCommand::Settings) => {}
                    other => panic!("reset emitted a forbidden command: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn controller_reset_leaves_local_profile_config_and_backups_unchanged() {
        let (mut state, dir, _info) = state_with_profile("reset-files");
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        state.config.save_app().expect("save app config");
        state.export_backup_for_reset().expect("backup export");
        let before = snapshot_tree(&dir.path);
        assert!(!before.is_empty(), "local files exist before the reset");

        state.controller_reset.select(ResetTarget::All);
        state
            .controller_reset
            .request_confirm(ResetGate::READY_IDLE)
            .expect("confirmation opens for the backed-up session");
        let mut sent = Vec::new();
        assert_eq!(
            state
                .controller_reset
                .confirm(ResetGate::READY_IDLE, recorded_send(&mut sent)),
            ResetOutcome::Sent {
                target: ResetTarget::All
            }
        );
        assert_eq!(sent, vec![reset_command(ResetTarget::All)]);
        assert_eq!(
            snapshot_tree(&dir.path),
            before,
            "a controller reset must not write or delete local data"
        );
    }

    #[test]
    fn controller_reset_sender_rejection_sets_failed_result_and_error() {
        let mut state = ready_reset_state();
        state.controller_reset.set_loss_accepted(true);
        state.controller_reset.select(ResetTarget::Settings);
        state
            .controller_reset
            .request_confirm(ResetGate::READY_IDLE)
            .expect("confirmation opens");
        let outcome = state.controller_reset.confirm(ResetGate::READY_IDLE, |_| {
            Err("streamer is busy".to_string())
        });
        assert_eq!(
            outcome,
            ResetOutcome::Failed("streamer is busy".to_string())
        );
        assert_eq!(
            state.controller_reset.last_result(),
            Some(ResetResult::Failed)
        );
        assert_eq!(state.controller_reset.error(), Some("streamer is busy"));
        assert_eq!(
            state.controller_reset.last_target(),
            Some(ResetTarget::Settings)
        );
    }

    #[test]
    fn controller_reset_star_confirmation_is_the_most_destructive_text() {
        assert_eq!(command_text(ResetTarget::All), "$RST=*");
        let text = crate::controller_reset::consequence(ResetTarget::All);
        assert!(crate::controller_reset::is_most_destructive(
            ResetTarget::All
        ));
        assert!(text.to_lowercase().contains("most destructive"));
        assert!(text.to_lowercase().contains("startup lines"));
        assert!(text.to_lowercase().contains("build info"));
        assert!(!crate::controller_reset::is_most_destructive(
            ResetTarget::DriverParameters
        ));
    }

    #[test]
    fn eta_uses_elapsed_time_and_progress() {
        let mut job = JobView::default();
        job.loaded("sample.nc".into(), sample_program(), 100);
        job.started(1, 100);
        job.progress(41, 40, 100, Duration::from_secs(90));

        let eta = job.eta().expect("a running job with progress has an ETA");
        assert_eq!(eta, Duration::from_secs(135));
        assert_eq!(format_elapsed(eta), "02:15");

        job.paused(41, JobPauseReason::ProgramStop);
        assert_eq!(job.eta(), Some(Duration::from_secs(135)));
    }

    #[test]
    fn eta_is_hidden_when_idle_complete_or_at_zero_progress() {
        let mut job = JobView::default();
        assert_eq!(job.eta(), None, "an empty view has no ETA");

        job.loaded("sample.nc".into(), sample_program(), 100);
        assert_eq!(job.eta(), None, "a loaded but idle job has no ETA");

        job.started(1, 100);
        assert_eq!(job.eta(), None, "no confirmed line means no ETA");

        job.progress(1, 0, 100, Duration::from_secs(5));
        assert_eq!(job.eta(), None, "zero progress means no ETA");

        job.progress(1, 50, 100, Duration::from_secs(5));
        assert!(job.eta().is_some());

        job.completed(Duration::from_secs(10));
        assert_eq!(job.eta(), None, "a completed job has no ETA");

        job.started(1, 100);
        job.progress(1, 50, 100, Duration::from_secs(5));
        job.aborted(51, JobAbortReason::OperatorStop);
        assert_eq!(job.eta(), None, "an aborted job has no ETA");
    }

    #[test]
    fn eta_rejects_invalid_progress_and_clamps_the_maximum() {
        let elapsed = Duration::from_secs(60);
        assert_eq!(estimate_remaining(elapsed, 0.0), None);
        assert_eq!(estimate_remaining(elapsed, -0.25), None);
        assert_eq!(estimate_remaining(elapsed, f32::NAN), None);
        assert_eq!(estimate_remaining(elapsed, f32::INFINITY), None);
        assert_eq!(estimate_remaining(elapsed, 1.0), None);
        assert_eq!(estimate_remaining(elapsed, 2.0), None);
        assert_eq!(
            estimate_remaining(Duration::ZERO, 0.5),
            None,
            "no elapsed time means no rate"
        );

        let huge = estimate_remaining(Duration::from_secs(3_600), 0.000_1)
            .expect("a tiny progress sample still produces a bounded estimate");
        assert_eq!(huge, MAX_JOB_ETA);
        assert!(huge <= MAX_JOB_ETA);
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

    #[test]
    fn profile_connect_surfaces_the_saved_maps_for_that_controller() {
        let (state, _dir, _info) = state_with_profile("saved-maps");
        let maps = state.saved_height_maps();
        assert_eq!(maps.len(), 2);
        assert_eq!(
            maps.iter().map(|map| map.name.as_str()).collect::<Vec<_>>(),
            vec!["top", "bottom"]
        );
        assert!(state.selected_saved_map().is_none());
    }

    #[test]
    fn profile_probe_offset_loads_on_connect_and_saves_without_controller_writes() {
        let dir = TempRoot::new("probe-offset");
        let store = ConfigStore::new(&dir.path);
        let info = parse_controller_info(FLEXIHAL);
        let key = ProfileKey::from_controller(&info);
        let mut data = ControllerData::new();
        data.probe_offset = ProbeOffsetRecord {
            x_mm: 4.5,
            y_mm: -2.5,
        };
        store.save_controller_data(&key, &data).expect("profile");

        let mut state = AppState::new();
        state.config = ConfigModel::from_store(store.clone());
        state.begin_live("test transport");
        state.apply_controller_info(&info);
        assert_eq!(
            state.probe.probe_offset().expect("loaded offset"),
            XyPoint::new(4.5, -2.5)
        );

        state
            .probe
            .set_probe_offset(XyPoint::new(9.0, 1.25))
            .expect("editor update");
        let path = state.save_probe_offset().expect("save offset");
        assert!(path.exists());
        assert_eq!(
            store
                .load_controller_data(&key)
                .expect("load")
                .expect("data")
                .probe_offset,
            ProbeOffsetRecord {
                x_mm: 9.0,
                y_mm: 1.25
            }
        );
    }

    #[test]
    fn profile_probe_defaults_load_all_shared_fields_with_three_decimals() {
        let (state, _dir, _info, _store) =
            state_with_probe_defaults("probe-defaults-load", profile_probe_defaults());
        assert_eq!(
            shared_probe_text(&state),
            vec![
                "22.500".to_string(),
                "6.500".to_string(),
                "220.250".to_string(),
                "1.500".to_string(),
                "2.000".to_string(),
                "0.250".to_string(),
                "9.500".to_string(),
            ]
        );
    }

    #[test]
    fn old_profile_without_probe_defaults_loads_the_factory_record() {
        let dir = TempRoot::new("probe-defaults-old");
        let store = ConfigStore::new(&dir.path);
        let info = parse_controller_info(FLEXIHAL);
        let key = ProfileKey::from_controller(&info);
        let path = store.profile_path(&key);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        // A profile written before `probe_defaults` existed: the controller
        // data carries only the schema version, so every newer field defaults.
        fs::write(
            &path,
            format!(
                r#"{{"profile":"{}","data":{{"schema_version":1}}}}"#,
                key.id()
            ),
        )
        .expect("write old profile");

        let mut state = AppState::new();
        state.config = ConfigModel::from_store(store);
        state.begin_live("test transport");
        state.apply_controller_info(&info);
        assert_eq!(
            shared_probe_text(&state),
            vec![
                "10.000".to_string(),
                "5.000".to_string(),
                "100.000".to_string(),
                "2.000".to_string(),
                "3.000".to_string(),
                "0.000".to_string(),
                "10.000".to_string(),
            ]
        );
    }

    #[test]
    fn probe_defaults_save_writes_the_exact_record_and_reloads_it() {
        let dir = TempRoot::new("probe-defaults-save");
        let store = ConfigStore::new(&dir.path);
        let info = parse_controller_info(FLEXIHAL);
        let key = ProfileKey::from_controller(&info);
        let mut state = AppState::new();
        state.config = ConfigModel::from_store(store.clone());
        state.begin_live("test transport");
        state.apply_controller_info(&info);
        assert!(state.probe.apply_defaults(&profile_probe_defaults()));

        let path = state.save_probe_defaults().expect("save defaults");
        assert_eq!(path, store.profile_path(&key));
        assert_eq!(state.config.probe_defaults(), profile_probe_defaults());
        assert_eq!(
            store
                .load_controller_data(&key)
                .expect("load")
                .expect("data")
                .probe_defaults,
            profile_probe_defaults()
        );

        let mut reopened = AppState::new();
        reopened.config = ConfigModel::from_store(store);
        reopened.begin_live("reload");
        reopened.apply_controller_info(&info);
        assert_eq!(
            shared_probe_text(&reopened),
            vec![
                "22.500".to_string(),
                "6.500".to_string(),
                "220.250".to_string(),
                "1.500".to_string(),
                "2.000".to_string(),
                "0.250".to_string(),
                "9.500".to_string(),
            ]
        );
    }

    #[test]
    fn probe_defaults_load_and_save_are_refused_during_a_probe_run() {
        let (mut state, _dir, _info, _store) =
            state_with_probe_defaults("probe-defaults-active", profile_probe_defaults());
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        let ctx = probe_default_context();
        assert!(state.probe.select_card(ProbeCard::ZTouchPlate, &ctx));
        let setup = state.probe.prepare(&ctx).expect("z plan");
        state.probe.begin(setup);
        assert!(state.probe.is_running());

        let before = shared_probe_text(&state);
        let error = state
            .save_probe_defaults()
            .expect_err("an active run blocks save");
        assert!(error.contains("probe run"), "{error}");
        assert_eq!(shared_probe_text(&state), before, "save left fields alone");

        state
            .probe
            .set_field_text(ProbeField::SafeZ, "99.000")
            .expect("local edit");
        let edited = shared_probe_text(&state);
        state.load_probe_defaults();
        assert_eq!(
            shared_probe_text(&state),
            edited,
            "a profile load must not overwrite a run"
        );
    }

    #[test]
    fn probe_defaults_save_without_an_active_profile_is_refused() {
        let mut state = AppState::new();
        assert!(state.config.key().is_none());
        let error = state
            .save_probe_defaults()
            .expect_err("no controller profile");
        assert!(error.contains("no controller profile"), "{error}");
    }

    #[test]
    fn invalid_probe_default_field_is_named_and_writes_nothing() {
        let (mut state, _dir, _info, store) =
            state_with_probe_defaults("probe-defaults-invalid", profile_probe_defaults());
        let key = ProfileKey::from_controller(&parse_controller_info(FLEXIHAL));
        state
            .probe
            .set_field_text(ProbeField::Search, "0")
            .expect("zero is valid for the editor");
        let before = store
            .load_controller_data(&key)
            .expect("load")
            .expect("data")
            .probe_defaults;

        let error = state
            .save_probe_defaults()
            .expect_err("zero search is not a valid default");
        assert!(error.contains("SEARCH"), "{error}");
        assert_eq!(state.probe.field_text(ProbeField::Search), "0");
        assert_eq!(
            store
                .load_controller_data(&key)
                .expect("load")
                .expect("data")
                .probe_defaults,
            before,
            "an invalid record must not be written"
        );
    }

    #[test]
    fn probe_defaults_load_and_save_emit_no_controller_command() {
        let (mut state, _dir, _info, _store) =
            state_with_probe_defaults("probe-defaults-no-command", profile_probe_defaults());
        let mut sent = Vec::new();
        // These are local profile operations and take no command sender. The
        // recorder stays empty across both paths.
        drop(recorded_send(&mut sent));
        state.load_probe_defaults();
        assert!(state.probe.apply_defaults(&second_probe_defaults()));
        let _ = state.save_probe_defaults().expect("save defaults");
        assert!(sent.is_empty(), "{sent:?}");
    }

    #[test]
    fn probe_defaults_save_failure_keeps_the_previous_record() {
        let (mut state, _dir, _info, store) =
            state_with_probe_defaults("probe-defaults-failure", profile_probe_defaults());
        let key = ProfileKey::from_controller(&parse_controller_info(FLEXIHAL));
        assert!(state.probe.apply_defaults(&second_probe_defaults()));
        let path = state.save_probe_defaults().expect("first save");
        assert_eq!(
            store
                .load_controller_data(&key)
                .expect("load")
                .expect("data")
                .probe_defaults,
            second_probe_defaults()
        );

        // Make the atomic replace fail without touching the in-memory record.
        fs::remove_file(&path).expect("remove profile");
        fs::create_dir_all(&path).expect("block profile path");
        assert!(state.probe.apply_defaults(&profile_probe_defaults()));
        state
            .save_probe_defaults()
            .expect_err("a blocked profile path fails the write");
        assert_eq!(
            state.config.probe_defaults(),
            second_probe_defaults(),
            "a failed save keeps the previous defaults"
        );
        assert!(path.is_dir(), "the failed write did not replace the path");
    }

    #[test]
    fn probe_defaults_are_stored_per_controller_profile() {
        let dir = TempRoot::new("probe-defaults-separation");
        let store = ConfigStore::new(&dir.path);
        let first = parse_controller_info(FLEXIHAL);
        let mut second = parse_controller_info(FLEXIHAL);
        second.board = Some("OtherBoard".into());
        assert_ne!(
            ProfileKey::from_controller(&first),
            ProfileKey::from_controller(&second)
        );

        let mut first_state = AppState::new();
        first_state.config = ConfigModel::from_store(store.clone());
        first_state.begin_live("first transport");
        first_state.apply_controller_info(&first);
        assert!(first_state.probe.apply_defaults(&profile_probe_defaults()));
        first_state
            .save_probe_defaults()
            .expect("save first defaults");

        let mut second_state = AppState::new();
        second_state.config = ConfigModel::from_store(store.clone());
        second_state.begin_live("second transport");
        second_state.apply_controller_info(&second);
        assert!(second_state.probe.apply_defaults(&second_probe_defaults()));
        second_state
            .save_probe_defaults()
            .expect("save second defaults");

        let first_data = store
            .load_controller_data(&ProfileKey::from_controller(&first))
            .expect("load first")
            .expect("first profile");
        let second_data = store
            .load_controller_data(&ProfileKey::from_controller(&second))
            .expect("load second")
            .expect("second profile");
        assert_eq!(first_data.probe_defaults, profile_probe_defaults());
        assert_eq!(second_data.probe_defaults, second_probe_defaults());

        let mut reopened = AppState::new();
        reopened.config = ConfigModel::from_store(store);
        reopened.begin_live("first again");
        reopened.apply_controller_info(&first);
        assert_eq!(
            reopened.probe.field_text(ProbeField::SafeZ),
            "22.500",
            "the first profile keeps its own defaults"
        );
    }

    #[test]
    fn demo_probe_defaults_state_is_reproducible() {
        let dir = TempRoot::new("probe-defaults-demo");
        let store = ConfigStore::new(&dir.path);
        let info = parse_controller_info(FLEXIHAL);
        let key = ProfileKey::from_controller(&info);
        let mut data = ControllerData::new();
        data.probe_defaults = profile_probe_defaults();
        store.save_controller_data(&key, &data).expect("profile");

        let snapshot = || {
            let mut state = AppState::new();
            state.config = ConfigModel::from_store(store.clone());
            state.begin_live("demo");
            state.apply_controller_info(&info);
            assert!(state.probe_defaults_demo_ready());
            let path = state.save_probe_defaults().expect("demo save");
            (shared_probe_text(&state), path)
        };
        let first = snapshot();
        let second = snapshot();
        assert_eq!(first, second);
        assert_eq!(
            first.0,
            vec![
                "22.500".to_string(),
                "6.500".to_string(),
                "220.250".to_string(),
                "1.500".to_string(),
                "2.000".to_string(),
                "0.250".to_string(),
                "9.500".to_string(),
            ]
        );
    }

    #[test]
    fn offsets_nc_export_and_preview_use_the_config_export_dir() {
        let (mut state, dir, _info) = state_with_profile("offsets-nc");
        let mut offsets = Offsets::default();
        offsets.wcs[Wcs::G54.index()] = Some(Position::xyz(1.0, 2.0, 3.0));
        offsets.g92 = Some(Position::xyz(9.0, 9.0, 9.0));
        state.apply_offsets(&offsets);

        let export = state.export_offsets_nc().expect("export offsets");
        assert_eq!(export.path, dir.path.join("exports").join("offsets.nc"));
        assert_eq!(export.wcs_lines, 1);
        let text = fs::read_to_string(&export.path).expect("read export");
        assert!(text.contains("G10 L2 P1 X1.000 Y2.000 Z3.000"), "{text}");
        assert!(
            !text
                .lines()
                .any(|line| line.trim_start().starts_with("G92")),
            "{text}"
        );

        state.open_offsets_preview().expect("open preview");
        assert_eq!(
            state
                .offsets_backup
                .preview()
                .expect("preview")
                .lines()
                .len(),
            1
        );
        assert!(matches!(
            state.take_offsets_program().expect("load program"),
            Command::LoadProgram { .. }
        ));
        assert!(state.offsets_backup.preview().is_none());
    }

    #[test]
    fn selecting_a_saved_map_never_applies_compensation_or_starts_a_job() {
        let (mut state, _dir, _info) = state_with_profile("saved-select");
        let source = "G21 G90 G54\nG0 X0 Y0\nG1 Z-1 F200\nM30\n";
        let program = Arc::new(ghs_gcode::parse_program(source).expect("program"));
        state
            .height_map
            .remember_exact_source("part.nc", source.to_string());
        state.height_map.on_job_loaded("part.nc", &program, false);
        state.job.loaded("part.nc".into(), Arc::clone(&program), 4);

        state
            .select_saved_height_map("top")
            .expect("saved map opens");
        let selected = state.selected_saved_map().expect("selected");
        assert_eq!(selected.record.name, "top");
        assert!(selected.overlay.cells.len() == 4);

        // Loading is a view action: no compensation is applied, no program is
        // loaded, and the job runner stays exactly where it was.
        assert!(state.height_map.applied().is_none());
        assert!(state.height_map.pending().is_none());
        assert!(state.height_map.last_error().is_none());
        assert_eq!(state.job.state, JobState::Idle);
        assert_eq!(state.job.name, "part.nc");
        assert_eq!(state.job.current_line, 0);
        assert!(state.probe.run().is_none());
        assert!(
            state.height_map.original().is_some(),
            "the current program is recorded only for a later explicit Apply"
        );

        // The existing gated Apply can use the loaded record, but only builds a
        // preview; it still produces no Start command.
        let snapshot = selected.snapshot();
        let frame = selected.frame;
        let gate = crate::height_compensation::HeightApplyGate {
            ready: true,
            machine_state: MachineState::Idle,
            job_active: false,
            offsets_reported: true,
            frame,
        };
        state
            .height_map
            .request_apply(&snapshot, &gate)
            .expect("apply preview");
        let preview = state.height_map.pending().expect("preview").clone();
        assert_eq!(preview.program_name, "part.nc [height map]");
        assert!(matches!(
            preview.load().command(),
            ghs_engine::Command::LoadProgram { .. }
        ));
        assert_eq!(state.job.state, JobState::Idle);

        // Switching the viewed record discards the unconfirmed preview; it
        // cannot survive into another map's confirmation step.
        state
            .select_saved_height_map("bottom")
            .expect("second saved map opens");
        assert!(state.height_map.pending().is_none());
    }

    #[test]
    fn a_completed_probe_run_saves_into_the_active_profile() {
        let (mut state, _dir, _info) = state_with_profile("saved-from-run");
        let ctx = ProbeContext {
            live: true,
            ready: true,
            machine_state: MachineState::Idle,
            job_active: false,
            machine_pos_mm: Position::xyz(0.0, 0.0, 5.0),
            work_pos_mm: Position::zero(),
            wcs: Wcs::G54,
            wcs_value_mm: Position::xyz(1.0, 2.0, 3.0),
            g92_mm: Position::zero(),
            tlo_z_mm: 0.0,
            spindle_on: false,
            coolant_on: false,
            has_probe: true,
            has_probe_connected_signal: false,
            probe_disconnected: false,
            probe_triggered: false,
            offsets_reported: true,
            units: Units::Millimetre,
        };
        state
            .probe
            .select_card(crate::probe::ProbeCard::HeightMap, &ctx);
        state.probe.probe_connected_confirmed = true;
        let setup = state
            .probe
            .preflight(&ctx)
            .setup
            .expect("height-map preflight");
        let name = setup.plan.name.clone();
        let expected = setup.plan.expected_probes;
        state.probe.begin(setup);
        assert!(state.probe.on_job_loaded(&name));
        assert!(state.probe.on_job_started());
        for index in 0..expected {
            let x = 10.0 + (index % 4) as f64 * 10.0;
            let y = 20.0 + (index / 4) as f64 * 10.0;
            assert!(state.probe.bind_result(ghs_engine::ProbeResult::new(
                true,
                Position::xyz(x, y, -1.0 - index as f64 * 0.01)
            )));
        }
        assert!(state.probe.on_job_completed());
        assert_eq!(
            state.probe.run().map(|run| run.phase),
            Some(crate::probe::ProbePhase::Complete)
        );

        let saved = state
            .save_current_height_map("run result", 1_700_000_000)
            .expect("save the measured map");
        assert!(saved.exists());
        let maps = state.saved_height_maps();
        assert_eq!(maps.len(), 3, "two fixture maps plus the new one");
        let record = maps.last().expect("record");
        assert_eq!(record.name, "run result");
        assert_eq!(record.created_at_unix_s, Some(1_700_000_000));
        assert_eq!(record.columns, 4);
        assert_eq!(record.rows, 4);
        assert_eq!(record.samples.len(), expected);
        assert_eq!(record.frame.wcs, Wcs::G54);
        assert_eq!(record.frame.wcs_offset, Position::xyz(1.0, 2.0, 3.0));
        assert_eq!(
            state
                .height_map_library
                .last_saved()
                .map(|notice| notice.name.as_str()),
            Some("run result")
        );
        assert_eq!(state.job.state, JobState::Idle, "saving never starts a job");
    }

    #[test]
    fn selecting_a_saved_map_is_refused_while_a_probe_run_is_active() {
        let (mut state, _dir, _info) = state_with_profile("saved-running");
        // A placeholder run is enough to prove the navigation gate; it is not
        // started and never reaches the controller.
        let program =
            Arc::new(ghs_gcode::parse_program("G21\nG0 X0\nM30\n").expect("probe program"));
        state.job.loaded("height-map".into(), program, 3);
        let ctx = ProbeContext {
            live: true,
            ready: true,
            machine_state: MachineState::Idle,
            job_active: false,
            machine_pos_mm: Position::zero(),
            work_pos_mm: Position::zero(),
            wcs: Wcs::G54,
            wcs_value_mm: Position::zero(),
            g92_mm: Position::zero(),
            tlo_z_mm: 0.0,
            spindle_on: false,
            coolant_on: false,
            has_probe: true,
            has_probe_connected_signal: false,
            probe_disconnected: false,
            probe_triggered: false,
            offsets_reported: true,
            units: Units::Millimetre,
        };
        state
            .probe
            .select_card(crate::probe::ProbeCard::HeightMap, &ctx);
        state.probe.probe_connected_confirmed = true;
        let setup = state
            .probe
            .preflight(&ctx)
            .setup
            .expect("height-map preflight");
        state.probe.begin(setup);

        let error = state
            .select_saved_height_map("top")
            .expect_err("an active run blocks navigation");
        assert!(error.contains("probe run is active"), "{error}");
        assert!(state.selected_saved_map().is_none());
    }

    fn controller_tool(number: u32, name: &str) -> ToolEntry {
        ToolEntry {
            number,
            radius_mm: Some(3.0),
            offset: Position::xyz(f64::from(number), -2.0, -30.0),
            name: Some(name.to_string()),
            pocket: Some(number),
        }
    }

    #[test]
    fn controller_tools_stay_separate_from_the_app_table() {
        let mut state = AppState::new();
        assert!(!state.controller_tools.reported());
        assert!(state.controller_tools.is_empty());

        state
            .tools
            .insert(ghs_config::ToolRecord {
                number: 1,
                name: "App tool".into(),
                diameter_mm: 6.0,
                length_mm: 35.0,
                pocket: Some(1),
            })
            .expect("app tool");
        let app_before = state.tools.table().clone();

        assert!(state.apply_controller_tools(&[
            controller_tool(3, "Controller three"),
            controller_tool(7, "Controller seven"),
        ]));
        assert!(state.controller_tools.reported());
        assert_eq!(state.controller_tools.len(), 2);
        assert_eq!(state.controller_tools.entries()[0].number, 3);
        assert_eq!(state.tools.table(), &app_before);

        // A later empty controller report is still a report, and still does not
        // touch the app table.
        assert!(state.apply_controller_tools(&[]));
        assert!(state.controller_tools.reported());
        assert!(state.controller_tools.is_empty());
        assert_eq!(state.tools.table(), &app_before);
    }

    #[test]
    fn startup_lines_load_from_a_controller_report_and_survive_a_reconnect_reset() {
        let mut state = AppState::new();
        assert!(!state.startup_lines.reported());
        assert_eq!(state.startup_lines.rows().len(), 2);

        let lines = [
            StartupLine {
                index: 0,
                line: "G21 G90".into(),
            },
            StartupLine {
                index: 1,
                line: "M3 S0".into(),
            },
        ];
        assert!(state.apply_startup_lines(&lines));
        assert!(state.startup_lines.fully_reported());
        assert_eq!(state.startup_lines.row(0).expect("row 0").stored, "G21 G90");
        assert_eq!(state.startup_lines.row(1).expect("row 1").stored, "M3 S0");

        // A new session never shows the previous controller's startup lines.
        state.begin_live("test transport");
        assert!(!state.startup_lines.reported());
        assert!(
            state
                .startup_lines
                .rows()
                .iter()
                .all(|row| row.stored.is_empty())
        );
    }

    #[test]
    fn startup_write_gate_mirrors_connection_machine_and_job_state() {
        let mut state = AppState::new();
        // `AppState::new` starts in mock mode.
        assert_eq!(
            state.startup_write_gate().blocked_reason(),
            Some("mock mode: no controller")
        );

        state.begin_live("test transport");
        state.connection = ConnectionState::Connecting;
        assert_eq!(
            state.startup_write_gate().blocked_reason(),
            Some("not connected")
        );

        state.connection = ConnectionState::Ready;
        state.machine_state = "Alarm";
        assert_eq!(
            state.startup_write_gate().blocked_reason(),
            Some("alarm or door")
        );

        state.machine_state = "Idle";
        let program = Arc::new(ghs_gcode::parse_program("G21\nM30\n").expect("program"));
        state.job.loaded("job.nc".into(), program, 2);
        state.job.started(1, 2);
        assert_eq!(
            state.startup_write_gate().blocked_reason(),
            Some("job active")
        );

        state.job.completed(Duration::from_secs(1));
        state.machine_state = "Run";
        assert_eq!(
            state.startup_write_gate().blocked_reason(),
            Some("controller not Idle")
        );

        state.machine_state = "Idle";
        assert_eq!(state.startup_write_gate().blocked_reason(), None);
    }

    #[test]
    fn user_macro_add_edit_delete_round_trips_through_the_profile() {
        let (mut state, dir, info) = state_with_profile("macro-roundtrip");
        let store = ConfigStore::new(&dir.path);
        let key = ProfileKey::from_controller(&info);

        let path = state
            .save_macro(None, "Zero XY", "G10 L20 P1 X0 Y0", true)
            .expect("add macro");
        assert!(path.exists());
        assert_eq!(state.user_macro_list().len(), 1);
        assert!(state.user_macro_list()[0].confirm);

        state
            .save_macro(Some("Zero XY"), "Zero work XY", "G10 L20 P1 X0 Y0", false)
            .expect("edit macro");
        state
            .save_macro(None, "Spindle off", "M5", false)
            .expect("second macro");
        assert_eq!(state.user_macro_list().len(), 2);
        assert_eq!(state.user_macro_list()[0].name, "Zero work XY");
        assert!(!state.user_macro_list()[0].confirm);

        state
            .delete_macro("Spindle off")
            .expect("delete second macro");
        assert_eq!(state.user_macro_list().len(), 1);

        let stored = store
            .load_controller_data(&key)
            .expect("load")
            .expect("profile");
        assert_eq!(stored.macros, state.user_macro_list());

        let mut reopened = ConfigModel::from_store(store);
        reopened.on_controller(&info).expect("reload");
        assert_eq!(reopened.macros(), state.user_macro_list());
        assert_eq!(reopened.macros()[0].name, "Zero work XY");
        assert_eq!(reopened.macros()[0].command, "G10 L20 P1 X0 Y0");
    }

    #[test]
    fn macro_save_rejects_duplicate_names_and_bad_commands_without_writing() {
        let (mut state, dir, _info) = state_with_profile("macro-validation");
        state
            .save_macro(None, "Safe Z", "G53 G0 Z0", true)
            .expect("first macro");
        let path = state.config.profile_path().expect("profile path");
        let before = fs::read(&path).expect("read profile");

        let duplicate = state
            .save_macro(None, " Safe Z ", "M5", false)
            .expect_err("duplicate name");
        assert!(duplicate.contains("already exists"), "{duplicate}");

        let control = state
            .save_macro(None, "Bad\nName", "M5", false)
            .expect_err("control character");
        assert!(control.contains("control"), "{control}");

        let empty = state
            .save_macro(None, "Empty", "  ", false)
            .expect_err("empty command");
        assert!(empty.contains("required"), "{empty}");

        let system = state
            .save_macro(None, "Home", "$H", false)
            .expect_err("system command");
        assert!(system.contains("system"), "{system}");

        let multi = state
            .save_macro(None, "Two lines", "G0 X0\nG0 Y0", false)
            .expect_err("multi-line command");
        assert!(multi.contains("single MDI line"), "{multi}");

        assert_eq!(state.user_macro_list().len(), 1);
        assert_eq!(fs::read(&path).expect("read profile"), before);
        let _ = dir;
    }

    #[test]
    fn user_macros_never_cross_a_controller_profile() {
        let dir = TempRoot::new("macro-profiles");
        let store = ConfigStore::new(&dir.path);
        let first = parse_controller_info(FLEXIHAL);
        let mut second = parse_controller_info(FLEXIHAL);
        second.board = Some("OtherBoard".into());

        let mut writer = AppState::new();
        writer.config = ConfigModel::from_store(store.clone());
        writer.begin_live("first transport");
        writer.apply_controller_info(&first);
        writer
            .save_macro(None, "First only", "M5", false)
            .expect("save first profile macro");

        let mut reader = AppState::new();
        reader.config = ConfigModel::from_store(store.clone());
        reader.begin_live("second transport");
        reader.apply_controller_info(&second);
        assert!(
            reader.user_macro_list().is_empty(),
            "a different controller must not inherit macros"
        );
        reader
            .save_macro(None, "Second only", "M3", false)
            .expect("save second profile macro");

        let first_data = store
            .load_controller_data(&ProfileKey::from_controller(&first))
            .expect("first load")
            .expect("first profile");
        let second_data = store
            .load_controller_data(&ProfileKey::from_controller(&second))
            .expect("second load")
            .expect("second profile");
        assert_eq!(
            first_data
                .macros
                .iter()
                .map(|mac| mac.name.as_str())
                .collect::<Vec<_>>(),
            vec!["First only"]
        );
        assert_eq!(
            second_data
                .macros
                .iter()
                .map(|mac| mac.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Second only"]
        );

        let mut reopened = AppState::new();
        reopened.config = ConfigModel::from_store(store);
        reopened.begin_live("first transport again");
        reopened.apply_controller_info(&first);
        assert_eq!(
            reopened
                .user_macro_list()
                .iter()
                .map(|mac| mac.name.as_str())
                .collect::<Vec<_>>(),
            vec!["First only"]
        );
    }

    #[test]
    fn macro_save_is_refused_for_a_corrupt_profile() {
        let dir = TempRoot::new("macro-corrupt");
        let store = ConfigStore::new(&dir.path);
        let info = parse_controller_info(FLEXIHAL);
        let path = store.profile_path(&ProfileKey::from_controller(&info));
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let corrupt = b"{ not json";
        fs::write(&path, corrupt).expect("write corrupt profile");

        let mut state = AppState::new();
        state.config = ConfigModel::from_store(store);
        state.begin_live("test transport");
        state.apply_controller_info(&info);
        assert!(state.config.corrupt_profile());

        let error = state
            .save_macro(None, "Safe Z", "G53 G0 Z0", true)
            .expect_err("corrupt profile refused");
        assert!(error.contains("corrupt"), "{error}");
        assert_eq!(fs::read(&path).expect("read"), corrupt);
        assert!(state.user_macro_list().is_empty());
    }

    #[test]
    fn a_confirmed_macro_is_not_sent_before_the_second_click() {
        let (mut state, _dir, _info) = ready_state_with_macro("macro-confirm", true);
        let request = state.request_macro_run("Safe Z").expect("run request");
        assert!(matches!(request, MacroRunRequest::Confirm(_)));
        assert_eq!(
            state.macro_run_pending().map(|mac| mac.name.as_str()),
            Some("Safe Z")
        );

        // No command is produced by the request itself. The captured record is
        // the only thing that can produce one.
        assert!(state.cancel_macro_run());
        assert!(state.confirm_macro_run().is_err());
        assert!(state.macro_run_pending().is_none());
    }

    #[test]
    fn a_confirmed_macro_produces_exactly_one_mdi_command() {
        let (mut state, _dir, _info) = ready_state_with_macro("macro-one-command", true);
        let request = state.request_macro_run("Safe Z").expect("run request");
        assert!(matches!(request, MacroRunRequest::Confirm(_)));

        let command = state.confirm_macro_run().expect("confirmed command");
        assert_eq!(command, Command::Mdi("G53 G0 Z0".into()));
        assert!(state.confirm_macro_run().is_err());
        assert!(state.macro_run_pending().is_none());
    }

    #[test]
    fn cancelling_a_macro_confirmation_sends_nothing() {
        let (mut state, _dir, _info) = ready_state_with_macro("macro-cancel", true);
        state.request_macro_run("Safe Z").expect("run request");
        assert!(state.cancel_macro_run());
        assert!(state.confirm_macro_run().is_err());
        assert!(state.macro_run_pending().is_none());
    }

    #[test]
    fn macro_run_gate_refuses_mock_not_ready_alarm_door_job_and_not_idle() {
        let mut mock = AppState::new();
        assert_eq!(
            mock.macro_gate().blocked_reason(),
            Some("mock mode: no controller")
        );
        let error = mock
            .request_macro_run("Safe Z")
            .expect_err("mock mode refused");
        assert!(error.contains("mock"), "{error}");

        let (mut state, _dir, _info) = state_with_profile("macro-gates");
        state
            .save_macro(None, "Safe Z", "G53 G0 Z0", false)
            .expect("macro");
        assert_eq!(state.macro_gate().blocked_reason(), Some("not connected"));
        let error = state
            .request_macro_run("Safe Z")
            .expect_err("not ready refused");
        assert!(error.contains("not connected"), "{error}");

        state.connection = ConnectionState::Ready;
        state.machine_state = "Alarm";
        assert_eq!(state.macro_gate().blocked_reason(), Some("alarm or door"));
        assert!(state.request_macro_run("Safe Z").is_err());

        state.machine_state = "Door";
        assert_eq!(state.macro_gate().blocked_reason(), Some("alarm or door"));

        state.machine_state = "Idle";
        let program = sample_program();
        state.job.loaded("job.nc".into(), program, 4);
        state.job.started(1, 4);
        assert_eq!(state.macro_gate().blocked_reason(), Some("job active"));
        let error = state
            .request_macro_run("Safe Z")
            .expect_err("active job refused");
        assert!(error.contains("job active"), "{error}");

        state.job.completed(Duration::from_secs(1));
        state.machine_state = "Run";
        assert_eq!(
            state.macro_gate().blocked_reason(),
            Some("controller not Idle")
        );
        assert!(state.request_macro_run("Safe Z").is_err());

        state.machine_state = "Idle";
        assert_eq!(state.macro_gate().blocked_reason(), None);
        assert!(matches!(
            state.request_macro_run("Safe Z"),
            Ok(MacroRunRequest::Send(_))
        ));
    }

    #[test]
    fn macro_save_requires_a_verified_profile() {
        let dir = TempRoot::new("macro-save-gates");
        let store = ConfigStore::new(&dir.path);

        let mut no_profile = AppState::new();
        no_profile.config = ConfigModel::from_store(store.clone());
        let error = no_profile
            .save_macro(None, "Safe Z", "G53 G0 Z0", true)
            .expect_err("no profile");
        assert!(error.contains("no controller profile"), "{error}");

        let mut anonymous = ControllerInfo::new();
        anonymous.version = "1.1f".into();
        let key = ProfileKey::from_controller(&anonymous);
        assert!(!key.is_identified());
        store
            .save_controller_data(&key, &ControllerData::new())
            .expect("anonymous profile");
        let mut unverified = AppState::new();
        unverified.config = ConfigModel::from_store(store);
        unverified.begin_live("test transport");
        unverified.apply_controller_info(&anonymous);
        assert!(unverified.config.unverified_identity());
        assert!(unverified.config.has_pending_profile());
        let error = unverified
            .save_macro(None, "Safe Z", "G53 G0 Z0", true)
            .expect_err("unverified profile");
        assert!(
            error.contains("unverified") || error.contains("not verified"),
            "{error}"
        );
    }

    #[test]
    fn loading_or_connecting_never_runs_or_arms_a_macro() {
        let dir = TempRoot::new("macro-no-auto-run");
        let store = ConfigStore::new(&dir.path);
        let info = parse_controller_info(FLEXIHAL);
        let key = ProfileKey::from_controller(&info);
        let mut data = ControllerData::new();
        data.macros = vec![
            ConfigMacro {
                name: "Direct".into(),
                command: "G0 X1".into(),
                confirm: false,
            },
            ConfigMacro {
                name: "Confirmed".into(),
                command: "G0 X2".into(),
                confirm: true,
            },
        ];
        store.save_controller_data(&key, &data).expect("profile");

        let mut state = AppState::new();
        state.config = ConfigModel::from_store(store);
        state.begin_live("test transport");
        state.apply_controller_info(&info);

        assert_eq!(state.user_macro_list().len(), 2);
        assert!(state.macro_run_pending().is_none());
        assert!(state.macro_last_error().is_none());
        assert!(!state.cancel_macro_run());
        assert!(state.confirm_macro_run().is_err());
        assert!(
            state
                .console
                .iter()
                .all(|line| !line.text.starts_with("Macro '")),
            "loading a profile must not send a macro"
        );

        // The handshake completing is also not a trigger.
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        state.apply_controller_info(&info);
        assert!(state.macro_run_pending().is_none());
        assert!(state.macro_last_error().is_none());
    }

    #[test]
    fn a_macro_send_rejection_is_recorded_once_and_not_retried() {
        let (mut state, _dir, _info) = ready_state_with_macro("macro-rejection", false);
        let request = state.request_macro_run("Safe Z").expect("run request");
        assert!(matches!(request, MacroRunRequest::Send(_)));
        state.macro_send_failed("Macro 'Safe Z' not sent: busy");
        assert!(
            state
                .macro_last_error()
                .is_some_and(|error| error.contains("busy"))
        );
        assert!(state.confirm_macro_run().is_err());
        assert!(state.macro_run_pending().is_none());
    }

    // ------------------------------------------------------- program search

    fn search_program(lines: &[&str]) -> Arc<Program> {
        let source: String = lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>();
        Arc::new(ghs_gcode::parse_program(&source).expect("search program parses"))
    }

    fn compiled_search(query: &str, program: &Program, mode: ProgramSearchMode) -> ProgramSearch {
        let mut search = ProgramSearch::default();
        search.set_mode(mode);
        search.set_query(query);
        search.refresh(program, 0x5eed);
        search
    }

    #[test]
    fn program_search_matches_source_text_case_insensitively() {
        let program = search_program(&["G21", "G0 X10 Y20", "G1 X30 F800", "(X10 comment)"]);

        let lower = compiled_search("x10", &program, ProgramSearchMode::Filter);
        assert_eq!(lower.match_count(), 2);
        assert_eq!(lower.current_source_index(), Some(1));
        assert!(lower.is_match(1));
        assert!(lower.is_match(3));

        let upper = compiled_search("G0 X10", &program, ProgramSearchMode::Filter);
        assert_eq!(upper.match_count(), 1);
        assert_eq!(upper.current_source_index(), Some(1));
        assert_eq!(
            compiled_search("missing", &program, ProgramSearchMode::Filter).match_count(),
            0
        );
    }

    #[test]
    fn program_search_accepts_plain_and_n_prefixed_line_numbers() {
        let lines: Vec<String> = (1..=130).map(|line| format!("G1 X{line} F800")).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let program = search_program(&refs);

        for query in ["120", "N120", "n120"] {
            let search = compiled_search(query, &program, ProgramSearchMode::Filter);
            assert_eq!(search.match_count(), 1, "{query}");
            assert_eq!(search.current_source_index(), Some(119), "{query}");
            assert_eq!(program.lines[119].line, 120);
        }

        assert_eq!(
            compiled_search("N999", &program, ProgramSearchMode::Filter).match_count(),
            0
        );
    }

    #[test]
    fn program_search_filter_maps_visible_rows_to_original_lines() {
        let program = search_program(&["G21", "G0 X1", "G1 Y1 F800", "G0 X2"]);
        let search = compiled_search("G0", &program, ProgramSearchMode::Filter);

        assert_eq!(search.visible_count(4), 2);
        let visible: Vec<usize> = (0..search.visible_count(4))
            .filter_map(|row| search.visible_source_index(row, 4))
            .collect();
        assert_eq!(visible, vec![1, 3]);
        assert_eq!(program.lines[visible[0]].line, 2);
        assert_eq!(program.lines[visible[1]].line, 4);
        assert_eq!(search.source_visible_index(3, 4), Some(1));
        assert_eq!(search.source_visible_index(2, 4), None);
    }

    #[test]
    fn program_search_highlight_keeps_every_line() {
        let program = search_program(&["G21", "G0 X1", "G1 Y1 F800", "G0 X2"]);
        let search = compiled_search("G0", &program, ProgramSearchMode::Highlight);

        assert_eq!(search.visible_count(4), 4);
        assert_eq!(search.match_count(), 2);
        assert!(search.is_match(1));
        assert!(!search.is_match(2));
        for row in 0..4 {
            assert_eq!(search.visible_source_index(row, 4), Some(row));
        }
    }

    #[test]
    fn program_search_next_and_previous_wrap() {
        let program = search_program(&["G0 X0", "G1 X1 F800", "G0 X2", "G1 X3 F800", "G0 X4"]);
        let mut search = compiled_search("G0", &program, ProgramSearchMode::Filter);
        assert_eq!(search.current_position(), Some(1));
        assert_eq!(search.current_source_index(), Some(0));

        assert!(search.next_match());
        assert_eq!(search.current_source_index(), Some(2));
        assert!(search.next_match());
        assert_eq!(search.current_source_index(), Some(4));
        assert!(search.next_match());
        assert_eq!(search.current_source_index(), Some(0), "next wraps");

        assert!(search.previous_match());
        assert_eq!(search.current_source_index(), Some(4), "previous wraps");
        assert!(search.previous_match());
        assert_eq!(search.current_source_index(), Some(2));
        assert_eq!(search.current_position(), Some(2));
    }

    #[test]
    fn program_search_keeps_the_streaming_line_and_never_moves_the_job() {
        let program = search_program(&["G21", "G0 X10", "G1 X20 F800", "G0 X30"]);
        let mut state = AppState::new();
        state
            .job
            .loaded("search.nc".into(), Arc::clone(&program), 4);
        state.job.progress(3, 2, 4, Duration::ZERO);

        state.set_program_search_query("G1");
        assert!(state.program_search.is_active());
        assert_eq!(state.display_line(), 3);
        assert_eq!(state.program_search_visible_count(), 1);
        // The current streaming line is the only match, and it stays active.
        assert_eq!(state.program_search_scroll_index(), Some(0));
        assert_eq!(state.program_search.visible_source_index(0, 4), Some(2));
        assert_eq!(program.lines[2].line, 3);

        assert!(state.program_search.next_match());
        assert_eq!(state.display_line(), 3);
        assert_eq!(state.job.current_line, 3);
    }

    #[test]
    fn program_search_jump_line_does_not_change_the_loaded_program() {
        let lines: Vec<String> = (1..=130).map(|line| format!("G1 X{line} F800")).collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let program = search_program(&refs);
        let mut state = AppState::new();
        state
            .job
            .loaded("jump.nc".into(), Arc::clone(&program), 130);
        state.job.progress(7, 6, 130, Duration::ZERO);

        state.set_program_search_query("N120");
        assert_eq!(state.program_search.current_source_index(), Some(119));
        assert_eq!(state.program_search_scroll_index(), Some(0));
        assert_eq!(state.display_line(), 7);
        assert_eq!(state.job.line_count(), 130);
        assert!(Arc::ptr_eq(
            state.job.program().expect("program remains loaded"),
            &program
        ));
    }

    #[test]
    fn program_search_clear_restores_all_lines_and_the_current_line() {
        let program = search_program(&["G21", "G0 X1", "G1 Y1 F800", "G0 X2"]);
        let mut state = AppState::new();
        state.job.loaded("clear.nc".into(), Arc::clone(&program), 4);
        state.job.progress(3, 2, 4, Duration::ZERO);
        state.set_program_search_query("G0");
        assert_eq!(state.program_search_visible_count(), 2);

        state.clear_program_search();
        assert!(!state.program_search.is_active());
        assert_eq!(state.program_search.match_count(), 0);
        assert_eq!(state.program_search.current_position(), None);
        assert_eq!(state.program_search_visible_count(), 4);
        assert_eq!(state.display_line(), 3);
        assert_eq!(state.program_search.visible_source_index(3, 4), Some(3));
    }

    #[test]
    fn program_search_scans_the_mock_program_without_a_job() {
        let mut state = AppState::new();
        assert!(!state.job.has_program());
        assert_eq!(state.display_program_lines(), 30);

        state.set_program_search_query("G1");
        assert!(state.program_search.match_count() > 0);
        assert!(state.program_search_visible_count() < 30);

        state.set_program_search_mode(ProgramSearchMode::Highlight);
        assert_eq!(state.program_search_visible_count(), 30);
        assert_eq!(state.display_line(), 13);
    }

    #[test]
    fn program_search_handles_a_missing_program() {
        let mut state = AppState::new();
        state.begin_live("no program");
        state.set_program_search_query("G1");

        assert_eq!(state.display_program_lines(), 0);
        assert_eq!(state.display_line(), 0);
        assert!(state.program_search.is_active());
        assert_eq!(state.program_search.match_count(), 0);
        assert_eq!(state.program_search_visible_count(), 0);
        assert_eq!(state.program_search_scroll_index(), None);

        state.set_program_search_mode(ProgramSearchMode::Highlight);
        assert_eq!(state.program_search_visible_count(), 0);
    }

    fn ready_run_options_state() -> AppState {
        let mut state = AppState::new();
        state.begin_live("run options test");
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        state
    }

    #[test]
    fn run_option_accessors_copy_into_state_and_mark_changes() {
        let mut state = ready_run_options_state();
        assert!(!state.optional_stop);
        assert!(!state.single_block);
        assert!(!state.block_delete);

        assert!(!state.sync_run_options(false, false, false));
        assert!(state.sync_run_options(true, false, false));
        assert!(state.optional_stop);
        assert!(!state.single_block);
        assert!(!state.block_delete);

        assert!(!state.sync_run_options(true, false, false));
        assert!(state.sync_run_options(false, true, true));
        assert!(!state.optional_stop);
        assert!(state.single_block);
        assert!(state.block_delete);
    }

    #[test]
    fn each_run_option_toggle_sends_exactly_one_command() {
        for (option, expected) in [
            (RunOption::OptionalStop, Command::SetOptionalStop(true)),
            (RunOption::SingleBlock, Command::SetSingleBlock(true)),
            (RunOption::BlockDelete, Command::SetBlockDelete(true)),
        ] {
            let mut state = ready_run_options_state();
            let mut sent = Vec::new();
            let outcome = state.dispatch_run_option(option, |command| {
                sent.push(command);
                Ok(())
            });

            assert_eq!(outcome, RunOptionOutcome::Sent);
            assert_eq!(sent, vec![expected]);
            assert_eq!(sent.len(), 1, "a toggle must send exactly one command");
            assert!(
                !state.run_option_value(option),
                "a sent command must not update the displayed value optimistically"
            );
        }
    }

    #[test]
    fn a_rejected_run_option_leaves_the_displayed_value_unchanged() {
        let mut state = ready_run_options_state();
        assert!(state.sync_run_options(true, false, false));
        assert!(state.optional_stop);

        let mut sent = 0;
        let outcome = state.dispatch_run_option(RunOption::OptionalStop, |command| {
            sent += 1;
            assert_eq!(command, Command::SetOptionalStop(false));
            Err("controller refused the switch".to_string())
        });

        assert_eq!(sent, 1);
        assert!(matches!(
            outcome,
            RunOptionOutcome::Rejected(ref message)
                if message.contains("OPTIONAL STOP NOT CHANGED")
                    && message.contains("controller refused the switch")
        ));
        assert!(state.optional_stop, "the old state must stay displayed");
        assert!(state.run_options_error.as_deref().is_some_and(|error| {
            error.contains("OPTIONAL STOP NOT CHANGED")
                && error.contains("controller refused the switch")
        }));
    }

    #[test]
    fn block_delete_is_refused_while_a_job_is_running_or_paused() {
        let mut state = ready_run_options_state();
        assert!(state.sync_run_options(false, false, true));
        state.job.state = JobState::Paused;

        assert_eq!(
            state.run_option_command(RunOption::BlockDelete),
            Err(RunOptionRefusal::JobActive)
        );
        assert!(state.run_option_command(RunOption::OptionalStop).is_ok());
        assert!(state.run_option_command(RunOption::SingleBlock).is_ok());

        let mut sent = Vec::new();
        let outcome = state.dispatch_run_option(RunOption::BlockDelete, |command| {
            sent.push(command);
            Ok(())
        });
        assert_eq!(
            outcome,
            RunOptionOutcome::Refused(RunOptionRefusal::JobActive)
        );
        assert!(sent.is_empty(), "a refused toggle must send nothing");
        assert!(state.block_delete, "the authoritative value stays visible");

        state.job.state = JobState::Running;
        assert_eq!(
            state.run_option_command(RunOption::BlockDelete),
            Err(RunOptionRefusal::JobActive)
        );
    }

    #[test]
    fn mock_mode_keeps_the_sheet_visible_but_disables_every_toggle() {
        let mut state = AppState::new();
        assert!(!state.live);

        state.open_run_options();
        assert!(state.run_options_open);
        for option in RunOption::ALL {
            assert_eq!(
                state.run_option_refusal(option),
                Some(RunOptionRefusal::Mock)
            );
            assert_eq!(
                state.run_option_command(option),
                Err(RunOptionRefusal::Mock)
            );
            assert_eq!(state.run_option_hint(option), "Mock mode: no controller");
        }
        assert_eq!(
            state.run_options_lock_reason(),
            Some("MOCK MODE: run options need a live controller")
        );
    }

    #[test]
    fn reset_and_disconnect_clear_the_three_run_options() {
        let mut state = ready_run_options_state();
        assert!(state.sync_run_options(true, true, true));
        state.open_run_options();

        // A new session clears the switches and the sheet; the demo/mock
        // defaults are never carried into a controller session.
        state.begin_live("new session");
        assert!(!state.optional_stop);
        assert!(!state.single_block);
        assert!(!state.block_delete);
        assert!(!state.run_options_open);

        // The accessor copy is also what clears the UI after a disconnect.
        state.connection = ConnectionState::Ready;
        assert!(state.sync_run_options(true, true, true));
        assert!(state.sync_run_options(false, false, false));
        assert!(!state.optional_stop);
        assert!(!state.single_block);
        assert!(!state.block_delete);
    }

    #[test]
    fn opening_and_closing_the_run_options_sheet_sends_nothing() {
        let mut state = ready_run_options_state();
        assert!(state.sync_run_options(false, true, false));
        let before = (state.optional_stop, state.single_block, state.block_delete);

        state.open_run_options();
        assert!(state.run_options_open);
        assert_eq!(
            (state.optional_stop, state.single_block, state.block_delete,),
            before
        );

        state.run_options_error = Some("previous rejection".to_string());
        state.close_run_options();
        assert!(!state.run_options_open);
        assert_eq!(
            (state.optional_stop, state.single_block, state.block_delete,),
            before
        );
        assert_eq!(state.run_options_error, None);
    }

    // ------------------------------------------- program breakpoints / run-to

    fn breakpoint_state() -> AppState {
        let mut state = AppState::new();
        state.begin_live("breakpoint test");
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        state
            .job
            .loaded("breakpoints.nc".into(), sample_program(), 4);
        state
    }

    fn recorded_send(sent: &mut Vec<Command>) -> impl FnMut(Command) -> Result<(), String> + '_ {
        move |command| {
            sent.push(command);
            Ok(())
        }
    }

    #[test]
    fn breakpoint_toggle_adds_removes_and_maps_source_lines() {
        let mut state = breakpoint_state();
        assert_eq!(state.job.breakpoints.count(), 0);

        let mut sent = Vec::new();
        let outcome = state.dispatch_toggle_breakpoint(2, recorded_send(&mut sent));
        assert_eq!(
            outcome,
            BreakpointOutcome::Sent {
                line: 2,
                enabled: true
            }
        );
        assert_eq!(
            sent,
            vec![Command::SetBreakpoint {
                line: 2,
                enabled: true
            }]
        );
        assert!(state.job.breakpoints.contains(2));
        assert_eq!(state.job.breakpoints.count(), 1);

        sent.clear();
        let outcome = state.dispatch_toggle_breakpoint(2, recorded_send(&mut sent));
        assert_eq!(
            outcome,
            BreakpointOutcome::Sent {
                line: 2,
                enabled: false
            }
        );
        assert_eq!(
            sent,
            vec![Command::SetBreakpoint {
                line: 2,
                enabled: false
            }]
        );
        assert!(!state.job.breakpoints.contains(2));
        assert_eq!(state.job.breakpoints.count(), 0);
    }

    #[test]
    fn breakpoint_command_rejects_lines_outside_the_streamed_program() {
        let state = breakpoint_state();
        assert_eq!(
            state.breakpoint_command(0),
            Err(BreakpointRefusal::LineOutOfRange)
        );
        assert_eq!(
            state.breakpoint_command(5),
            Err(BreakpointRefusal::LineOutOfRange)
        );
        assert_eq!(
            state.breakpoint_command(4),
            Ok(Command::SetBreakpoint {
                line: 4,
                enabled: true
            })
        );
    }

    #[test]
    fn run_to_requires_confirmation_then_sends_breakpoint_and_start_once() {
        let mut state = breakpoint_state();
        let mut sent = Vec::new();

        state.request_run_to(2).expect("run-to request");
        assert!(sent.is_empty(), "opening the sheet sends nothing");
        assert_eq!(
            state
                .job
                .breakpoints
                .pending_run_to()
                .map(|pending| { (pending.line, pending.source.as_str()) }),
            Some((2, "G0 X10"))
        );
        assert_eq!(state.job.breakpoints.count(), 0);

        let outcome = state.confirm_run_to(recorded_send(&mut sent));
        assert_eq!(outcome, RunToOutcome::Started { line: 2 });
        assert_eq!(
            sent,
            vec![
                Command::SetBreakpoint {
                    line: 2,
                    enabled: true
                },
                Command::StartProgram { from_line: None },
            ]
        );
        assert_eq!(state.job.breakpoints.count(), 1);
        assert!(state.job.breakpoints.contains(2));
        assert!(state.job.breakpoints.pending_run_to().is_none());
    }

    #[test]
    fn cancelling_run_to_sends_nothing_and_leaves_no_breakpoint() {
        let mut state = breakpoint_state();
        let mut sent = Vec::new();
        state.request_run_to(3).expect("run-to request");

        assert!(state.cancel_run_to());
        assert!(sent.is_empty(), "cancel sends no command");
        assert!(state.job.breakpoints.pending_run_to().is_none());
        assert_eq!(state.job.breakpoints.count(), 0);

        let outcome = state.confirm_run_to(recorded_send(&mut sent));
        assert_eq!(outcome, RunToOutcome::Refused(RunToRefusal::NoPending));
        assert!(sent.is_empty());
    }

    #[test]
    fn run_to_while_a_job_is_active_only_sets_the_breakpoint() {
        let mut state = breakpoint_state();
        state.job.started(1, 4);
        let mut sent = Vec::new();

        state
            .request_run_to(3)
            .expect("run-to request while running");
        let outcome = state.confirm_run_to(recorded_send(&mut sent));
        assert_eq!(outcome, RunToOutcome::BreakpointOnly { line: 3 });
        assert_eq!(
            sent,
            vec![Command::SetBreakpoint {
                line: 3,
                enabled: true
            }],
            "an active job must not be started or restarted"
        );
        assert!(state.job.is_running());
        assert!(state.job.breakpoints.contains(3));

        state.job.paused(2, JobPauseReason::Operator);
        sent.clear();
        state
            .request_run_to(2)
            .expect("run-to request while paused");
        let outcome = state.confirm_run_to(recorded_send(&mut sent));
        assert_eq!(outcome, RunToOutcome::BreakpointOnly { line: 2 });
        assert_eq!(
            sent,
            vec![Command::SetBreakpoint {
                line: 2,
                enabled: true
            }]
        );
        assert!(state.job.is_paused());
    }

    #[test]
    fn breakpoint_and_run_to_refuse_mock_not_ready_alarm_and_door() {
        let mut mock = AppState::new();
        mock.job.loaded("mock.nc".into(), sample_program(), 4);
        let mut sent = Vec::new();
        assert_eq!(
            mock.dispatch_toggle_breakpoint(2, recorded_send(&mut sent)),
            BreakpointOutcome::Refused(BreakpointRefusal::Mock)
        );
        assert_eq!(mock.request_run_to(2), Err(RunToRefusal::Mock));
        assert!(sent.is_empty());

        let mut connecting = AppState::new();
        connecting.begin_live("connecting");
        connecting.job.loaded("c.nc".into(), sample_program(), 4);
        assert_eq!(
            connecting.dispatch_toggle_breakpoint(2, recorded_send(&mut sent)),
            BreakpointOutcome::Refused(BreakpointRefusal::NotReady)
        );
        assert_eq!(connecting.request_run_to(2), Err(RunToRefusal::NotReady));

        for machine in ["Alarm", "Door"] {
            let mut locked = breakpoint_state();
            locked.machine_state = machine;
            assert_eq!(
                locked.dispatch_toggle_breakpoint(2, recorded_send(&mut sent)),
                BreakpointOutcome::Refused(BreakpointRefusal::MachineLocked),
                "{machine}"
            );
            assert_eq!(
                locked.request_run_to(2),
                Err(RunToRefusal::MachineLocked),
                "{machine}"
            );
        }
        assert!(sent.is_empty(), "every refusal sends nothing");
    }

    #[test]
    fn earlier_breakpoints_are_listed_for_the_run_to_warning() {
        let mut state = breakpoint_state();
        let mut sent = Vec::new();
        state.dispatch_toggle_breakpoint(2, recorded_send(&mut sent));
        state.dispatch_toggle_breakpoint(4, recorded_send(&mut sent));
        assert_eq!(state.job.breakpoints.earlier_than(2), Vec::<u32>::new());
        assert_eq!(state.job.breakpoints.earlier_than(4), vec![2]);
        assert_eq!(state.job.breakpoints.earlier_than(5), vec![2, 4]);
    }

    #[test]
    fn a_new_program_clears_breakpoints_selection_and_run_to() {
        let mut state = breakpoint_state();
        let mut sent = Vec::new();
        state.dispatch_toggle_breakpoint(2, recorded_send(&mut sent));
        state.select_program_line(Some(3));
        state.request_run_to(3).expect("pending run-to");
        assert_eq!(state.job.breakpoints.count(), 1);
        assert_eq!(state.job.breakpoints.selected(), Some(3));
        assert!(state.job.breakpoints.pending_run_to().is_some());

        let next = Arc::new(
            ghs_gcode::parse_program("G21\nG1 X1 F800\nM30\n").expect("next program parses"),
        );
        state.job.loaded("next.nc".into(), next, 3);

        assert_eq!(state.job.breakpoints.count(), 0);
        assert_eq!(state.job.breakpoints.selected(), None);
        assert!(state.job.breakpoints.pending_run_to().is_none());
        assert_eq!(state.job.breakpoints.error, None);
    }

    #[test]
    fn a_breakpoint_pause_shows_the_line_and_keeps_resume_available() {
        let mut state = breakpoint_state();
        let mut sent = Vec::new();
        state.dispatch_toggle_breakpoint(3, recorded_send(&mut sent));
        state.job.started(1, 4);

        state.job.paused(3, JobPauseReason::Breakpoint);
        assert_eq!(
            state.pause_detail().as_deref(),
            Some("BREAKPOINT AT LINE 3")
        );
        assert_eq!(state.job.current_line, 3);
        assert!(state.job.is_paused());
        assert!(!state.job.tool_change_pending());
        assert_eq!(state.job.pause_reason, Some(JobPauseReason::Breakpoint));
        // The shell's RESUME gate requires a ready, unlocked, paused job with
        // no pending tool change and no watchdog pause; a breakpoint pause
        // satisfies all of them.
        assert!(state.live);
        assert!(state.connection.is_ready_like());
        assert!(!matches!(state.machine_state, "Alarm" | "Door"));
        assert_ne!(state.job.pause_reason, Some(JobPauseReason::Watchdog));

        // Clearing the app-side marker must not clear the pause itself.
        sent.clear();
        assert_eq!(
            state.dispatch_toggle_breakpoint(3, recorded_send(&mut sent)),
            BreakpointOutcome::Sent {
                line: 3,
                enabled: false
            }
        );
        assert!(state.job.is_paused());
        assert_eq!(state.job.pause_reason, Some(JobPauseReason::Breakpoint));

        // Re-arming the pause line is refused; after resume it is allowed, so
        // RESUME cannot be trapped in a pause loop by a stray toggle tap.
        assert_eq!(
            state.breakpoint_command(3),
            Err(BreakpointRefusal::PausedAtLine)
        );
        assert_eq!(
            state.run_to_refusal(Some(3)),
            Some(RunToRefusal::PausedAtLine)
        );
        state.job.resumed();
        assert_eq!(state.pause_detail(), None);
        assert_eq!(
            state.breakpoint_command(3),
            Ok(Command::SetBreakpoint {
                line: 3,
                enabled: true
            })
        );
    }

    fn check_state() -> AppState {
        let mut state = AppState::new();
        state.begin_live("check test");
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        state.job.loaded("check.nc".into(), sample_program(), 4);
        state
    }

    #[test]
    fn check_requires_confirmation_then_sends_start_check_run_once() {
        let mut state = check_state();
        let mut sent = Vec::new();

        state.request_check().expect("check request");
        assert!(state.check_confirm_open);
        assert!(sent.is_empty(), "opening the sheet sends nothing");

        let outcome = state.confirm_check(recorded_send(&mut sent));
        assert_eq!(outcome, CheckOutcome::Started);
        assert_eq!(sent, vec![Command::StartCheckRun]);
        assert!(!state.check_confirm_open, "the sheet is consumed");
        assert!(state.job.check_pending());

        let outcome = state.confirm_check(recorded_send(&mut sent));
        assert_eq!(outcome, CheckOutcome::NotConfirmed);
        assert_eq!(sent, vec![Command::StartCheckRun], "only one command");
    }

    #[test]
    fn cancelling_check_confirmation_sends_nothing() {
        let mut state = check_state();
        let mut sent = Vec::new();
        state.request_check().expect("check request");

        assert!(state.cancel_check_confirm());
        assert!(!state.check_confirm_open);
        assert!(sent.is_empty(), "cancel sends no command");

        let outcome = state.confirm_check(recorded_send(&mut sent));
        assert_eq!(outcome, CheckOutcome::NotConfirmed);
        assert!(sent.is_empty());
    }

    #[test]
    fn check_is_refused_in_mock_not_ready_locked_and_active_states() {
        let mut mock = AppState::new();
        assert_eq!(mock.check_refusal(), Some(CheckRefusal::Mock));
        assert_eq!(
            mock.request_check(),
            Err(CheckRefusal::Mock),
            "no sheet opens in mock mode"
        );

        let mut not_ready = check_state();
        not_ready.connection = ConnectionState::Connecting;
        assert_eq!(not_ready.check_refusal(), Some(CheckRefusal::NotReady));

        let mut alarm = check_state();
        alarm.machine_state = "Alarm";
        assert_eq!(
            alarm.check_refusal(),
            Some(CheckRefusal::MachineLocked),
            "Alarm must refuse a dry run"
        );

        let mut door = check_state();
        door.machine_state = "Door";
        assert_eq!(
            door.check_refusal(),
            Some(CheckRefusal::MachineLocked),
            "Door must refuse a dry run"
        );

        let mut active = check_state();
        active.job.started(1, 4);
        assert_eq!(active.check_refusal(), Some(CheckRefusal::JobActive));

        let mut hold = check_state();
        hold.machine_state = "Hold";
        assert_eq!(hold.check_refusal(), Some(CheckRefusal::MachineNotIdle));
    }

    #[test]
    fn check_started_shows_checking_and_check_completed_keeps_the_program() {
        let mut state = check_state();
        let mut sent = Vec::new();
        state.request_check().expect("check request");
        assert_eq!(
            state.confirm_check(recorded_send(&mut sent)),
            CheckOutcome::Started
        );

        assert!(
            state
                .job
                .apply_event(&EngineEvent::CheckStarted { total_lines: 4 })
        );
        assert_eq!(state.job.status_label(), "CHECKING");
        assert!(state.job.is_check());
        assert!(!state.job.check_pending());

        assert!(state.job.apply_event(&EngineEvent::JobStarted {
            from_line: 1,
            total_lines: 4,
        }));
        assert_eq!(state.job.status_label(), "CHECKING");

        assert!(state.job.apply_event(&EngineEvent::JobProgress {
            current_line: 2,
            completed: 1,
            total: 4,
            elapsed: Duration::from_secs(3),
        }));
        assert_eq!(state.job.current_line, 2);

        assert!(state.job.apply_event(&EngineEvent::JobCompleted {
            elapsed: Duration::from_secs(5),
        }));
        assert_eq!(state.job.status_label(), "CHECK COMPLETE");
        assert!(state.job.apply_event(&EngineEvent::CheckCompleted {
            elapsed: Duration::from_secs(7),
        }));
        assert_eq!(state.job.status_label(), "CHECK COMPLETE");
        assert_eq!(state.job.elapsed, Duration::from_secs(7));
        assert!(state.job.has_program());
        assert_eq!(state.job.name, "check.nc");
    }

    #[test]
    fn check_pending_blocks_a_second_check_and_a_normal_start() {
        let mut state = check_state();
        let mut sent = Vec::new();
        state.request_check().expect("check request");
        assert_eq!(
            state.confirm_check(recorded_send(&mut sent)),
            CheckOutcome::Started
        );
        assert!(state.job.check_pending());

        assert_eq!(state.check_refusal(), Some(CheckRefusal::CheckActive));
        assert_eq!(state.request_check(), Err(CheckRefusal::CheckActive));

        sent.clear();
        assert_eq!(
            state.dispatch_start(recorded_send(&mut sent)),
            StartOutcome::Refused(CheckRefusal::CheckActive)
        );
        assert!(sent.is_empty(), "a pending check blocks StartProgram");
    }

    #[test]
    fn normal_start_sends_start_program_and_clears_the_check_label() {
        let mut state = check_state();
        state.job.sync_check_run(true);
        state.job.check_started(4);
        state.job.started(1, 4);
        state.job.completed(Duration::from_secs(5));
        assert_eq!(state.job.status_label(), "CHECK COMPLETE");

        let mut sent = Vec::new();
        assert_eq!(
            state.dispatch_start(recorded_send(&mut sent)),
            StartOutcome::Sent
        );
        assert_eq!(sent, vec![Command::StartProgram { from_line: None }]);
        assert!(!state.job.is_check());

        state.job.apply_event(&EngineEvent::JobStarted {
            from_line: 1,
            total_lines: 4,
        });
        assert_eq!(state.job.status_label(), "RUNNING");
    }

    #[test]
    fn stop_during_check_uses_stop_program_without_reset() {
        let mut check = check_state();
        check.job.check_started(4);
        check.job.started(1, 4);
        assert_eq!(
            check.stop_program_command(),
            Command::StopProgram { reset: false },
            "a dry run has no motion to discard"
        );

        let mut job = check_state();
        job.job.started(1, 4);
        assert_eq!(
            job.stop_program_command(),
            Command::StopProgram { reset: true },
            "a real job keeps the guarded reset"
        );
    }

    #[test]
    fn check_run_accessor_updates_after_polls_and_a_new_load() {
        let mut state = check_state();
        assert!(!state.job.is_check());
        assert!(!state.job.sync_check_run(false));

        assert!(state.job.sync_check_run(true));
        assert!(state.job.is_check(), "the accessor recovers a missed event");

        state.job.begin_check();
        assert!(state.job.check_pending());
        assert!(state.job.sync_check_run(true));
        assert!(
            !state.job.check_pending(),
            "the accessor clears the entry wait"
        );

        assert!(state.job.sync_check_run(false));
        assert!(!state.job.is_check(), "a normal start clears the label");

        assert!(state.job.sync_check_run(true));
        state.job.loaded("next.nc".into(), sample_program(), 4);
        assert!(!state.job.is_check(), "a new program is not a dry run");
    }

    #[test]
    fn job_started_without_a_check_event_clears_a_stale_check_label() {
        let mut job = JobView::default();
        job.loaded("check.nc".into(), sample_program(), 4);
        job.sync_check_run(true);
        job.check_completed(Duration::from_secs(5));
        assert!(job.is_check());

        // A run-to or probe start does not go through `dispatch_start`; the
        // `JobStarted` event alone must clear the previous dry-run label.
        job.started(1, 4);
        assert!(!job.is_check());
        assert_eq!(job.status_label(), "RUNNING");
    }

    #[test]
    fn check_enter_error_clears_pending_and_leaves_the_program_loaded() {
        let mut state = check_state();
        state.request_check().expect("check request");
        let mut sent = Vec::new();
        assert_eq!(
            state.confirm_check(recorded_send(&mut sent)),
            CheckOutcome::Started
        );

        assert!(state.job.apply_event(&EngineEvent::JobError {
            line: 0,
            code: 0,
            message: "controller did not enter check mode ($C)".into(),
        }));
        assert!(!state.job.check_pending());
        assert!(state.job.has_program());
        assert!(!state.job.is_check());
    }

    fn travel_start_state(over_travel: bool) -> AppState {
        let mut state = AppState::new();
        state.begin_live("travel start test");
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        state.settings.apply_settings(&[
            ghs_engine::Setting::new(20, SettingValue::Bool(true)),
            ghs_engine::Setting::new(130, SettingValue::Int(100)),
            ghs_engine::Setting::new(131, SettingValue::Int(100)),
            ghs_engine::Setting::new(132, SettingValue::Int(100)),
        ]);
        let mut offsets = Offsets::default();
        offsets.wcs[Wcs::G54.index()] = Some(Position::zero());
        state.apply_offsets(&offsets);
        let source = if over_travel {
            "G21\nG0 X150 Y0 Z0\nM30\n"
        } else {
            "G21\nG0 X50 Y0 Z0\nM30\n"
        };
        let program =
            Arc::new(ghs_gcode::parse_program(source).expect("travel test program parses"));
        state.job.loaded("travel.nc".into(), program, 3);
        state
    }

    #[test]
    fn within_travel_start_sends_start_program_directly() {
        let mut state = travel_start_state(false);
        assert!(matches!(
            state.program_limits().status,
            LimitsStatus::WithinTravel
        ));
        let mut sent = Vec::new();

        assert_eq!(
            state.dispatch_start(recorded_send(&mut sent)),
            StartOutcome::Sent
        );
        assert_eq!(sent, vec![Command::StartProgram { from_line: None }]);
        assert!(!state.start_confirm_open());
    }

    #[test]
    fn not_checked_start_sends_start_program_directly() {
        let mut state = AppState::new();
        state.begin_live("not checked start test");
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        state.job.loaded("unknown.nc".into(), sample_program(), 4);
        assert!(matches!(
            state.program_limits().status,
            LimitsStatus::Unchecked(_)
        ));
        let mut sent = Vec::new();

        assert_eq!(
            state.dispatch_start(recorded_send(&mut sent)),
            StartOutcome::Sent
        );
        assert_eq!(sent, vec![Command::StartProgram { from_line: None }]);
        assert!(!state.start_confirm_open());

        // An explicit soft-limits-off report is also NOT CHECKED, not an
        // overrun, so it keeps the direct-start behavior.
        let mut soft_off = travel_start_state(true);
        soft_off
            .settings
            .apply_settings(&[ghs_engine::Setting::new(20, SettingValue::Bool(false))]);
        assert!(matches!(
            soft_off.program_limits().status,
            LimitsStatus::SoftLimitsOff
        ));
        sent.clear();
        assert_eq!(
            soft_off.dispatch_start(recorded_send(&mut sent)),
            StartOutcome::Sent
        );
        assert_eq!(sent, vec![Command::StartProgram { from_line: None }]);
    }

    #[test]
    fn over_travel_start_opens_confirmation_and_sends_nothing() {
        let mut state = travel_start_state(true);
        let mut sent = Vec::new();

        assert_eq!(
            state.dispatch_start(recorded_send(&mut sent)),
            StartOutcome::ConfirmationRequired
        );
        assert!(sent.is_empty(), "opening the confirmation sends nothing");
        assert!(state.start_confirm_open());
        assert_eq!(
            state.dispatch_start(recorded_send(&mut sent)),
            StartOutcome::ConfirmationRequired,
            "pressing START again keeps the sheet open"
        );
        assert!(sent.is_empty(), "a second START still sends nothing");
        let limits = state.pending_start_limits().expect("pending limits");
        assert_eq!(
            limits.overruns(),
            vec!["X MAX 150.000 > $130 100.000".to_string()]
        );
        assert_eq!(
            limits.bounds_text(),
            "X 0.000 .. 150.000 | Y 0.000 .. 0.000 | Z 0.000 .. 0.000"
        );
        assert_eq!(
            limits.travel_text(),
            "X $130 100.000 | Y $131 100.000 | Z $132 100.000"
        );
    }

    #[test]
    fn confirming_over_travel_start_sends_exactly_one_start_program() {
        let mut state = travel_start_state(true);
        let mut sent = Vec::new();
        assert_eq!(
            state.dispatch_start(recorded_send(&mut sent)),
            StartOutcome::ConfirmationRequired
        );

        assert_eq!(
            state.confirm_start(recorded_send(&mut sent)),
            StartOutcome::Sent
        );
        assert_eq!(sent, vec![Command::StartProgram { from_line: None }]);
        assert!(!state.start_confirm_open(), "the sheet is consumed");

        assert_eq!(
            state.confirm_start(recorded_send(&mut sent)),
            StartOutcome::NotConfirmed
        );
        assert_eq!(sent.len(), 1, "only one StartProgram was sent");
    }

    #[test]
    fn cancelling_over_travel_start_sends_nothing() {
        let mut state = travel_start_state(true);
        let mut sent = Vec::new();
        assert_eq!(
            state.dispatch_start(recorded_send(&mut sent)),
            StartOutcome::ConfirmationRequired
        );
        assert!(sent.is_empty());

        assert!(state.cancel_start_confirm());
        assert!(!state.start_confirm_open());
        assert!(sent.is_empty(), "cancel sends nothing");
        assert_eq!(
            state.confirm_start(recorded_send(&mut sent)),
            StartOutcome::NotConfirmed
        );
        assert!(sent.is_empty(), "a cancelled sheet cannot start later");
    }

    #[test]
    fn program_change_clears_pending_over_travel_start() {
        let mut state = travel_start_state(true);
        let mut sent = Vec::new();
        assert_eq!(
            state.dispatch_start(recorded_send(&mut sent)),
            StartOutcome::ConfirmationRequired
        );

        let replacement = Arc::new(
            ghs_gcode::parse_program("G21\nG0 X10\nM30\n").expect("replacement program parses"),
        );
        state.job.loaded("replacement.nc".into(), replacement, 3);
        assert!(state.sync_start_confirm(), "a new program clears the sheet");
        assert!(!state.start_confirm_open());
        assert_eq!(
            state.confirm_start(recorded_send(&mut sent)),
            StartOutcome::NotConfirmed
        );
        assert!(sent.is_empty(), "the replaced program never starts");
    }

    #[test]
    fn disconnect_clears_pending_over_travel_start() {
        let mut state = travel_start_state(true);
        let mut sent = Vec::new();
        assert_eq!(
            state.dispatch_start(recorded_send(&mut sent)),
            StartOutcome::ConfirmationRequired
        );

        state.connection = ConnectionState::Disconnected;
        assert!(
            state.sync_start_confirm(),
            "a dropped link clears the sheet"
        );
        assert!(!state.start_confirm_open());
        assert_eq!(
            state.confirm_start(recorded_send(&mut sent)),
            StartOutcome::NotConfirmed
        );
        assert!(sent.is_empty(), "the stale start never fires");
    }

    #[test]
    fn machine_leaving_idle_clears_pending_over_travel_start() {
        let mut state = travel_start_state(true);
        let mut sent = Vec::new();
        assert_eq!(
            state.dispatch_start(recorded_send(&mut sent)),
            StartOutcome::ConfirmationRequired
        );

        state.machine_state = "Run";
        assert!(state.sync_start_confirm(), "leaving Idle clears the sheet");
        assert!(!state.start_confirm_open());
        assert_eq!(
            state.confirm_start(recorded_send(&mut sent)),
            StartOutcome::NotConfirmed
        );
        assert!(sent.is_empty(), "a non-Idle machine never starts");

        // Confirmation also re-checks the machine without waiting for a poll.
        assert_eq!(
            state.dispatch_start(recorded_send(&mut sent)),
            StartOutcome::ConfirmationRequired,
            "a fresh sheet opens only after the current limits report"
        );
        state.machine_state = "Run";
        assert_eq!(
            state.confirm_start(recorded_send(&mut sent)),
            StartOutcome::Stale
        );
        assert!(sent.is_empty(), "a stale confirmation sends nothing");
    }

    #[test]
    fn run_to_is_gated_by_reported_travel_while_check_is_not() {
        // A check run has its own sheet and command.
        let mut check = travel_start_state(true);
        let mut sent = Vec::new();
        check.request_check().expect("check request");
        assert_eq!(
            check.confirm_check(recorded_send(&mut sent)),
            CheckOutcome::Started
        );
        assert_eq!(sent, vec![Command::StartCheckRun]);
        assert!(!check.start_confirm_open());

        // Run-to starts the job, so it uses the same over-travel gate as
        // START: the breakpoint is set, then the start is held.
        let mut run_to = travel_start_state(true);
        sent.clear();
        run_to.request_run_to(2).expect("run-to request");
        assert_eq!(
            run_to.confirm_run_to(recorded_send(&mut sent)),
            RunToOutcome::StartHeld { line: 2 }
        );
        assert_eq!(
            sent,
            vec![Command::SetBreakpoint {
                line: 2,
                enabled: true,
            }],
            "the held start must not send StartProgram"
        );
        assert!(run_to.start_confirm_open());
        assert_eq!(run_to.pending_start_run_to_line(), Some(2));
        assert!(run_to.job.breakpoints.contains(2));

        // Confirming the held start sends exactly one normal StartProgram;
        // the breakpoint is not sent twice.
        assert_eq!(
            run_to.confirm_start(recorded_send(&mut sent)),
            StartOutcome::Sent
        );
        assert_eq!(
            sent,
            vec![
                Command::SetBreakpoint {
                    line: 2,
                    enabled: true,
                },
                Command::StartProgram { from_line: None },
            ]
        );
        assert!(!run_to.start_confirm_open());
        assert!(run_to.job.breakpoints.contains(2));
    }

    #[test]
    fn cancelling_a_held_run_to_start_sends_nothing_and_keeps_the_breakpoint() {
        let mut state = travel_start_state(true);
        let mut sent = Vec::new();
        state.request_run_to(2).expect("run-to request");
        assert_eq!(
            state.confirm_run_to(recorded_send(&mut sent)),
            RunToOutcome::StartHeld { line: 2 }
        );
        sent.clear();

        assert!(state.cancel_start_confirm());
        assert!(sent.is_empty(), "cancel sends no command");
        assert!(!state.start_confirm_open());
        assert!(state.job.breakpoints.contains(2));
        assert_eq!(
            state.confirm_start(recorded_send(&mut sent)),
            StartOutcome::NotConfirmed
        );
        assert!(sent.is_empty());
    }

    #[test]
    fn held_run_to_start_is_stale_after_the_program_changes() {
        let mut state = travel_start_state(true);
        let mut sent = Vec::new();
        state.request_run_to(2).expect("run-to request");
        assert_eq!(
            state.confirm_run_to(recorded_send(&mut sent)),
            RunToOutcome::StartHeld { line: 2 }
        );
        sent.clear();

        let other = Arc::new(ghs_gcode::parse_program("G21\nG0 X10 Y0 Z0\nM30\n").expect("parses"));
        state.job.loaded("other.nc".into(), other, 3);
        assert_eq!(
            state.confirm_start(recorded_send(&mut sent)),
            StartOutcome::Stale
        );
        assert!(sent.is_empty(), "a stale held start must not send");
    }

    // ------------------------------------------------------- setup wizard

    fn setup_state(label: &str) -> (AppState, TempRoot, ConfigStore) {
        let dir = TempRoot::new(label);
        let store = ConfigStore::new(&dir.path);
        let mut state = AppState::new();
        state.config = ConfigModel::from_store(store.clone());
        state.setup = crate::setup::SetupModel::new(state.config.app().units);
        (state, dir, store)
    }

    fn acknowledged_setup_state(label: &str) -> (AppState, TempRoot, ConfigStore) {
        let (mut state, dir, store) = setup_state(label);
        state.open_setup();
        for item in SafetyItem::ALL {
            state.setup_toggle_safety(item);
        }
        (state, dir, store)
    }

    #[test]
    fn first_run_setup_opens_once_only_when_incomplete() {
        let (mut state, _dir, store) = setup_state("setup-first-run");
        assert!(!state.config.setup_complete());
        assert!(state.auto_open_setup(false));
        assert!(state.setup.is_open());
        assert_eq!(state.setup.step(), SetupStep::Safety);
        assert_eq!(state.setup.acknowledged_count(), 0);

        state.remind_later_setup();
        assert!(!state.setup.is_open());
        assert!(
            !state.auto_open_setup(false),
            "REMIND ME LATER must not re-open the wizard in the same run"
        );

        let mut complete = AppState::new();
        complete.config = ConfigModel::from_store(store);
        complete
            .config
            .set_setup_complete(true)
            .expect("write completion flag");
        complete.setup = crate::setup::SetupModel::new(CoreUnits::Mm);
        assert!(
            !complete.auto_open_setup(false),
            "a completed setup must not open on startup"
        );
        assert!(!complete.setup.is_open());
    }

    #[test]
    fn a_demo_initial_screen_suppresses_the_first_run_setup() {
        let (mut state, _dir, _store) = setup_state("setup-demo-screen");
        assert!(!state.auto_open_setup(true));
        assert!(!state.setup.is_open());
        assert!(
            !state.auto_open_setup(false),
            "the startup prompt was already considered"
        );
    }

    #[test]
    fn safety_gate_blocks_next_until_every_item_is_checked_and_unchecking_blocks_again() {
        let (mut state, _dir, _store) = setup_state("setup-gate");
        state.open_setup();
        assert_eq!(state.setup.step(), SetupStep::Safety);

        for item in SafetyItem::ALL {
            assert!(
                !state.setup_next(),
                "advanced with {} unchecked",
                item.title()
            );
            state.setup_toggle_safety(item);
        }
        assert!(state.setup_next());
        assert_eq!(state.setup.step(), SetupStep::Units);

        assert!(state.setup_back());
        state.setup_toggle_safety(SafetyItem::Workholding);
        assert!(!state.setup.can_advance());
        assert!(!state.setup_next());
        assert_eq!(state.setup.step(), SetupStep::Safety);
    }

    #[test]
    fn back_navigation_preserves_checked_items_and_units() {
        let (mut state, _dir, _store) = acknowledged_setup_state("setup-back");
        assert!(state.setup_next());
        state.setup_set_units(CoreUnits::Inch);
        assert!(state.setup_next());
        assert_eq!(state.setup.step(), SetupStep::Connection);

        assert!(state.setup_back());
        assert_eq!(state.setup.step(), SetupStep::Units);
        assert_eq!(state.setup.units(), CoreUnits::Inch);
        assert!(state.setup_back());
        assert_eq!(state.setup.step(), SetupStep::Safety);
        assert!(state.setup.all_safety_checked());
        assert_eq!(state.setup.acknowledged_count(), SafetyItem::ALL.len());
        assert_eq!(state.setup.units(), CoreUnits::Inch);
    }

    #[test]
    fn finish_setup_saves_units_and_closes() {
        let (mut state, _dir, store) = acknowledged_setup_state("setup-units");
        assert!(state.setup_next());
        state.setup_set_units(CoreUnits::Inch);
        assert!(state.setup_next());
        assert!(state.setup_next());
        assert_eq!(state.setup.step(), SetupStep::Review);

        state.finish_setup().expect("finish setup");
        assert!(!state.setup.is_open());

        let reopened = ConfigModel::from_store(store);
        assert_eq!(reopened.app().units, CoreUnits::Inch);
    }

    #[test]
    fn finish_setup_sets_the_flag_through_the_config_model() {
        let (mut state, _dir, store) = acknowledged_setup_state("setup-flag");
        assert!(state.setup_next());
        assert!(state.setup_next());
        assert!(state.setup_next());
        assert_eq!(state.setup.step(), SetupStep::Review);

        state.finish_setup().expect("finish setup");
        assert!(state.config.setup_complete());
        assert!(ConfigModel::from_store(store).setup_complete());
    }

    #[test]
    fn save_failure_keeps_the_wizard_open_and_the_flag_false() {
        let dir = TempRoot::new("setup-save-failure");
        let store = ConfigStore::new(&dir.path);
        let path = store.app_config_path();
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let corrupt = b"{ not json";
        fs::write(&path, corrupt).expect("write corrupt app config");

        let mut state = AppState::new();
        state.config = ConfigModel::from_store(store);
        state.setup = crate::setup::SetupModel::new(CoreUnits::Mm);
        state.open_setup();
        for item in SafetyItem::ALL {
            state.setup_toggle_safety(item);
        }
        assert!(state.setup_next());
        assert!(state.setup_next());
        assert!(state.setup_next());

        let error = state.finish_setup().expect_err("save must be refused");
        assert!(error.contains("corrupt"), "{error}");
        assert!(state.setup.is_open(), "the wizard stays open");
        assert_eq!(state.setup.step(), SetupStep::Review);
        assert!(state.setup.error().is_some());
        assert!(!state.config.setup_complete());
        assert_eq!(fs::read(&path).expect("read"), corrupt);
    }

    #[test]
    fn remind_me_later_closes_without_setting_the_flag() {
        let (mut state, _dir, store) = acknowledged_setup_state("setup-later");
        assert!(state.setup_next());
        assert!(state.setup_next());
        assert!(state.setup_next());

        state.remind_later_setup();
        assert!(!state.setup.is_open());
        assert!(!state.config.setup_complete());
        assert!(!store.app_config_path().exists());
    }

    #[test]
    fn utility_reopen_starts_at_safety_and_completing_again_saves_the_flag() {
        let (mut state, _dir, store) = setup_state("setup-reopen");
        state
            .config
            .set_setup_complete(true)
            .expect("pre-complete setup");
        state.open_setup();
        assert!(state.config.setup_complete());
        assert_eq!(state.setup.step(), SetupStep::Safety);
        assert_eq!(state.setup.acknowledged_count(), 0);

        for item in SafetyItem::ALL {
            state.setup_toggle_safety(item);
        }
        assert!(state.setup_next());
        assert!(state.setup_next());
        assert!(state.setup_next());
        state.finish_setup().expect("finish again");
        assert!(state.config.setup_complete());
        assert!(ConfigModel::from_store(store).setup_complete());
    }

    #[test]
    fn no_wizard_path_emits_a_controller_command() {
        let (mut state, _dir, _store) = setup_state("setup-no-command");
        let mut sent = Vec::new();
        let recorder = recorded_send(&mut sent);

        state.open_setup();
        for item in SafetyItem::ALL {
            state.setup_toggle_safety(item);
        }
        assert!(state.setup_next());
        state.setup_set_units(CoreUnits::Inch);
        assert!(state.setup_next());
        assert!(state.setup_next());
        state.finish_setup().expect("finish setup");
        state.remind_later_setup();
        drop(recorder);

        assert!(
            sent.is_empty(),
            "the wizard never builds or dispatches a controller command"
        );
    }

    #[test]
    fn demo_setup_pages_are_reproducible_and_do_not_complete_setup() {
        let (mut state, _dir, _store) = setup_state("setup-demo");
        state.open_setup_units_for_demo();
        assert!(state.setup.is_open());
        assert_eq!(state.setup.step(), SetupStep::Units);
        assert!(state.setup.all_safety_checked());

        state.open_setup_review_for_demo();
        assert!(state.setup.is_open());
        assert_eq!(state.setup.step(), SetupStep::Review);
        assert_eq!(state.setup.acknowledged_count(), SafetyItem::ALL.len());
        assert!(!state.config.setup_complete());

        state.open_setup();
        assert_eq!(state.setup.step(), SetupStep::Safety);
        assert_eq!(state.setup.acknowledged_count(), 0);
    }

    /// A live, ready, Idle state with one completed tool-length probe.
    ///
    /// Reference -10, contact -8 gives delta +2; `current_tlo_mm` selects the
    /// proposed target (`current + 2`).
    fn completed_tool_length_state(label: &str, current_tlo_mm: f64) -> AppState {
        let (mut state, _dir, _info) = state_with_profile(label);
        state.connection = ConnectionState::Ready;
        state.machine_state = "Idle";
        let ctx = ProbeContext {
            live: true,
            ready: true,
            machine_state: MachineState::Idle,
            job_active: false,
            machine_pos_mm: Position::xyz(0.0, 0.0, 0.0),
            work_pos_mm: Position::zero(),
            wcs: Wcs::G54,
            wcs_value_mm: Position::zero(),
            g92_mm: Position::zero(),
            tlo_z_mm: current_tlo_mm,
            spindle_on: false,
            coolant_on: false,
            has_probe: true,
            has_probe_connected_signal: false,
            probe_disconnected: false,
            probe_triggered: false,
            offsets_reported: true,
            units: Units::Millimetre,
        };
        state
            .probe
            .select_card(crate::probe::ProbeCard::ToolLength, &ctx);
        state.probe.probe_connected_confirmed = true;
        state
            .probe
            .set_field_text(ProbeField::ReferenceZ, "-10.000")
            .expect("reference Z");
        let setup = state
            .probe
            .preflight(&ctx)
            .setup
            .expect("tool-length preflight");
        let name = setup.plan.name.clone();
        state.probe.begin(setup);
        assert!(state.probe.on_job_loaded(&name));
        assert!(state.probe.on_job_started());
        assert!(state.probe.bind_result(ghs_engine::ProbeResult::new(
            true,
            Position::xyz(0.0, 0.0, -8.0)
        )));
        assert!(state.probe.on_job_completed());
        state
    }

    #[test]
    fn probe_tlo_confirm_sends_exactly_one_mdi_line_and_one_offsets_request() {
        let mut state = completed_tool_length_state("probe-tlo-send", 4.0);
        state
            .request_probe_tool_length(ToolLengthAction::Apply)
            .expect("open confirmation");
        let mut sent = Vec::new();
        let write = state
            .confirm_probe_tool_length(recorded_send(&mut sent))
            .expect("confirmed TLO write");
        assert_eq!(
            sent,
            vec![
                Command::Mdi("G43.1 Z6.000".to_string()),
                Command::System(ghs_engine::SystemCommand::Offsets),
            ]
        );
        assert_eq!(write.line, "G43.1 Z6.000");
        assert_eq!(write.target_tlo_mm, 6.0);
        assert!(state.probe.tool_length_write_in_flight());
    }

    #[test]
    fn probe_tlo_matching_read_back_marks_applied_from_live_offsets() {
        let mut state = completed_tool_length_state("probe-tlo-applied", 4.0);
        state
            .request_probe_tool_length(ToolLengthAction::Apply)
            .expect("open confirmation");
        let mut sent = Vec::new();
        state
            .confirm_probe_tool_length(recorded_send(&mut sent))
            .expect("confirmed TLO write");

        let offsets = Offsets {
            tlo: Some(vec![0.0, 0.0, 6.0]),
            ..Offsets::default()
        };
        state.apply_offsets(&offsets);

        assert_eq!(
            state.probe.tool_length_write().map(|write| &write.status),
            Some(&crate::probe::ToolLengthWriteStatus::Applied {
                reported_tlo_mm: 6.0
            })
        );
        assert!(!state.probe.tool_length_write_in_flight());
        assert_eq!(
            state
                .probe
                .tool_length_preview()
                .expect("preview")
                .current_tlo_mm,
            4.0
        );
    }

    #[test]
    fn probe_tlo_mismatched_read_back_fails_and_keeps_the_old_value() {
        let mut state = completed_tool_length_state("probe-tlo-mismatch", 4.0);
        state
            .request_probe_tool_length(ToolLengthAction::Apply)
            .expect("open confirmation");
        let mut sent = Vec::new();
        state
            .confirm_probe_tool_length(recorded_send(&mut sent))
            .expect("confirmed TLO write");

        let offsets = Offsets {
            tlo: Some(vec![0.0, 0.0, 6.4]),
            ..Offsets::default()
        };
        state.apply_offsets(&offsets);

        let write = state.probe.tool_length_write().expect("write");
        assert!(
            matches!(
                &write.status,
                crate::probe::ToolLengthWriteStatus::Failed { message }
                    if message.contains("6.400") && message.contains("6.000")
            ),
            "{:?}",
            write.status
        );
        assert_eq!(
            state
                .probe
                .tool_length_preview()
                .expect("preview")
                .current_tlo_mm,
            4.0,
            "the old TLO stays displayed"
        );
    }

    #[test]
    fn probe_tlo_gates_refuse_without_sending_a_command() {
        let cases = [
            "live controller",
            "not ready",
            "Alarm",
            "Door",
            "job is active",
            "Idle",
            "$#",
        ];
        for (index, needle) in cases.into_iter().enumerate() {
            let mut state = completed_tool_length_state(&format!("probe-tlo-gate-{index}"), 4.0);
            match index {
                0 => {
                    state.live = false;
                    state.connection = ConnectionState::Mock;
                }
                1 => state.connection = ConnectionState::Connecting,
                2 => state.machine_state = "Alarm",
                3 => state.machine_state = "Door",
                4 => state.job.started(1, 4),
                5 => state.machine_state = "Hold",
                6 => state.offsets.command_sent("G10 L20 P1 X0.000"),
                _ => unreachable!(),
            }
            // Open a confirmation through the model so the refusal below is the
            // gate, not a missing confirmation.
            state
                .probe
                .request_tool_length_action(ToolLengthAction::Apply)
                .expect("model confirmation");
            let mut sent = Vec::new();

            let error = state
                .request_probe_tool_length(ToolLengthAction::Clear)
                .expect_err("request gate closed");
            assert!(error.contains(needle), "request: {error}");
            let error = state
                .confirm_probe_tool_length(recorded_send(&mut sent))
                .expect_err("confirm gate closed");
            assert!(error.contains(needle), "confirm: {error}");
            assert!(sent.is_empty(), "a refused gate sends nothing");
        }
    }

    #[test]
    fn probe_tlo_path_has_no_motion_start_load_g10_or_tlr_command() {
        for action in [ToolLengthAction::Apply, ToolLengthAction::Clear] {
            let mut state = completed_tool_length_state("probe-tlo-commands", 4.0);
            state
                .request_probe_tool_length(action)
                .expect("open confirmation");
            let mut sent = Vec::new();
            state
                .confirm_probe_tool_length(recorded_send(&mut sent))
                .expect("confirmed TLO write");
            assert_eq!(sent.len(), 2, "{sent:?}");
            for command in &sent {
                match command {
                    Command::Mdi(line) => {
                        assert!(
                            line == "G43.1 Z6.000" || line == "G49",
                            "unexpected MDI line {line}"
                        );
                        for forbidden in [
                            "G0", "G1 ", "G10", "G28", "G30", "G53", "G92", "M3", "M4", "M5", "M8",
                            "M9", "$TLR",
                        ] {
                            assert!(!line.contains(forbidden), "forbidden {forbidden} in {line}");
                        }
                    }
                    Command::System(ghs_engine::SystemCommand::Offsets) => {}
                    other => panic!("unexpected command {other:?}"),
                }
            }
            assert!(!sent.iter().any(|command| matches!(
                command,
                Command::LoadProgram { .. }
                    | Command::StartProgram { .. }
                    | Command::ResumeProgram
                    | Command::Jog(_)
                    | Command::Home
                    | Command::SetSetting { .. }
            )));
        }
    }
}
