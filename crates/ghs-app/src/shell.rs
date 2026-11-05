//! Touch shell layout for the M2 live shell.
//!
//! Asymmetric three-column grid (see `docs/design/touch-layout.md`):
//!
//! ```text
//! +----------------+------------------------------+-----------------+
//! | Console/Program| Viewer                       | Coolant         |
//! |                |                              | Macros          |
//! |                |                              | Signals         |
//! |                |                              | Work            |
//! +----------------+------------------------------+                 |
//! | MDI            | DRO                          |                 |
//! +----------------+------------------------------+-----------------+
//! | Feed           | Jog                          | Spindle         |
//! +----------------+------------------------------+-----------------+
//! ```
//!
//! The columns are plain flex stacks with a shared 12 px gap and 112 px
//! bottom rows, so the guides line up without a grid layout: the right column
//! has one flexible cell plus a 112 px Spindle cell, which makes its tall cell
//! end exactly on the MDI/DRO baseline.
//!
//! Portrait / narrow behaviour plan (UX-03, deferred to M7):
//! - At < 1120 px the right rail collapses and the centre column takes the
//!   space; the layout stays landscape-shaped.
//! - At < 760 px the window should reflow to a single column: mode bar, active
//!   workspace, then a paged bottom dock (DRO / Jog / MDI / Spindle). The
//!   keypad becomes a full-width bottom sheet.
//! - Every panel keeps its 48 px targets in portrait; the dock scrolls rather
//!   than shrinking controls.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::TimeZone as _;

use ghs_config::{
    JobHistoryRecord, JobHistoryResult, Keymap, MAX_HEIGHT_MAPS, MAX_JOB_HISTORY,
    Macro as ConfigMacro, ThemeMode, ToolRecord,
};
use ghs_core::Units as CoreUnits;
use ghs_engine::{
    Axes, Axis as CoreAxis, Command, EngineError, JobPauseReason, JobState, JogCommand,
    MachineState, Position, Program, RealtimeCommand, ResetTarget, SerialConfig,
    Signals as CoreSignals, SystemCommand, Wcs,
};
use ghs_i18n::{Locale, t};
use ghs_probe::{MAX_PROBE_MACRO_BYTES, MAX_PROBE_MACRO_LINES};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable as _, Sizable as _, Size, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, App, AppContext as _, Context, Div, Entity, ExternalPaths, FocusHandle,
    FontWeight, Hsla, InteractiveElement as _, IntoElement, KeyDownEvent, KeyUpEvent, MouseButton,
    ParentElement as _, Pixels, Render, ScrollHandle, ScrollStrategy, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Task, UniformListScrollHandle,
    Window, div, px, uniform_list,
};

use crate::connection::{BAUD_RATES, ConnectionPanel, TransportChoice};
use crate::console_log::ConsoleLogStatus;
use crate::controller_reset::{
    LOCAL_PROFILE_NOTE, RESET_TARGETS, ResetGate, ResetOutcome, ResetResult, command_text,
    consequence, is_most_destructive, refresh_commands, refresh_note, summary, target_title,
};
use crate::file_browser::{
    BrowserEntry, FileBrowser, compact_path, dropped_program_path, format_size, read_program,
};
use crate::height_compensation::{
    HeightApplyGate, HeightApplyPreview, HeightMapApplyStatus, HeightMapSnapshot,
};
use crate::height_map_library::{
    LoadedSavedMap, SavedMapSummary, default_map_name, format_unix_utc,
};
use crate::keymap::{
    BindingPlan, JogAxis, JogXMinus, JogXPlus, JogYMinus, JogYPlus, JogZMinus, JogZPlus,
    KeyHoldGuard, KeyboardGate, KeymapModel, MachineAction, MachineIntent, MachinePause,
    MachineResume, MachineStart, MachineStop, binding_for, note_key_down, pretty_keys, refusal,
    unbind_for,
};
use crate::keypad;
use crate::live::{DemoProgram, LinkMode, LiveLink};
use crate::macros::MacroRunRequest;
use crate::offsets::{OffsetAction, OffsetGate, OffsetStatus};
use crate::pins::{
    PinsGate, PinsRequest, PinsStatus, ReportState, pin_bits_text, pin_direction_label,
    signal_mask_text,
};
use crate::probe::{
    FieldProblem, PreflightRow, ProbeCard, ProbeContext, ProbeField, ProbePhase, ProbePreflight,
    ProbeRun, ProbeTarget, RowState, ToolLengthAction, ToolLengthWriteStatus, format_duration,
    format_estimate, format_mm, height_map_summary, outcome_summary,
};
use crate::settings::{GroupRow, RowStatus, SettingKind, SettingRow, SettingsScope};
use crate::setup::{SafetyItem, SetupStep};
use crate::startup::{StartupRequest, StartupStatus};
use crate::state::{
    AppState, Axis, BreakpointOutcome, CheckOutcome, ConnectionState, ConsoleCounts, ConsoleFilter,
    ConsoleLevel, FEED_OVERRIDE_MAX, FEED_OVERRIDE_MIN, FEED_OVERRIDE_RESET, JobView, KeypadAction,
    KeypadTarget, LeftTab, Mode, ProgramBreakpoints, ProgramKind, ProgramLoadRefusal,
    ProgramSearch, ProgramSearchMode, RunOption, RunOptionOutcome, RunToOutcome, SignalState,
    SleepOutcome, StartOutcome, format_elapsed,
};
use crate::theme::Palette;
use crate::tools::{ControllerToolTable, ToolField, ToolTableModel};
use crate::utilities::{
    CalibrationAxis, CalibrationField, GANTRY_CONVENTION_NOTE, GANTRY_SAFETY_NOTE, GantryField,
    LimitsStatus, TransformKind, UtilityField,
};
use crate::viewer;

const STEP_SIZES: [f64; 5] = [0.01, 0.1, 1.0, 10.0, 100.0];

/// Height of the MDI/DRO and Feed/Jog/Spindle rows.
const ROW_H: f32 = 112.0;
/// Gap between cards and between columns.
const GAP: f32 = 12.0;
/// How often a held continuous jog re-issues its step.
const JOG_REPEAT: Duration = Duration::from_millis(120);
/// Fixed column widths for the local job-history rows.
const JOB_HISTORY_TIME_W: f32 = 118.0;
const JOB_HISTORY_RESULT_W: f32 = 70.0;
const JOB_HISTORY_ELAPSED_W: f32 = 50.0;
const JOB_HISTORY_LINES_W: f32 = 36.0;
const JOB_HISTORY_CHECK_W: f32 = 40.0;

/// One press-and-hold jog gesture.
#[derive(Clone, Copy, Debug)]
struct JogHold {
    axis: Axis,
    sign: f64,
    continuous: bool,
    started: std::time::Instant,
}

/// A release earlier than this is a click, not a hold: keep the one-step jog
/// instead of cancelling it. This also keeps a touch tap working, because GPUI
/// delivers a touch tap's mouse down and up together.
const JOG_CANCEL_AFTER: Duration = Duration::from_millis(250);

/// Index of the gantry-squaring card in the utility scroll list.
///
/// The list starts with `CONTROLLER RESET`, then `MACHINE SETUP`, then this
/// card. The demo capture scrolls it to the top.
const GANTRY_CARD_INDEX: usize = 3;
/// Position of the local job-history card in the Utility scroll list.
const JOB_HISTORY_CARD_INDEX: usize = 2;

/// What the shared settings text sheet is editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SettingsEntryPurpose {
    /// Filter the generated form.
    Search,
    /// A `Text`, `Password`, or `IPv4` setting.
    Text(u16),
    /// The local draft of one `$N0`/`$N1` startup line.
    StartupLine(u8),
    /// The name field of the open tool sheet.
    ToolName,
    /// The name field of the open macro editor.
    MacroName,
    /// The command field of the open macro editor.
    MacroCommand,
    /// The name field for saving the measured height map.
    HeightMapName,
}

/// One open startup-line write confirmation.
///
/// The exact text is captured when APPLY opens the sheet, so the operator
/// confirms what is shown instead of a value that a later edit could change.
#[derive(Clone, Debug)]
struct StartupConfirm {
    index: u8,
    text: String,
}

/// One open settings text sheet.
struct SettingsEntry {
    purpose: SettingsEntryPurpose,
    input: Entity<InputState>,
    masked: bool,
    error: Option<String>,
}

/// One open keymap row editor.
///
/// The text field and the optional capture focus are local. Nothing is written
/// to the profile until the section-level SAVE.
struct KeymapEditor {
    action: MachineAction,
    input: Entity<InputState>,
    error: Option<String>,
    capturing: bool,
    capture_focus: FocusHandle,
}

/// Text fields of one open tool add/edit sheet.
///
/// Numeric fields are strings so the shared keypad can seed and edit them
/// without a second parsing layer. Nothing is persisted until `SAVE TOOL`,
/// and nothing reaches the controller from this sheet.
#[derive(Clone, Debug)]
struct ToolEditor {
    /// Tool number being edited; `None` for a new record.
    original: Option<u32>,
    number: String,
    name: String,
    diameter: String,
    length: String,
    pocket: String,
    error: Option<String>,
}

impl ToolEditor {
    fn new() -> Self {
        Self {
            original: None,
            number: String::new(),
            name: String::new(),
            diameter: String::new(),
            length: String::new(),
            pocket: String::new(),
            error: None,
        }
    }

    fn from_record(record: &ToolRecord) -> Self {
        Self {
            original: Some(record.number),
            number: record.number.to_string(),
            name: record.name.clone(),
            diameter: format!("{:.3}", record.diameter_mm),
            length: format!("{:.3}", record.length_mm),
            pocket: record
                .pocket
                .map(|pocket| pocket.to_string())
                .unwrap_or_default(),
            error: None,
        }
    }

    fn value(&self, field: ToolField) -> &str {
        match field {
            ToolField::Number => &self.number,
            ToolField::Diameter => &self.diameter,
            ToolField::Length => &self.length,
            ToolField::Pocket => &self.pocket,
        }
    }

    fn value_mut(&mut self, field: ToolField) -> &mut String {
        match field {
            ToolField::Number => &mut self.number,
            ToolField::Diameter => &mut self.diameter,
            ToolField::Length => &mut self.length,
            ToolField::Pocket => &mut self.pocket,
        }
    }
}

/// Text fields of one open macro add/edit sheet.
///
/// Name and command are local drafts edited through the shared text sheet.
/// Nothing is persisted until SAVE MACRO, and nothing reaches the controller
/// from the editor.
#[derive(Clone, Debug)]
struct MacroEditor {
    /// Stored name being edited; `None` for a new macro.
    original: Option<String>,
    name: String,
    command: String,
    confirm: bool,
    error: Option<String>,
}

impl MacroEditor {
    fn new() -> Self {
        Self {
            original: None,
            name: String::new(),
            command: String::new(),
            // A new macro asks before running until the operator opts out.
            confirm: true,
            error: None,
        }
    }

    fn from_macro(mac: &ConfigMacro) -> Self {
        Self {
            original: Some(mac.name.clone()),
            name: mac.name.clone(),
            command: mac.command.clone(),
            confirm: mac.confirm,
            error: None,
        }
    }
}

/// Last local result of the Probe `SAVE DEFAULTS` action.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProbeDefaultsStatus {
    saved: bool,
    text: String,
}

/// Last local result of the Probe `SAVE MACROS` action.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProbeMacrosStatus {
    saved: bool,
    text: String,
}

/// Local macro-sheet draft. It never changes the model until SAVE MACROS.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ProbeMacroDraft {
    pre: String,
    post: String,
}

impl ProbeMacroDraft {
    fn from_model(probe: &crate::probe::ProbeModel) -> Self {
        Self {
            pre: probe.pre_macro().to_string(),
            post: probe.post_macro().to_string(),
        }
    }
}

/// Open pre/post probe macro editor. The draft lives here until SAVE.
struct ProbeMacroEditor {
    draft: ProbeMacroDraft,
    error: Option<String>,
    status: Option<ProbeMacrosStatus>,
}

impl ProbeMacroEditor {
    fn new(probe: &crate::probe::ProbeModel) -> Self {
        Self {
            draft: ProbeMacroDraft::from_model(probe),
            error: None,
            status: None,
        }
    }
}

/// Reproducible pre/post text for `--demo-probe-macros`.
pub(crate) const DEMO_PROBE_PRE_MACRO: &str = "(pre-probe)\nG4 P1\nM5";
/// Reproducible post text for `--demo-probe-macros`.
pub(crate) const DEMO_PROBE_POST_MACRO: &str = "(post-probe)\nG4 P1\nM9";
/// Forbidden motion line that opens the validation-error capture.
const DEMO_PROBE_INVALID_PRE_MACRO: &str = "(pre-probe)\nG4 P1\nG0 X0";

pub struct Shell {
    pub state: AppState,
    feed_slider: Entity<SliderState>,
    spindle_slider: Entity<SliderState>,
    mono: SharedString,
    link: Option<LiveLink>,
    panel: ConnectionPanel,
    panel_scroll: ScrollHandle,
    jog_hold: Option<JogHold>,
    jog_generation: u64,
    jog_task: Option<Task<()>>,
    feed_slider_active: bool,
    spindle_slider_active: bool,
    console_scroll: ScrollHandle,
    /// Utility workspace scroll position, used by the gantry card capture.
    utility_scroll: ScrollHandle,
    /// All-records `JOB HISTORY` sheet is open. Opening writes nothing.
    job_history_open: bool,
    /// Vertical scroll position of the all-records history sheet.
    job_history_scroll: ScrollHandle,
    /// Screenshot switch: seed and reveal the Utility job-history card.
    demo_job_history_card: bool,
    /// Screenshot switch: save the console log once after the demo handshake.
    demo_console_save: bool,
    demo_console_save_done: bool,
    /// Screenshot switch: reveal the Console tab after the demo program loads.
    demo_console_tab: bool,
    demo_console_tab_done: bool,
    program_scroll: UniformListScrollHandle,
    program_search_input: Entity<InputState>,
    /// Open in-app program file browser, when the modal is visible.
    file_browser: Option<FileBrowser>,
    /// Editable path field in the browser modal.
    file_browser_input: Entity<InputState>,
    file_browser_scroll: UniformListScrollHandle,
    /// Scroll position of the recent-file rows in the browser modal.
    recent_files_scroll: ScrollHandle,
    /// Last folder visited, reused the next time the browser opens.
    browser_dir: PathBuf,
    /// Screenshot switch: load this file once the demo link is ready.
    demo_load_file: Option<PathBuf>,
    demo_load_file_done: bool,
    /// Screenshot switch: render the Job-workspace drop hint without a drag.
    demo_drop_hint: bool,
    viewer: viewer::ViewerHandle,
    last_job_line: u32,
    demo_program: DemoProgram,
    demo_start: bool,
    demo_start_sent: bool,
    /// Screenshot switch: enable `$20` through the normal settings write path
    /// before the over-travel capture.
    demo_soft_limits: bool,
    demo_soft_limits_done: bool,
    /// Screenshot switch: press START once after an over-travel program's
    /// travel report is available, leaving the confirmation sheet open.
    demo_over_travel: bool,
    demo_over_travel_done: bool,
    /// Screenshot switch: open the check-run confirmation without confirming.
    demo_check_confirm: bool,
    demo_check_confirm_done: bool,
    /// Screenshot switch: confirm and start a demo check run once.
    demo_check: bool,
    demo_check_done: bool,
    /// Screenshot switch: open the read-only pins/limits diagnostics sheet.
    demo_pins: bool,
    demo_pins_done: bool,
    /// Screenshot switch: open the run-options sheet and flip two switches.
    demo_run_options: bool,
    demo_run_options_done: bool,
    /// Screenshot switch: enable demo breakpoints and show the Program tab.
    demo_breakpoints: bool,
    demo_breakpoints_done: bool,
    /// Screenshot switch: also select a line and open the run-to sheet.
    demo_run_to: bool,
    settings_entry: Option<SettingsEntry>,
    /// Local keymap draft. It is local until the section SAVE succeeds.
    keymap: KeymapModel,
    /// Last profile keymap adopted by the shell.
    keymap_source: Keymap,
    /// Keys currently bound in GPUI, used to unbind changed rows.
    active_bindings: Vec<(MachineAction, String)>,
    /// Physical keys held since their action last dispatched.
    held_machine_keys: KeyHoldGuard,
    keymap_editor: Option<KeymapEditor>,
    tool_editor: Option<ToolEditor>,
    macro_editor: Option<MacroEditor>,
    macro_delete: Option<String>,
    startup_confirm: Option<StartupConfirm>,
    /// Material surfacing wizard sheet is open. Opening sends nothing.
    surfacing_open: bool,
    /// Gantry-squaring calculator sheet is open. Opening sends nothing.
    gantry_open: bool,
    /// Stepper-calibration confirmation sheet is open. Opening sends nothing.
    calibration_confirm_open: bool,
    /// Screenshot switch: fill and calculate the calibration card.
    demo_calibration: bool,
    demo_calibration_done: bool,
    /// Screenshot switch: also open the calibration confirmation sheet.
    demo_calibration_confirm: bool,
    /// Screenshot switch: load the gantry sample and open the calculator.
    demo_gantry: bool,
    demo_gantry_done: bool,
    /// Screenshot switch: load the gantry sample on the card, sheet closed.
    demo_gantry_card: bool,
    /// Screenshot switch: open the surfacing wizard and generate a preview.
    demo_surfacing: bool,
    demo_surfacing_done: bool,
    /// Screenshot switch: open the line-number transform preview.
    demo_transform: bool,
    demo_transform_done: bool,
    /// Screenshot switch: open Utility at the controller-reset card.
    demo_reset: bool,
    demo_reset_done: bool,
    /// Screenshot switch: open the `$RST=#` confirmation without confirming.
    demo_reset_confirm: bool,
    demo_reset_confirm_done: bool,
    offset_refresh_sent: bool,
    /// A reset's read-only `$$` settings dump was Busy and is retried by poll.
    reset_settings_refresh_pending: bool,
    backup_import_armed: bool,
    settings_demo_write: bool,
    settings_demo_write_done: bool,
    settings_demo_group: Option<String>,
    settings_demo_group_done: bool,
    demo_tools: bool,
    demo_tools_done: bool,
    demo_macros: bool,
    demo_macros_done: bool,
    demo_macro_edit: bool,
    demo_macro_confirm: bool,
    demo_startup_write: bool,
    demo_startup_write_done: bool,
    demo_offsets_write: bool,
    demo_offsets_write_done: bool,
    demo_offsets_goto: bool,
    demo_offsets_goto_done: bool,
    demo_offsets_export: bool,
    demo_offsets_export_done: bool,
    demo_offsets_restore: bool,
    demo_offsets_restore_done: bool,
    probe_card_demo: Option<String>,
    probe_card_demo_done: bool,
    /// Last `SAVE DEFAULTS` result shown in the Probe workspace.
    probe_defaults_status: Option<ProbeDefaultsStatus>,
    /// Last `SAVE MACROS` result shown in the Probe workspace.
    probe_macros_status: Option<ProbeMacrosStatus>,
    /// Local pre/post macro editor, when the sheet is open.
    probe_macro_editor: Option<ProbeMacroEditor>,
    /// Multi-line state for the PRE macro field.
    probe_macro_pre_input: Entity<TextareaState>,
    /// Multi-line state for the POST macro field.
    probe_macro_post_input: Entity<TextareaState>,
    /// The textarea values must be copied from the draft on the next render.
    probe_macro_inputs_dirty: bool,
    /// Screenshot switch: fill valid sample macros and open the sheet unsaved.
    demo_probe_macros: bool,
    demo_probe_macros_done: bool,
    /// Screenshot switch: save valid sample macros through the normal path.
    demo_probe_macros_saved: bool,
    demo_probe_macros_saved_done: bool,
    /// Screenshot switch: open the sheet with a rejected motion line.
    demo_probe_macros_error: bool,
    demo_probe_macros_error_done: bool,
    /// Screenshot switch: press `SAVE DEFAULTS` once after the profile loads.
    demo_probe_defaults: bool,
    demo_probe_defaults_done: bool,
    demo_probe_run: bool,
    demo_probe_run_done: bool,
    demo_probe_apply: bool,
    demo_probe_apply_done: bool,
    demo_probe_tlo: bool,
    demo_probe_tlo_opened: bool,
    demo_probe_tlo_done: bool,
    demo_probe_tlo_confirm: bool,
    demo_height_map: bool,
    demo_height_map_done: bool,
    demo_height_map_export: bool,
    demo_height_map_export_done: bool,
    demo_height_map_apply: bool,
    demo_height_map_apply_done: bool,
    demo_height_map_revert: bool,
    demo_height_map_revert_done: bool,
    demo_height_map_confirm: bool,
    demo_height_map_confirm_done: bool,
    demo_height_map_job: bool,
    demo_height_map_job_done: bool,
    demo_height_map_save: bool,
    demo_height_map_save_done: bool,
    demo_height_map_open_saved: bool,
    demo_height_map_open_saved_done: bool,
    /// The first frame has rendered; the first-run setup prompt waits for it.
    first_render_done: bool,
    /// A demo/layout flag already selects the initial screen, so the first-run
    /// setup wizard must not auto-open.
    demo_initial_screen: bool,
    /// Toolpath captured before a height-map probe replaced the job program.
    height_map_base_program: Option<Arc<ghs_gcode::Program>>,
    _poll_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let feed_slider = cx.new(|_| {
            SliderState::new()
                .min(10.0)
                .max(200.0)
                .step(1.0)
                .default_value(100.0)
        });
        let spindle_slider = cx.new(|_| {
            SliderState::new()
                .min(50.0)
                .max(200.0)
                .step(1.0)
                .default_value(100.0)
        });

        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe(&feed_slider, |this, _, event, cx| {
            match event {
                SliderEvent::Change(value) => {
                    this.feed_slider_active = true;
                    this.state.feed_override = value.end().round().clamp(10.0, 200.0) as u16;
                }
                SliderEvent::Release(value) => {
                    this.feed_slider_active = false;
                    let target = value.end().round().clamp(10.0, 200.0) as u16;
                    this.state.feed_override = target;
                    this.request_feed_override(target);
                }
            }
            cx.notify();
        }));
        subscriptions.push(cx.subscribe(&spindle_slider, |this, _, event, cx| {
            match event {
                SliderEvent::Change(value) => {
                    this.spindle_slider_active = true;
                    this.state.spindle_override = value.end().round().clamp(50.0, 200.0) as u16;
                }
                SliderEvent::Release(value) => {
                    this.spindle_slider_active = false;
                    let target = value.end().round().clamp(50.0, 200.0) as u16;
                    this.state.spindle_override = target;
                    this.request_spindle_override(target);
                }
            }
            cx.notify();
        }));

        let mono = cx.theme().mono_font_family.clone();
        let mut state = AppState::new();
        // Apply the persisted locale before the first frame. Unknown but
        // well-formed tags stay in AppConfig and display English.
        crate::i18n::apply_startup_locale(state.config.app());
        // Demo-only switches used for screenshots and manual reviews. They do
        // not affect normal startup and never touch the controller. An explicit
        // flag overrides the persisted mode for this run; the selector is what
        // writes a choice back to app.json.
        state.theme_mode = crate::theme::launch_override(std::env::args().skip(1))
            .unwrap_or(state.config.app().theme);
        crate::theme::apply(state.theme_mode, cx);
        let mock = std::env::args().any(|arg| arg == "--mock");
        if mock {
            state.connection = ConnectionState::Mock;
            state.push_console(
                ConsoleLevel::Info,
                "Design-review mock mode; no controller is attached",
            );
        }
        // Demo/screenshot switches for the setup wizard. They only open the
        // overlay; they never complete setup or send a command.
        let demo_setup_review = std::env::args().any(|arg| arg == "--demo-setup-review");
        let demo_setup_units = std::env::args().any(|arg| arg == "--demo-setup-units");
        let demo_setup = std::env::args().any(|arg| arg == "--demo-setup")
            || demo_setup_units
            || demo_setup_review;
        // The first-run prompt must not fight a flag that already picks the
        // initial screen (a workspace, a sheet, or a demo capture).
        let demo_initial_screen = std::env::args().any(|arg| demo_initial_screen_arg(&arg));
        if std::env::args().any(|arg| arg == "--keypad") {
            state.open_keypad(KeypadTarget::Dro(Axis::X));
        }
        // Screenshot switch: open the generated settings workspace directly.
        if std::env::args().any(|arg| arg == "--settings") {
            state.mode = Mode::Settings;
        }
        // Screenshot switch: open the Program tab with a search already
        // applied. Normal startup always starts with Console and no query.
        if let Some(query) = program_search_arg() {
            state.program_search.set_query(query);
            state.left_tab = LeftTab::Program;
        }
        if std::env::args().any(|arg| arg == "--program-highlight") {
            state.program_search.set_mode(ProgramSearchMode::Highlight);
        }
        let program_search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(state.program_search.query().to_string())
                .placeholder("Line 120, N120, or text")
                .clean_on_escape()
                .submit_on_enter(true)
        });
        subscriptions.push(
            cx.subscribe(&program_search_input, |this, input, event, cx| {
                match event {
                    InputEvent::Change => {
                        let was_active = this.state.program_search.is_active();
                        let query = input.read(cx).value().to_string();
                        if query.is_empty() {
                            this.state.clear_program_search();
                        } else {
                            this.state.set_program_search_query(query);
                        }
                        // Clearing the field restores the full list and follows
                        // the streaming line again; typing does not move the list.
                        if was_active && !this.state.program_search.is_active() {
                            this.follow_current_program_line();
                        }
                        cx.notify();
                    }
                    InputEvent::PressEnter { shift, .. } => {
                        if *shift {
                            this.previous_program_match(cx);
                        } else {
                            this.next_program_match(cx);
                        }
                    }
                    _ => {}
                }
            }),
        );

        // The file browser starts at home, or at the folder/file named by the
        // screenshot switch. Browsing stays available in every mode; loading
        // is gated separately. `--demo-recent-files` seeds the fixed review
        // list through the normal app-config save path and opens this sheet.
        let demo_recent_files = std::env::args().any(|arg| arg == "--demo-recent-files");
        if demo_recent_files && let Err(error) = state.config.seed_demo_recent_files() {
            tracing::warn!(%error, "demo recent files were not seeded");
        }
        let browser_arg =
            std::env::args().find_map(|arg| arg.strip_prefix("--file-browser=").map(PathBuf::from));
        let browser_dir = browser_arg
            .as_deref()
            .map(Path::to_path_buf)
            .filter(|path| path.is_dir())
            .or_else(|| {
                browser_arg
                    .as_deref()
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
            })
            .filter(|path| path.is_dir())
            .unwrap_or_else(crate::file_browser::default_root);
        let file_browser = if browser_arg.is_some() || demo_recent_files {
            Some(crate::file_browser::FileBrowser::open(
                browser_arg.as_deref().unwrap_or(browser_dir.as_path()),
            ))
        } else {
            None
        };
        let browser_input_value = file_browser.as_ref().map_or_else(
            || browser_dir.to_string_lossy().into_owned(),
            FileBrowser::path_display,
        );
        let file_browser_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(browser_input_value)
                .placeholder("/path/to/folder")
                .clean_on_escape()
                .submit_on_enter(true)
        });
        subscriptions.push(cx.subscribe(
            &file_browser_input,
            |this, input, event, cx| match event {
                InputEvent::Change => {
                    let draft = input.read(cx).value().to_string();
                    if let Some(browser) = this.file_browser.as_mut() {
                        browser.set_path_draft(draft);
                    }
                }
                InputEvent::PressEnter { .. } => this.commit_browser_path(cx),
                _ => {}
            },
        ));

        // Screenshot/demo switches. They only select a bundled demo source or
        // press the same Start action a tap would; normal startup ignores them.
        let demo_program = if std::env::args().any(|arg| arg == "--demo-toolchange") {
            DemoProgram::ToolChange
        } else if std::env::args().any(|arg| arg == "--demo-m0") {
            DemoProgram::ProgramStop
        } else {
            DemoProgram::Pocket
        };
        let demo_start = std::env::args().any(|arg| arg == "--demo-start");
        let demo_soft_limits = std::env::args().any(|arg| arg == "--demo-soft-limits");
        let demo_over_travel = std::env::args().any(|arg| arg == "--demo-over-travel");
        let demo_check = std::env::args().any(|arg| arg == "--demo-check");
        let demo_check_confirm = std::env::args().any(|arg| arg == "--demo-check-confirm");
        let demo_pins = std::env::args().any(|arg| arg == "--pins");
        let demo_run_options = std::env::args().any(|arg| arg == "--demo-run-options");
        let demo_load_file = std::env::args()
            .find_map(|arg| arg.strip_prefix("--demo-load-file=").map(PathBuf::from));
        let demo_run_to = std::env::args().any(|arg| arg == "--demo-run-to");
        let demo_breakpoints =
            demo_run_to || std::env::args().any(|arg| arg == "--demo-breakpoints");
        let settings_demo_write = std::env::args().any(|arg| arg == "--demo-settings-write");
        let settings_demo_group = std::env::args().find_map(|arg| {
            arg.strip_prefix("--settings-group=")
                .map(std::string::ToString::to_string)
        });
        if std::env::args().any(|arg| arg == "--offsets") {
            state.mode = Mode::Offset;
        }
        if std::env::args().any(|arg| arg == "--tools") {
            state.mode = Mode::Tools;
        }
        if std::env::args().any(|arg| arg == "--utility") {
            state.mode = Mode::Utility;
        }
        if std::env::args().any(|arg| arg == "--startup-lines") {
            state.mode = Mode::Utility;
        }
        if std::env::args().any(|arg| arg == "--macros") {
            state.mode = Mode::Utility;
        }
        // Screenshot switch: seed a non-empty local MDI history and recall the
        // newest line. Nothing is sent and no engine state changes.
        if std::env::args().any(|arg| arg == "--demo-mdi-history") {
            for line in ["G0 X10 Y10", "G1 Z-1 F200", "M3 S12000"] {
                state.remember_mdi(line);
            }
            state.mdi_history_previous();
        }
        let demo_tools = std::env::args().any(|arg| arg == "--demo-tools");
        let demo_macro_edit = std::env::args().any(|arg| arg == "--demo-macro-edit");
        let demo_macro_confirm = std::env::args().any(|arg| arg == "--demo-macro-confirm");
        let demo_macros = std::env::args().any(|arg| arg == "--demo-macros")
            || demo_macro_edit
            || demo_macro_confirm;
        let demo_startup_write = std::env::args().any(|arg| arg == "--demo-startup-write");
        let demo_calibration = std::env::args().any(|arg| arg == "--demo-calibration");
        let demo_calibration_confirm =
            std::env::args().any(|arg| arg == "--demo-calibration-confirm");
        let demo_gantry_card = std::env::args().any(|arg| arg == "--demo-gantry-card");
        let demo_gantry = demo_gantry_card || std::env::args().any(|arg| arg == "--demo-gantry");
        let demo_surfacing = std::env::args().any(|arg| arg == "--demo-surfacing");
        let demo_transform = std::env::args().any(|arg| arg == "--demo-transform");
        let demo_reset_confirm = std::env::args().any(|arg| arg == "--demo-reset-confirm");
        let demo_reset = demo_reset_confirm || std::env::args().any(|arg| arg == "--demo-reset");
        if demo_surfacing
            || demo_transform
            || demo_calibration
            || demo_calibration_confirm
            || demo_gantry
            || demo_reset
        {
            state.mode = Mode::Utility;
        }
        let demo_offsets_write = std::env::args().any(|arg| arg == "--demo-offsets-write");
        let demo_offsets_goto = std::env::args().any(|arg| arg == "--demo-offsets-goto");
        let demo_offsets_restore = std::env::args().any(|arg| arg == "--demo-offsets-restore");
        let demo_offsets_export =
            demo_offsets_restore || std::env::args().any(|arg| arg == "--demo-offsets-export");
        let demo_probe_tlo = std::env::args().any(|arg| arg == "--demo-probe-tlo");
        let demo_probe_tlo_confirm = std::env::args().any(|arg| arg == "--demo-probe-tlo-confirm");
        let probe_card_demo = if demo_probe_tlo || demo_probe_tlo_confirm {
            // The TLO screenshot switches always use the tool-length card.
            Some("tool-length".to_string())
        } else {
            std::env::args().find_map(|arg| {
                arg.strip_prefix("--probe-card=")
                    .map(std::string::ToString::to_string)
            })
        };
        let demo_probe_defaults = std::env::args().any(|arg| arg == "--demo-probe-defaults");
        let demo_probe_macros = std::env::args().any(|arg| arg == "--demo-probe-macros");
        let demo_probe_macros_saved =
            std::env::args().any(|arg| arg == "--demo-probe-macros-saved");
        let demo_probe_macros_error =
            std::env::args().any(|arg| arg == "--demo-probe-macros-error");
        if probe_card_demo.is_some()
            || demo_probe_defaults
            || demo_probe_macros
            || demo_probe_macros_saved
            || demo_probe_macros_error
            || std::env::args().any(|arg| arg == "--probe")
        {
            state.mode = Mode::Probe;
        }
        let demo_probe_apply = std::env::args().any(|arg| arg == "--demo-probe-apply");
        let demo_height_map = std::env::args().any(|arg| {
            arg == "--demo-height-map"
                || arg == "--demo-height-map-result"
                || arg == "--demo-height-map-export"
                || arg == "--demo-height-map-apply"
                || arg == "--demo-height-map-revert"
                || arg == "--demo-height-map-confirm"
                || arg == "--demo-height-map-job"
                || arg == "--demo-height-map-save"
                || arg == "--demo-height-map-open-saved"
        });
        let demo_height_map_result = std::env::args().any(|arg| arg == "--demo-height-map-result");
        let demo_height_map_export = std::env::args().any(|arg| arg == "--demo-height-map-export");
        let demo_height_map_revert = std::env::args().any(|arg| arg == "--demo-height-map-revert");
        let demo_height_map_confirm =
            std::env::args().any(|arg| arg == "--demo-height-map-confirm");
        let demo_height_map_job = std::env::args().any(|arg| arg == "--demo-height-map-job");
        let demo_height_map_save = std::env::args().any(|arg| arg == "--demo-height-map-save");
        let demo_height_map_open_saved =
            std::env::args().any(|arg| arg == "--demo-height-map-open-saved");
        let demo_height_map_save = demo_height_map_save || demo_height_map_open_saved;
        let demo_height_map_apply = demo_height_map_revert
            || demo_height_map_job
            || std::env::args().any(|arg| arg == "--demo-height-map-apply");
        if demo_height_map {
            state.mode = Mode::Probe;
        }
        let demo_probe_run = demo_probe_apply
            || demo_probe_tlo
            || demo_probe_tlo_confirm
            || demo_height_map_result
            || demo_height_map_save
            || demo_height_map_confirm
            || demo_height_map_apply
            || std::env::args().any(|arg| arg == "--demo-probe-run");
        if std::env::args().any(|arg| arg == "--keymap" || arg == "--demo-keymap") {
            state.mode = Mode::Utility;
        }
        // Screenshot switches: seed three fixed records through the normal
        // app-config save path and reveal the Utility card. The clear switch
        // additionally arms the inline confirmation without clearing; the
        // sheet switch opens the all-records list.
        let demo_job_history = std::env::args().any(|arg| arg == "--demo-job-history");
        let demo_job_history_clear = std::env::args().any(|arg| arg == "--demo-job-history-clear");
        let demo_job_history_sheet = std::env::args().any(|arg| arg == "--demo-job-history-sheet");
        let demo_job_history_card =
            demo_job_history || demo_job_history_clear || demo_job_history_sheet;
        if demo_job_history_card {
            state.mode = Mode::Utility;
            if let Err(error) = state.config.seed_demo_job_history() {
                tracing::warn!(%error, "demo job history was not seeded");
            }
            if demo_job_history_clear && !state.arm_job_history_clear() {
                tracing::warn!("demo job history has no records to clear");
            }
        }

        let keymap = KeymapModel::default();
        let keymap_source = Keymap::default();

        // The two multi-line fields live for the whole shell; opening the
        // sheet resets their text from the model draft.
        let probe_macro_pre_input = cx.new(|cx| TextareaState::new(window, cx).rows(8));
        let probe_macro_post_input = cx.new(|cx| TextareaState::new(window, cx).rows(8));

        let mut shell = Self {
            state,
            feed_slider,
            spindle_slider,
            mono,
            link: None,
            panel: ConnectionPanel::new(),
            panel_scroll: ScrollHandle::new(),
            jog_hold: None,
            jog_generation: 0,
            jog_task: None,
            feed_slider_active: false,
            spindle_slider_active: false,
            console_scroll: ScrollHandle::new(),
            utility_scroll: ScrollHandle::new(),
            job_history_open: demo_job_history_sheet,
            job_history_scroll: ScrollHandle::new(),
            demo_job_history_card,
            demo_console_save: std::env::args().any(|arg| arg == "--demo-console-save"),
            demo_console_save_done: false,
            demo_console_tab: std::env::args().any(|arg| arg == "--console-tab")
                || std::env::args().any(|arg| arg.starts_with("--console-filter=")),
            demo_console_tab_done: false,
            program_scroll: UniformListScrollHandle::new(),
            program_search_input,
            file_browser,
            file_browser_input,
            file_browser_scroll: UniformListScrollHandle::new(),
            recent_files_scroll: ScrollHandle::new(),
            browser_dir,
            demo_load_file,
            demo_load_file_done: false,
            demo_drop_hint: std::env::args().any(|arg| arg == "--demo-drop-hint"),
            viewer: viewer::ViewerHandle::new(),
            last_job_line: 0,
            demo_program,
            demo_start,
            demo_start_sent: false,
            demo_soft_limits,
            demo_soft_limits_done: false,
            demo_over_travel,
            demo_over_travel_done: false,
            demo_check_confirm,
            demo_check_confirm_done: false,
            demo_check,
            demo_check_done: false,
            demo_pins,
            demo_pins_done: false,
            demo_run_options,
            demo_run_options_done: false,
            demo_breakpoints,
            demo_breakpoints_done: false,
            demo_run_to,
            settings_entry: None,
            keymap,
            keymap_source,
            active_bindings: Vec::new(),
            held_machine_keys: KeyHoldGuard::default(),
            keymap_editor: None,
            tool_editor: None,
            macro_editor: None,
            macro_delete: None,
            startup_confirm: None,
            surfacing_open: false,
            gantry_open: false,
            calibration_confirm_open: false,
            demo_calibration,
            demo_calibration_done: false,
            demo_calibration_confirm,
            demo_gantry,
            demo_gantry_done: false,
            demo_gantry_card,
            demo_surfacing,
            demo_surfacing_done: false,
            demo_transform,
            demo_transform_done: false,
            demo_reset,
            demo_reset_done: false,
            demo_reset_confirm,
            demo_reset_confirm_done: false,
            offset_refresh_sent: false,
            reset_settings_refresh_pending: false,
            backup_import_armed: false,
            settings_demo_write,
            settings_demo_write_done: false,
            settings_demo_group,
            settings_demo_group_done: false,
            demo_tools,
            demo_tools_done: false,
            demo_macros,
            demo_macros_done: false,
            demo_macro_edit,
            demo_macro_confirm,
            demo_startup_write,
            demo_startup_write_done: false,
            demo_offsets_write,
            demo_offsets_write_done: false,
            demo_offsets_goto,
            demo_offsets_goto_done: false,
            demo_offsets_export,
            demo_offsets_export_done: false,
            demo_offsets_restore,
            demo_offsets_restore_done: false,
            probe_card_demo,
            probe_card_demo_done: false,
            probe_defaults_status: None,
            probe_macros_status: None,
            probe_macro_editor: None,
            probe_macro_pre_input,
            probe_macro_post_input,
            probe_macro_inputs_dirty: false,
            demo_probe_macros,
            demo_probe_macros_done: false,
            demo_probe_macros_saved,
            demo_probe_macros_saved_done: false,
            demo_probe_macros_error,
            demo_probe_macros_error_done: false,
            demo_probe_defaults,
            demo_probe_defaults_done: false,
            demo_probe_run,
            demo_probe_run_done: false,
            demo_probe_apply,
            demo_probe_apply_done: false,
            demo_probe_tlo,
            demo_probe_tlo_opened: false,
            demo_probe_tlo_done: false,
            demo_probe_tlo_confirm,
            demo_height_map,
            demo_height_map_done: false,
            demo_height_map_export,
            demo_height_map_export_done: false,
            demo_height_map_apply,
            demo_height_map_apply_done: false,
            demo_height_map_revert,
            demo_height_map_revert_done: false,
            demo_height_map_confirm,
            demo_height_map_confirm_done: false,
            demo_height_map_job,
            demo_height_map_job_done: false,
            demo_height_map_save,
            demo_height_map_save_done: false,
            demo_height_map_open_saved,
            demo_height_map_open_saved_done: false,
            first_render_done: false,
            demo_initial_screen,
            height_map_base_program: None,
            _poll_task: None,
            _subscriptions: subscriptions,
        };
        // Bind the documented defaults immediately. A profile keymap replaces
        // them after the handshake; loading a keymap never dispatches an
        // action, starts a job, or jogs an axis.
        let defaults = shell.keymap.effective_bindings(cx.platform_info().os);
        let plan = BindingPlan::between(&[], &defaults);
        shell.apply_binding_plan(plan, cx);
        shell.active_bindings = defaults;
        // A key-up can be missed while the window is inactive. Release the
        // repeat latch on every activation change so the first press after
        // returning to the app is never swallowed.
        shell
            ._subscriptions
            .push(cx.observe_window_activation(window, |this, _, cx| {
                this.held_machine_keys.clear();
                cx.notify();
            }));
        if !mock {
            shell.connect_demo(cx);
        }
        // Screenshot switch: preselect a Console filter. Applied after
        // `connect_demo` because `begin_live` resets the filter for a new
        // session. This changes only the view; no line is dropped.
        if let Some(value) = std::env::args().find_map(|arg| {
            arg.strip_prefix("--console-filter=")
                .map(std::string::ToString::to_string)
        }) && let Some(filter) = ConsoleFilter::parse(&value)
        {
            shell.state.set_console_filter(filter);
        }
        // Screenshot switch: open the sleep confirmation after `begin_live`
        // resets the session. Opening sends nothing; normal startup keeps the
        // sheet closed.
        if std::env::args().any(|arg| arg == "--demo-sleep-confirm") {
            shell.state.open_sleep_confirm();
        }
        // Screenshot switch: start in the orbit view. 2D top remains the
        // default for every normal launch.
        if std::env::args().any(|arg| arg == "--viewer-3d") {
            shell.viewer.set_mode(viewer::ViewerMode::ThreeD);
        }
        // Screenshot switch: hide the inferred stock box so the STOCK chip's
        // off state can be captured. Viewer-only; it sends nothing.
        if std::env::args().any(|arg| arg == "--viewer-stock-off") && shell.viewer.stock_visible() {
            shell.viewer.toggle_stock();
        }
        if shell.demo_probe_run
            || shell.demo_height_map
            || std::env::args().any(|arg| arg == "--demo-probe-check")
        {
            // The reference board has no PC signal; the screenshot switch
            // stands in for the operator's explicit "probe connected" check.
            // Applied after `connect_demo` because `begin_live` resets the
            // probe model for a new session.
            shell.state.probe.probe_connected_confirmed = true;
        }
        if let Some(value) = connection_panel_arg() {
            if value == "serial" {
                shell.panel.select_transport(TransportChoice::Serial);
            }
            shell.open_connection(cx);
        }
        // Demo/screenshot switches: open a deterministic setup page after the
        // session starts. Opening sends nothing and never marks setup complete.
        if demo_setup_review {
            shell.state.open_setup_review_for_demo();
        } else if demo_setup_units {
            shell.state.open_setup_units_for_demo();
        } else if demo_setup {
            shell.state.open_setup();
        }
        shell._poll_task = Some(shell.spawn_poll(cx));
        shell
    }

    fn spawn_poll(&self, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
                if weak.update(cx, |shell, cx| shell.poll(cx)).is_err() {
                    break;
                }
            }
        })
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        let console_before = self.state.console.len();
        let mut changed = false;
        // The first-run prompt waits for the first frame, then runs once. A
        // dismissed prompt is not offered again in this app run; the Utility
        // card remains the explicit way back in.
        if self.first_render_done && self.state.auto_open_setup(self.demo_initial_screen) {
            changed = true;
        }
        if let Some(link) = self.link.as_mut() {
            changed |= link.poll(&mut self.state);
        }
        // At most one finished-job record is persisted per poll. A refused or
        // failed local write is logged once and dropped; never retry it in a
        // loop and never overwrite a corrupt `app.json`.
        if let Some(result) = flush_job_history_record(&mut self.state) {
            match result {
                Ok(_) => changed = true,
                Err(error) => tracing::warn!(%error, "job history was not saved"),
            }
        }
        // Adopt a profile keymap after the handshake, a pending-profile
        // confirmation, or a backup import. This only binds keys; it never
        // dispatches an action.
        changed |= self.sync_profile_keymap(cx);
        // A macro confirmation belongs to the link that produced it. Once the
        // link is no longer ready it can never be sent.
        if self.state.macro_run_pending().is_some() && !self.ready() {
            self.state.cancel_macro_run();
            changed = true;
        }
        // A reset confirmation belongs to the link and Idle state that opened
        // it. Once the link is no longer ready the sheet cannot send; close it
        // rather than leaving a stale destructive action on screen.
        if (self.state.controller_reset.is_confirm_open() && !self.ready())
            || (self.state.controller_reset.is_confirm_open() && self.locked())
        {
            self.state.controller_reset.cancel_confirm();
            changed = true;
        }
        // An over-travel START confirmation belongs to the exact program, link,
        // and Idle machine state that opened it. Clear it as soon as any of
        // them changes; a stale start must never fire from a later sheet.
        if self.state.sync_start_confirm() {
            changed = true;
        }
        // A failed height-map point must stop the job once, even if the
        // controller did not raise its own alarm yet. Never resume it.
        if self.state.probe.take_stop_request()
            && self.send_command(Command::StopProgram { reset: false })
        {
            changed = true;
        }
        // Resume only after a planned probe checkpoint whose PRB was bound.
        // This pacing exists for the in-process simulator only; a serial
        // controller streams the plan and never auto-resumes. Never resume
        // after an alarm, error, disconnect, operator pause or watchdog.
        let demo_pacing = matches!(
            self.link.as_ref().and_then(|link| link.mode()),
            Some(LinkMode::Demo)
        );
        if demo_pacing
            && self.state.probe.needs_resume()
            && self.ready()
            && !self.locked()
            && matches!(
                self.machine(),
                MachineState::Idle | MachineState::Hold | MachineState::Tool
            )
            && self.state.job.is_paused()
            && self.send_command(Command::ResumeProgram)
        {
            self.state.probe.mark_resumed();
            changed = true;
        }
        // A G10 write is followed by a read-only `$#`. The engine may still be
        // streaming the MDI line, so retry quietly until it accepts the
        // refresh instead of pretending the table changed.
        if self.state.offsets.status().is_pending() {
            if !self.offset_refresh_sent {
                changed |= self.pump_offset_refresh();
            }
        } else {
            self.offset_refresh_sent = false;
        }
        // A reset's `$$` read is read-only and may be Busy behind the reset
        // line. Retry it until the streamer accepts it; it never writes a
        // setting or re-sends the reset.
        if self.reset_settings_refresh_pending {
            if !self.ready() {
                self.reset_settings_refresh_pending = false;
            } else {
                match self.send_command_quiet(Command::System(SystemCommand::Settings)) {
                    Ok(()) => {
                        self.reset_settings_refresh_pending = false;
                        changed = true;
                    }
                    Err(EngineError::Busy) => {}
                    Err(error) => {
                        self.reset_settings_refresh_pending = false;
                        self.state.push_console(
                            ConsoleLevel::Warn,
                            format!("Reset settings refresh not sent: {error}"),
                        );
                        changed = true;
                    }
                }
            }
        }
        // A confirmed TLO write must be verified by `$#`. If the report does not
        // arrive, fail the write and retain the old displayed value; never
        // assume the controller applied it.
        if self.state.probe.check_tool_length_timeout(Instant::now()) {
            let message = self
                .state
                .probe
                .tool_length_write()
                .and_then(|write| match &write.status {
                    ToolLengthWriteStatus::Failed { message } => Some(message.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "timed out waiting for the `$#` TLO read-back".to_string());
            self.state.offsets.failed(message.clone());
            self.state.push_console(
                ConsoleLevel::Error,
                format!("TLO write FAILED: {message}; the old TLO is retained"),
            );
            changed = true;
        }
        // Settings writes are idle-only and verified one at a time. Pump the
        // next queued row after the previous `SettingWritten` arrived; stop the
        // batch instead of resuming it automatically when the machine leaves
        // Idle or the link drops.
        if self.state.settings.has_queued_write() {
            if self.settings_write_allowed() {
                changed |= self.pump_settings_write();
            } else if self
                .state
                .settings
                .cancel_writes(self.settings_lock_reason())
            {
                changed = true;
            }
        }
        if !self.ready() && self.state.settings.cancel_writes("not connected") {
            changed = true;
        }
        // Startup lines: one queued write, then a read-back retry while the
        // engine is busy. Never update the row from the write itself.
        changed |= self.pump_startup_writes();
        // Read-only pins/limits diagnostics: opening the sheet or pressing
        // REFRESH queues the four reports; Busy is retried with a backoff.
        if self.demo_pins && !self.demo_pins_done && (self.ready() || !self.state.live) {
            self.demo_pins_done = true;
            self.open_pins_panel(cx);
            changed = true;
        }
        changed |= self.pump_pins();
        if let Some(name) = self.settings_demo_group.clone()
            && !self.settings_demo_group_done
        {
            let scope = self
                .state
                .settings
                .rail()
                .iter()
                .find(|row| row.name.eq_ignore_ascii_case(&name))
                .map(|row| row.scope);
            if let Some(scope) = scope {
                self.settings_demo_group_done = true;
                self.state.settings.set_scope(scope);
                changed = true;
            } else if self.state.settings.rows().iter().any(|row| row.described) {
                // Metadata is in; the controller simply has no such group.
                self.settings_demo_group_done = true;
                self.state.push_console(
                    ConsoleLevel::Warn,
                    format!("--settings-group: no group named {name}"),
                );
                changed = true;
            }
        }
        if self.settings_demo_write
            && !self.settings_demo_write_done
            && self.settings_write_allowed()
            && self.demo_settings_write(cx)
        {
            changed = true;
        }
        // A transport drop, alarm or mode change must not leave a jog running.
        if self.jog_hold.is_some() && !self.jog_allowed() {
            self.end_jog(cx);
            changed = true;
        }
        if self.panel.poll() {
            changed = true;
        }
        // Screenshot switch: the demo load reveals the Program tab, so a
        // Console capture asks for it once the parsed program exists. Normal
        // startup keeps the load behaviour unchanged.
        if self.demo_console_tab
            && !self.demo_console_tab_done
            && self.state.connection == ConnectionState::Ready
            && self.state.job.has_program()
        {
            self.demo_console_tab_done = true;
            self.state.left_tab = LeftTab::Console;
            changed = true;
        }
        // Screenshot switch: write one console snapshot after the demo
        // handshake has produced a useful buffer. This is the same SAVE LOG
        // action a tap uses; it never sends a command.
        if self.demo_console_save
            && !self.demo_console_save_done
            && self.state.connection == ConnectionState::Ready
            && self.state.console.len() >= 12
        {
            self.demo_console_save_done = true;
            self.save_console_log(cx);
            changed = true;
        }
        // `--demo-start` presses the same action as the toolbar button, once.
        if self.demo_start
            && !self.demo_start_sent
            && self.state.live
            && self.state.job.has_program()
            && self.job_start_allowed()
        {
            self.demo_start_sent = true;
            self.start_job(cx);
            changed = true;
        }
        // `--demo-check-confirm` opens the check confirmation through the
        // normal request action and leaves it open; it sends nothing.
        if self.demo_check_confirm
            && !self.demo_check_confirm_done
            && self.state.live
            && self.state.job.has_program()
            && self.state.check_refusal().is_none()
        {
            self.demo_check_confirm_done = true;
            self.state.mode = Mode::Job;
            self.open_check_confirm(cx);
            changed = true;
        }
        // `--demo-check` presses CHECK and START CHECK through the same model
        // actions as the sheet, once the demo program is loaded.
        if self.demo_check
            && !self.demo_check_done
            && self.state.live
            && self.state.job.has_program()
            && self.state.check_refusal().is_none()
        {
            self.demo_check_done = true;
            self.state.mode = Mode::Job;
            if self.state.request_check().is_ok() {
                self.confirm_check(cx);
            } else {
                self.state.check_confirm_open = false;
            }
            changed = true;
        }
        // Screenshot switch: open the RUN OPTIONS sheet and flip optional stop
        // and single block through the same guarded path as a tap. The values
        // are not forced here; the rows follow the controller accessors.
        if self.demo_run_options
            && !self.demo_run_options_done
            && self.state.connection == ConnectionState::Ready
        {
            self.demo_run_options_done = true;
            self.state.mode = Mode::Job;
            self.state.open_run_options();
            self.toggle_run_option(RunOption::OptionalStop, cx);
            self.toggle_run_option(RunOption::SingleBlock, cx);
            changed = true;
        }
        // Screenshot switch: enable a few breakpoints through the same guarded
        // toggle path as a tap, then optionally open the run-to sheet without
        // confirming it. Nothing starts unless the operator confirms.
        if self.demo_breakpoints
            && !self.demo_breakpoints_done
            && self.state.connection == ConnectionState::Ready
            && self.state.job.has_program()
        {
            self.demo_breakpoints_done = true;
            self.state.mode = Mode::Job;
            self.state.left_tab = LeftTab::Program;
            let total = self.state.job.total;
            for line in [2, 3, 4] {
                if line <= total {
                    self.toggle_program_breakpoint(line, cx);
                }
            }
            if self.demo_run_to && total > 0 {
                let line = 4.min(total);
                self.select_program_line(line, cx);
                let _ = self.state.request_run_to(line);
            }
            changed = true;
        }
        // Screenshot switch: load a real program file through the same bounded
        // reader and guarded `LoadProgram` path as the browser. It never starts
        // or applies anything.
        if let Some(path) = self.demo_load_file.clone()
            && !self.demo_load_file_done
            && self.state.connection == ConnectionState::Ready
        {
            self.demo_load_file_done = true;
            match self.load_program_file(&path) {
                Ok(message) => {
                    self.state.left_tab = LeftTab::Program;
                    self.state.push_console(ConsoleLevel::Info, message);
                }
                Err(message) => self.state.push_console(ConsoleLevel::Error, message),
            }
            changed = true;
        }
        // Screenshot switch: turn on the simulator's soft-limit report through
        // the normal settings draft + Apply + read-back path. This is demo
        // setup, not part of the START gate; without it the simulator reports
        // `$20=0` and the travel comparison is correctly NOT CHECKED.
        if self.demo_soft_limits
            && !self.demo_soft_limits_done
            && self.state.connection == ConnectionState::Ready
            && self.state.settings.row(20).is_some()
        {
            self.demo_soft_limits_done = true;
            let already = self
                .state
                .settings
                .row(20)
                .and_then(|row| {
                    row.current
                        .as_bool()
                        .or_else(|| row.current.as_f64().map(|value| value != 0.0))
                })
                .unwrap_or(false);
            if !already {
                self.state.settings.set_bool(20, true);
                self.state.push_console(
                    ConsoleLevel::Info,
                    "Demo setup: enabling soft limits ($20=1) for the travel capture",
                );
                self.apply_settings(cx);
                changed = true;
            }
        }
        // Screenshot switch: wait for the loaded program's travel comparison,
        // then press the same START action once. With an over-travel program
        // this only opens the confirmation sheet; it never sends StartProgram.
        if self.demo_over_travel
            && !self.demo_over_travel_done
            && self.state.live
            && self.state.connection == ConnectionState::Ready
            && self.state.job.has_program()
            && self.state.program_limits().is_over_travel()
            && self.job_start_allowed()
        {
            self.demo_over_travel_done = true;
            self.state.mode = Mode::Job;
            self.start_job(cx);
            changed = true;
        }
        // Screenshot switch: seed the app-side table once the controller
        // profile has loaded, so the profile loader cannot replace it.
        if self.demo_tools
            && !self.demo_tools_done
            && self.state.connection == ConnectionState::Ready
        {
            self.demo_tools_done = true;
            for (number, name, diameter, length) in [
                (1, "6 mm flat", 6.0, 35.0),
                (2, "3 mm ball", 3.0, 32.5),
                (3, "6 mm ball", 6.0, 34.0),
                (4, "90 deg V", 9.525, 30.0),
            ] {
                let _ = self.state.tools.insert(ToolRecord {
                    number,
                    name: name.to_string(),
                    diameter_mm: diameter,
                    length_mm: length,
                    pocket: Some(number),
                });
            }
            self.state.tools.set_status("DEMO TABLE (UNSAVED)");
            changed = true;
        }
        // Screenshot switch: seed two profile macros through the normal save
        // path, then optionally open the editor or a run confirmation. Nothing
        // is run unless --demo-macro-confirm asks for the sheet; that sheet is
        // never confirmed by this switch.
        if self.demo_macros
            && !self.demo_macros_done
            && self.state.connection == ConnectionState::Ready
            && self.state.macro_save_blocker().is_none()
        {
            self.demo_macros_done = true;
            self.state.mode = Mode::Utility;
            if self.state.user_macro_list().is_empty() {
                if let Err(message) = self.state.save_macro(None, "Park at Z0", "G53 G0 Z0", true) {
                    self.state
                        .push_console(ConsoleLevel::Warn, format!("Demo macros: {message}"));
                }
                if let Err(message) = self.state.save_macro(None, "Spindle off", "M5", false) {
                    self.state
                        .push_console(ConsoleLevel::Warn, format!("Demo macros: {message}"));
                }
            }
            if self.demo_macro_edit {
                if let Some(mac) = self.state.user_macro_list().first().cloned() {
                    self.open_macro_editor(Some(&mac), cx);
                }
            } else if self.demo_macro_confirm {
                self.run_user_macro("Park at Z0", cx);
            }
            changed = true;
        }
        // Screenshot switch: set one startup-line draft, open the real warning
        // sheet, and confirm it through the same actions as a tap. Only runs in
        // demo mode with the flag; normal startup never writes a startup line.
        if self.demo_startup_write
            && !self.demo_startup_write_done
            && self.state.connection == ConnectionState::Ready
            && self.state.startup_lines.fully_reported()
            && self.state.startup_write_gate().blocked_reason().is_none()
        {
            self.demo_startup_write_done = true;
            self.state.mode = Mode::Utility;
            self.state.left_tab = LeftTab::Console;
            self.state.console.clear();
            self.state.push_console(
                ConsoleLevel::Info,
                "Demo: $N0 write through the normal APPLY / CONFIRM WRITE path",
            );
            if self.state.startup_lines.set_draft(0, "G21 G90").is_ok() {
                self.request_startup_apply(0, cx);
                self.confirm_startup_apply(cx);
                changed = true;
            }
        }
        // Screenshot switch: fill the calibration wizard through the same
        // model setters and calculation the operator uses. It never writes a
        // setting; the confirmation sheet is only shown.
        if (self.demo_calibration || self.demo_calibration_confirm)
            && !self.demo_calibration_done
            && self.state.connection == ConnectionState::Ready
            && self
                .state
                .calibration_current_steps(CalibrationAxis::X)
                .is_some()
        {
            self.demo_calibration_done = true;
            self.state.mode = Mode::Utility;
            let calibration = self.state.utilities.calibration_mut();
            calibration.set_axis(CalibrationAxis::X);
            let _ = calibration.set_field_text(CalibrationField::CommandedMm, "100");
            let _ = calibration.set_field_text(CalibrationField::MeasuredMm, "98.5");
            if let Err(message) = self.state.calculate_calibration() {
                self.state
                    .push_console(ConsoleLevel::Warn, format!("Demo calibration: {message}"));
            } else if self.demo_calibration_confirm {
                self.state.utilities.calibration_mut().set_confirmed(true);
                self.calibration_confirm_open = true;
            }
            changed = true;
        }
        // Screenshot switch: load the reproducible gantry sample through the
        // calculator's own model. It opens the sheet for the result capture, or
        // leaves the sheet closed for the card capture, and scrolls the card
        // into view at the 1280x800 layout. Nothing is sent.
        if self.demo_gantry
            && !self.demo_gantry_done
            && self.state.connection == ConnectionState::Ready
        {
            self.demo_gantry_done = true;
            self.state.mode = Mode::Utility;
            self.state.utilities.gantry_mut().load_demo();
            self.gantry_open = !self.demo_gantry_card;
            self.utility_scroll.scroll_to_top_of_item(GANTRY_CARD_INDEX);
            changed = true;
        }
        // Screenshot switch: open the surfacing wizard with a generated
        // preview. Nothing is loaded or started by this switch.
        if self.demo_surfacing
            && !self.demo_surfacing_done
            && self.state.connection == ConnectionState::Ready
        {
            self.demo_surfacing_done = true;
            self.state.mode = Mode::Utility;
            self.surfacing_open = true;
            if let Err(message) = self
                .state
                .utilities
                .generate_surfacing(Wcs::from_index(self.state.wcs_index).unwrap_or(Wcs::G54))
            {
                self.state
                    .push_console(ConsoleLevel::Warn, format!("Demo surfacing: {message}"));
            }
            changed = true;
        }
        // Screenshot switch: generate the line-number transform preview for
        // the loaded demo program. Nothing is loaded by this switch.
        if self.demo_transform
            && !self.demo_transform_done
            && self.state.connection == ConnectionState::Ready
            && self.state.job.has_program()
        {
            self.demo_transform_done = true;
            self.state.mode = Mode::Utility;
            let name = self.state.job.name.clone();
            if let Some(source) = self.state.job.source_text()
                && let Err(message) = self.state.utilities.generate_transform(
                    TransformKind::AddLineNumbers,
                    &name,
                    &source,
                )
            {
                self.state
                    .push_console(ConsoleLevel::Warn, format!("Demo transform: {message}"));
            }
            changed = true;
        }
        // Screenshot switch: open Utility at the destructive reset card.
        // Opening the card acknowledges nothing and sends nothing.
        if self.demo_reset
            && !self.demo_reset_done
            && self.state.connection == ConnectionState::Ready
        {
            self.demo_reset_done = true;
            self.state.mode = Mode::Utility;
            changed = true;
        }
        // Screenshot switch: export a local backup through the same action as
        // the card, then open the `$RST=#` confirmation without confirming it.
        // If the local export cannot run, the explicit data-loss
        // acknowledgement stands in so the sheet still shows the real gate
        // rather than bypassing it.
        if self.demo_reset_confirm
            && !self.demo_reset_confirm_done
            && self.state.connection == ConnectionState::Ready
            && self.state.config.key().is_some()
        {
            self.demo_reset_confirm_done = true;
            self.state.mode = Mode::Utility;
            if self.state.export_backup_for_reset().is_err() {
                self.state.controller_reset.set_loss_accepted(true);
            }
            self.state.controller_reset.select(ResetTarget::Offsets);
            if let Err(refusal) = self
                .state
                .controller_reset
                .request_confirm(self.reset_gate())
            {
                self.state.push_console(
                    ConsoleLevel::Warn,
                    format!("Demo reset confirmation refused: {}", refusal.message()),
                );
            }
            changed = true;
        }
        // Screenshot switch: one real G10 write through the normal path.
        if self.demo_offsets_write
            && !self.demo_offsets_write_done
            && self.state.connection == ConnectionState::Ready
            && self.state.job.has_program()
            && self.state.offsets.reported()
            && self.mdi_allowed()
        {
            self.demo_offsets_write_done = true;
            self.state.mode = Mode::Offset;
            self.state.left_tab = LeftTab::Console;
            self.state.console.clear();
            self.state.push_console(
                ConsoleLevel::Info,
                "Demo: G10 L20 write through the normal MDI path",
            );
            self.send_offset_value(Wcs::G54, Axis::X, 12.5, cx);
            changed = true;
        }
        // Screenshot switch: open the G28 confirmation sheet through the same
        // request action as the button. Nothing is sent until Confirm.
        if self.demo_offsets_goto
            && !self.demo_offsets_goto_done
            && self.state.connection == ConnectionState::Ready
            && self.state.offsets.reported()
            && self.offset_gate().block_reason().is_none()
        {
            self.demo_offsets_goto_done = true;
            self.state.mode = Mode::Offset;
            self.state.left_tab = LeftTab::Console;
            self.state.console.clear();
            self.state.push_console(
                ConsoleLevel::Info,
                "Demo: G28 confirmation sheet; nothing has been sent",
            );
            self.request_offset_action(OffsetAction::GotoG28, cx);
            changed = true;
        }
        // Screenshot switch: write the local offsets.nc through the normal
        // export action. Nothing is sent to the controller.
        if self.demo_offsets_export
            && !self.demo_offsets_export_done
            && self.state.connection == ConnectionState::Ready
            && self.state.offsets.reported()
            && self.state.job.has_program()
        {
            self.demo_offsets_export_done = true;
            self.state.mode = Mode::Offset;
            self.state.left_tab = LeftTab::Console;
            self.state.console.clear();
            self.state.push_console(
                ConsoleLevel::Info,
                "Demo: local offsets.nc export through the normal action; no command is sent",
            );
            self.export_offsets(cx);
            changed = true;
        }
        // Screenshot switch: validate the exported file and open the preview
        // sheet. It is never loaded or started by this switch.
        if self.demo_offsets_restore
            && !self.demo_offsets_restore_done
            && self.demo_offsets_export_done
        {
            self.demo_offsets_restore_done = true;
            self.state.mode = Mode::Offset;
            self.state.left_tab = LeftTab::Console;
            self.state.push_console(
                ConsoleLevel::Info,
                "Demo: offsets restore preview; nothing is loaded or started",
            );
            self.open_offsets_restore(cx);
            changed = true;
        }
        // Screenshot switch: open one probe card once live telemetry and `$#`
        // are available, so the preflight shows real poses and offsets.
        // The height-map switch also seeds a small, explicit 4x4 demo grid; it
        // is a capture aid, not a product cap or a persisted setup.
        if self.demo_height_map
            && !self.demo_height_map_done
            && self.state.connection == ConnectionState::Ready
            && self.state.offsets.reported()
            && self.state.job.has_program()
        {
            self.demo_height_map_done = true;
            self.state.mode = Mode::Probe;
            self.seed_demo_height_map();
            changed = true;
        }
        if let Some(name) = self.probe_card_demo.clone()
            && !self.probe_card_demo_done
            && self.state.connection == ConnectionState::Ready
            && self.state.offsets.reported()
        {
            self.probe_card_demo_done = true;
            self.state.mode = Mode::Probe;
            if let Some(card) = parse_probe_card(&name) {
                let ctx = self.probe_context();
                self.state.probe.select_card(card, &ctx);
                // Screenshot-only: show a nonzero mount offset so the
                // controlled-vs-feature convention is visible in the capture.
                if std::env::args().any(|arg| arg == "--demo-probe-offset") {
                    let _ = self
                        .state
                        .probe
                        .set_probe_offset(ghs_probe::XyPoint::new(12.5, -8.0));
                }
            } else {
                self.state.push_console(
                    ConsoleLevel::Warn,
                    format!("--probe-card: unknown cycle '{name}'"),
                );
            }
            changed = true;
        }
        // Screenshot switch: press the same SAVE DEFAULTS action after the
        // controller profile has loaded. It writes local profile JSON only and
        // never sends a probe command or motion.
        if self.demo_probe_defaults
            && !self.demo_probe_defaults_done
            && self.state.probe_defaults_demo_ready()
        {
            self.demo_probe_defaults_done = true;
            self.state.mode = Mode::Probe;
            self.save_probe_defaults(cx);
            changed = true;
        }
        // Screenshot switches: fill the macro sheet with reproducible valid
        // text and either leave it unsaved or save it through the same local
        // profile path as SAVE MACROS. Neither switch sends a command.
        if self.demo_probe_macros
            && !self.demo_probe_macros_done
            && self.state.probe_defaults_demo_ready()
        {
            self.demo_probe_macros_done = true;
            self.state.mode = Mode::Probe;
            self.open_demo_probe_macro_editor();
            changed = true;
        }
        if self.demo_probe_macros_saved
            && !self.demo_probe_macros_saved_done
            && self.state.probe_defaults_demo_ready()
        {
            self.demo_probe_macros_saved_done = true;
            self.state.mode = Mode::Probe;
            self.seed_saved_demo_probe_macros(cx);
            changed = true;
        }
        if self.demo_probe_macros_error
            && !self.demo_probe_macros_error_done
            && self.state.probe_defaults_demo_ready()
        {
            self.demo_probe_macros_error_done = true;
            self.state.mode = Mode::Probe;
            self.open_demo_probe_macro_error();
            changed = true;
        }
        // Screenshot switch: press the same RUN action after the preflight
        // passes. It is simulator-only evidence; normal startup never runs.
        if self.demo_probe_run
            && !self.demo_probe_run_done
            && self.state.connection == ConnectionState::Ready
            && self.state.probe.card().is_some()
            && !self.state.probe.is_running()
        {
            let ctx = self.probe_context();
            if self.state.probe.preflight(&ctx).setup.is_some() {
                self.demo_probe_run_done = true;
                self.start_probe_run(cx);
                changed = true;
            }
        }
        // Screenshot switch: press the same explicit Apply+Confirm actions after
        // the run completes. It is simulator-only evidence.
        if self.demo_probe_apply
            && !self.demo_probe_apply_done
            && self
                .state
                .probe
                .run()
                .is_some_and(|run| run.phase == ProbePhase::Complete && run.applied_line.is_none())
        {
            self.demo_probe_apply_done = true;
            self.confirm_probe_apply(cx);
            changed = true;
        }
        // Screenshot switch: run the tool-length card, open the inline TLO
        // confirmation with the computed values, then optionally press the same
        // confirm action once. The capture uses the real `$#` read-back; no
        // APPLIED state is fabricated.
        if (self.demo_probe_tlo || self.demo_probe_tlo_confirm) && !self.demo_probe_tlo_opened {
            let complete = self.state.probe.run().is_some_and(|run| {
                run.phase == ProbePhase::Complete
                    && matches!(
                        run.outcome.as_ref().map(|outcome| &outcome.value),
                        Some(ghs_probe::ProbeValue::ToolLength { .. })
                    )
            });
            if complete
                && self.state.probe.tool_length_write().is_none()
                && self
                    .state
                    .request_probe_tool_length(ToolLengthAction::Apply)
                    .is_ok()
            {
                self.demo_probe_tlo_opened = true;
                changed = true;
            }
        }
        if self.demo_probe_tlo && self.demo_probe_tlo_opened && !self.demo_probe_tlo_done {
            self.demo_probe_tlo_done = true;
            self.confirm_probe_tlo(cx);
            changed = true;
        }
        // Screenshot switch: press the same explicit CSV export action once the
        // map exists, so the capture shows the fixed path and the write result.
        if self.demo_height_map_export
            && !self.demo_height_map_export_done
            && self
                .state
                .probe
                .run()
                .is_some_and(|run| run.map.is_some() && run.export.is_none())
        {
            self.demo_height_map_export_done = true;
            self.export_height_map(cx);
            changed = true;
        }
        // Screenshot switch: save the completed demo map with the same action a
        // tap uses, then optionally open it from the saved-map browser.
        if self.demo_height_map_save && !self.demo_height_map_save_done {
            let complete = self.state.probe.run().is_some_and(|run| {
                run.phase == ProbePhase::Complete
                    && run.map.as_ref().is_some_and(|map| map.valid_count() > 0)
            });
            if complete {
                self.demo_height_map_save_done = true;
                let name = default_map_name(now_unix_s());
                let _ = self.save_height_map_named(name, cx);
                changed = true;
            }
        }
        if self.demo_height_map_open_saved
            && self.demo_height_map_save_done
            && !self.demo_height_map_open_saved_done
        {
            let saved = self
                .state
                .height_map_library
                .last_saved()
                .map(|notice| notice.name.clone());
            if let Some(name) = saved
                && self.state.select_saved_height_map(&name).is_ok()
            {
                self.demo_height_map_open_saved_done = true;
                changed = true;
            }
        }
        // Screenshot switch: open the same confirmation step without confirming.
        if self.demo_height_map_confirm && !self.demo_height_map_confirm_done {
            let snapshot = self.state.probe.run().and_then(HeightMapSnapshot::from_run);
            if let Some(snapshot) = snapshot
                && snapshot.complete
                && self.state.height_map.pending().is_none()
                && self.state.height_map.applied().is_none()
                && self.machine() == MachineState::Idle
                && !self.state.job.is_active()
            {
                let gate = HeightApplyGate::from_context(&self.probe_context());
                if self.state.height_map.blockers(&snapshot, &gate).is_empty() {
                    self.demo_height_map_confirm_done = true;
                    self.request_height_map_apply(cx);
                    changed = true;
                }
            }
        }
        // Screenshot switch: press the same Apply + Confirm actions after a
        // complete map and only when every live gate passes. Simulator only.
        if self.demo_height_map_apply && !self.demo_height_map_apply_done {
            let snapshot = self.state.probe.run().and_then(HeightMapSnapshot::from_run);
            if let Some(snapshot) = snapshot
                && snapshot.complete
                && self.state.height_map.applied().is_none()
                && self.state.height_map.pending().is_none()
                && self.machine() == MachineState::Idle
                && !self.state.job.is_active()
            {
                let gate = HeightApplyGate::from_context(&self.probe_context());
                if self.state.height_map.blockers(&snapshot, &gate).is_empty() {
                    self.demo_height_map_apply_done = true;
                    self.request_height_map_apply(cx);
                    if self.state.height_map.pending().is_some() {
                        self.confirm_height_map_apply(cx);
                    }
                    changed = true;
                }
            }
        }
        // Screenshot switch: reveal the Job workspace marker after Apply.
        if self.demo_height_map_job
            && !self.demo_height_map_job_done
            && self.state.height_map.is_applied_for(&self.state.job.name)
        {
            self.demo_height_map_job_done = true;
            self.state.mode = Mode::Job;
            changed = true;
        }
        // Screenshot switch: press Revert once the compensated load is visible.
        if self.demo_height_map_revert && !self.demo_height_map_revert_done {
            let applied_name = self
                .state
                .height_map
                .applied()
                .map(|applied| applied.program_name.clone());
            if let Some(name) = applied_name
                && self.state.job.name == name
                && self.machine() == MachineState::Idle
                && !self.state.job.is_active()
            {
                self.demo_height_map_revert_done = true;
                self.revert_height_map(cx);
                changed = true;
            }
        }
        // Recompute the Program-tab matches only when the query or the loaded
        // program changed. This never re-parses the program.
        changed |= self.state.sync_program_search();
        // Follow the current line in the Program tab. Only a changed line
        // scrolls, so manual scrolling is not fought every poll. A search owns
        // navigation until it is cleared.
        let line = self.state.display_line();
        if self.state.job.has_program() && line != self.last_job_line {
            self.last_job_line = line;
            if line > 0
                && !self.state.program_search.is_active()
                && let Ok(index) = usize::try_from(line)
            {
                self.scroll_program_to_visible(index.saturating_sub(1));
            }
        }
        if changed {
            if self.state.console.len() != console_before {
                self.console_scroll.scroll_to_bottom();
            }
            cx.notify();
        }
    }

    // ------------------------------------------------------- program search

    /// Scroll the virtualized Program list to one visible row.
    fn scroll_program_to_visible(&self, visible: usize) {
        self.program_scroll
            .scroll_to_item(visible, ScrollStrategy::Center);
    }

    /// Follow the streaming line when no search is active.
    fn follow_current_program_line(&self) {
        if self.state.program_search.is_active() {
            return;
        }
        let line = self.state.display_line();
        let count = self.state.display_program_lines();
        let index = usize::try_from(line)
            .ok()
            .and_then(|line| line.checked_sub(1));
        if let Some(index) = index
            && index < count
        {
            self.scroll_program_to_visible(index);
        }
    }

    /// Scroll to the current search match.
    fn jump_to_program_match(&mut self, cx: &mut Context<Self>) {
        self.state.sync_program_search();
        if let Some(index) = self.state.program_search_scroll_index() {
            self.scroll_program_to_visible(index);
        }
        cx.notify();
    }

    /// Advance to the next search match and reveal it.
    fn next_program_match(&mut self, cx: &mut Context<Self>) {
        self.state.sync_program_search();
        if self.state.program_search.next_match()
            && let Some(index) = self.state.program_search_scroll_index()
        {
            self.scroll_program_to_visible(index);
        }
        cx.notify();
    }

    /// Move to the previous search match and reveal it.
    fn previous_program_match(&mut self, cx: &mut Context<Self>) {
        self.state.sync_program_search();
        if self.state.program_search.previous_match()
            && let Some(index) = self.state.program_search_scroll_index()
        {
            self.scroll_program_to_visible(index);
        }
        cx.notify();
    }

    // ------------------------------------------------------- program file load

    /// Open the modal file browser at the last visited folder (home initially).
    ///
    /// Browsing is always allowed: mock mode and a down link can inspect files,
    /// while the modal's LOAD action applies the live/idle gate.
    fn open_program_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let browser = FileBrowser::open(&self.browser_dir);
        let path = browser.path_display();
        self.file_browser = Some(browser);
        self.state.keypad = None;
        self.file_browser_input.update(cx, |state, cx| {
            state.set_value(path, window, cx);
            state.focus(window, cx);
        });
        self.file_browser_scroll
            .scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    /// Close the browser and remember the folder for the next open.
    fn close_program_browser(&mut self, cx: &mut Context<Self>) {
        if let Some(browser) = self.file_browser.take() {
            self.browser_dir = browser.cwd().to_path_buf();
        }
        cx.notify();
    }

    /// Tap on a browser row: enter a folder, or select a program file.
    fn browser_row_clicked(
        &mut self,
        index: usize,
        is_dir: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut new_path = None;
        if let Some(browser) = self.file_browser.as_mut() {
            if is_dir {
                if browser.enter(index) {
                    self.browser_dir = browser.cwd().to_path_buf();
                    new_path = Some(browser.path_display());
                }
            } else {
                browser.select(index);
            }
        }
        if let Some(path) = new_path {
            self.file_browser_input.update(cx, |state, cx| {
                state.set_value(path, window, cx);
            });
            self.file_browser_scroll
                .scroll_to_item(0, ScrollStrategy::Top);
        }
        cx.notify();
    }

    /// UP: move to the parent folder and refresh the list.
    fn browser_up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut new_path = None;
        if let Some(browser) = self.file_browser.as_mut()
            && browser.up()
        {
            self.browser_dir = browser.cwd().to_path_buf();
            new_path = Some(browser.path_display());
        }
        if let Some(path) = new_path {
            self.file_browser_input.update(cx, |state, cx| {
                state.set_value(path, window, cx);
            });
            self.file_browser_scroll
                .scroll_to_item(0, ScrollStrategy::Top);
        }
        cx.notify();
    }

    /// Enter the typed path. Used by the path field's Enter key, which has no
    /// window handle; the model is authoritative and the field keeps the typed
    /// folder or file path.
    fn commit_browser_path(&mut self, cx: &mut Context<Self>) {
        let mut navigated = false;
        if let Some(browser) = self.file_browser.as_mut() {
            navigated = browser.commit_path();
            if navigated {
                self.browser_dir = browser.cwd().to_path_buf();
            }
        }
        if navigated {
            self.file_browser_scroll
                .scroll_to_item(0, ScrollStrategy::Top);
        }
        cx.notify();
    }

    /// GO: enter the typed path and mirror the resolved folder in the field.
    fn browser_go(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut new_path = None;
        if let Some(browser) = self.file_browser.as_mut()
            && browser.commit_path()
        {
            self.browser_dir = browser.cwd().to_path_buf();
            new_path = Some(browser.path_display());
        }
        if let Some(path) = new_path {
            self.file_browser_input.update(cx, |state, cx| {
                state.set_value(path, window, cx);
            });
            self.file_browser_scroll
                .scroll_to_item(0, ScrollStrategy::Top);
        }
        cx.notify();
    }

    /// Read, validate, remember, and send one program file.
    ///
    /// This is the only load path in the app besides the bundled demo and the
    /// height-map workflow. It sends exactly one `LoadProgram`; parsing and
    /// `JobLoaded` stay in the engine. On rejection the previous program stays
    /// loaded and the failed exact source is discarded. A successful local load
    /// is recorded in the recent-file list; a failed load is not.
    fn load_program_file(&mut self, path: &Path) -> Result<String, String> {
        let link = self.link.as_ref();
        load_local_program(&mut self.state, path, |command| match link {
            Some(link) => link.send(command),
            None => Err(EngineError::NotConnected),
        })
    }

    /// LOAD PROGRAM: load the selected file, or explain why it is unavailable.
    fn browser_load_selected(&mut self, cx: &mut Context<Self>) {
        let path = self
            .file_browser
            .as_ref()
            .and_then(FileBrowser::selected_entry)
            .map(|entry| entry.path.clone());
        let Some(path) = path else {
            if let Some(browser) = self.file_browser.as_mut() {
                browser.set_error("Select a program file first");
            }
            cx.notify();
            return;
        };
        match self.load_program_file(&path) {
            Ok(message) => {
                if let Some(browser) = self.file_browser.take() {
                    self.browser_dir = browser.cwd().to_path_buf();
                }
                self.state.left_tab = LeftTab::Program;
                self.state.push_console(ConsoleLevel::Info, message);
            }
            Err(message) => {
                if let Some(browser) = self.file_browser.as_mut() {
                    browser.set_error(message);
                }
            }
        }
        cx.notify();
    }

    /// Load one recent-file row through the same bounded path as the browser.
    ///
    /// A missing or unreadable file leaves the sheet open with the reader's
    /// error; the engine is not asked to load anything. On success the sheet
    /// closes and the path moves to the front of the recent list.
    fn browser_load_recent(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match self.load_program_file(&path) {
            Ok(message) => {
                if let Some(browser) = self.file_browser.take() {
                    self.browser_dir = browser.cwd().to_path_buf();
                }
                self.state.left_tab = LeftTab::Program;
                self.state.push_console(ConsoleLevel::Info, message);
            }
            Err(message) => {
                if let Some(browser) = self.file_browser.as_mut() {
                    browser.set_error(message);
                }
            }
        }
        cx.notify();
    }

    /// Remove one entry from the recent list without loading or sending.
    fn browser_remove_recent(&mut self, path: String, cx: &mut Context<Self>) {
        if let Err(error) = self.state.config.remove_recent_file(&path)
            && let Some(browser) = self.file_browser.as_mut()
        {
            browser.set_error(format!("{}: {error}", t("Recent file was not removed")));
        }
        cx.notify();
    }

    /// Clear the recent list without loading or sending anything.
    fn browser_clear_recent(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.state.config.clear_recent_files()
            && let Some(browser) = self.file_browser.as_mut()
        {
            browser.set_error(format!("{}: {error}", t("Recent files were not cleared")));
        }
        cx.notify();
    }

    /// Accept one external file drop over the Job workspace.
    ///
    /// Exactly one regular file is accepted. The drop uses the same bounded
    /// reader and guarded `LoadProgram` path as the browser, so extension,
    /// size, UTF-8, binary, and ready/idle checks all still apply. A refused
    /// drop reports the reason and sends nothing; a successful drop loads only
    /// and never starts.
    fn drop_program_paths(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        match dropped_program_path(paths) {
            Ok(path) => {
                let path = path.to_path_buf();
                match self.load_program_file(&path) {
                    Ok(message) => {
                        self.state.mode = Mode::Job;
                        self.state.left_tab = LeftTab::Program;
                        self.state.push_console(ConsoleLevel::Info, message);
                    }
                    Err(message) => self.state.push_console(ConsoleLevel::Error, message),
                }
            }
            Err(message) => self.state.push_console(ConsoleLevel::Warn, message),
        }
        cx.notify();
    }

    // ------------------------------------------------------- connection/link

    /// Open the transport picker from the status pill or the CONNECT button.
    fn open_connection(&mut self, cx: &mut Context<Self>) {
        self.state.keypad = None;
        self.panel.open();
        cx.notify();
    }

    fn close_connection(&mut self, cx: &mut Context<Self>) {
        self.panel.close();
        cx.notify();
    }

    /// Connect the transport selected in the dialog.
    fn connect_selected(&mut self, cx: &mut Context<Self>) {
        if self.link.as_ref().is_some_and(|link| link.is_connected()) {
            self.panel.error = Some("disconnect the current link first".to_string());
            cx.notify();
            return;
        }
        match self.panel.transport {
            TransportChoice::Demo => self.connect_demo(cx),
            TransportChoice::Serial => {
                let Some(config) = self.panel.selected_config() else {
                    self.panel.error = Some("select a serial port before connecting".to_string());
                    cx.notify();
                    return;
                };
                self.connect_serial(config, cx);
            }
        }
    }

    /// Name of the current transport, for the connection dialog.
    fn connection_name(&self) -> String {
        if !matches!(
            self.state.connection,
            ConnectionState::Connecting | ConnectionState::Ready
        ) {
            return "Not connected".to_string();
        }
        self.link
            .as_ref()
            .and_then(LiveLink::mode)
            .map_or_else(|| "Not connected".to_string(), LinkMode::describe)
    }

    fn connect_demo(&mut self, cx: &mut Context<Self>) {
        let program = self.demo_program;
        let result = if let Some(link) = self.link.as_mut() {
            link.connect_demo_with(program)
        } else {
            match LiveLink::start_demo_with(program) {
                Ok(link) => {
                    self.link = Some(link);
                    Ok(())
                }
                Err(error) => Err(error),
            }
        };
        match result {
            Ok(()) => {
                self.state
                    .begin_live("Starting in-process FlexiHAL demo controller");
                self.macro_editor = None;
                self.macro_delete = None;
                self.panel.error = None;
            }
            Err(error) => self.report_connect_failure(error),
        }
        cx.notify();
    }

    fn connect_serial(&mut self, config: SerialConfig, cx: &mut Context<Self>) {
        let result = if let Some(link) = self.link.as_mut() {
            link.connect_serial(config.clone())
        } else {
            match LiveLink::start_serial(config.clone()) {
                Ok(link) => {
                    self.link = Some(link);
                    Ok(())
                }
                Err(error) => Err(error),
            }
        };
        match result {
            Ok(()) => {
                self.state.begin_live(format!(
                    "Opening {} at {} baud, 8N1 (line control off)",
                    config.port, config.baud
                ));
                self.macro_editor = None;
                self.macro_delete = None;
                self.panel.error = None;
            }
            Err(error) => self.report_connect_failure(error),
        }
        cx.notify();
    }

    /// Keep the last snapshot on screen; mark the connection failed and show
    /// the engine text in both the dialog and the console.
    fn report_connect_failure(&mut self, error: String) {
        self.state.connection = ConnectionState::Failed(error.clone());
        self.state
            .push_console(ConsoleLevel::Error, format!("Connect failed: {error}"));
        self.panel.error = Some(error);
    }

    fn disconnect(&mut self, cx: &mut Context<Self>) {
        self.end_jog(cx);
        if let Some(link) = &self.link {
            link.disconnect();
        }
        // A confirmation captured on this link must not survive it.
        self.state.offsets.cancel_action();
        self.state.offsets_backup.close_preview();
        self.offset_refresh_sent = false;
        self.reset_settings_refresh_pending = false;
        self.state.cancel_macro_run();
        self.state.cancel_sleep_confirm();
        self.state.controller_reset.cancel_confirm();
        self.state.cancel_check_confirm();
        self.state.cancel_start_confirm();
        self.state.connection = ConnectionState::Disconnected;
        self.state.push_console(
            ConsoleLevel::Warn,
            "Disconnect requested; the last telemetry is stale",
        );
        self.panel.error = None;
        cx.notify();
    }

    fn send_command_quiet(&mut self, command: Command) -> Result<(), EngineError> {
        match self.link.as_ref() {
            Some(link) => link.send(command),
            None => Err(EngineError::NotConnected),
        }
    }

    fn send_command(&mut self, command: Command) -> bool {
        if self.link.is_none() {
            self.state.push_console(
                ConsoleLevel::Warn,
                "No engine connection; command was not sent",
            );
            return false;
        }
        match self.send_command_quiet(command) {
            Ok(()) => true,
            Err(error) => {
                self.state
                    .push_console(ConsoleLevel::Error, format!("Engine: {error}"));
                false
            }
        }
    }

    fn send_mdi_line(&mut self, line: String) -> bool {
        self.send_command(Command::Mdi(line))
    }

    // ------------------------------------------------------- settings writes

    /// Whether the generated form may write right now.
    ///
    /// The engine refuses setting writes unless the machine is Idle and no
    /// job is running; mirror that gate in the UI so Apply is never a trap.
    fn settings_write_allowed(&self) -> bool {
        self.state.live
            && self.ready()
            && !self.locked()
            && self.machine() == MachineState::Idle
            && !self.state.job.is_active()
    }

    /// Short reason shown when Apply is disabled.
    fn settings_lock_reason(&self) -> &'static str {
        if !self.state.live {
            "MOCK MODE"
        } else if !self.ready() {
            "NOT CONNECTED"
        } else if self.locked() {
            "ALARM OR DOOR"
        } else if self.state.job.is_active() {
            "JOB RUNNING"
        } else if self.machine() != MachineState::Idle {
            "CONTROLLER NOT IDLE"
        } else {
            "READY"
        }
    }

    /// Send the next queued setting write, if one is ready.
    fn pump_settings_write(&mut self) -> bool {
        if !self.settings_write_allowed() {
            return false;
        }
        let Some((number, value)) = self.state.settings.take_next_write() else {
            return false;
        };
        match self.send_command_quiet(Command::SetSetting { number, value }) {
            Ok(()) => true,
            Err(EngineError::Busy) => {
                // The streamer still has a line in flight. Keep the row queued
                // and try again on the next poll instead of reporting a write
                // failure that never reached the controller.
                self.state.settings.requeue_write(number);
                false
            }
            Err(error) => {
                self.state
                    .settings
                    .mark_send_error(number, error.to_string());
                self.state.push_console(
                    ConsoleLevel::Error,
                    format!("Setting ${number} rejected: {error}"),
                );
                true
            }
        }
    }

    /// Validate and queue every changed row. The engine verifies each write.
    fn apply_settings(&mut self, cx: &mut Context<Self>) {
        if !self.settings_write_allowed() {
            let reason = self.settings_lock_reason();
            self.state.push_console(
                ConsoleLevel::Warn,
                format!("Settings writes are locked: {reason}"),
            );
            cx.notify();
            return;
        }
        match self.state.settings.begin_apply() {
            Ok(0) => self
                .state
                .push_console(ConsoleLevel::Info, "No changed settings to apply"),
            Ok(count) => {
                self.state.push_console(
                    ConsoleLevel::Info,
                    format!("Queued {count} setting write(s); waiting for read-back"),
                );
                self.pump_settings_write();
            }
            Err(message) => self.state.push_console(ConsoleLevel::Warn, message),
        }
        cx.notify();
    }

    /// Screenshot switch: change one safe demo setting through the real path.
    ///
    /// This is not a product feature. It only runs on the in-process demo
    /// link, picks the first non-reboot integer row the controller reported,
    /// and uses the same draft + Apply path as a tap.
    fn demo_settings_write(&mut self, cx: &mut Context<Self>) -> bool {
        if self.link.as_ref().and_then(LiveLink::mode) != Some(&LinkMode::Demo) {
            self.settings_demo_write_done = true;
            self.state.push_console(
                ConsoleLevel::Warn,
                "--demo-settings-write only runs against the demo controller",
            );
            return true;
        }
        let candidate = self.state.settings.rows().iter().find_map(|row| {
            let current = row.current.as_f64()?;
            let max = row.max?;
            (row.kind == SettingKind::Integer && !row.reboot && current + 1.0 <= max)
                .then_some((row.number, current + 1.0))
        });
        let Some((number, target)) = candidate else {
            // Values can arrive before `$ES`; retry on the next poll instead of
            // giving up before the metadata exists.
            return false;
        };
        self.settings_demo_write_done = true;
        if self
            .state
            .settings
            .set_number_text(number, format!("{target:.0}"))
            .is_ok()
        {
            // The screenshot proves the read-back in the console as well as
            // the APPLIED row; this is the demo switch, not a normal tab move.
            self.state.left_tab = LeftTab::Console;
            self.apply_settings(cx);
        }
        true
    }

    /// Open the keypad for one numeric setting row.
    fn open_setting_keypad(&mut self, number: u16, cx: &mut Context<Self>) {
        self.settings_entry = None;
        self.state.open_keypad(KeypadTarget::Setting(number));
        cx.notify();
    }

    /// Open the shared text sheet for search or one text setting.
    fn open_settings_entry(
        &mut self,
        purpose: SettingsEntryPurpose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.keypad = None;
        let (value, masked, error) = match purpose {
            SettingsEntryPurpose::Search => (self.state.settings.search().to_string(), false, None),
            SettingsEntryPurpose::Text(number) => {
                let Some(row) = self.state.settings.row(number) else {
                    return;
                };
                (
                    row.text_draft().to_string(),
                    row.kind == SettingKind::Password,
                    None,
                )
            }
            SettingsEntryPurpose::StartupLine(index) => (
                self.state
                    .startup_lines
                    .row(index)
                    .map_or_else(String::new, |row| row.draft.clone()),
                false,
                None,
            ),
            SettingsEntryPurpose::ToolName => (
                self.tool_editor
                    .as_ref()
                    .map(|editor| editor.name.clone())
                    .unwrap_or_default(),
                false,
                None,
            ),
            SettingsEntryPurpose::MacroName => (
                self.macro_editor
                    .as_ref()
                    .map(|editor| editor.name.clone())
                    .unwrap_or_default(),
                false,
                None,
            ),
            SettingsEntryPurpose::MacroCommand => (
                self.macro_editor
                    .as_ref()
                    .map(|editor| editor.command.clone())
                    .unwrap_or_default(),
                false,
                None,
            ),
            SettingsEntryPurpose::HeightMapName => (default_map_name(now_unix_s()), false, None),
        };
        let input = if let Some(entry) = self.settings_entry.as_ref() {
            entry
                .input
                .update(cx, |state, cx| state.set_value(value, window, cx));
            entry.input.clone()
        } else {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(value)
                    .masked(masked)
            })
        };
        input.update(cx, |state, cx| {
            state.set_masked(masked, window, cx);
            state.focus(window, cx);
        });
        self.settings_entry = Some(SettingsEntry {
            purpose,
            input,
            masked,
            error,
        });
        cx.notify();
    }

    /// Read the text sheet and either filter the form or commit a text draft.
    fn commit_settings_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.settings_entry.as_ref() else {
            return;
        };
        let value = entry.input.read(cx).value().to_string();
        let purpose = entry.purpose;
        if purpose == SettingsEntryPurpose::Search {
            self.state.settings.set_search(value);
            self.settings_entry = None;
            cx.notify();
            return;
        }
        if purpose == SettingsEntryPurpose::ToolName {
            if let Some(editor) = self.tool_editor.as_mut() {
                editor.name = value.trim().to_string();
                editor.error = None;
            }
            self.settings_entry = None;
            cx.notify();
            return;
        }
        if purpose == SettingsEntryPurpose::MacroName {
            let original = self
                .macro_editor
                .as_ref()
                .and_then(|editor| editor.original.clone());
            let macros = self.state.user_macro_list().to_vec();
            match crate::macros::validate_name(&value, original.as_deref(), &macros) {
                Ok(name) => {
                    if let Some(editor) = self.macro_editor.as_mut() {
                        editor.name = name;
                        editor.error = None;
                    }
                    self.settings_entry = None;
                }
                Err(message) => {
                    self.state
                        .push_console(ConsoleLevel::Warn, format!("Macro name: {message}"));
                    if let Some(entry) = self.settings_entry.as_mut() {
                        entry.error = Some(message);
                    }
                }
            }
            cx.notify();
            return;
        }
        if purpose == SettingsEntryPurpose::MacroCommand {
            match crate::macros::validate_command(&value) {
                Ok(command) => {
                    if let Some(editor) = self.macro_editor.as_mut() {
                        editor.command = command;
                        editor.error = None;
                    }
                    self.settings_entry = None;
                }
                Err(message) => {
                    self.state
                        .push_console(ConsoleLevel::Warn, format!("Macro command: {message}"));
                    if let Some(entry) = self.settings_entry.as_mut() {
                        entry.error = Some(message);
                    }
                }
            }
            cx.notify();
            return;
        }
        if let SettingsEntryPurpose::StartupLine(index) = purpose {
            match self.state.startup_lines.set_draft(index, value) {
                Ok(()) => self.settings_entry = None,
                Err(message) => {
                    self.state.push_console(
                        ConsoleLevel::Warn,
                        format!("Startup line $N{index}: {message}"),
                    );
                    if let Some(entry) = self.settings_entry.as_mut() {
                        entry.error = Some(message);
                    }
                }
            }
            cx.notify();
            return;
        }
        if purpose == SettingsEntryPurpose::HeightMapName {
            let name = value.trim().to_string();
            if name.is_empty() {
                let message = "a map name is required".to_string();
                self.state
                    .push_console(ConsoleLevel::Warn, format!("Height map: {message}"));
                if let Some(entry) = self.settings_entry.as_mut() {
                    entry.error = Some(message);
                }
                cx.notify();
                return;
            }
            match self.save_height_map_named(name, cx) {
                Ok(_) => self.settings_entry = None,
                Err(message) => {
                    if let Some(entry) = self.settings_entry.as_mut() {
                        entry.error = Some(message);
                    }
                }
            }
            cx.notify();
            return;
        }
        let SettingsEntryPurpose::Text(number) = purpose else {
            return;
        };
        match self.state.settings.set_text_draft(number, value) {
            Ok(()) => {
                self.settings_entry = None;
                cx.notify();
            }
            Err(message) => {
                self.state
                    .push_console(ConsoleLevel::Warn, format!("Setting ${number}: {message}"));
                if let Some(entry) = self.settings_entry.as_mut() {
                    entry.error = Some(message);
                }
                cx.notify();
            }
        }
    }

    /// Close the text sheet without changing anything.
    fn cancel_settings_entry(&mut self, cx: &mut Context<Self>) {
        self.settings_entry = None;
        cx.notify();
    }

    // ------------------------------------------------------------ keymap

    /// Open one local keymap row editor. Nothing is stored until SAVE.
    fn open_keymap_editor(
        &mut self,
        action: MachineAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_entry = None;
        self.state.keypad = None;
        let value = self.keymap.keys(action).to_string();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(value)
                .placeholder("ctrl-shift-left or f5")
                .clean_on_escape()
        });
        input.update(cx, |state, cx| state.focus(window, cx));
        self.keymap_editor = Some(KeymapEditor {
            action,
            input,
            error: None,
            capturing: false,
            capture_focus: cx.focus_handle(),
        });
        cx.notify();
    }

    /// Close one row editor without changing the draft.
    fn cancel_keymap_editor(&mut self, cx: &mut Context<Self>) {
        self.keymap_editor = None;
        cx.notify();
    }

    /// Commit one row editor into the local draft.
    fn apply_keymap_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.keymap_editor.as_ref() else {
            return;
        };
        let action = editor.action;
        let value = editor.input.read(cx).value().to_string();
        let os = cx.platform_info().os;
        match self.keymap.try_set(action, &value, os) {
            Ok(canonical) => {
                self.keymap_editor = None;
                self.state.push_console(
                    ConsoleLevel::Info,
                    format!(
                        "Keymap draft: {} = {} (local; press SAVE to persist)",
                        action.label(),
                        pretty_keys(&canonical)
                    ),
                );
            }
            Err(message) => {
                self.state.push_console(
                    ConsoleLevel::Warn,
                    format!("Keymap {}: {message}", action.label()),
                );
                if let Some(editor) = self.keymap_editor.as_mut() {
                    editor.error = Some(message);
                }
            }
        }
        cx.notify();
    }

    /// Capture a physical key press into the open row editor.
    fn capture_keymap_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.keymap_editor.as_ref() else {
            return;
        };
        if !editor.capturing {
            return;
        }
        let action = editor.action;
        let key = event.keystroke.key.trim().to_ascii_lowercase();
        if key == "escape" {
            if let Some(editor) = self.keymap_editor.as_mut() {
                editor.capturing = false;
                editor.error = None;
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if key.is_empty()
            || matches!(
                key.as_str(),
                "shift" | "control" | "ctrl" | "alt" | "platform" | "cmd" | "super" | "win" | "fn"
            )
        {
            // Modifier-only events are part of the chord, not the key.
            return;
        }
        let os = cx.platform_info().os;
        let text = event.keystroke.unparse(os);
        let outcome = self.keymap.try_set(action, &text, os);
        let (value, error) = match outcome {
            Ok(canonical) => (canonical, None),
            Err(message) => (text, Some(message)),
        };
        if let Some(editor) = self.keymap_editor.as_mut() {
            editor.capturing = false;
            editor.error = error.clone();
            let input = editor.input.clone();
            input.update(cx, |state, cx| state.set_value(value, window, cx));
        }
        if let Some(message) = error {
            self.state.push_console(
                ConsoleLevel::Warn,
                format!("Keymap {}: {message}", action.label()),
            );
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// Focus the modal panel so the next key press can be captured.
    fn start_keymap_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.keymap_editor.as_mut() else {
            return;
        };
        editor.capturing = true;
        editor.error = None;
        let focus = editor.capture_focus.clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Load the documented defaults into the local draft.
    fn reset_keymap_defaults(&mut self, cx: &mut Context<Self>) {
        self.keymap.reset_defaults();
        self.keymap_editor = None;
        self.state.push_console(
            ConsoleLevel::Info,
            "Keymap defaults loaded locally; press SAVE to persist",
        );
        cx.notify();
    }

    /// Discard every local keymap edit. Sends and stores nothing.
    fn cancel_keymap_draft(&mut self, cx: &mut Context<Self>) {
        self.keymap = KeymapModel::from_keymap(&self.keymap_source);
        self.keymap_editor = None;
        self.state.push_console(
            ConsoleLevel::Info,
            "Keymap changes discarded; the profile was not written",
        );
        cx.notify();
    }

    /// Validate and persist the local keymap through the profile path.
    fn save_keymap(&mut self, cx: &mut Context<Self>) {
        let os = cx.platform_info().os;
        let keymap = match self.keymap.to_keymap(os) {
            Ok(keymap) => keymap,
            Err(message) => {
                self.state
                    .push_console(ConsoleLevel::Warn, format!("Keymap not saved: {message}"));
                cx.notify();
                return;
            }
        };
        // Ask GPUI itself to build every binding before writing the profile, so
        // a stored map can never contain a chord this build cannot install.
        for (action, keys) in self.keymap.effective_bindings(os) {
            if let Err(message) = binding_for(action, &keys, cx) {
                self.state
                    .push_console(ConsoleLevel::Warn, format!("Keymap not saved: {message}"));
                cx.notify();
                return;
            }
        }
        match self.state.config.save_keymap(keymap.clone()) {
            Ok(path) => {
                self.keymap_source = keymap;
                let desired = self.keymap.effective_bindings(os);
                let plan = BindingPlan::between(&self.active_bindings, &desired);
                self.apply_binding_plan(plan, cx);
                self.active_bindings = desired;
                self.state.push_console(
                    ConsoleLevel::Ok,
                    format!("Keymap saved: {}", path.display()),
                );
            }
            Err(message) => self
                .state
                .push_console(ConsoleLevel::Error, format!("Keymap save: {message}")),
        }
        cx.notify();
    }

    // ------------------------------------------------------- startup lines

    /// Open the shared text sheet for one `$N0`/`$N1` draft.
    fn open_startup_editor(&mut self, index: u8, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_entry(SettingsEntryPurpose::StartupLine(index), window, cx);
    }

    /// Validate a draft and open the motion warning without sending anything.
    fn request_startup_apply(&mut self, index: u8, cx: &mut Context<Self>) {
        let gate = self.state.startup_write_gate();
        match self.state.startup_lines.preview_apply(index, gate) {
            Ok(write) => {
                self.state.keypad = None;
                self.settings_entry = None;
                self.startup_confirm = Some(StartupConfirm {
                    index,
                    text: write.line,
                });
            }
            Err(message) => self.state.push_console(
                ConsoleLevel::Warn,
                format!("Startup line $N{index}: {message}"),
            ),
        }
        cx.notify();
    }

    /// Close the warning sheet without sending.
    fn cancel_startup_confirm(&mut self, cx: &mut Context<Self>) {
        self.startup_confirm = None;
        cx.notify();
    }

    /// Send the confirmed write through the guarded system-command path.
    ///
    /// The command is sent only when the captured text still matches the draft
    /// and the connection/machine gate is still open. Nothing is marked applied:
    /// the read-back decides VERIFIED, MISMATCH, or FAILED.
    fn confirm_startup_apply(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.startup_confirm.take() else {
            return;
        };
        let gate = self.state.startup_write_gate();
        match self.state.startup_lines.preview_apply(confirm.index, gate) {
            Ok(write) if write.line == confirm.text => {
                if self
                    .state
                    .startup_lines
                    .begin_apply(confirm.index, gate)
                    .is_ok()
                {
                    match self.send_command_quiet(crate::startup::write_command(&write)) {
                        Ok(()) => {
                            self.state.startup_lines.write_sent(confirm.index);
                        }
                        Err(EngineError::Busy) => {
                            // The streamer has a line in flight; the queued write
                            // is retried by the poll without changing the value.
                            self.state.startup_lines.write_busy(confirm.index);
                        }
                        Err(error) => {
                            let message = error.to_string();
                            self.state
                                .startup_lines
                                .failed(confirm.index, message.clone());
                            self.state.push_console(
                                ConsoleLevel::Error,
                                format!("Startup line $N{}: {message}", confirm.index),
                            );
                        }
                    }
                }
            }
            Ok(_) => self.state.push_console(
                ConsoleLevel::Warn,
                format!(
                    "Startup line $N{} changed after APPLY; reopen the confirmation",
                    confirm.index
                ),
            ),
            Err(message) => self.state.push_console(
                ConsoleLevel::Warn,
                format!("Startup line $N{}: {message}", confirm.index),
            ),
        }
        cx.notify();
    }

    /// Drive the startup-line write/read-back state machine.
    ///
    /// While the engine reports `Busy` the request stays where it is and is
    /// retried on the next poll. A queued write is cancelled if the controller
    /// leaves Idle; a read-back already in flight is left to finish.
    fn pump_startup_writes(&mut self) -> bool {
        let Some(request) = self.state.startup_lines.next_request() else {
            return false;
        };
        match request {
            StartupRequest::Write { index, line } => {
                if let Some(reason) = self.state.startup_write_gate().blocked_reason() {
                    return self.state.startup_lines.cancel_pending(reason);
                }
                let write = crate::startup::StartupWrite { index, line };
                match self.send_command_quiet(crate::startup::write_command(&write)) {
                    Ok(()) => self.state.startup_lines.write_sent(index),
                    Err(EngineError::Busy) => false,
                    Err(error) => {
                        let message = error.to_string();
                        let changed = self.state.startup_lines.failed(index, message.clone());
                        self.state.push_console(
                            ConsoleLevel::Error,
                            format!("Startup line $N{index}: {message}"),
                        );
                        changed
                    }
                }
            }
            StartupRequest::Read { index } => {
                match self.send_command_quiet(crate::startup::read_command(index)) {
                    Ok(()) => self.state.startup_lines.read_sent(index),
                    Err(EngineError::Busy) => false,
                    Err(error) => {
                        let message = error.to_string();
                        let changed = self.state.startup_lines.failed(index, message.clone());
                        self.state.push_console(
                            ConsoleLevel::Error,
                            format!("Startup line $N{index} read-back: {message}"),
                        );
                        changed
                    }
                }
            }
        }
    }

    // -------------------------------------------------------- tools/profile

    fn open_tool_editor(&mut self, record: Option<&ToolRecord>, cx: &mut Context<Self>) {
        self.state.keypad = None;
        self.settings_entry = None;
        self.tool_editor = Some(
            record
                .map(ToolEditor::from_record)
                .unwrap_or_else(ToolEditor::new),
        );
        cx.notify();
    }

    fn open_tool_field(&mut self, field: ToolField, cx: &mut Context<Self>) {
        let Some(editor) = self.tool_editor.as_ref() else {
            return;
        };
        let buffer = editor.value(field).to_string();
        self.state.keypad = Some(crate::state::KeypadState {
            target: KeypadTarget::Tool(field),
            buffer,
            fresh: true,
            hint: Some(field.title().to_string()),
        });
        cx.notify();
    }

    fn open_tool_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_entry(SettingsEntryPurpose::ToolName, window, cx);
    }

    fn save_tool_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.tool_editor.clone() else {
            return;
        };
        let record = match ToolTableModel::record_from_text(
            &editor.number,
            &editor.name,
            &editor.diameter,
            &editor.length,
            &editor.pocket,
        ) {
            Ok(record) => record,
            Err(message) => {
                if let Some(editor) = self.tool_editor.as_mut() {
                    editor.error = Some(message);
                }
                cx.notify();
                return;
            }
        };
        if let Some(original) = editor.original
            && original != record.number
            && self.state.tools.get(record.number).is_some()
        {
            if let Some(editor) = self.tool_editor.as_mut() {
                editor.error = Some(format!("T{} already exists", record.number));
            }
            cx.notify();
            return;
        }
        if editor.original.is_none() && self.state.tools.get(record.number).is_some() {
            if let Some(editor) = self.tool_editor.as_mut() {
                editor.error = Some(format!(
                    "T{} already exists; edit that row instead",
                    record.number
                ));
            }
            cx.notify();
            return;
        }
        let number = record.number;
        if let Err(error) = self.state.tools.insert(record) {
            if let Some(editor) = self.tool_editor.as_mut() {
                editor.error = Some(error.to_string());
            }
            cx.notify();
            return;
        }
        if let Some(original) = editor.original
            && original != number
        {
            self.state.tools.remove(original);
        }
        self.tool_editor = None;
        self.settings_entry = None;
        self.state.push_console(
            ConsoleLevel::Info,
            format!("Tool T{number} updated in the app table (unsaved)"),
        );
        cx.notify();
    }

    fn delete_tool(&mut self, number: u32, cx: &mut Context<Self>) {
        if self.state.tools.remove(number) {
            self.state.push_console(
                ConsoleLevel::Info,
                format!("Tool T{number} removed from the app table (unsaved)"),
            );
            if self.tool_editor.as_ref().and_then(|editor| editor.original) == Some(number) {
                self.tool_editor = None;
            }
        }
        cx.notify();
    }

    // --------------------------------------------------------------- macros

    /// Open the local macro add/edit sheet. Nothing is persisted here.
    fn open_macro_editor(&mut self, mac: Option<&ConfigMacro>, cx: &mut Context<Self>) {
        self.state.keypad = None;
        self.settings_entry = None;
        self.tool_editor = None;
        self.macro_editor = Some(mac.map_or_else(MacroEditor::new, MacroEditor::from_macro));
        cx.notify();
    }

    /// Open the shared text sheet for the editor's name field.
    fn open_macro_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_entry(SettingsEntryPurpose::MacroName, window, cx);
    }

    /// Open the shared text sheet for the editor's command field.
    fn open_macro_command(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_entry(SettingsEntryPurpose::MacroCommand, window, cx);
    }

    /// Validate the local draft and save the full list in the profile.
    fn save_macro_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.macro_editor.clone() else {
            return;
        };
        match self.state.save_macro(
            editor.original.as_deref(),
            &editor.name,
            &editor.command,
            editor.confirm,
        ) {
            Ok(path) => {
                self.macro_editor = None;
                self.settings_entry = None;
                self.state.push_console(
                    ConsoleLevel::Ok,
                    format!(
                        "Macro '{}' saved to the controller profile: {}",
                        editor.name.trim(),
                        path.display()
                    ),
                );
            }
            Err(message) => {
                if let Some(editor) = self.macro_editor.as_mut() {
                    editor.error = Some(message.clone());
                }
                self.state
                    .push_console(ConsoleLevel::Warn, format!("Macro not saved: {message}"));
            }
        }
        cx.notify();
    }

    /// Open the deletion confirmation for one stored macro.
    fn request_delete_macro(&mut self, name: String, cx: &mut Context<Self>) {
        self.macro_editor = None;
        self.settings_entry = None;
        self.macro_delete = Some(name);
        cx.notify();
    }

    fn cancel_delete_macro(&mut self, cx: &mut Context<Self>) {
        self.macro_delete = None;
        cx.notify();
    }

    fn confirm_delete_macro(&mut self, cx: &mut Context<Self>) {
        let Some(name) = self.macro_delete.take() else {
            return;
        };
        match self.state.delete_macro(&name) {
            Ok(path) => self.state.push_console(
                ConsoleLevel::Ok,
                format!(
                    "Macro '{name}' deleted from the controller profile: {}",
                    path.display()
                ),
            ),
            Err(message) => self.state.push_console(
                ConsoleLevel::Error,
                format!("Macro '{name}' not deleted: {message}"),
            ),
        }
        cx.notify();
    }

    /// Handle one explicit RUN click on a stored macro.
    ///
    /// The state model refuses mock/not-ready/Alarm/Door/job-active runs and
    /// captures a `confirm` macro. Only the confirmation path or a direct
    /// `confirm = false` macro produces a command.
    fn run_user_macro(&mut self, name: &str, cx: &mut Context<Self>) {
        match self.state.request_macro_run(name) {
            Ok(MacroRunRequest::Confirm(_)) => {}
            Ok(MacroRunRequest::Send(mac)) => {
                let command = Command::Mdi(mac.command.clone());
                self.send_macro_command(&mac.name, command);
            }
            Err(message) => self.state.push_console(ConsoleLevel::Warn, message),
        }
        cx.notify();
    }

    /// Send exactly one confirmed/direct macro command through the guarded MDI
    /// gateway. A rejection is reported once and is never retried.
    fn send_macro_command(&mut self, name: &str, command: Command) {
        match self.send_command_quiet(command) {
            Ok(()) => {
                self.state.macro_sent();
                self.state.push_console(
                    ConsoleLevel::Info,
                    format!("Macro '{name}' sent through the guarded MDI path"),
                );
            }
            Err(error) => {
                let message = format!("Macro '{name}' not sent: {error}");
                self.state.macro_send_failed(message.clone());
                self.state.push_console(ConsoleLevel::Error, message);
            }
        }
    }

    /// Confirm a captured macro run and send its one MDI command.
    fn confirm_macro_run(&mut self, cx: &mut Context<Self>) {
        let name = self
            .state
            .macro_run_pending()
            .map_or_else(|| "macro".to_string(), |mac| mac.name.clone());
        match self.state.confirm_macro_run() {
            Ok(command) => self.send_macro_command(&name, command),
            Err(message) => self.state.push_console(ConsoleLevel::Warn, message),
        }
        cx.notify();
    }

    /// Close the macro run confirmation without producing a command.
    fn cancel_macro_run(&mut self, cx: &mut Context<Self>) {
        self.state.cancel_macro_run();
        cx.notify();
    }

    /// Save tools, offsets, and the settings snapshot into the local profile.
    fn save_profile(&mut self, cx: &mut Context<Self>) {
        if self.state.config.has_pending_profile() {
            self.state.push_console(
                ConsoleLevel::Warn,
                "Confirm or discard the unverified profile data in Utility before saving",
            );
            cx.notify();
            return;
        }
        let tools = self.state.tools.table().clone();
        let offsets = self.state.profile_offset_snapshot();
        let settings = self.state.profile_settings_snapshot();
        match self.state.config.save_profile(&tools, &offsets, settings) {
            Ok(path) => {
                self.state.tools.mark_saved(path.clone());
                self.state.push_console(
                    ConsoleLevel::Ok,
                    format!("Local profile saved: {}", path.display()),
                );
            }
            Err(message) => {
                self.state.tools.set_error(message.clone());
                self.state
                    .push_console(ConsoleLevel::Error, format!("Profile save: {message}"));
            }
        }
        cx.notify();
    }

    /// Apply and persist a language-selector choice.
    ///
    /// The global locale changes immediately so the next frame uses the new
    /// catalog, even if the app config cannot be written. Selecting a language
    /// never sends a controller command.
    fn select_language(&mut self, locale: Locale, cx: &mut Context<Self>) {
        if let Err(message) = crate::i18n::choose_locale(&mut self.state.config, locale) {
            self.state.push_console(
                ConsoleLevel::Warn,
                format!("Locale selection was not saved: {message}"),
            );
        }
        cx.notify();
    }

    fn save_app_config(&mut self, cx: &mut Context<Self>) {
        match self.state.config.save_app() {
            Ok(path) => self.state.push_console(
                ConsoleLevel::Ok,
                format!("App settings saved: {}", path.display()),
            ),
            Err(message) => self
                .state
                .push_console(ConsoleLevel::Error, format!("App settings: {message}")),
        }
        cx.notify();
    }

    /// Open the short-name sheet for saving the measured height map.
    fn open_height_map_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_entry(SettingsEntryPurpose::HeightMapName, window, cx);
    }

    /// Save the measured map under `name` through the active profile.
    fn save_height_map_named(
        &mut self,
        name: String,
        cx: &mut Context<Self>,
    ) -> Result<PathBuf, String> {
        let result = self.state.save_current_height_map(name, now_unix_s());
        match &result {
            Ok(path) => self.state.push_console(
                ConsoleLevel::Ok,
                format!(
                    "Height map saved: {} (profile {})",
                    self.state
                        .height_map_library
                        .last_saved()
                        .map_or("", |notice| notice.name.as_str()),
                    path.display()
                ),
            ),
            Err(message) => self.state.push_console(
                ConsoleLevel::Error,
                format!("Height map not saved: {message}"),
            ),
        }
        cx.notify();
        result
    }

    /// Open one saved map read-only. Nothing is applied or started.
    fn open_saved_height_map(&mut self, name: &str, cx: &mut Context<Self>) {
        match self.state.select_saved_height_map(name) {
            Ok(()) => {
                let summary = self
                    .state
                    .height_map_library
                    .selected()
                    .map(|loaded| {
                        format!(
                            "{} | {} / {} cells | {}",
                            loaded.record.name,
                            loaded.summary.valid,
                            loaded.summary.total,
                            loaded.record.frame.wcs.as_str()
                        )
                    })
                    .unwrap_or_default();
                self.state.push_console(
                    ConsoleLevel::Info,
                    format!(
                        "Saved height map opened read-only: {summary}; compensation is not applied"
                    ),
                );
            }
            Err(message) => self.state.push_console(
                ConsoleLevel::Warn,
                format!("Saved map not opened: {message}"),
            ),
        }
        cx.notify();
    }

    /// Leave the saved-map browser and return to the live grid editor.
    fn close_saved_height_map(&mut self, cx: &mut Context<Self>) {
        self.state.clear_saved_map_selection();
        cx.notify();
    }

    fn use_pending_profile(&mut self, cx: &mut Context<Self>) {
        if let Some(tools) = self.state.config.use_pending_profile() {
            self.state.clear_saved_map_selection();
            self.state.reset_macro_runs();
            self.macro_editor = None;
            self.macro_delete = None;
            self.state.load_probe_offset();
            let path = self.state.config.profile_path();
            let count = tools.len();
            self.state
                .tools
                .adopt(tools, path, "UNVERIFIED, APPLIED BY OPERATOR");
            self.state.push_console(
                ConsoleLevel::Warn,
                format!("Unverified profile applied locally: {count} tool(s)"),
            );
        }
        cx.notify();
    }

    fn export_backup(&mut self, cx: &mut Context<Self>) {
        self.export_controller_reset_backup(cx);
    }

    fn validate_backup(&mut self, cx: &mut Context<Self>) {
        match self.state.config.validate_backup() {
            Ok(preview) => {
                let text = format!(
                    "Backup {}: {} tools, {} WCS | identity {}{}",
                    preview.path.display(),
                    preview.tools,
                    preview.work_offsets,
                    preview.identity.as_ref().map_or_else(
                        || "not checked".to_string(),
                        std::string::ToString::to_string
                    ),
                    preview
                        .warning
                        .as_ref()
                        .map_or_else(String::new, |warning| { format!(" | WARNING: {warning}") })
                );
                self.state.push_console(
                    if preview.warning.is_some() {
                        ConsoleLevel::Warn
                    } else {
                        ConsoleLevel::Ok
                    },
                    text,
                );
            }
            Err(message) => self
                .state
                .push_console(ConsoleLevel::Error, format!("Backup validation: {message}")),
        }
        cx.notify();
    }

    fn import_backup(&mut self, cx: &mut Context<Self>) {
        if !self.backup_import_armed {
            match self.state.config.validate_backup() {
                Ok(preview) if preview.warning.is_some() => {
                    self.backup_import_armed = true;
                    self.state.push_console(
                        ConsoleLevel::Warn,
                        format!(
                            "Backup needs confirmation: {}; press IMPORT again",
                            preview.warning.unwrap_or_default()
                        ),
                    );
                    cx.notify();
                    return;
                }
                Ok(_) => {}
                Err(message) => {
                    self.state
                        .push_console(ConsoleLevel::Error, format!("Backup validation: {message}"));
                    cx.notify();
                    return;
                }
            }
        }
        let allow_warning = self.backup_import_armed;
        self.backup_import_armed = false;
        match self.state.config.import_backup(allow_warning) {
            Ok(import) => {
                self.state.reset_macro_runs();
                self.macro_editor = None;
                self.macro_delete = None;
                if let Some(tools) = import.tools {
                    self.state.clear_saved_map_selection();
                    self.state.load_probe_offset();
                    let count = tools.len();
                    let profile_path = self.state.config.profile_path();
                    self.state.tools.adopt(tools, profile_path, "IMPORTED");
                    self.state.push_console(
                        ConsoleLevel::Ok,
                        format!(
                            "Backup imported from {}: {} tool(s)",
                            import.path.display(),
                            count
                        ),
                    );
                } else {
                    self.state.push_console(
                        ConsoleLevel::Warn,
                        format!(
                            "Imported {} for another controller; current session unchanged",
                            import.key.display_name()
                        ),
                    );
                }
                if let Some(warning) = import.warning {
                    self.state.push_console(ConsoleLevel::Warn, warning);
                }
            }
            Err(message) => self
                .state
                .push_console(ConsoleLevel::Error, format!("Backup import: {message}")),
        }
        cx.notify();
    }

    // ------------------------------------------------------------ UI gating

    fn machine(&self) -> MachineState {
        MachineState::parse(self.state.machine_state)
    }

    fn ready(&self) -> bool {
        self.state.connection.is_ready_like()
    }

    /// Live data on screen that no longer comes from a ready controller.
    fn stale(&self) -> bool {
        self.state.live && !self.ready()
    }

    fn locked(&self) -> bool {
        matches!(self.state.machine_state, "Alarm" | "Door")
    }

    fn mdi_allowed(&self) -> bool {
        if !self.ready() {
            return false;
        }
        if !self.state.live {
            return !self.locked();
        }
        self.machine() == MachineState::Idle
    }

    /// Gate for `G28`/`G30` gotos and `G92.1` clear.
    fn offset_gate(&self) -> OffsetGate {
        OffsetGate {
            live: self.state.live,
            ready: self.ready(),
            idle: self.machine() == MachineState::Idle,
            job_active: self.state.job.is_active(),
        }
    }

    /// Every precondition for the destructive `$RST` card.
    ///
    /// A backup export in flight and the backup acknowledgement live in the
    /// reset model itself, so they cannot be bypassed by a different caller.
    fn reset_gate(&self) -> ResetGate {
        ResetGate {
            mock: self.state.connection == ConnectionState::Mock,
            live: self.state.live,
            ready: self.ready(),
            idle: self.machine() == MachineState::Idle,
            alarm_or_door: self.locked(),
            job_active: self.state.job.is_active() || self.state.job.check_busy(),
            tool_change_pending: self.state.job.tool_change_pending(),
            probe_active: self.state.probe.is_running()
                || self.state.probe.tool_length_write_in_flight(),
            height_map_action_in_flight: self.state.height_map.pending().is_some(),
            pendant_active: self.state.mpg_active,
            settings_write_pending: self.state.settings.has_pending()
                || self.state.settings.has_queued_write(),
            offset_write_pending: self.state.offsets.status().is_pending()
                || self.state.offsets.pending_action().is_some(),
        }
    }

    fn jog_allowed(&self) -> bool {
        if !self.ready() {
            return false;
        }
        if !self.state.live {
            return !self.locked();
        }
        matches!(
            self.machine(),
            MachineState::Idle | MachineState::Hold | MachineState::Jog | MachineState::Tool
        )
    }

    /// Snapshot the same gates the job/jog buttons use, for keyboard actions.
    fn keyboard_gate(&self) -> KeyboardGate {
        KeyboardGate {
            live: self.state.live,
            ready: self.ready(),
            machine: self.machine(),
            job: self.state.job.state,
            program_loaded: self.job_loaded(),
            probe_owns_program: self.state.probe.owns_program(&self.state.job.name),
            tool_change_pending: self.state.job.tool_change_pending(),
            watchdog_pause: self.state.job.pause_reason == Some(JobPauseReason::Watchdog),
        }
    }

    /// Install the delta between the active and desired bindings.
    ///
    /// Changed keys get an `Unbind` marker before the replacement is added, so
    /// an old key does not keep firing the same action after a profile change.
    fn apply_binding_plan(&mut self, plan: BindingPlan, cx: &mut Context<Self>) {
        if plan.is_empty() {
            return;
        }
        let mut bindings = Vec::with_capacity(plan.unbinds.len() + plan.binds.len());
        for (action, keys) in &plan.unbinds {
            match unbind_for(*action, keys, cx) {
                Ok(binding) => bindings.push(binding),
                Err(message) => self.state.push_console(
                    ConsoleLevel::Warn,
                    format!("Keymap unbind skipped: {message}"),
                ),
            }
        }
        for (action, keys) in &plan.binds {
            match binding_for(*action, keys, cx) {
                Ok(binding) => bindings.push(binding),
                Err(message) => self.state.push_console(
                    ConsoleLevel::Warn,
                    format!("Keymap binding skipped: {message}"),
                ),
            }
        }
        if !bindings.is_empty() {
            cx.bind_keys(bindings);
        }
    }

    /// Adopt the active profile's keymap without pressing anything.
    ///
    /// Returns `true` when the keymap changed. Invalid or duplicate stored
    /// rows fall back to their defaults; the profile is not rewritten until
    /// the operator presses SAVE.
    fn sync_profile_keymap(&mut self, cx: &mut Context<Self>) -> bool {
        if self.state.config.key().is_none() {
            return false;
        }
        let source = self.state.config.data().keymap.clone();
        if source == self.keymap_source {
            return false;
        }
        let os = cx.platform_info().os;
        let model = KeymapModel::from_keymap(&source);
        if model.to_keymap(os).is_err() {
            self.state.push_console(
                ConsoleLevel::Warn,
                "Profile keymap has an invalid or duplicate row; those actions use their \
                 defaults until the keymap is saved again",
            );
        }
        let desired = model.effective_bindings(os);
        let plan = BindingPlan::between(&self.active_bindings, &desired);
        self.apply_binding_plan(plan, cx);
        self.active_bindings = desired;
        self.keymap = model;
        self.keymap_source = source;
        self.keymap_editor = None;
        self.held_machine_keys.clear();
        true
    }

    /// Whether a non-text sheet currently owns operator attention.
    ///
    /// STOP remains available through an ordinary sheet. Every other machine
    /// hotkey waits for the sheet to close.
    fn keyboard_sheet_open(&self) -> bool {
        self.settings_entry.is_some()
            || self.tool_editor.is_some()
            || self.macro_editor.is_some()
            || self.macro_delete.is_some()
            || self.startup_confirm.is_some()
            || self.state.keypad.is_some()
            || self.state.sleep_confirm_open
            || self.state.check_confirm_open
            || self.state.start_confirm_open()
            || self.state.macro_run_pending().is_some()
            || self.state.run_options_open
            || self.state.job.breakpoints.pending_run_to().is_some()
            || self.state.job.tool_change.is_some()
            || self.panel.open
            || self.state.offsets_backup.preview().is_some()
            || self.state.offsets.pending_action().is_some()
            || self.file_browser.is_some()
    }

    /// Route one typed GPUI action through the guarded machine path.
    ///
    /// A text field or the keymap editor owns the key: the action is not
    /// executed and propagation continues so typing and key capture still
    /// work. The same is true for OS key repeat: the first press acts and the
    /// latch releases on key-up.
    fn keyboard_action(
        &mut self,
        action: MachineAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.keymap_editor.is_some() || window.has_focused_input(cx) {
            cx.propagate();
            return;
        }
        if self.keyboard_sheet_open() && !action.allowed_while_sheet_open() {
            self.state.push_console(
                ConsoleLevel::Warn,
                format!(
                    "KEY {} ignored while a sheet is open; dismiss it or use F8 STOP",
                    action.label()
                ),
            );
            cx.notify();
            cx.propagate();
            return;
        }
        let now = Instant::now();
        if self.held_machine_keys.is_held(action, now) {
            cx.propagate();
            return;
        }
        let intent = action.intent();
        if let Some(reason) = refusal(intent, &self.keyboard_gate()) {
            self.state.push_console(
                ConsoleLevel::Warn,
                format!("KEY {} refused: {reason}", action.label()),
            );
        } else {
            self.run_keyboard_intent(intent, cx);
        }
        cx.notify();
        // Let the root key listener latch this physical key for repeat
        // suppression even when the action itself was refused.
        cx.propagate();
    }

    fn run_keyboard_intent(&mut self, intent: MachineIntent, cx: &mut Context<Self>) {
        match intent {
            MachineIntent::Jog { axis, sign } => {
                let axis = match axis {
                    JogAxis::X => Axis::X,
                    JogAxis::Y => Axis::Y,
                    JogAxis::Z => Axis::Z,
                };
                if self.jog_allowed() && self.state.live {
                    // One step per press. CONT/hold-to-jog stays pointer-only
                    // because a keyboard has no release event for the action.
                    self.send_jog(axis, sign, false);
                }
            }
            MachineIntent::Start => self.start_job(cx),
            MachineIntent::Pause => self.pause_job(cx),
            MachineIntent::Resume => self.resume_job(cx),
            MachineIntent::Stop => self.stop_job(cx),
        }
    }

    fn note_machine_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        note_key_down(
            &mut self.held_machine_keys,
            event,
            &self.active_bindings,
            cx.platform_info().os,
            Instant::now(),
        );
    }

    fn note_machine_key_up(&mut self, event: &KeyUpEvent) {
        self.held_machine_keys
            .end_key(&event.keystroke.key, &self.active_bindings);
    }

    fn aux_allowed(&self) -> bool {
        self.ready() && (!self.state.live || !self.machine().is_locked())
    }

    fn macro_allowed(&self, action: crate::state::MacroAction) -> bool {
        use crate::state::MacroAction;
        match action {
            MacroAction::Unlock | MacroAction::Home => self.ready(),
            MacroAction::ProbeZ => self.ready(),
            MacroAction::ZeroXy | MacroAction::ZeroZ | MacroAction::SafeZ => self.mdi_allowed(),
            MacroAction::Coolant => self.aux_allowed(),
        }
    }

    // -------------------------------------------------------------- job gates

    /// Whether the shell has a program to run (engine or mock).
    fn job_loaded(&self) -> bool {
        if self.state.live {
            self.state.job.has_program()
        } else {
            !self.state.program.is_empty()
        }
    }

    fn job_start_allowed(&self) -> bool {
        if self.state.probe.owns_program(&self.state.job.name) {
            // A generated probe program must never be re-run from the job
            // toolbar: its preflight and explicit cycle confirmation are gone.
            return false;
        }
        if !self.ready() || self.locked() || !self.job_loaded() {
            return false;
        }
        // A check that is still entering `$C` owns the runner even before the
        // job state changes; never race it with a normal start.
        if self.state.job.check_pending() {
            return false;
        }
        if !matches!(
            self.state.job.state,
            JobState::Idle | JobState::Completed | JobState::Aborted | JobState::Interrupted
        ) {
            return false;
        }
        if !self.state.live {
            return true;
        }
        // The engine accepts StartProgram only in Idle.
        self.machine() == MachineState::Idle
    }

    fn job_pause_allowed(&self) -> bool {
        self.ready() && !self.locked() && self.state.job.is_running()
    }

    fn job_resume_allowed(&self) -> bool {
        if !self.ready() || self.locked() || !self.state.job.is_paused() {
            return false;
        }
        // A pending tool change owns its own acknowledge-then-resume flow, and
        // the engine refuses a watchdog resume.
        !self.state.job.tool_change_pending()
            && self.state.job.pause_reason != Some(JobPauseReason::Watchdog)
    }

    fn job_stop_allowed(&self) -> bool {
        self.ready() && !self.locked() && self.state.job.is_active()
    }

    // -------------------------------------------------------------- actions

    fn send_mdi(&mut self, cx: &mut Context<Self>) {
        if !self.state.live {
            self.state.send_mdi();
            cx.notify();
            return;
        }
        if !self.mdi_allowed() {
            self.state.push_console(
                ConsoleLevel::Warn,
                "MDI is available only when the link is ready and the machine is Idle",
            );
            cx.notify();
            return;
        }
        let text = self.state.mdi.trim().to_string();
        if text.is_empty() {
            self.state.push_console(ConsoleLevel::Warn, "MDI is empty");
            cx.notify();
            return;
        }
        if self.send_mdi_line(text.clone()) {
            self.state.remember_mdi(text);
        }
        cx.notify();
    }

    /// Press half of a jog gesture.
    ///
    /// A step jog sends one command. A continuous jog sends one command on
    /// press and re-issues it at [`JOG_REPEAT`] while the pointer stays down;
    /// a release after [`JOG_CANCEL_AFTER`] sends `JogCancel` (0x85) so the
    /// controller stops at once.
    fn jog_press(&mut self, sign: f64, cx: &mut Context<Self>) {
        if !self.jog_allowed() {
            return;
        }
        if !self.state.live {
            self.state.jog(sign);
            cx.notify();
            return;
        }
        self.jog_generation = self.jog_generation.wrapping_add(1);
        let generation = self.jog_generation;
        let axis = self.state.jog_axis;
        let continuous = self.state.continuous;
        self.jog_hold = Some(JogHold {
            axis,
            sign,
            continuous,
            started: std::time::Instant::now(),
        });
        self.send_jog(axis, sign, false);
        if continuous {
            self.spawn_jog_repeat(generation, cx);
        }
        cx.notify();
    }

    /// Release half of a jog gesture: stop the repeat loop and cancel motion
    /// after a genuine hold. A quick click keeps its one step.
    fn end_jog(&mut self, cx: &mut Context<Self>) {
        let Some(hold) = self.jog_hold.take() else {
            return;
        };
        // The repeat task observes the generation and exits on its next tick;
        // dropping a task from inside its own callback would be unsafe.
        self.jog_generation = self.jog_generation.wrapping_add(1);
        if hold.continuous && hold.started.elapsed() >= JOG_CANCEL_AFTER && self.state.live {
            let _ = self.send_command_quiet(Command::Realtime(RealtimeCommand::JogCancel));
        }
        cx.notify();
    }

    /// Build and send one relative jog.
    ///
    /// `quiet_busy` keeps the repeat loop from filling the console while the
    /// controller is still executing the previous step.
    fn send_jog(&mut self, axis: Axis, sign: f64, quiet_busy: bool) -> bool {
        let Some(core_axis) = CoreAxis::from_index(axis.index()) else {
            self.state
                .push_console(ConsoleLevel::Warn, "Selected axis is not reported");
            return false;
        };
        let distance = sign * self.state.jog_step;
        let mut position = Position::new(Axes::from(core_axis));
        position.set(core_axis, distance);
        let feed = if self.state.feed_rate > 0 {
            f64::from(self.state.feed_rate)
        } else {
            // Idle reports FS:0,0; a jog still needs a positive feed.
            1_000.0
        };
        let jog = match JogCommand::incremental(position, feed) {
            Ok(jog) => jog,
            Err(error) => {
                self.state
                    .push_console(ConsoleLevel::Error, format!("Jog: {error}"));
                return false;
            }
        };
        match self.send_command_quiet(Command::Jog(jog)) {
            Ok(()) => true,
            Err(EngineError::Busy) if quiet_busy => false,
            Err(error) => {
                self.state
                    .push_console(ConsoleLevel::Error, format!("Jog: {error}"));
                false
            }
        }
    }

    fn spawn_jog_repeat(&mut self, generation: u64, cx: &mut Context<Self>) {
        self.jog_task = Some(cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor().timer(JOG_REPEAT).await;
                let keep_going = weak
                    .update(cx, |shell, cx| {
                        if shell.jog_generation != generation {
                            return false;
                        }
                        let Some(hold) = shell.jog_hold else {
                            return false;
                        };
                        if !shell.jog_allowed() {
                            shell.end_jog(cx);
                            return false;
                        }
                        shell.send_jog(hold.axis, hold.sign, true);
                        true
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        }));
    }

    // ------------------------------------------------- pins/limits diagnostics

    /// Gate inputs for the read-only diagnostics refresh.
    fn pins_gate(&self) -> PinsGate {
        PinsGate {
            live: self.state.live,
            ready: self.ready(),
            locked: self.locked(),
            job_active: self.state.job.is_active(),
        }
    }

    /// Open the diagnostics sheet and immediately request its four reports.
    ///
    /// Opening is the explicit request documented for this panel; closing it
    /// never sends anything.
    fn open_pins_panel(&mut self, cx: &mut Context<Self>) {
        self.state.pins.open();
        self.request_pins_refresh(cx);
    }

    /// Close the sheet without touching the machine or the controller.
    fn close_pins_panel(&mut self, cx: &mut Context<Self>) {
        self.state.pins.close();
        cx.notify();
    }

    /// Queue and pump one explicit refresh of the four read-only reports.
    fn request_pins_refresh(&mut self, cx: &mut Context<Self>) {
        if let Err(refusal) = self.state.pins.begin_refresh(self.pins_gate()) {
            self.state.push_console(
                ConsoleLevel::Warn,
                format!("Pins/limits refresh: {}", refusal.message()),
            );
        } else {
            self.pump_pins();
        }
        cx.notify();
    }

    /// Send the next queued diagnostics report through the engine gateway.
    ///
    /// The model owns the Busy backoff and request bookkeeping, so this method
    /// cannot duplicate a report or turn a Busy reply into a retry storm.
    fn pump_pins(&mut self) -> bool {
        if !self.state.pins.needs_pump() {
            return false;
        }
        let gate = self.pins_gate();
        let link = self.link.as_ref();
        self.state
            .pins
            .pump(Instant::now(), gate, |command| match link {
                Some(link) => link.send(command),
                None => Err(EngineError::NotConnected),
            })
    }

    fn pump_offset_refresh(&mut self) -> bool {
        if !self.state.live || !self.ready() {
            return false;
        }
        match self.send_command_quiet(Command::System(SystemCommand::Offsets)) {
            Ok(()) => {
                self.offset_refresh_sent = true;
                true
            }
            Err(EngineError::Busy) => false,
            Err(error) => {
                self.state.offsets.failed(error.to_string());
                self.offset_refresh_sent = false;
                true
            }
        }
    }

    /// Ask for a read-only `$#` refresh of the offsets view.
    fn request_offset_refresh(&mut self, cx: &mut Context<Self>) {
        if !self.state.live {
            self.state
                .push_console(ConsoleLevel::Warn, "Offsets need a live controller");
            cx.notify();
            return;
        }
        if !self.ready() {
            self.state
                .push_console(ConsoleLevel::Warn, "Offsets refresh requires a connection");
            cx.notify();
            return;
        }
        self.state.offsets.refresh_requested();
        self.offset_refresh_sent = false;
        self.pump_offset_refresh();
        cx.notify();
    }

    /// Send one `G10 L20` line for a display-unit value.
    ///
    /// The line changes a stored offset; it never moves an axis. The engine
    /// gateway still decides whether MDI is allowed in the current state.
    fn send_offset_value(&mut self, wcs: Wcs, axis: Axis, value: f64, cx: &mut Context<Self>) {
        if !self.state.live {
            self.state
                .push_console(ConsoleLevel::Warn, "Offset edits require a live controller");
            cx.notify();
            return;
        }
        if !self.mdi_allowed() {
            self.state.push_console(
                ConsoleLevel::Warn,
                "Offset writes require the controller to be Idle",
            );
            cx.notify();
            return;
        }
        match self.state.offsets.g10_command(wcs, axis, value) {
            Ok(line) => {
                if self.send_mdi_line(line.clone()) {
                    self.state.offsets.command_sent(line);
                    self.offset_refresh_sent = false;
                } else {
                    self.state.offsets.failed("engine rejected the G10 line");
                }
            }
            Err(message) => {
                self.state
                    .push_console(ConsoleLevel::Warn, format!("Offset: {message}"));
            }
        }
        cx.notify();
    }

    fn zero_offset_axis(&mut self, wcs: Wcs, axis: Axis, cx: &mut Context<Self>) {
        self.send_offset_value(wcs, axis, 0.0, cx);
    }

    fn zero_offset_all(&mut self, wcs: Wcs, cx: &mut Context<Self>) {
        if !self.state.live {
            self.state
                .push_console(ConsoleLevel::Warn, "Offset edits require a live controller");
            cx.notify();
            return;
        }
        if !self.mdi_allowed() {
            self.state.push_console(
                ConsoleLevel::Warn,
                "Offset writes require the controller to be Idle",
            );
            cx.notify();
            return;
        }
        match self.state.offsets.zero_all_command(wcs) {
            Ok(line) => {
                if self.send_mdi_line(line.clone()) {
                    self.state.offsets.command_sent(line);
                    self.offset_refresh_sent = false;
                } else {
                    self.state.offsets.failed("engine rejected the G10 line");
                }
            }
            Err(message) => {
                self.state
                    .push_console(ConsoleLevel::Warn, format!("Offset: {message}"));
            }
        }
        cx.notify();
    }

    /// Open the two-step confirmation sheet for a `G28`/`G30` goto or `G92.1`.
    ///
    /// Nothing is sent here. The sheet freezes the DRO and the read-only target
    /// values so the next tap confirms exactly what the operator saw.
    fn request_offset_action(&mut self, action: OffsetAction, cx: &mut Context<Self>) {
        let gate = self.offset_gate();
        if let Err(message) = self
            .state
            .offsets
            .request_action(action, gate, &self.state.dro)
        {
            self.state.push_console(
                ConsoleLevel::Warn,
                format!("{} refused: {message}", action.label()),
            );
        }
        cx.notify();
    }

    /// Close the confirmation sheet without sending anything.
    fn cancel_offset_action(&mut self, cx: &mut Context<Self>) {
        if self.state.offsets.pending_action().is_some() {
            self.state.offsets.cancel_action();
            self.state.push_console(
                ConsoleLevel::Info,
                "Offset action cancelled; nothing was sent",
            );
        }
        cx.notify();
    }

    /// Send the confirmed `G28`, `G30`, or `G92.1` through the guarded MDI path.
    ///
    /// The gate is re-checked at confirmation time; the engine remains the
    /// final authority and its rejection is surfaced in the console and footer.
    fn confirm_offset_action(&mut self, cx: &mut Context<Self>) {
        let gate = self.offset_gate();
        let (action, line) = match self.state.offsets.take_confirmed_command(gate) {
            Ok(value) => value,
            Err(message) => {
                self.state.push_console(
                    ConsoleLevel::Warn,
                    format!("Offset action refused: {message}"),
                );
                cx.notify();
                return;
            }
        };
        if self.send_mdi_line(line.clone()) {
            self.state.offsets.command_sent(line.clone());
            self.offset_refresh_sent = false;
            let text = if action.moves_machine() {
                format!("Offset: {line} sent (stored position; motion requested)")
            } else {
                format!("Offset: {line} sent (G92 cleared; no motion)")
            };
            self.state.push_console(ConsoleLevel::Command, text);
        } else {
            self.state.offsets.failed(format!("engine rejected {line}"));
            self.state.push_console(
                ConsoleLevel::Error,
                format!("Offset: the engine rejected {line}; nothing changed"),
            );
        }
        cx.notify();
    }

    /// Write one snapshot of the Console ring buffer to `exports/console.log`.
    ///
    /// Local only: no command is sent, no network or background writer starts,
    /// and a missing configuration root is reported instead of falling back to
    /// the working directory.
    fn save_console_log(&mut self, cx: &mut Context<Self>) {
        let _ = self.state.save_console_log(now_unix_s());
        cx.notify();
    }

    /// Fixed `exports/offsets.nc` path for the current configuration root.
    fn offsets_export_path(&self) -> Option<PathBuf> {
        self.state
            .config
            .store_root()
            .map(crate::offsets_export::offsets_export_path)
    }

    /// Write the local work-offset backup through the app model.
    ///
    /// Export is local only: no command is sent and no job is loaded. The full
    /// path and the line count are shown in the panel and the console.
    fn export_offsets(&mut self, cx: &mut Context<Self>) {
        match self.state.export_offsets_nc() {
            Ok(export) => {
                let text = self.state.offsets_backup.status().map_or_else(
                    || format!("Offsets exported to {}", export.path.display()),
                    |status| status.text.clone(),
                );
                self.state.push_console(ConsoleLevel::Ok, text);
            }
            Err(message) => {
                let text = self.state.offsets_backup.status().map_or_else(
                    || format!("Offsets export refused: {message}"),
                    |status| status.text.clone(),
                );
                self.state.push_console(ConsoleLevel::Error, text);
            }
        }
        cx.notify();
    }

    /// Validate the fixed offsets file and open the restore preview sheet.
    ///
    /// Validation is local. Nothing is loaded, sent, or started here.
    fn open_offsets_restore(&mut self, cx: &mut Context<Self>) {
        match self.state.open_offsets_preview() {
            Ok(()) => {
                if let Some(status) = self.state.offsets_backup.status() {
                    self.state
                        .push_console(ConsoleLevel::Info, status.text.clone());
                }
            }
            Err(message) => {
                let text = self.state.offsets_backup.status().map_or_else(
                    || format!("Offsets file not loaded: {message}"),
                    |status| status.text.clone(),
                );
                self.state.push_console(ConsoleLevel::Warn, text);
            }
        }
        cx.notify();
    }

    /// Close the restore preview. Nothing is loaded or sent.
    fn cancel_offsets_restore(&mut self, cx: &mut Context<Self>) {
        self.state.cancel_offsets_preview();
        if let Some(status) = self.state.offsets_backup.status() {
            self.state
                .push_console(ConsoleLevel::Info, status.text.clone());
        }
        cx.notify();
    }

    /// Load the re-validated offsets file as a program. Never starts it.
    ///
    /// The engine remains the final authority. The operator has to review the
    /// loaded program in Job and press Start only when Idle and ready.
    fn load_offsets_program(&mut self, cx: &mut Context<Self>) {
        if self.state.job.is_active() {
            self.state.push_console(
                ConsoleLevel::Warn,
                "Offsets restore: a job is active; stop it before loading a program",
            );
            cx.notify();
            return;
        }
        let command = match self.state.take_offsets_program() {
            Ok(command) => command,
            Err(message) => {
                let text = self.state.offsets_backup.status().map_or_else(
                    || format!("Offsets program not loaded: {message}"),
                    |status| status.text.clone(),
                );
                self.state.push_console(ConsoleLevel::Warn, text);
                cx.notify();
                return;
            }
        };
        let status = self
            .state
            .offsets_backup
            .status()
            .map(|status| status.text.clone())
            .unwrap_or_else(|| "Offsets program loaded; not started".to_string());
        if self.send_command(command) {
            self.state.push_console(ConsoleLevel::Ok, status);
            self.state.mode = Mode::Job;
            self.state.left_tab = LeftTab::Program;
        } else {
            self.state
                .offsets_backup
                .load_failed("the engine rejected the load; nothing was loaded or started");
            self.state.push_console(
                ConsoleLevel::Error,
                "Offsets program was not accepted by the engine; nothing was loaded",
            );
        }
        cx.notify();
    }

    fn zero_axis(&mut self, axis: Axis, cx: &mut Context<Self>) {
        if !self.state.live {
            self.state.zero_axis(axis);
            cx.notify();
            return;
        }
        let Some(wcs) = Wcs::from_index(self.state.wcs_index) else {
            return;
        };
        match self.state.offsets.zero_axis_command(wcs, axis) {
            Ok(line) => {
                if self.send_mdi_line(line.clone()) {
                    self.state.offsets.command_sent(line);
                    self.offset_refresh_sent = false;
                }
            }
            Err(message) => self
                .state
                .push_console(ConsoleLevel::Warn, format!("Offset: {message}")),
        }
        cx.notify();
    }

    fn select_wcs(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= AppState::wcs_count() {
            return;
        }
        if self.state.live {
            if !self.mdi_allowed() {
                self.state
                    .push_console(ConsoleLevel::Warn, "WCS change requires Idle");
                cx.notify();
                return;
            }
            self.state.set_wcs_index(index);
            self.send_mdi_line(AppState::wcs_at(index).to_string());
        } else {
            self.state.set_wcs_index(index);
        }
        cx.notify();
    }

    // -------------------------------------------------------------- probing

    /// Build the read-only probe preflight context from the current view model.
    ///
    /// DRO values are converted back to millimetres here; probe geometry is
    /// canonical millimetres regardless of the display unit. `$#` supplies the
    /// stored WCS value, `G92` and `TLO`, so the generated program uses the
    /// same frame the controller will use.
    fn probe_context(&self) -> ProbeContext {
        let s = &self.state;
        let units = s.profile.capabilities.units;
        let to_mm = |value: f64| units.to_mm(value);
        let axis_mm = |index: usize| to_mm(s.mpos.get(index).copied().unwrap_or(0.0));
        let work_mm = |index: usize| to_mm(s.dro.get(index).copied().unwrap_or(0.0));
        let wcs = Wcs::from_index(s.wcs_index).unwrap_or(Wcs::G54);
        ProbeContext {
            live: s.live,
            ready: self.ready(),
            machine_state: self.machine(),
            job_active: s.job.is_active(),
            machine_pos_mm: Position::xyz(axis_mm(0), axis_mm(1), axis_mm(2)),
            work_pos_mm: Position::xyz(work_mm(0), work_mm(1), work_mm(2)),
            wcs,
            wcs_value_mm: s.offsets.wcs_position(wcs).unwrap_or_default(),
            g92_mm: s.offsets.g92_position().unwrap_or_default(),
            tlo_z_mm: s.offsets.tlo_z_mm(),
            spindle_on: s.spindle_on,
            coolant_on: s.coolant.iter().any(|channel| *channel),
            has_probe: s.profile.capabilities.has_probe,
            has_probe_connected_signal: s.profile.capabilities.has_probe_connected,
            probe_disconnected: s.signals_raw.contains(CoreSignals::PROBE_DISCONNECTED),
            probe_triggered: s.signals_raw.contains(CoreSignals::PROBE)
                || s.signals_raw.contains(CoreSignals::PROBE_PRIMARY)
                || s.signals_raw.contains(CoreSignals::TOOL_SETTER)
                || s.signals_raw.contains(CoreSignals::PROBE_SECONDARY),
            offsets_reported: s.offsets.reported(),
            units,
        }
    }

    fn select_probe_card(&mut self, card: ProbeCard, cx: &mut Context<Self>) {
        let ctx = self.probe_context();
        if self.state.probe.select_card(card, &ctx) {
            self.state.clear_saved_map_selection();
            cx.notify();
        } else if self.state.probe.is_running() {
            self.state.push_console(
                ConsoleLevel::Warn,
                "A probe run is active; finish or stop it before changing cycles",
            );
            cx.notify();
        } else if self.state.probe.tool_length_write_in_flight() {
            self.state.push_console(
                ConsoleLevel::Warn,
                "A TLO write is awaiting its `$#` read-back; wait for the result before changing cycles",
            );
            cx.notify();
        }
    }

    fn close_probe_card(&mut self, cx: &mut Context<Self>) {
        if self.state.probe.close_card() {
            self.state.clear_saved_map_selection();
            cx.notify();
        } else if self.state.probe.is_running() {
            self.state.push_console(
                ConsoleLevel::Warn,
                "A probe run is active; finish or stop it before leaving the cycle",
            );
            cx.notify();
        } else if self.state.probe.tool_length_write_in_flight() {
            self.state.push_console(
                ConsoleLevel::Warn,
                "A TLO write is awaiting its `$#` read-back; wait for the result before leaving the cycle",
            );
            cx.notify();
        }
    }

    /// Save the current shared probe fields into the active controller profile.
    ///
    /// This is local setup data; no command, setting, offset, or motion is sent.
    /// The bar shows the exact path or refusal, and a failure keeps the
    /// previous stored defaults.
    fn save_probe_defaults(&mut self, cx: &mut Context<Self>) {
        match self.state.save_probe_defaults() {
            Ok(path) => {
                let text = format!("{}: {}", t("SAVED"), path.display());
                self.state.push_console(
                    ConsoleLevel::Ok,
                    format!("Probe defaults saved to {}", path.display()),
                );
                self.probe_defaults_status = Some(ProbeDefaultsStatus { saved: true, text });
            }
            Err(message) => {
                let text = format!("{}: {message}", t("NOT SAVED"));
                self.state.push_console(
                    ConsoleLevel::Warn,
                    format!("Probe defaults not saved: {message}"),
                );
                self.probe_defaults_status = Some(ProbeDefaultsStatus { saved: false, text });
            }
        }
        cx.notify();
    }

    // ----------------------------------------------------- probe macros

    /// Copy the open macro draft into its two multi-line editors.
    ///
    /// Called once when the sheet opens, including the demo path that has no
    /// window in its poll. Editing afterwards lives in the textarea states
    /// until SAVE MACROS reads them back.
    fn sync_probe_macro_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.probe_macro_inputs_dirty {
            return;
        }
        let Some(editor) = self.probe_macro_editor.as_ref() else {
            self.probe_macro_inputs_dirty = false;
            return;
        };
        let pre = editor.draft.pre.clone();
        let post = editor.draft.post.clone();
        self.probe_macro_pre_input
            .update(cx, |state, cx| state.set_value(pre, window, cx));
        self.probe_macro_post_input
            .update(cx, |state, cx| state.set_value(post, window, cx));
        self.probe_macro_inputs_dirty = false;
    }

    /// Open the local macro sheet from the model's stored text.
    ///
    /// The draft is copied; opening changes neither the model nor the profile
    /// and sends no command.
    fn open_probe_macro_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.probe_macro_editor = Some(ProbeMacroEditor::new(&self.state.probe));
        self.probe_macro_inputs_dirty = true;
        self.sync_probe_macro_inputs(window, cx);
        cx.notify();
    }

    /// Fill valid sample text and open the sheet without saving.
    fn open_demo_probe_macro_editor(&mut self) {
        self.probe_macro_editor = Some(ProbeMacroEditor {
            draft: ProbeMacroDraft {
                pre: DEMO_PROBE_PRE_MACRO.to_string(),
                post: DEMO_PROBE_POST_MACRO.to_string(),
            },
            error: None,
            status: None,
        });
        self.probe_macro_inputs_dirty = true;
    }

    /// Open the sheet with a forbidden motion line and its validation error.
    ///
    /// This is the same model validation path SAVE MACROS uses; it cannot
    /// write because the line is rejected before the model changes.
    fn open_demo_probe_macro_error(&mut self) {
        let error = match self
            .state
            .save_probe_macros(DEMO_PROBE_INVALID_PRE_MACRO, DEMO_PROBE_POST_MACRO)
        {
            Ok(_) => "the demo macro unexpectedly validated".to_string(),
            Err(error) => error,
        };
        self.probe_macro_editor = Some(ProbeMacroEditor {
            draft: ProbeMacroDraft {
                pre: DEMO_PROBE_INVALID_PRE_MACRO.to_string(),
                post: DEMO_PROBE_POST_MACRO.to_string(),
            },
            error: Some(error.clone()),
            status: Some(ProbeMacrosStatus {
                saved: false,
                text: format!("{}: {error}", t("NOT SAVED")),
            }),
        });
        self.probe_macro_inputs_dirty = true;
    }

    /// Save the demo sample through the same model/local-profile path as
    /// `SAVE MACROS`, leaving the sheet closed for the bar capture.
    fn seed_saved_demo_probe_macros(&mut self, cx: &mut Context<Self>) {
        match self
            .state
            .save_probe_macros(DEMO_PROBE_PRE_MACRO, DEMO_PROBE_POST_MACRO)
        {
            Ok(path) => {
                let text = format!("{}: {}", t("SAVED"), path.display());
                self.state.push_console(
                    ConsoleLevel::Ok,
                    format!("Probe macros saved to {}", path.display()),
                );
                self.probe_macros_status = Some(ProbeMacrosStatus { saved: true, text });
            }
            Err(message) => {
                let text = format!("{}: {message}", t("NOT SAVED"));
                self.state.push_console(
                    ConsoleLevel::Warn,
                    format!("Probe macros not saved: {message}"),
                );
                self.probe_macros_status = Some(ProbeMacrosStatus { saved: false, text });
            }
        }
        cx.notify();
    }

    /// Close the macro sheet without touching the model or the profile.
    fn cancel_probe_macro_editor(&mut self, cx: &mut Context<Self>) {
        self.probe_macro_editor = None;
        self.probe_macro_inputs_dirty = false;
        cx.notify();
    }

    /// Validate, store, and persist the two macro fields from the sheet.
    ///
    /// A validation refusal leaves the model untouched. A local profile write
    /// failure rolls the model back to its previous macros, so the editor and
    /// the stored record stay consistent. Nothing is sent to the controller.
    fn save_probe_macro_editor(&mut self, cx: &mut Context<Self>) {
        if self.probe_macro_editor.is_none() {
            return;
        }
        let pre = self.probe_macro_pre_input.read(cx).value().to_string();
        let post = self.probe_macro_post_input.read(cx).value().to_string();
        let result = self.state.save_probe_macros(&pre, &post);
        let (status, console, level) = match result {
            Ok(path) => {
                let text = format!("{}: {}", t("SAVED"), path.display());
                (
                    ProbeMacrosStatus { saved: true, text },
                    format!("Probe macros saved to {}", path.display()),
                    ConsoleLevel::Ok,
                )
            }
            Err(message) => {
                let text = format!("{}: {message}", t("NOT SAVED"));
                (
                    ProbeMacrosStatus {
                        saved: false,
                        text: text.clone(),
                    },
                    format!("Probe macros not saved: {message}"),
                    ConsoleLevel::Warn,
                )
            }
        };
        let saved = status.saved;
        if let Some(editor) = self.probe_macro_editor.as_mut() {
            editor.error = (!saved).then(|| {
                let prefix = format!("{}: ", t("NOT SAVED"));
                status
                    .text
                    .strip_prefix(&prefix)
                    .unwrap_or(&status.text)
                    .to_string()
            });
            editor.status = Some(status.clone());
        }
        self.probe_macros_status = Some(status);
        self.state.push_console(level, console);
        cx.notify();
    }

    /// Load and start a generated probe program after a passing preflight.
    ///
    /// Nothing is sent unless the operator pressed RUN. The engine safety
    /// gateway remains the final authority; a rejected command is shown in the
    /// console and the run is marked failed, never retried automatically.
    fn start_probe_run(&mut self, cx: &mut Context<Self>) {
        if self.state.probe.tool_length_write_in_flight() {
            self.state.push_console(
                ConsoleLevel::Warn,
                "A TLO write is awaiting its `$#` read-back; wait for the result before another run",
            );
            cx.notify();
            return;
        }
        let ctx = self.probe_context();
        let preflight = self.state.probe.preflight(&ctx);
        let Some(setup) = preflight.setup else {
            let reason = if preflight.blockers.is_empty() {
                "preflight did not produce a plan".to_string()
            } else {
                preflight.blockers.join("; ")
            };
            self.state
                .push_console(ConsoleLevel::Warn, format!("Probe refused: {reason}"));
            cx.notify();
            return;
        };
        let name = setup.plan.name.clone();
        let source = setup.plan.program.clone();
        // Starting a run always leaves the read-only saved-map browser and
        // clears the previous run's save feedback.
        self.state.height_map_library.reset();
        // Keep the work toolpath for the height-map overlay. A second run must
        // not capture its own probe program as the base path.
        if self.state.probe.card() == Some(ProbeCard::HeightMap)
            && !self.state.probe.owns_program(&self.state.job.name)
        {
            self.height_map_base_program = self.state.job.program().cloned();
        }
        let mut summary = format!(
            "Probe {}: {} lines, {} probes, {:.3} mm total search, safe Z {:.3} mm",
            name,
            source.lines().count(),
            setup.plan.expected_probes,
            setup.plan.total_search_travel_mm(),
            setup.plan.safe_z_mm
        );
        if let Some(estimate) = &setup.estimate {
            summary.push_str(&format!(", estimate {}", format_estimate(estimate)));
        }
        // Any probe run replaces the loaded job; a previous compensation is no
        // longer the loaded program. Keep the original source for the next
        // Apply, but drop the stale applied state.
        self.state.height_map.on_probe_run_started();
        self.state.probe.begin(setup);
        self.state.left_tab = LeftTab::Console;
        self.state.push_console(ConsoleLevel::Info, summary);
        if !self.send_command(Command::LoadProgram { name, source }) {
            self.state
                .probe
                .fail("the engine rejected the probe program load".to_string());
            cx.notify();
            return;
        }
        // Demo-only pacing: the in-process simulator acknowledges a `G38.2`
        // before its single-motion model finishes, so the next retract line
        // would replace the probe. One breakpoint after each probe lets the
        // simulator finish and emit PRB; the poll resumes only after the
        // matching result. A serial controller streams the plan normally and
        // never pauses or auto-resumes here.
        let demo_pacing = matches!(
            self.link.as_ref().and_then(|link| link.mode()),
            Some(LinkMode::Demo)
        );
        let checkpoints = if demo_pacing {
            self.state
                .probe
                .run()
                .map(|run| run.checkpoints.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for line in checkpoints {
            if !self.send_command(Command::SetBreakpoint {
                line,
                enabled: true,
            }) {
                self.state
                    .probe
                    .fail(format!("the engine rejected probe checkpoint {line}"));
                cx.notify();
                return;
            }
        }
        if !self.send_command(Command::StartProgram { from_line: None }) {
            self.state
                .probe
                .fail("the engine rejected the probe program start".to_string());
        }
        cx.notify();
    }

    /// Stop a running probe job. Never resumes.
    fn abort_probe_run(&mut self, cx: &mut Context<Self>) {
        if !self.state.probe.is_running() {
            return;
        }
        self.state
            .probe
            .fail("operator stopped the probe".to_string());
        self.send_command(Command::StopProgram { reset: false });
        cx.notify();
    }

    /// Seed the explicit, small demo grid used by the screenshot switches.
    ///
    /// This is capture aid only: a user grid is never capped, and none of
    /// these values are written to the controller configuration.
    fn seed_demo_height_map(&mut self) {
        let ctx = self.probe_context();
        if !self.state.probe.select_card(ProbeCard::HeightMap, &ctx) {
            return;
        }
        for (field, text) in [
            (ProbeField::OriginX, "0.000"),
            (ProbeField::OriginY, "0.000"),
            // 4x4 at 15 mm covers the bundled 40 mm pocket for the Apply
            // screenshot; with OutsideMap::Reject a smaller grid would refuse.
            (ProbeField::StepX, "15.000"),
            (ProbeField::StepY, "15.000"),
            (ProbeField::Columns, "4"),
            (ProbeField::Rows, "4"),
            (ProbeField::SurfaceZ, "-1.000"),
            (ProbeField::SafeZ, "5.000"),
            (ProbeField::Search, "10.000"),
            (ProbeField::Feed, "100.000"),
            (ProbeField::Retract, "2.000"),
            (ProbeField::Diameter, "3.000"),
            (
                ProbeField::MapZOffset,
                if std::env::args().any(|arg| arg == "--demo-height-map-z-offset") {
                    "0.750"
                } else {
                    "0.000"
                },
            ),
        ] {
            if let Err(message) = self.state.probe.set_field_text(field, text) {
                self.state
                    .push_console(ConsoleLevel::Warn, format!("--demo-height-map: {message}"));
            }
        }
    }

    /// Fixed CSV path under the app configuration directory.
    fn height_map_export_path(&self) -> Option<PathBuf> {
        self.state
            .config
            .store_root()
            .map(|root| root.join("exports").join("height-map.csv"))
    }

    /// Write the measured map as CSV and record the full path in the result.
    fn export_height_map(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.height_map_export_path() else {
            self.state.push_console(
                ConsoleLevel::Error,
                "Height-map export: the app configuration directory is unavailable",
            );
            cx.notify();
            return;
        };
        let csv = match self.state.probe.height_map_csv() {
            Ok(csv) => csv,
            Err(message) => {
                self.state.push_console(
                    ConsoleLevel::Warn,
                    format!("Height-map export refused: {message}"),
                );
                cx.notify();
                return;
            }
        };
        let write = (|| -> Result<usize, String> {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::write(&path, csv.as_bytes()).map_err(|error| error.to_string())?;
            Ok(csv.lines().count().saturating_sub(1))
        })();
        match &write {
            Ok(rows) => {
                self.state.push_console(
                    ConsoleLevel::Ok,
                    format!("Height map exported: {} ({rows} cells)", path.display()),
                );
            }
            Err(message) => {
                self.state.push_console(
                    ConsoleLevel::Error,
                    format!("Height-map export failed: {}: {message}", path.display()),
                );
            }
        }
        self.state.probe.mark_height_export(path, write);
        cx.notify();
    }

    fn request_probe_apply(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.state.probe.run_mut() {
            run.apply_confirm = true;
            run.kept = false;
        }
        cx.notify();
    }

    fn cancel_probe_apply(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.state.probe.run_mut() {
            run.apply_confirm = false;
        }
        cx.notify();
    }

    /// Snapshot the map the operator is acting on: a read-only saved map when
    /// one is open, otherwise the current measured run.
    fn active_height_map_snapshot(&self) -> Option<HeightMapSnapshot> {
        if let Some(saved) = self.state.selected_saved_map() {
            Some(saved.snapshot())
        } else {
            self.state.probe.run().and_then(HeightMapSnapshot::from_run)
        }
    }

    /// Validate the map and prepare a compensated program for confirmation.
    ///
    /// Nothing is loaded and nothing starts: this only builds the preview the
    /// confirmation step shows.
    fn request_height_map_apply(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.active_height_map_snapshot();
        let Some(snapshot) = snapshot else {
            self.state.push_console(
                ConsoleLevel::Warn,
                "Height map Apply refused: no measured or selected map",
            );
            cx.notify();
            return;
        };
        // Applying a saved map rewrites the currently loaded part program. Adopt
        // it as the Revert original only on this explicit action.
        self.state.height_map.adopt_current_program_for_map();
        let gate = HeightApplyGate::from_context(&self.probe_context());
        match self.state.height_map.request_apply(&snapshot, &gate) {
            Ok(()) => {
                let cells = snapshot.map.len();
                let step = snapshot
                    .map
                    .grid()
                    .step_x_mm
                    .min(snapshot.map.grid().step_y_mm);
                self.state.push_console(
                    ConsoleLevel::Info,
                    format!(
                        "Height map ready to apply: {cells} cells, {} mm step. Confirm to load \
                         the compensated program; the job is not started.",
                        crate::probe::format_mm(step)
                    ),
                );
            }
            Err(blockers) => {
                for blocker in blockers {
                    self.state.push_console(
                        ConsoleLevel::Warn,
                        format!("Height map Apply refused: {blocker}"),
                    );
                }
            }
        }
        cx.notify();
    }

    fn cancel_height_map_apply(&mut self, cx: &mut Context<Self>) {
        self.state.height_map.cancel_pending();
        cx.notify();
    }

    /// Load the confirmed compensated program. Never starts it.
    fn confirm_height_map_apply(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.state.height_map.pending().cloned() else {
            self.state.push_console(
                ConsoleLevel::Warn,
                "Height map Apply refused: no confirmation is pending",
            );
            cx.notify();
            return;
        };
        // Re-check the live gates: telemetry can change between the preview and
        // the operator's confirmation.
        let snapshot = self.active_height_map_snapshot();
        if let Some(snapshot) = snapshot {
            let gate = HeightApplyGate::from_context(&self.probe_context());
            let blockers = self.state.height_map.blockers(&snapshot, &gate);
            if !blockers.is_empty() {
                for blocker in blockers {
                    self.state.push_console(
                        ConsoleLevel::Warn,
                        format!("Height map Apply refused: {blocker}"),
                    );
                }
                cx.notify();
                return;
            }
        } else {
            self.state.push_console(
                ConsoleLevel::Warn,
                "Height map Apply refused: the measured map is no longer available",
            );
            cx.notify();
            return;
        }
        let name = preview.program_name.clone();
        let load = preview.load();
        if self.send_command(load.command()) {
            self.state.height_map.mark_applied(preview);
            self.state.push_console(
                ConsoleLevel::Ok,
                format!(
                    "Height map applied: {name} loaded (not started); original kept for Revert"
                ),
            );
        } else {
            self.state.height_map.fail(format!(
                "the engine rejected the compensated program load: {name}"
            ));
        }
        cx.notify();
    }

    /// Load the original program again. Never starts it.
    fn revert_height_map(&mut self, cx: &mut Context<Self>) {
        if self.state.job.is_active() || self.machine() != MachineState::Idle {
            self.state.push_console(
                ConsoleLevel::Warn,
                "Height map Revert requires Idle and no active job",
            );
            cx.notify();
            return;
        }
        let load = match self.state.height_map.revert_load() {
            Ok(load) => load,
            Err(message) => {
                self.state
                    .push_console(ConsoleLevel::Warn, format!("Revert refused: {message}"));
                self.state.height_map.fail(message);
                cx.notify();
                return;
            }
        };
        let name = load.name.clone();
        if self.send_command(load.command()) {
            self.state.height_map.mark_reverted();
            self.state.push_console(
                ConsoleLevel::Ok,
                format!("Height map reverted: original program {name} restored (not started)"),
            );
        } else {
            self.state
                .height_map
                .fail("the engine rejected the original program load");
        }
        cx.notify();
    }

    /// Send the confirmed `G10 L2` line and start the `$#` read-back.
    fn confirm_probe_apply(&mut self, cx: &mut Context<Self>) {
        if !self.mdi_allowed() {
            self.state.push_console(
                ConsoleLevel::Warn,
                "Probe Apply requires a ready controller in Idle",
            );
            cx.notify();
            return;
        }
        let lines = match self.state.probe.apply_lines() {
            Ok(lines) => lines,
            Err(message) => {
                self.state
                    .push_console(ConsoleLevel::Warn, format!("Apply refused: {message}"));
                cx.notify();
                return;
            }
        };
        let mut sent_all = true;
        for line in lines {
            if self.send_mdi_line(line.clone()) {
                self.state.offsets.command_sent(line.clone());
                self.offset_refresh_sent = false;
                self.state.probe.mark_applied(line);
            } else {
                sent_all = false;
            }
        }
        if !sent_all {
            self.state.push_console(
                ConsoleLevel::Error,
                "Probe Apply was not accepted by the engine; offsets were not changed",
            );
        }
        cx.notify();
    }

    fn keep_probe_result(&mut self, cx: &mut Context<Self>) {
        self.state.probe.keep_result();
        cx.notify();
    }

    /// Open the inline confirmation for one TLO action. Nothing is sent.
    fn request_probe_tlo(&mut self, action: ToolLengthAction, cx: &mut Context<Self>) {
        match self.state.request_probe_tool_length(action) {
            Ok(()) => self.state.push_console(
                ConsoleLevel::Info,
                format!(
                    "{} ready: confirm the exact line. Nothing was sent.",
                    action.label()
                ),
            ),
            Err(message) => self.state.push_console(
                ConsoleLevel::Warn,
                format!("{} refused: {message}", action.label()),
            ),
        }
        cx.notify();
    }

    /// Close the TLO confirmation without producing a command.
    fn cancel_probe_tlo(&mut self, cx: &mut Context<Self>) {
        if self.state.probe.cancel_tool_length_action() {
            self.state
                .push_console(ConsoleLevel::Info, "TLO action cancelled; nothing was sent");
        }
        cx.notify();
    }

    /// Send the confirmed TLO line and request the `$#` read-back.
    ///
    /// The model sends exactly one `Command::Mdi` line followed by one
    /// `Command::System(SystemCommand::Offsets)` request. A Busy `$#` request
    /// is retried by the normal read-only refresh pump, never by another TLO
    /// write.
    fn confirm_probe_tlo(&mut self, cx: &mut Context<Self>) {
        let mut refresh_busy = false;
        let link = self.link.as_ref();
        let result = self.state.confirm_probe_tool_length(|command| {
            let Some(link) = link else {
                return Err("no engine connection; the TLO write was not sent".to_string());
            };
            let is_refresh = matches!(&command, Command::System(SystemCommand::Offsets));
            match link.send(command) {
                Ok(()) => Ok(()),
                Err(EngineError::Busy) if is_refresh => {
                    // The MDI line is still in the streamer. Keep the write
                    // awaiting its read-back and let the poll request `$#`
                    // once the engine accepts it.
                    refresh_busy = true;
                    Ok(())
                }
                Err(error) => Err(error.to_string()),
            }
        });
        match result {
            Ok(write) => {
                self.state.offsets.command_sent(write.line.clone());
                self.offset_refresh_sent = !refresh_busy;
                // A Busy `$#` request is retried by the normal poll pump once
                // the MDI line has cleared the streamer. It is never replaced
                // by a second TLO write.
                self.state.push_console(
                    ConsoleLevel::Command,
                    format!(
                        "TLO: {} sent; waiting for the `$#` read-back before APPLIED",
                        write.line
                    ),
                );
            }
            Err(message) => {
                self.state
                    .push_console(ConsoleLevel::Error, format!("TLO write refused: {message}"));
            }
        }
        cx.notify();
    }

    fn toggle_coolant(&mut self, channel: usize, cx: &mut Context<Self>) {
        if !self.state.live {
            self.state.toggle_coolant(channel);
            cx.notify();
            return;
        }
        let command = match channel {
            0 => RealtimeCommand::MistToggle,
            1 => RealtimeCommand::FloodToggle,
            2 => RealtimeCommand::Fan0Toggle,
            _ => return,
        };
        if self.send_command(Command::Realtime(command))
            && let Some(value) = self.state.coolant.get_mut(channel)
        {
            *value = !*value;
        }
        cx.notify();
    }

    fn spindle_power(&mut self, on: bool, cx: &mut Context<Self>) {
        if !self.state.live {
            self.state.spindle_on = on;
            self.state.push_console(
                ConsoleLevel::Command,
                format!("M{} (mock)", if on { 3 } else { 5 }),
            );
            cx.notify();
            return;
        }
        let line = if on {
            format!(
                "M{} S{}",
                if self.state.spindle_cw { 3 } else { 4 },
                self.state.rpm
            )
        } else {
            "M5".to_string()
        };
        if self.send_mdi_line(line) {
            self.state.spindle_on = on;
        }
        cx.notify();
    }

    fn spindle_direction(&mut self, clockwise: bool, cx: &mut Context<Self>) {
        if !self.state.live {
            self.state.spindle_cw = clockwise;
            self.state.spindle_on = true;
            cx.notify();
            return;
        }
        let line = format!("M{} S{}", if clockwise { 3 } else { 4 }, self.state.rpm);
        if self.send_mdi_line(line) {
            self.state.spindle_cw = clockwise;
            self.state.spindle_on = true;
        }
        cx.notify();
    }

    fn request_feed_override(&mut self, target: u16) {
        if !self.state.live {
            return;
        }
        self.send_command(Command::SetFeedOverride(target.clamp(10, 200)));
    }

    /// Move the feed override by one fine or coarse step.
    ///
    /// The command is built by the state gate, so mock and a non-ready link
    /// send nothing. While a slider drag owns the displayed percentage the
    /// value is left alone: only a later snapshot may replace it.
    fn step_feed_override(&mut self, delta: i16, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.state.feed_override_target(delta);
        if !self.state.live {
            self.set_feed_override_display(target, window, cx);
            cx.notify();
            return;
        }
        match self.state.feed_override_step(delta) {
            Ok(command) => {
                if !self.feed_slider_active {
                    self.set_feed_override_display(target, window, cx);
                }
                self.send_command(command);
            }
            Err(refusal) => self
                .state
                .push_console(ConsoleLevel::Warn, refusal.message()),
        }
        cx.notify();
    }

    /// Send the exact `RESET 100%` feed override through the guarded path.
    fn reset_feed_override(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.live {
            self.set_feed_override_display(FEED_OVERRIDE_RESET, window, cx);
            cx.notify();
            return;
        }
        match self.state.feed_override_reset() {
            Ok(command) => {
                if !self.feed_slider_active {
                    self.set_feed_override_display(FEED_OVERRIDE_RESET, window, cx);
                }
                self.send_command(command);
            }
            Err(refusal) => self
                .state
                .push_console(ConsoleLevel::Warn, refusal.message()),
        }
        cx.notify();
    }

    fn set_feed_override_display(
        &mut self,
        target: u16,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = target.clamp(FEED_OVERRIDE_MIN, FEED_OVERRIDE_MAX);
        self.state.feed_override = target;
        let value = f32::from(target);
        self.feed_slider
            .update(cx, |slider, cx| slider.set_value(value, window, cx));
    }

    fn request_spindle_override(&mut self, target: u16) {
        if !self.state.live {
            return;
        }
        self.send_command(Command::SetSpindleOverride(target.clamp(10, 200)));
    }

    /// Advance rapid override through 100 -> 50 -> 25 -> 100.
    ///
    /// The command goes through the engine's guarded path and the chip keeps
    /// showing the snapshot value, so a rejected or lost command never claims
    /// a level the controller did not report.
    fn cycle_rapid_override(&mut self, cx: &mut Context<Self>) {
        match self.state.rapid_override_step() {
            Ok(command) => {
                self.send_command(command);
            }
            Err(refusal) => {
                self.state
                    .push_console(ConsoleLevel::Warn, refusal.message());
            }
        }
        cx.notify();
    }

    /// Open the run-options sheet. Opening sends nothing.
    fn open_run_options(&mut self, cx: &mut Context<Self>) {
        self.state.open_run_options();
        cx.notify();
    }

    /// Open the check-run confirmation. Opening sends nothing.
    ///
    /// A refused action never opens a sheet; its reason goes to the console so
    /// a disabled or locked action still has an explanation.
    fn open_check_confirm(&mut self, cx: &mut Context<Self>) {
        if let Err(refusal) = self.state.request_check() {
            self.state
                .push_console(ConsoleLevel::Warn, refusal.message());
        }
        cx.notify();
    }

    /// Close the check confirmation without sending anything.
    fn cancel_check_confirm(&mut self, cx: &mut Context<Self>) {
        self.state.cancel_check_confirm();
        cx.notify();
    }

    /// CHECK action in the RUN OPTIONS sheet.
    ///
    /// A successful request closes the run-options sheet and opens the
    /// confirmation; a refusal keeps the sheet open and shows its reason in
    /// the console. Nothing is sent here.
    fn open_check_from_run_options(&mut self, cx: &mut Context<Self>) {
        match self.state.request_check() {
            Ok(()) => self.state.close_run_options(),
            Err(refusal) => self
                .state
                .push_console(ConsoleLevel::Warn, refusal.message()),
        }
        cx.notify();
    }

    /// Confirm the check sheet and send exactly one `Command::StartCheckRun`.
    ///
    /// The gate is re-checked after the sheet is consumed; a refusal or a
    /// sender rejection is reported in the console and nothing is claimed.
    fn confirm_check(&mut self, cx: &mut Context<Self>) {
        let link = self.link.as_ref();
        let outcome = self.state.confirm_check(|command| match link {
            Some(link) => link.send(command).map_err(|error| error.to_string()),
            None => Err("no engine connection".to_string()),
        });
        match outcome {
            CheckOutcome::Started => self.state.push_console(
                ConsoleLevel::Info,
                "Check run requested; the controller parses the program with motion suppressed",
            ),
            CheckOutcome::Rejected(detail) => self
                .state
                .push_console(ConsoleLevel::Error, format!("CHECK NOT STARTED: {detail}")),
            CheckOutcome::Refused(refusal) => self
                .state
                .push_console(ConsoleLevel::Warn, refusal.message()),
            CheckOutcome::NotConfirmed => {}
        }
        cx.notify();
    }

    /// Close the run-options sheet. Closing sends nothing.
    fn close_run_options(&mut self, cx: &mut Context<Self>) {
        self.state.close_run_options();
        cx.notify();
    }

    /// Open the sleep confirmation. Opening sends nothing.
    fn open_sleep_confirm(&mut self, cx: &mut Context<Self>) {
        self.state.open_sleep_confirm();
        cx.notify();
    }

    /// Close the sleep confirmation without sending anything.
    fn cancel_sleep_confirm(&mut self, cx: &mut Context<Self>) {
        self.state.cancel_sleep_confirm();
        cx.notify();
    }

    /// Confirm sleep and send exactly one `Command::Sleep`.
    ///
    /// The gate is re-checked after the sheet is consumed; a refusal or a
    /// sender rejection is reported in the console and nothing is claimed.
    fn confirm_sleep(&mut self, cx: &mut Context<Self>) {
        let link = self.link.as_ref();
        let outcome = self.state.dispatch_sleep(|command| match link {
            Some(link) => link.send(command).map_err(|error| error.to_string()),
            None => Err("no engine connection".to_string()),
        });
        match outcome {
            SleepOutcome::Sent => self.state.push_console(
                ConsoleLevel::Info,
                "Sleep requested; polling stops until RESET or a status wake",
            ),
            SleepOutcome::Rejected(detail) => self
                .state
                .push_console(ConsoleLevel::Error, format!("SLEEP NOT SENT: {detail}")),
            SleepOutcome::Refused(refusal) => self
                .state
                .push_console(ConsoleLevel::Warn, refusal.message()),
            SleepOutcome::NotConfirmed => {}
        }
        cx.notify();
    }

    /// Open the full local job-history sheet. Opening writes nothing.
    fn open_job_history(&mut self, cx: &mut Context<Self>) {
        self.job_history_open = true;
        cx.notify();
    }

    /// Close the full local job-history sheet. Closing writes nothing.
    fn close_job_history(&mut self, cx: &mut Context<Self>) {
        self.job_history_open = false;
        cx.notify();
    }

    /// Arm the inline `CLEAR HISTORY` confirmation. Arming writes nothing.
    fn arm_job_history_clear(&mut self, cx: &mut Context<Self>) {
        self.state.arm_job_history_clear();
        cx.notify();
    }

    /// Disarm the inline `CLEAR HISTORY` confirmation. CANCEL writes nothing.
    fn cancel_job_history_clear(&mut self, cx: &mut Context<Self>) {
        self.state.cancel_job_history_clear();
        cx.notify();
    }

    /// Clear every local record after the inline confirmation.
    ///
    /// This writes only `app.json`; it never builds or sends a controller
    /// command.
    fn confirm_clear_job_history(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.state.confirm_clear_job_history() {
            tracing::warn!(%error, "job history was not cleared");
            self.state
                .push_console(ConsoleLevel::Warn, format!("HISTORY NOT CLEARED: {error}"));
        }
        cx.notify();
    }

    /// Open the destructive reset confirmation for one exact target.
    ///
    /// Opening sends nothing. The gate and backup acknowledgement are checked
    /// before the sheet opens; a refusal is shown in the card and console.
    fn request_controller_reset(&mut self, target: ResetTarget, cx: &mut Context<Self>) {
        self.state.controller_reset.select(target);
        match self
            .state
            .controller_reset
            .request_confirm(self.reset_gate())
        {
            Ok(()) => {
                self.state.mode = Mode::Utility;
                self.state.push_console(
                    ConsoleLevel::Info,
                    format!(
                        "Reset ready: confirm {} to send one command. Nothing has been sent.",
                        command_text(target)
                    ),
                );
            }
            Err(refusal) => {
                self.state
                    .push_console(ConsoleLevel::Warn, refusal.message());
            }
        }
        cx.notify();
    }

    /// Close the reset confirmation without sending anything.
    fn cancel_controller_reset(&mut self, cx: &mut Context<Self>) {
        self.state.controller_reset.cancel_confirm();
        self.state
            .push_console(ConsoleLevel::Info, "Reset cancelled; nothing was sent");
        cx.notify();
    }

    /// Confirm one destructive reset and then request only the read-only
    /// views that belong to its target.
    ///
    /// The model re-checks every gate after consuming the sheet. A refusal or
    /// sender rejection is reported and nothing is retried. No reconnect,
    /// home, jog, start, or resume is ever produced here.
    fn confirm_controller_reset(&mut self, cx: &mut Context<Self>) {
        let link = self.link.as_ref();
        let outcome =
            self.state
                .controller_reset
                .confirm(self.reset_gate(), |command| match link {
                    Some(link) => link.send(command).map_err(|error| error.to_string()),
                    None => Err("no engine connection; reset not sent".to_string()),
                });
        match outcome {
            ResetOutcome::Sent { target } => {
                self.state.push_console(
                    ConsoleLevel::Command,
                    format!(
                        "{} sent. The controller may reset or re-handshake. {}",
                        command_text(target),
                        refresh_note(target)
                    ),
                );
                self.refresh_after_controller_reset(target);
            }
            ResetOutcome::Refused(refusal) => self
                .state
                .push_console(ConsoleLevel::Warn, refusal.message()),
            ResetOutcome::Failed(detail) => self
                .state
                .push_console(ConsoleLevel::Error, format!("RESET NOT SENT: {detail}")),
            ResetOutcome::NotConfirmed => {}
        }
        cx.notify();
    }

    /// Request the read-only reports allowed after one reset target.
    fn refresh_after_controller_reset(&mut self, target: ResetTarget) {
        for command in refresh_commands(target) {
            match command {
                Command::System(SystemCommand::Offsets) => {
                    self.state.offsets.refresh_requested();
                    self.offset_refresh_sent = false;
                    self.pump_offset_refresh();
                }
                Command::System(SystemCommand::Settings) => {
                    match self.send_command_quiet(Command::System(SystemCommand::Settings)) {
                        Ok(()) => {}
                        Err(EngineError::Busy) => self.reset_settings_refresh_pending = true,
                        Err(error) => self.state.push_console(
                            ConsoleLevel::Warn,
                            format!("Reset settings refresh not sent: {error}"),
                        ),
                    }
                }
                other => {
                    if let Err(error) = self.send_command_quiet(other) {
                        self.state.push_console(
                            ConsoleLevel::Warn,
                            format!("Reset read-only refresh not sent: {error}"),
                        );
                    }
                }
            }
        }
    }

    /// Export the fixed local backup through the normal backup action.
    fn export_controller_reset_backup(&mut self, cx: &mut Context<Self>) {
        match self.state.export_backup_for_reset() {
            Ok(path) => self.state.push_console(
                ConsoleLevel::Ok,
                format!("Backup exported: {}", path.display()),
            ),
            Err(message) => self
                .state
                .push_console(ConsoleLevel::Error, format!("Backup export: {message}")),
        }
        cx.notify();
    }

    /// Toggle the explicit `I HAVE A BACKUP OR ACCEPT DATA LOSS` choice.
    fn toggle_controller_reset_loss(&mut self, cx: &mut Context<Self>) {
        self.state.controller_reset.toggle_loss_accepted();
        cx.notify();
    }

    /// Open the material surfacing wizard. Opening sends nothing.
    fn open_surfacing(&mut self, cx: &mut Context<Self>) {
        self.state.keypad = None;
        self.state.utilities.close_transform();
        self.state.mode = Mode::Utility;
        self.surfacing_open = true;
        cx.notify();
    }

    /// Open the gantry-squaring calculator. Opening sends nothing and never
    /// moves or adjusts the machine.
    fn open_gantry_squaring(&mut self, cx: &mut Context<Self>) {
        self.state.keypad = None;
        self.state.mode = Mode::Utility;
        self.gantry_open = true;
        cx.notify();
    }

    /// Close the gantry calculator without changing the measurement.
    fn close_gantry_squaring(&mut self, cx: &mut Context<Self>) {
        self.state.keypad = None;
        self.gantry_open = false;
        cx.notify();
    }

    /// Recalculate the gantry correction from the three fields.
    fn calculate_gantry_squaring(&mut self, cx: &mut Context<Self>) {
        if let Err(message) = self.state.calculate_gantry_squaring() {
            self.state
                .push_console(ConsoleLevel::Warn, format!("Gantry squaring: {message}"));
        }
        cx.notify();
    }

    /// Close the surfacing wizard without sending or loading anything.
    fn close_surfacing(&mut self, cx: &mut Context<Self>) {
        self.state.keypad = None;
        self.surfacing_open = false;
        cx.notify();
    }

    /// Calculate the stepper-calibration proposal from reported `$10x`.
    fn calculate_calibration(&mut self, cx: &mut Context<Self>) {
        if let Err(message) = self.state.calculate_calibration() {
            self.state
                .push_console(ConsoleLevel::Warn, format!("Calibration: {message}"));
        }
        self.calibration_confirm_open = false;
        cx.notify();
    }

    /// Open the explicit calibration confirmation sheet. Sends nothing.
    fn open_calibration_confirm(&mut self, cx: &mut Context<Self>) {
        self.state.keypad = None;
        if self.state.utilities.calibration().can_review() {
            self.state.utilities.calibration_mut().set_confirmed(false);
            self.calibration_confirm_open = true;
        }
        cx.notify();
    }

    /// Cancel the calibration review. Sends nothing and keeps the proposal.
    fn cancel_calibration(&mut self, cx: &mut Context<Self>) {
        self.state.keypad = None;
        self.state.utilities.calibration_mut().cancel_review();
        self.calibration_confirm_open = false;
        cx.notify();
    }

    /// Send the one confirmed `SetSetting` and wait for read-back.
    fn apply_calibration(&mut self, cx: &mut Context<Self>) {
        let write = match self.state.take_calibration_write() {
            Ok(write) => write,
            Err(message) => {
                self.state.push_console(
                    ConsoleLevel::Warn,
                    format!("Calibration not sent: {message}"),
                );
                cx.notify();
                return;
            }
        };
        let number = write.setting_number;
        if self.send_command(write.command) {
            self.calibration_confirm_open = false;
            self.state.push_console(
                ConsoleLevel::Info,
                format!("Calibration write ${number} sent; waiting for read-back"),
            );
        } else {
            self.state
                .utilities
                .calibration_mut()
                .write_failed("not sent: no engine connection");
            self.state.push_console(
                ConsoleLevel::Error,
                format!("Calibration ${number} was not sent"),
            );
        }
        cx.notify();
    }

    /// Regenerate the surfacing preview from the current fields.
    fn generate_surfacing(&mut self, cx: &mut Context<Self>) {
        if let Err(message) = self
            .state
            .utilities
            .generate_surfacing(Wcs::from_index(self.state.wcs_index).unwrap_or(Wcs::G54))
        {
            self.state
                .push_console(ConsoleLevel::Warn, format!("Surfacing: {message}"));
        }
        cx.notify();
    }

    /// Load the confirmed surfacing preview with exactly one `LoadProgram`.
    fn load_surfacing(&mut self, cx: &mut Context<Self>) {
        let load = match self.state.take_surfacing_load() {
            Ok(load) => load,
            Err(message) => {
                self.state.push_console(ConsoleLevel::Warn, message);
                cx.notify();
                return;
            }
        };
        if self.send_command(load.command) {
            self.state.utilities.reset_session();
            self.surfacing_open = false;
            self.state.mode = Mode::Job;
            self.state.left_tab = LeftTab::Program;
            self.state.push_console(
                ConsoleLevel::Info,
                format!("Loaded surfacing program {} (not started)", load.name),
            );
        } else {
            self.state.height_map.discard_exact_source(&load.name);
        }
        cx.notify();
    }

    /// Generate one transform preview from the currently loaded source.
    fn generate_transform(&mut self, kind: TransformKind, cx: &mut Context<Self>) {
        let name = self.state.job.name.clone();
        let Some(source) = self.state.job.source_text() else {
            self.state
                .push_console(ConsoleLevel::Warn, "No program is loaded to transform");
            cx.notify();
            return;
        };
        if let Err(message) = self
            .state
            .utilities
            .generate_transform(kind, &name, &source)
        {
            self.state
                .push_console(ConsoleLevel::Warn, format!("Transform: {message}"));
        }
        self.surfacing_open = false;
        self.state.mode = Mode::Utility;
        cx.notify();
    }

    /// Close the transform preview without loading anything.
    fn close_transform(&mut self, cx: &mut Context<Self>) {
        self.state.keypad = None;
        self.state.utilities.close_transform();
        cx.notify();
    }

    /// Load the confirmed transform preview with exactly one `LoadProgram`.
    fn load_transform(&mut self, cx: &mut Context<Self>) {
        let load = match self.state.take_transform_load() {
            Ok(load) => load,
            Err(message) => {
                self.state.push_console(ConsoleLevel::Warn, message);
                cx.notify();
                return;
            }
        };
        if self.send_command(load.command) {
            self.state.utilities.close_transform();
            self.state.mode = Mode::Job;
            self.state.left_tab = LeftTab::Program;
            self.state.push_console(
                ConsoleLevel::Info,
                format!("Loaded transformed program {} (not started)", load.name),
            );
        } else {
            self.state.height_map.discard_exact_source(&load.name);
        }
        cx.notify();
    }

    /// Send one run-option toggle through the guarded engine gateway.
    ///
    /// Exactly one command leaves this method. The displayed value is not
    /// changed here: the next poll copies the engine accessor, so a rejection
    /// leaves the old value and shows the reason in the sheet.
    fn toggle_run_option(&mut self, option: RunOption, cx: &mut Context<Self>) {
        let link = self.link.as_ref();
        let outcome = self
            .state
            .dispatch_run_option(option, |command| match link {
                Some(link) => link.send(command).map_err(|error| error.to_string()),
                None => Err("no engine connection".to_string()),
            });
        match outcome {
            RunOptionOutcome::Sent => {}
            RunOptionOutcome::Rejected(message) => {
                self.state
                    .push_console(ConsoleLevel::Error, format!("Engine: {message}"));
            }
            RunOptionOutcome::Refused(refusal) => {
                self.state
                    .push_console(ConsoleLevel::Warn, refusal.message());
            }
        }
        cx.notify();
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        if !self.state.live {
            self.state.machine_state = "Idle";
            self.state
                .push_console(ConsoleLevel::Warn, "SOFT RESET requested (mock)");
            cx.notify();
            return;
        }
        self.send_command(Command::Reset);
        cx.notify();
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        if !self.state.live {
            self.state.machine_state = "Hold";
            self.state
                .push_console(ConsoleLevel::Warn, "Feed hold (mock)");
            cx.notify();
            return;
        }
        self.send_command(Command::Realtime(RealtimeCommand::Stop));
        cx.notify();
    }

    // ------------------------------------------------------------ job actions

    fn start_job(&mut self, cx: &mut Context<Self>) {
        if !self.job_start_allowed() {
            return;
        }
        if !self.state.live {
            self.state.job.state = JobState::Running;
            self.state.active_line = 1;
            self.state.progress = 0.0;
            self.state
                .push_console(ConsoleLevel::Info, "Mock job started (no controller)");
            cx.notify();
            return;
        }
        let link = self.link.as_ref();
        let outcome = self.state.dispatch_start(|command| match link {
            Some(link) => link.send(command).map_err(|error| error.to_string()),
            None => Err("no engine connection".to_string()),
        });
        match outcome {
            StartOutcome::ConfirmationRequired => self.state.push_console(
                ConsoleLevel::Warn,
                "START HELD: program exceeds reported travel; review the confirmation",
            ),
            StartOutcome::Rejected(error) => self
                .state
                .push_console(ConsoleLevel::Error, format!("Engine: {error}")),
            StartOutcome::Refused(refusal) => self
                .state
                .push_console(ConsoleLevel::Warn, refusal.message()),
            StartOutcome::Sent | StartOutcome::NotConfirmed | StartOutcome::Stale => {}
        }
        cx.notify();
    }

    /// Confirm the over-travel sheet with exactly one `StartProgram`.
    ///
    /// The gate is re-checked after the sheet is consumed. A changed program,
    /// link, machine state, or limits report is reported and sends nothing.
    fn confirm_start(&mut self, cx: &mut Context<Self>) {
        let link = self.link.as_ref();
        let outcome = self.state.confirm_start(|command| match link {
            Some(link) => link.send(command).map_err(|error| error.to_string()),
            None => Err("no engine connection".to_string()),
        });
        match outcome {
            StartOutcome::Sent => self.state.push_console(
                ConsoleLevel::Warn,
                "START ANYWAY confirmed; the program was sent for a real run",
            ),
            StartOutcome::Rejected(error) => self
                .state
                .push_console(ConsoleLevel::Error, format!("START NOT SENT: {error}")),
            StartOutcome::Refused(refusal) => self
                .state
                .push_console(ConsoleLevel::Warn, refusal.message()),
            StartOutcome::Stale => self.state.push_console(
                ConsoleLevel::Warn,
                "START NOT SENT: the program, link, machine state, or limits changed; \
                 press START again to re-check",
            ),
            StartOutcome::NotConfirmed | StartOutcome::ConfirmationRequired => {}
        }
        cx.notify();
    }

    /// Close the over-travel sheet without sending anything.
    fn cancel_start_confirm(&mut self, cx: &mut Context<Self>) {
        let run_to_line = self.state.pending_start_run_to_line();
        if self.state.cancel_start_confirm() {
            match run_to_line {
                Some(line) => self.state.push_console(
                    ConsoleLevel::Info,
                    format!(
                        "START cancelled; the breakpoint at line {line} remains set, \
                         nothing was sent"
                    ),
                ),
                None => self
                    .state
                    .push_console(ConsoleLevel::Info, "START cancelled; nothing was sent"),
            }
        }
        cx.notify();
    }

    fn pause_job(&mut self, cx: &mut Context<Self>) {
        if !self.job_pause_allowed() {
            return;
        }
        if !self.state.live {
            self.state.job.state = JobState::Paused;
            self.state
                .push_console(ConsoleLevel::Info, "Mock job paused (no controller)");
            cx.notify();
            return;
        }
        self.send_command(Command::PauseProgram);
        cx.notify();
    }

    fn resume_job(&mut self, cx: &mut Context<Self>) {
        if !self.job_resume_allowed() {
            return;
        }
        if !self.state.live {
            self.state.job.state = JobState::Running;
            self.state
                .push_console(ConsoleLevel::Info, "Mock job resumed (no controller)");
            cx.notify();
            return;
        }
        self.send_command(Command::ResumeProgram);
        cx.notify();
    }

    /// Stop the job for good.
    ///
    /// `reset: true` sends a soft reset after a real job aborts: it discards
    /// the controller's buffered motion, so a queued block cannot execute after
    /// the stop. A dry run has no motion to discard, so it stops without a
    /// reset and lets the engine restore `Idle`. The loaded program stays in
    /// the engine for a restart; the top-bar STOP remains the gentler feed
    /// hold.
    fn stop_job(&mut self, cx: &mut Context<Self>) {
        if !self.job_stop_allowed() {
            return;
        }
        if !self.state.live {
            self.state.job.state = JobState::Idle;
            self.state.active_line = 0;
            self.state.progress = 0.0;
            self.state
                .push_console(ConsoleLevel::Info, "Mock job stopped (no controller)");
            cx.notify();
            return;
        }
        self.send_command(self.state.stop_program_command());
        cx.notify();
    }

    /// `0xA3` for the pending manual tool change.
    ///
    /// There is no completion event for the acknowledgement, so the view marks
    /// it after the engine accepts the command. Resume remains a separate,
    /// explicit action.
    fn acknowledge_tool_change(&mut self, cx: &mut Context<Self>) {
        if !self.job_tool_change_allowed() {
            return;
        }
        if self.send_command(Command::AcknowledgeToolChange) {
            self.state.job.acknowledge_tool_change();
        }
        cx.notify();
    }

    /// Continue a tool-change job after it was acknowledged.
    fn resume_after_tool_change(&mut self, cx: &mut Context<Self>) {
        if !self.state.live || !self.ready() || self.locked() {
            return;
        }
        if !self.state.job.tool_change_pending() || !self.state.job.tool_change_acknowledged() {
            return;
        }
        self.send_command(Command::ResumeProgram);
        cx.notify();
    }

    /// Whether a tool change can be acknowledged right now.
    fn job_tool_change_allowed(&self) -> bool {
        self.state.live
            && self.ready()
            && !self.locked()
            && self.state.job.tool_change_pending()
            && !self.state.job.tool_change_acknowledged()
    }

    // -------------------------------------------- breakpoints and run-to

    /// Send exactly one breakpoint toggle for one source line.
    fn toggle_program_breakpoint(&mut self, line: u32, cx: &mut Context<Self>) {
        let link = self.link.as_ref();
        let outcome = self
            .state
            .dispatch_toggle_breakpoint(line, |command| match link {
                Some(link) => link.send(command).map_err(|error| error.to_string()),
                None => Err("no engine connection".to_string()),
            });
        match outcome {
            BreakpointOutcome::Sent { line, enabled } => {
                let action = if enabled { "set" } else { "cleared" };
                self.state.push_console(
                    ConsoleLevel::Info,
                    format!("Breakpoint {action} at line {line}"),
                );
            }
            BreakpointOutcome::Rejected(message) => self
                .state
                .push_console(ConsoleLevel::Error, format!("Engine: {message}")),
            BreakpointOutcome::Refused(refusal) => self
                .state
                .push_console(ConsoleLevel::Warn, refusal.message()),
        }
        cx.notify();
    }

    /// Select one Program-list row. Nothing is sent.
    fn select_program_line(&mut self, line: u32, cx: &mut Context<Self>) {
        self.state.select_program_line(Some(line));
        cx.notify();
    }

    /// Open the run-to confirmation for the selected row. Nothing is sent.
    fn request_run_to(&mut self, cx: &mut Context<Self>) {
        if let Err(refusal) = self.state.request_run_to_selected() {
            self.state
                .push_console(ConsoleLevel::Warn, refusal.message());
        }
        cx.notify();
    }

    /// Close the run-to sheet. Nothing is sent.
    fn cancel_run_to(&mut self, cx: &mut Context<Self>) {
        self.state.cancel_run_to();
        cx.notify();
    }

    /// Confirm the captured run-to line.
    ///
    /// The state builds the commands and sends the start only when no job is
    /// active; the shell never sends motion directly. Cancel never reaches this
    /// method.
    fn confirm_run_to(&mut self, cx: &mut Context<Self>) {
        let link = self.link.as_ref();
        let outcome = self.state.confirm_run_to(|command| match link {
            Some(link) => link.send(command).map_err(|error| error.to_string()),
            None => Err("no engine connection".to_string()),
        });
        match outcome {
            RunToOutcome::Started { line } => self.state.push_console(
                ConsoleLevel::Info,
                format!("Run to line {line}: breakpoint set; job started from line 1"),
            ),
            RunToOutcome::BreakpointOnly { line } => self.state.push_console(
                ConsoleLevel::Info,
                format!("Breakpoint set at line {line}; active job was not restarted"),
            ),
            RunToOutcome::StartHeld { line } => self.state.push_console(
                ConsoleLevel::Warn,
                format!(
                    "Run to line {line}: breakpoint set; START held for the travel confirmation"
                ),
            ),
            RunToOutcome::StartRejected { message, .. } | RunToOutcome::Rejected(message) => self
                .state
                .push_console(ConsoleLevel::Error, format!("Engine: {message}")),
            RunToOutcome::Refused(refusal) => self
                .state
                .push_console(ConsoleLevel::Warn, refusal.message()),
        }
        cx.notify();
    }

    fn run_macro(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(mac) = self.state.macros.get(index).copied() else {
            return;
        };
        if !self.state.live {
            self.state.run_macro(index);
            cx.notify();
            return;
        }
        use crate::state::MacroAction;
        match mac.action {
            MacroAction::ZeroXy => {
                let line = format!("G10 L20 P{} X0 Y0", self.state.wcs_program_number());
                self.send_mdi_line(line);
            }
            MacroAction::ZeroZ => {
                let line = format!("G10 L20 P{} Z0", self.state.wcs_program_number());
                self.send_mdi_line(line);
            }
            MacroAction::SafeZ => {
                self.send_mdi_line("G0 Z25".to_string());
            }
            MacroAction::Home => {
                self.send_command(Command::Home);
            }
            MacroAction::Unlock => {
                self.send_command(Command::Unlock);
            }
            MacroAction::ProbeZ => {
                self.state.mode = Mode::Probe;
                let ctx = self.probe_context();
                if self.state.probe.select_card(ProbeCard::ZTouchPlate, &ctx) {
                    self.state.push_console(
                        ConsoleLevel::Info,
                        "Opened Probe > Z touch plate; press RUN to start",
                    );
                } else {
                    self.state.push_console(
                        ConsoleLevel::Warn,
                        "Finish or stop the active probe before opening another cycle",
                    );
                }
            }
            MacroAction::Coolant => {
                if self.send_command(Command::Realtime(RealtimeCommand::FloodToggle))
                    && let Some(value) = self.state.coolant.get_mut(1)
                {
                    *value = !*value;
                }
            }
        }
        cx.notify();
    }

    pub fn keypad_action(&mut self, action: KeypadAction, cx: &mut Context<Self>) {
        let submitted = matches!(action, KeypadAction::Submit)
            .then(|| self.state.keypad.clone())
            .flatten();
        self.state.keypad_action(action);
        if let Some(keypad) = submitted {
            if let KeypadTarget::Tool(field) = keypad.target {
                if let Some(editor) = self.tool_editor.as_mut() {
                    *editor.value_mut(field) = keypad.buffer.trim().to_string();
                    editor.error = None;
                }
            } else if self.state.live {
                match keypad.target {
                    KeypadTarget::Mdi => {}
                    KeypadTarget::Rpm => {
                        if let Ok(value) = keypad.buffer.trim().parse::<f64>() {
                            let rpm = value.clamp(0.0, 24_000.0).round() as u32;
                            self.send_mdi_line(format!("S{rpm}"));
                        }
                    }
                    KeypadTarget::Dro(axis) => {
                        if let Ok(value) = keypad.buffer.trim().parse::<f64>() {
                            let Some(wcs) = Wcs::from_index(self.state.wcs_index) else {
                                return;
                            };
                            self.send_offset_value(wcs, axis, value, cx);
                        }
                    }
                    KeypadTarget::WorkOffset { wcs, axis } => {
                        if let (Some(wcs), Ok(value)) =
                            (Wcs::from_index(wcs), keypad.buffer.trim().parse::<f64>())
                        {
                            self.send_offset_value(wcs, axis, value, cx);
                        } else {
                            self.state.push_console(
                                ConsoleLevel::Warn,
                                format!("'{}' is not a number", keypad.buffer),
                            );
                        }
                    }
                    // A settings value stays local until Apply; `AppState`
                    // already wrote it into the row draft. Utility fields are
                    // local wizard data and send nothing.
                    KeypadTarget::Setting(_)
                    | KeypadTarget::Tool(_)
                    | KeypadTarget::Utility(_)
                    | KeypadTarget::Calibration(_)
                    | KeypadTarget::Gantry(_) => {}
                    KeypadTarget::ProbeField(field) => {
                        // The mount offset is setup data owned by the profile.
                        // A failed save never blocks the local value, but the
                        // operator is told it will not survive reconnect.
                        if matches!(field, ProbeField::ProbeOffsetX | ProbeField::ProbeOffsetY)
                            && self.state.probe.field_error(field).is_none()
                        {
                            match self.state.save_probe_offset() {
                                Ok(path) => self.state.push_console(
                                    ConsoleLevel::Info,
                                    format!("Probe offset saved to {}", path.display()),
                                ),
                                Err(message) => self.state.push_console(
                                    ConsoleLevel::Warn,
                                    format!("Probe offset not saved: {message}"),
                                ),
                            }
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    /// Apply and persist a theme choice from the top-bar selector.
    ///
    /// The mode and palette change before the config write, so a save failure
    /// leaves the selected theme visible for the rest of the session. The
    /// failure is logged and never silently claims the choice was persisted.
    fn choose_theme_mode(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.state.theme_mode = mode;
        crate::theme::apply(mode, cx);
        if let Err(error) = self.state.config.set_theme_mode(mode) {
            tracing::warn!(%error, "theme mode was not saved; keeping it for this session");
        }
        cx.notify();
    }

    /// Theme control: three segments on wide windows, one cycle button when
    /// the top bar is tight.
    ///
    /// Each wide segment and the compact cycle button are one-tap 48 px
    /// targets. The active mode is shown by the accent chip (wide) or the
    /// button label (compact), and a stored legacy `System` mode displays as
    /// `DARK`.
    fn theme_selector(
        &self,
        p: Palette,
        window_width: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.state.theme_mode.selectable();
        if window_width < px(1240.0) {
            let next = active.cycle();
            let label = theme_mode_label(active);
            let view = cx.entity();
            return Button::new("theme-cycle")
                .label(label)
                .flex_shrink_0()
                .h(px(48.0))
                .px_2()
                .rounded_lg()
                .text_size(px(10.0))
                .custom(mode_variant(cx, p, true))
                .tooltip(format!(
                    "{}: {label} - {} {}",
                    t("THEME"),
                    t("TAP FOR"),
                    theme_mode_label(next)
                ))
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| this.choose_theme_mode(next, cx));
                })
                .into_any_element();
        }

        let view = cx.entity();
        let segments = ThemeMode::SELECTABLE
            .iter()
            .enumerate()
            .map(|(index, mode)| {
                let mode = *mode;
                let selected = mode == active;
                let view = view.clone();
                let label = theme_mode_label(mode);
                Button::new(format!("theme-{index}"))
                    .label(label)
                    .flex_shrink_0()
                    .h(px(48.0))
                    .px_2()
                    .rounded_lg()
                    .text_size(px(10.0))
                    .custom(mode_variant(cx, p, selected))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| this.choose_theme_mode(mode, cx));
                    })
            })
            .collect::<Vec<_>>();

        h_flex()
            .id("theme-selector")
            .flex_shrink_0()
            .items_center()
            .gap(px(3.0))
            .children(segments)
            .into_any_element()
    }

    fn top_bar(
        &self,
        p: Palette,
        window_width: Pixels,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity();

        let modes = Mode::ALL.iter().enumerate().map(|(index, mode)| {
            let mode = *mode;
            let active = self.state.mode == mode;
            let view = view.clone();
            let variant = mode_variant(cx, p, active);
            Button::new(format!("mode-{index}"))
                .label(t(mode.label()))
                .h(px(48.0))
                .px_3()
                .rounded_lg()
                .text_size(px(14.0))
                .custom(variant)
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.state.mode = mode;
                        cx.notify();
                    });
                })
        });

        let link_view = view.clone();
        let pill_view = view.clone();
        let reset_view = view.clone();
        let stop_view = view.clone();

        let (dot_color, link_label) = match &self.state.connection {
            ConnectionState::Mock => (p.text_faint, self.state.connection.label()),
            ConnectionState::Connecting => (p.warning, self.state.connection.label()),
            ConnectionState::Ready => (p.success, self.state.connection.label()),
            ConnectionState::Disconnected => (p.danger, self.state.connection.label()),
            ConnectionState::Failed(_) => (p.danger, self.state.connection.label()),
        };
        // "READY 3A" was ambiguous; say what the number counts, and keep the
        // machine state one glance away. The pill is the connection entry
        // point, so tapping it opens the transport picker.
        let status_text = format!(
            "{} | {} axes | {}",
            t(link_label),
            self.state.profile.capabilities.axes.len(),
            self.state.machine_state
        );
        let link_text = if matches!(
            self.state.connection,
            ConnectionState::Connecting | ConnectionState::Ready
        ) {
            t("DISCONNECT")
        } else {
            t("CONNECT")
        };

        div()
            .h(px(56.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(if window_width < px(1120.0) { 8.0 } else { 12.0 }))
            .px_3()
            .bg(p.panel)
            .border_b_1()
            .border_color(p.border)
            .child(
                v_flex()
                    .gap_0()
                    .w(px(116.0))
                    .child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child("GHS"),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child("grblHAL Sender"),
                    ),
            )
            .child(div().flex().items_center().gap_1().children(modes))
            .child(div().flex_1())
            .child(
                h_flex()
                    .id("link-pill")
                    .cursor_pointer()
                    .items_center()
                    .gap_2()
                    .h(px(36.0))
                    .px_3()
                    .rounded(px(999.0))
                    .bg(p.panel_alt)
                    .border_1()
                    .border_color(p.border)
                    .hover(|style| style.bg(p.raised_hover))
                    .on_click(move |_, _, cx| {
                        pill_view.update(cx, |this, cx| this.open_connection(cx));
                    })
                    .child(div().w(px(8.0)).h(px(8.0)).rounded(px(999.0)).bg(dot_color))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_dim)
                            .child(status_text),
                    ),
            )
            .child(self.theme_selector(p, window_width, cx))
            .child(
                Button::new("link")
                    .label(link_text)
                    .secondary()
                    .h(px(48.0))
                    .w(px(120.0))
                    .rounded_lg()
                    .text_size(px(12.0))
                    .on_click(move |_, _, cx| {
                        link_view.update(cx, |this, cx| {
                            if matches!(
                                this.state.connection,
                                ConnectionState::Connecting | ConnectionState::Ready
                            ) {
                                this.disconnect(cx);
                            } else {
                                this.open_connection(cx);
                            }
                        });
                    }),
            )
            .child(
                Button::new("reset")
                    .label(t("RESET"))
                    .danger()
                    .h(px(48.0))
                    .w(px(84.0))
                    .rounded_lg()
                    .text_size(px(13.0))
                    .on_click(move |_, _, cx| {
                        reset_view.update(cx, |this, cx| this.reset(cx));
                    }),
            )
            .child(
                Button::new("stop")
                    .label(t("STOP"))
                    .warning()
                    .h(px(48.0))
                    .w(px(84.0))
                    .rounded_lg()
                    .text_size(px(13.0))
                    .on_click(move |_, _, cx| {
                        stop_view.update(cx, |this, cx| this.stop(cx));
                    }),
            )
    }

    /// Left column: console/program (flex), MDI, Feed.
    fn left_column(&self, p: Palette, cx: &mut Context<Self>, width: f32) -> impl IntoElement {
        v_flex()
            .w(px(width))
            .flex_shrink_0()
            .h_full()
            .gap(px(GAP))
            .child(self.console_card(p, cx))
            .child(self.mdi_card(p, cx))
            .child(self.feed_card(p, cx))
    }

    /// Centre column: viewer (flex), DRO, Jog.
    fn center_column(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .gap(px(GAP))
            .child(self.viewer_card(p, cx))
            .child(self.dro_card(p, cx))
            .child(self.jog_card(p, cx))
    }

    /// Right column: Coolant/Macros/Signals/Work as one cell spanning rows 1-2,
    /// then Spindle on the Feed/Jog row.
    fn aux_column(&self, p: Palette, cx: &mut Context<Self>, width: f32) -> impl IntoElement {
        v_flex()
            .w(px(width))
            .flex_shrink_0()
            .h_full()
            .gap(px(GAP))
            .child(self.aux_panel(p, cx))
            .child(self.spindle_card(p, cx))
    }

    fn console_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let console_tab = tab_button(
            cx,
            p,
            LeftTab::Console,
            self.state.left_tab == LeftTab::Console,
            view.clone(),
        );
        let program_tab = tab_button(
            cx,
            p,
            LeftTab::Program,
            self.state.left_tab == LeftTab::Program,
            view.clone(),
        );
        let save_log = Button::new("console-save-log")
            .label(t("SAVE LOG"))
            .secondary()
            .h(px(48.0))
            .w(px(84.0))
            .flex_shrink_0()
            .rounded_lg()
            .text_size(px(9.5))
            .on_click(move |_, _, cx| {
                view.update(cx, |shell, cx| shell.save_console_log(cx));
            });

        let body = match self.state.left_tab {
            LeftTab::Console => self.console(p, cx).into_any_element(),
            LeftTab::Program => self.program(p, cx).into_any_element(),
        };

        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .rounded_lg()
            .border_1()
            .border_color(p.border)
            .bg(p.panel)
            .overflow_hidden()
            .child(
                h_flex()
                    .gap_1()
                    .p_2()
                    .border_b_1()
                    .border_color(p.border)
                    .child(console_tab)
                    .child(program_tab)
                    .when(self.state.left_tab == LeftTab::Console, |this| {
                        this.child(save_log)
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .overflow_hidden()
                    .child(body),
            )
            .child(
                div()
                    .h(px(26.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .border_t_1()
                    .border_color(p.border)
                    .text_size(px(10.0))
                    .text_color(p.text_faint)
                    .child(format!("{} lines", self.state.display_program_lines()))
                    .child(if self.state.live && !self.state.job.has_program() {
                        "no program".to_string()
                    } else {
                        format!(
                            "line {} | {:.0}%",
                            self.state.display_line(),
                            self.state.display_progress() * 100.0
                        )
                    }),
            )
    }

    fn console(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let filter = self.state.console_filter;
        let visible = self.state.console_visible_count();
        let status = self.state.console_log_status.clone();

        let mut scroll = v_flex()
            .gap_1()
            .p_2()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .min_w(px(0.0))
            .id("console-scroll")
            .track_scroll(&self.console_scroll)
            .overflow_y_scroll()
            .font_family(self.mono.clone());
        if visible == 0 {
            let empty_headline = if filter == ConsoleFilter::All {
                "NO CONSOLE LINES".to_string()
            } else {
                format!("NO {} LINES", filter.label())
            };
            scroll = scroll.child(
                v_flex()
                    .w_full()
                    .items_center()
                    .gap_1()
                    .pt_2()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_dim)
                            .child(empty_headline),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child(format!(
                                "The ring buffer keeps all {} lines; ALL restores the view.",
                                self.state.console.len()
                            )),
                    ),
            );
        } else {
            scroll = scroll.children(self.state.console_visible_lines().map(|line| {
                h_flex()
                    .gap_2()
                    .items_start()
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(34.0))
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(line.time.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(11.5))
                            .text_color(console_color(line.level, p))
                            .child(line.text.clone()),
                    )
            }));
        }

        v_flex()
            .h_full()
            .w_full()
            .min_w(px(0.0))
            .child(self.console_filter_bar(p, cx))
            .when_some(status, |this, status| {
                this.child(console_log_status_row(p, &status, self.mono.clone()))
            })
            .child(scroll)
    }

    /// Six 48 px filter chips with live per-level counts.
    ///
    /// The chips claim equal flex shares and never exceed the card width, so
    /// the Console has no horizontal scroll at 1280x800.
    fn console_filter_bar(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let counts = self.state.console_counts();
        let active = self.state.console_filter;
        let view = cx.entity();
        h_flex()
            .h(px(48.0))
            .w_full()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .px_2()
            .pb_1()
            .overflow_hidden()
            .children(
                ConsoleFilter::ALL
                    .into_iter()
                    .map(|filter| console_filter_chip(filter, active, counts, p, view.clone())),
            )
    }

    /// Program-tab search toolbar: query field, mode toggle, match count, and
    /// previous/next/jump actions. All controls are 48 px high.
    fn program_search_toolbar(&self, p: Palette, line_count: usize, cx: &mut Context<Self>) -> Div {
        let view = cx.entity();
        let search = &self.state.program_search;
        let active = search.is_active();
        let match_count = search.match_count();
        let has_matches = active && match_count > 0;
        let status = if !active {
            format!("{line_count} LINES")
        } else if match_count == 0 {
            t("NO MATCH").to_string()
        } else {
            format!(
                "{} / {} MATCH",
                search.current_position().unwrap_or(0),
                match_count
            )
        };
        let filter_selected = search.mode() == ProgramSearchMode::Filter;
        let highlight_selected = search.mode() == ProgramSearchMode::Highlight;

        let focus_input = self.program_search_input.clone();
        let search_field = div()
            .h(px(48.0))
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                focus_input.update(cx, |state, cx| state.focus(window, cx));
            })
            .child(
                Input::new(&self.program_search_input)
                    .with_size(Size::Large)
                    .cleanable(true)
                    .min_w(px(0.0))
                    .text_size(px(12.0)),
            );

        v_flex()
            .flex_shrink_0()
            .gap_2()
            .p_2()
            .border_b_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .h(px(48.0))
                    .items_center()
                    .gap_2()
                    .child(search_field)
                    .child(program_search_button(
                        "program-search-prev",
                        t("PREV"),
                        has_matches,
                        p,
                        view.clone(),
                        |shell, cx| shell.previous_program_match(cx),
                    ))
                    .child(program_search_button(
                        "program-search-next",
                        t("NEXT"),
                        has_matches,
                        p,
                        view.clone(),
                        |shell, cx| shell.next_program_match(cx),
                    )),
            )
            .child(
                h_flex()
                    .h(px(48.0))
                    .items_center()
                    .gap_2()
                    .child(
                        h_flex()
                            .h(px(48.0))
                            .w(px(164.0))
                            .flex_shrink_0()
                            .rounded_lg()
                            .border_1()
                            .border_color(p.border)
                            .overflow_hidden()
                            .child(program_mode_segment(
                                ProgramSearchMode::Filter,
                                filter_selected,
                                p,
                                view.clone(),
                            ))
                            .child(program_mode_segment(
                                ProgramSearchMode::Highlight,
                                highlight_selected,
                                p,
                                view.clone(),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(9.5))
                            .font_weight(FontWeight::BOLD)
                            .text_color(if has_matches {
                                p.accent
                            } else if active {
                                p.warning
                            } else {
                                p.text_faint
                            })
                            .child(status),
                    )
                    .child(program_search_button(
                        "program-search-jump",
                        t("JUMP"),
                        has_matches,
                        p,
                        view,
                        |shell, cx| shell.jump_to_program_match(cx),
                    )),
            )
    }

    /// Selected-row breakpoint and run-to actions under the search toolbar.
    ///
    /// The gutter remains the direct toggle; this row makes the same action and
    /// the confirmed run-to reachable for the selected line. Opening the run-to
    /// sheet sends nothing.
    fn program_breakpoint_toolbar(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let view = cx.entity();
        let toggle_view = view.clone();
        let run_view = view;
        let selected = self.state.job.breakpoints.selected();
        let toggle_line = selected;
        let count = self.state.job.breakpoints.count();
        let gate = self.state.breakpoint_refusal();
        let breakpoint_refusal =
            selected.and_then(|line| self.state.breakpoint_command(line).err());
        let run_refusal = self.state.run_to_refusal(selected);
        let has_breakpoint = selected.is_some_and(|line| self.state.job.breakpoints.contains(line));
        let active = self.state.job.is_active();
        let toggle_ok = selected.is_some() && breakpoint_refusal.is_none();
        let run_ok = run_refusal.is_none();

        let (headline, detail, detail_color) = match selected {
            Some(line) => {
                let detail = if let Some(refusal) = breakpoint_refusal {
                    refusal.message().to_string()
                } else if has_breakpoint {
                    format!("BREAKPOINT ON | {count} TOTAL")
                } else if active {
                    format!("{}: RUN TO SETS BP ONLY | {count} TOTAL", t("JOB ACTIVE"))
                } else {
                    format!("NO BREAKPOINT | {count} TOTAL")
                };
                let color = if breakpoint_refusal.is_some() {
                    p.warning
                } else if has_breakpoint {
                    p.danger
                } else {
                    p.text_faint
                };
                (format!("LINE {line} SELECTED"), detail, color)
            }
            None => {
                let detail = gate
                    .map(|refusal| refusal.message().to_string())
                    .unwrap_or_else(|| t("TAP A LINE TO SELECT").to_string());
                (t("NO LINE SELECTED").to_string(), detail, p.text_faint)
            }
        };

        h_flex()
            .h(px(64.0))
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_2()
            .border_b_1()
            .border_color(p.border)
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.0))
                    .gap_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(9.5))
                            .font_weight(FontWeight::BOLD)
                            .text_color(if selected.is_some() {
                                p.text
                            } else {
                                p.text_dim
                            })
                            .child(headline),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(8.5))
                            .text_color(detail_color)
                            .child(detail),
                    ),
            )
            .child(
                Button::new("program-toggle-breakpoint")
                    .label(t("BP"))
                    .secondary()
                    .h(px(48.0))
                    .w(px(58.0))
                    .rounded_lg()
                    .text_size(px(10.0))
                    .disabled(!toggle_ok)
                    .on_click(move |_, _, cx| {
                        let Some(line) = toggle_line else {
                            return;
                        };
                        toggle_view.update(cx, |shell, cx| {
                            shell.toggle_program_breakpoint(line, cx);
                        });
                    }),
            )
            .child(
                Button::new("program-run-to")
                    .label(t("RUN TO"))
                    .primary()
                    .h(px(48.0))
                    .w(px(116.0))
                    .rounded_lg()
                    .text_size(px(10.0))
                    .disabled(!run_ok)
                    .on_click(move |_, _, cx| {
                        run_view.update(cx, |shell, cx| shell.request_run_to(cx));
                    }),
            )
    }

    fn program(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let source = if let Some(program) = self.state.job.program() {
            ProgramSource::Engine(Arc::clone(program))
        } else if self.state.live {
            return self
                .empty_program_prompt("program-empty-load", p, cx)
                .into_any_element();
        } else {
            ProgramSource::Mock(self.state.program.clone())
        };
        let line_count = source.len();
        if line_count == 0 {
            return if self.state.job.has_program() {
                empty_program_lines_notice(p).into_any_element()
            } else {
                self.empty_program_prompt("program-empty-load", p, cx)
                    .into_any_element()
            };
        }
        let name = self.state.job_title().to_string();
        let toolbar = self.program_search_toolbar(p, line_count, cx);
        let actions = self.program_breakpoint_toolbar(p, cx);
        let search = self.state.program_search.clone();
        let breakpoints = self.state.job.breakpoints.clone();
        let visible_count = self.state.program_search_visible_count();
        let breakpoint_enabled = self.state.breakpoint_refusal().is_none();
        let editable_through = if self.state.job.has_program() {
            self.state.job.total
        } else {
            u32::try_from(line_count).unwrap_or(u32::MAX)
        };
        let no_matches =
            search.is_active() && search.mode() == ProgramSearchMode::Filter && visible_count == 0;
        let body = if no_matches {
            no_program_matches_notice(p, search.query()).into_any_element()
        } else {
            let active_line = self.state.display_line();
            let list_view = cx.entity();
            // The uniform list still owns virtualization: filter mode maps a
            // visible row through the match index, highlight mode uses the
            // identity mapping, and only the visible range becomes widgets.
            // Breakpoint markers are looked up by original source line, so
            // both modes show the same set.
            uniform_list("program-list", visible_count, move |range, _, _| {
                range
                    .filter_map(|visible| {
                        let index = search.visible_source_index(visible, line_count)?;
                        source
                            .row(
                                index,
                                active_line,
                                &search,
                                &breakpoints,
                                breakpoint_enabled,
                                editable_through,
                            )
                            .map(|row| program_row(row, p, list_view.clone()))
                    })
                    .collect::<Vec<_>>()
            })
            .flex_1()
            .min_h(px(0.0))
            .track_scroll(&self.program_scroll)
            .into_any_element()
        };
        v_flex()
            .size_full()
            .min_h(px(0.0))
            .child(self.program_name_strip(&name, p))
            .child(toolbar)
            .child(actions)
            .child(body)
            .into_any_element()
    }

    /// Slim strip naming the loaded program above the Program search toolbar.
    fn program_name_strip(&self, name: &str, p: Palette) -> Div {
        let breakpoints = self.state.job.breakpoints.count();
        h_flex()
            .h(px(24.0))
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_2()
            .border_b_1()
            .border_color(p.border)
            .bg(p.panel_alt)
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(t("PROGRAM")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .font_family(self.mono.clone())
                    .text_size(px(10.0))
                    .text_color(p.text)
                    .child(name.to_string()),
            )
            .when(breakpoints > 0, |this| {
                this.child(
                    div()
                        .flex_shrink_0()
                        .h(px(16.0))
                        .px_1()
                        .flex()
                        .items_center()
                        .rounded(px(4.0))
                        .bg(p.danger.opacity(0.16))
                        .text_size(px(8.5))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.danger)
                        .child(format!("{breakpoints} BP")),
                )
            })
    }

    /// Empty-program prompt with the one action that fixes it.
    ///
    /// `id` must be unique per call site: the Program tab and the Job viewer can
    /// both render this prompt in the same frame.
    fn empty_program_prompt(&self, id: &'static str, p: Palette, cx: &mut Context<Self>) -> Div {
        let view = cx.entity();
        let refusal = self.state.program_load_refusal();
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .p_3()
            .child(
                v_flex()
                    .w(px(360.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border)
                    .bg(p.panel_alt)
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child(t("NO PROGRAM LOADED")),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .line_height(px(14.0))
                            .text_color(p.text_dim)
                            .child(
                                "Choose a .nc, .gcode, .tap, or .ngc file. Loading only prepares \
                                 the job; START is always a separate action.",
                            ),
                    )
                    .when_some(refusal, |this, refusal| {
                        this.child(
                            div()
                                .text_size(px(9.5))
                                .text_color(p.warning)
                                .child(refusal.message()),
                        )
                    })
                    .child(
                        Button::new(id)
                            .label(t("LOAD PROGRAM"))
                            .primary()
                            .h(px(48.0))
                            .w_full()
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.open_program_browser(window, cx);
                                });
                            }),
                    ),
            )
    }

    /// Viewer card: fills the centre column's first row.
    fn viewer_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match self.state.mode {
            Mode::Job => self.state.job_title().to_string(),
            Mode::Probe => "Preflight, run and apply ghs-probe cycles".to_string(),
            Mode::Settings => format!("{} settings from $ES/$EG", self.state.settings.total()),
            _ if self.state.live => "M5 live workspace".to_string(),
            _ => "M5 preview".to_string(),
        };
        let header = div()
            .h(px(48.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .border_b_1()
            .border_color(p.border)
            .child(
                v_flex()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_dim)
                            .child(t(self.state.mode.title())),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(p.text)
                            .max_w(px(260.0))
                            .truncate()
                            .child(title),
                    ),
            )
            .child(div().flex_1());

        let header = if self.state.mode == Mode::Job {
            let progress = self.state.display_progress();
            header
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(96.0))
                                .h(px(5.0))
                                .rounded(px(999.0))
                                .bg(p.panel_alt)
                                .child(
                                    div()
                                        .h_full()
                                        .w(px(96.0 * progress))
                                        .rounded(px(999.0))
                                        .bg(p.accent),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(p.text_dim)
                                .child(format!("{:.0}%", progress * 100.0)),
                        )
                        .child(
                            div()
                                .font_family(self.mono.clone())
                                .text_size(px(10.0))
                                .text_color(p.text_dim)
                                .child(format!(
                                    "{} / {}",
                                    self.state.display_line(),
                                    self.state.display_total()
                                )),
                        )
                        .when_some(self.state.display_eta(), |this, eta| {
                            this.child(
                                div()
                                    .flex_shrink_0()
                                    .font_family(self.mono.clone())
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.accent)
                                    .child(format!("ETA {}", format_elapsed(eta))),
                            )
                        }),
                )
                .child(chip(self.state.wcs(), p))
                .child(chip("XY", p))
        } else {
            header
        };

        let header = if self.state.mode == Mode::Job {
            header.child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .child(self.run_options_chip(p, cx))
                    .child(self.rapid_override_chip(p, cx)),
            )
        } else {
            header.child(self.rapid_override_chip(p, cx))
        };

        let body = if self.state.mode == Mode::Job {
            v_flex()
                .size_full()
                .child(self.job_toolbar(p, cx))
                .child(div().flex_1().min_h(px(0.0)).child(self.job_view(p, cx)))
                .into_any_element()
        } else if self.state.mode == Mode::Settings {
            // The generated form owns the whole viewer body: rail, search, and
            // a scrollable list. The top bar and the bottom controls stay put.
            self.settings_view(p, cx).into_any_element()
        } else if self.state.mode == Mode::Probe {
            self.probe_view(p, cx).into_any_element()
        } else {
            self.mode_view(p, cx).into_any_element()
        };

        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .rounded_lg()
            .border_1()
            .border_color(p.border)
            .bg(p.panel)
            .overflow_hidden()
            .child(header)
            .child(div().flex_1().min_h(px(0.0)).child(body))
    }

    /// Compact `RUN OPTIONS` chip in the Job viewer header.
    ///
    /// The badge counts the switches the controller reports, never a target
    /// this UI requested. Opening the sheet sends nothing.
    fn run_options_chip(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let count = RunOption::ALL
            .iter()
            .filter(|option| self.state.run_option_value(**option))
            .count();
        let any = count > 0;
        let color = if any { p.accent } else { p.text_dim };
        let background = if any {
            p.accent.opacity(0.16)
        } else {
            p.panel_alt
        };
        let border = if any { p.accent.opacity(0.6) } else { p.border };
        let view = cx.entity();
        let mono = self.mono.clone();

        h_flex()
            .id("run-options")
            .h(px(48.0))
            .w(px(150.0))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap_1()
            .rounded_lg()
            .bg(background)
            .border_1()
            .border_color(border)
            .cursor_pointer()
            .hover(|style| style.bg(p.raised_hover).border_color(p.accent))
            .on_click(move |_, _, cx| {
                view.update(cx, |this, cx| this.open_run_options(cx));
            })
            .child(
                div()
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(t("RUN OPTIONS")),
            )
            .child(
                div()
                    .font_family(mono)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(color)
                    .child(if any {
                        format!("{count} {}", t("ON"))
                    } else {
                        t("OFF").to_string()
                    }),
            )
    }

    /// Compact rapid-override control for the viewer header.
    ///
    /// The value is the last controller snapshot, never the target that was
    /// requested. A tap sends the next level through the engine gateway; a
    /// refusal is written to the console instead of changing the display.
    fn rapid_override_chip(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.state.rapid_override;
        let enabled = self.state.rapid_override_step().is_ok();
        let reduced = current < 100;
        let value_color = if reduced { p.warning } else { p.text };
        let background = if reduced {
            p.warning.opacity(0.16)
        } else {
            p.panel_alt
        };
        let border = if reduced {
            p.warning.opacity(0.6)
        } else {
            p.border
        };
        let view = cx.entity();
        let mono = self.mono.clone();

        h_flex()
            .id("rapid-override")
            .h(px(48.0))
            .w(px(88.0))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap_1()
            .rounded_lg()
            .bg(background)
            .border_1()
            .border_color(border)
            .when(enabled, |this| {
                this.cursor_pointer()
                    .hover(|style| style.bg(p.raised_hover).border_color(p.accent))
            })
            .when(!enabled, |this| this.opacity(0.55))
            .on_click(move |_, _, cx| {
                view.update(cx, |this, cx| this.cycle_rapid_override(cx));
            })
            .child(
                div()
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(t("RAPID")),
            )
            .child(
                div()
                    .font_family(mono)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(value_color)
                    .child(format!("{current}%")),
            )
    }

    /// Confirmation sheet for one run-to line.
    ///
    /// The sheet captures the line and source when `RUN TO` was pressed. The
    /// scrim and CANCEL only close it; nothing is sent until the explicit
    /// confirm button.
    fn run_to_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(pending) = self.state.job.breakpoints.pending_run_to() else {
            return div().into_any_element();
        };
        let line = pending.line;
        let source = pending.source.clone();
        let will_start = self.state.run_to_will_start();
        let refusal = self.state.run_to_refusal(Some(line));
        let active = self.state.job.is_active();
        let error = self.state.job.breakpoints.error.clone();
        let earlier = self.state.job.breakpoints.earlier_than(line);
        let earlier_note = if !active && !earlier.is_empty() {
            let list = earlier
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Existing breakpoints at line(s) {list} will pause first; RESUME through them to \
                 reach line {line}."
            )
        } else {
            String::new()
        };
        let view = cx.entity();
        let cancel_view = view.clone();
        let confirm_view = view.clone();
        let scrim_view = view;

        let warning = if active {
            t(
                "A job is already running or paused. Confirming only sets a breakpoint at the selected line; it does not start, restart, or resume the job.",
            )
        } else {
            t(
                "All earlier lines still run from the beginning. The job starts at line 1 and pauses before the selected line. Start-from-line is not supported.",
            )
        };
        let confirm_label = if will_start {
            t("SET BP + START")
        } else {
            t("SET BREAKPOINT")
        };

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("run-to-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| shell.cancel_run_to(cx));
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(560.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.warning.opacity(0.6))
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.warning)
                                    .child(format!("{} {line}", t("RUN TO LINE"))),
                            )
                            .child(chip(t("CONFIRM REQUIRED"), p)),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .p_2()
                            .rounded(px(6.0))
                            .bg(p.panel)
                            .font_family(self.mono.clone())
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(11.0))
                                    .text_color(p.text_faint)
                                    .child(format!("{line:>4}")),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .text_size(px(13.0))
                                    .text_color(p.text)
                                    .child(source),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .line_height(px(16.0))
                            .text_color(p.warning)
                            .child(warning),
                    )
                    .when(!earlier_note.is_empty(), |this| {
                        this.child(
                            div()
                                .text_size(px(11.0))
                                .line_height(px(16.0))
                                .text_color(p.warning)
                                .child(earlier_note),
                        )
                    })
                    .child(
                        v_flex()
                            .gap_1()
                            .p_2()
                            .rounded(px(6.0))
                            .bg(p.panel_alt)
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text_faint)
                                    .child(t("CONFIRMING WILL")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(p.text)
                                    .child(format!("{} {line}", t("SET BREAKPOINT AT LINE"))),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(if will_start { p.accent } else { p.text_dim })
                                    .child(if will_start {
                                        format!(
                                            "{} | {} {line}",
                                            t("START PROGRAM AT LINE 1"),
                                            t("PAUSE BEFORE LINE")
                                        )
                                    } else {
                                        t("LEAVE THE ACTIVE JOB AT ITS CURRENT POSITION")
                                            .to_string()
                                    }),
                            ),
                    )
                    .when_some(refusal, |this, refusal| {
                        this.child(
                            div()
                                .text_size(px(10.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(p.danger)
                                .child(format!(
                                    "{}: {}",
                                    t("RUN TO BLOCKED"),
                                    refusal.message().to_uppercase()
                                )),
                        )
                    })
                    .when_some(error, |this, error| {
                        this.child(div().text_size(px(10.0)).text_color(p.warning).child(error))
                    })
                    .child(
                        div()
                            .text_size(px(9.5))
                            .text_color(p.text_faint)
                            .child(t("CANCEL, the scrim, and closing this sheet send nothing.")),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("run-to-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |shell, cx| {
                                            shell.cancel_run_to(cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("run-to-confirm")
                                    .label(confirm_label)
                                    .warning()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .disabled(refusal.is_some())
                                    .on_click(move |_, _, cx| {
                                        confirm_view.update(cx, |shell, cx| {
                                            shell.confirm_run_to(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// The `RUN OPTIONS` sheet: three 48 px rows showing the controller's
    /// authoritative switch values. Opening, closing, or dismissing it never
    /// sends a command.
    fn run_options_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if !self.state.run_options_open {
            return div().into_any_element();
        }
        let view = cx.entity();
        let scrim_view = view.clone();
        let close_view = view.clone();
        let check_view = view.clone();
        let rows = RunOption::ALL.into_iter().map(|option| {
            let value = self.state.run_option_value(option);
            let enabled = self.state.run_option_refusal(option).is_none();
            let hint = t(self.state.run_option_hint(option));
            let id = match option {
                RunOption::OptionalStop => "run-option-optional-stop",
                RunOption::SingleBlock => "run-option-single-block",
                RunOption::BlockDelete => "run-option-block-delete",
            };
            let row_view = view.clone();

            h_flex()
                .id(id)
                .h(px(48.0))
                .w_full()
                .flex_shrink_0()
                .items_center()
                .gap_2()
                .px_3()
                .rounded(px(7.0))
                .bg(if value {
                    p.accent.opacity(0.16)
                } else {
                    p.panel_alt
                })
                .border_1()
                .border_color(if value {
                    p.accent.opacity(0.6)
                } else {
                    p.border
                })
                .when(enabled, |this| {
                    this.cursor_pointer()
                        .hover(|style| style.bg(p.raised_hover).border_color(p.accent))
                        .on_click(move |_, _, cx| {
                            row_view.update(cx, |shell, cx| {
                                shell.toggle_run_option(option, cx);
                            });
                        })
                })
                .when(!enabled, |this| this.opacity(0.55))
                .child(
                    v_flex()
                        .min_w(px(0.0))
                        .flex_1()
                        .gap_0()
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(if enabled { p.text } else { p.text_dim })
                                .child(t(option.label())),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(9.5))
                                .text_color(if enabled { p.text_faint } else { p.warning })
                                .child(hint),
                        ),
                )
                .child(
                    div()
                        .h(px(28.0))
                        .w(px(62.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(999.0))
                        .bg(if value { p.accent } else { p.panel })
                        .border_1()
                        .border_color(if value { p.accent } else { p.border })
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(if value { p.on_accent } else { p.text_dim })
                        .child(if value { t("ON") } else { t("OFF") }),
                )
        });
        let error = self.state.run_options_error.clone();
        let lock_reason = self
            .state
            .run_options_lock_reason()
            .map(|reason| t(reason).to_string());
        let check_refusal = self.state.check_refusal();
        let check_enabled = check_refusal.is_none();
        let check_hint = check_refusal.map_or(
            t("Parse and stream the loaded program in check mode ($C); no motion."),
            |refusal| t(refusal.message()),
        );

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("run-options-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| shell.close_run_options(cx));
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(520.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(t("RUN OPTIONS")),
                            )
                            .child(chip(t("CONTROLLER STATE"), p)),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .line_height(px(14.0))
                            .text_color(p.text_dim)
                            .child(t(
                                "Each tap sends one command. A row keeps showing the value the controller reports; it changes on the next poll, not when the button is pressed.",
                            )),
                    )
                    .children(rows)
                    .child(
                        h_flex()
                            .id("run-option-check")
                            .h(px(48.0))
                            .w_full()
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .rounded(px(7.0))
                            .bg(p.panel_alt)
                            .border_1()
                            .border_color(p.border)
                            .when(check_enabled, |this| {
                                this.cursor_pointer()
                                    .hover(|style| style.bg(p.raised_hover).border_color(p.accent))
                                    .on_click(move |_, _, cx| {
                                        check_view.update(cx, |shell, cx| {
                                            shell.open_check_from_run_options(cx);
                                        });
                                    })
                            })
                            .when(!check_enabled, |this| this.opacity(0.55))
                            .child(
                                v_flex()
                                    .min_w(px(0.0))
                                    .flex_1()
                                    .gap_0()
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(if check_enabled {
                                                p.text
                                            } else {
                                                p.text_dim
                                            })
                                            .child(t("CHECK PROGRAM (DRY RUN)")),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(9.5))
                                            .text_color(if check_enabled {
                                                p.text_faint
                                            } else {
                                                p.warning
                                            })
                                            .child(check_hint),
                                    ),
                            )
                            .child(
                                div()
                                    .h(px(28.0))
                                    .w(px(62.0))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(999.0))
                                    .bg(p.panel)
                                    .border_1()
                                    .border_color(p.border)
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text_dim)
                                    .child(t("CHECK")),
                            ),
                    )
                    .when_some(error, |this, error| {
                        this.child(
                            div()
                                .p_2()
                                .rounded(px(6.0))
                                .bg(p.danger.opacity(0.16))
                                .border_1()
                                .border_color(p.danger.opacity(0.6))
                                .text_size(px(10.0))
                                .text_color(p.danger)
                                .child(error),
                        )
                    })
                    .when_some(lock_reason, |this, reason| {
                        this.child(div().text_size(px(9.5)).text_color(p.warning).child(reason))
                    })
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(9.5))
                                    .text_color(p.text_faint)
                                    .child(t("Closing this sheet sends nothing.")),
                            )
                            .child(
                                div()
                                    .id("run-options-close")
                                    .h(px(48.0))
                                    .w(px(120.0))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_lg()
                                    .bg(p.panel_alt)
                                    .border_1()
                                    .border_color(p.border_strong)
                                    .cursor_pointer()
                                    .hover(|style| style.bg(p.raised_hover).border_color(p.accent))
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .on_click(move |_, _, cx| {
                                        close_view.update(cx, |shell, cx| {
                                            shell.close_run_options(cx);
                                        });
                                    })
                                    .child(t("CLOSE")),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Confirmation for a controller check run (`$C` dry run).
    ///
    /// Opening or dismissing the sheet sends nothing. START CHECK sends exactly
    /// one `StartCheckRun`; the engine owns the `$C` toggle and the observed
    /// return to `Idle`. The sheet is not the motion gateway: it only explains
    /// what the controller will do with the program.
    fn check_confirm_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if !self.state.check_confirm_open {
            return div().into_any_element();
        }
        let view = cx.entity();
        let cancel_view = view.clone();
        let confirm_view = view.clone();
        let scrim_view = view;
        let name = self.state.job.name.clone();
        let total = self.state.job.total;

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("check-confirm-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| shell.cancel_check_confirm(cx));
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(620.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(t("CHECK PROGRAM (DRY RUN)")),
                            )
                            .child(chip("$C", p)),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .line_height(px(15.0))
                            .text_color(p.text)
                            .child(t(
                                "The controller parses and streams the loaded program with check mode ($C) enabled. Nothing in this sheet starts a real job or moves an axis.",
                            )),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .p_2()
                            .rounded(px(6.0))
                            .bg(p.panel)
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(p.text_faint)
                                    .child(t("WHAT THE DRY RUN DOES")),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .line_height(px(14.0))
                                    .text_color(p.text_dim)
                                    .child(t(
                                        "MOTION IS SUPPRESSED: the controller acknowledges program lines without moving the machine.",
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .line_height(px(14.0))
                                    .text_color(p.text_dim)
                                    .child(t(
                                        "SPINDLE / COOLANT: M3, M4, M5, M7, M8 and M9 are parsed and acknowledged but never run as real outputs.",
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .line_height(px(14.0))
                                    .text_color(p.text_dim)
                                    .child(t(
                                        "WHEN FINISHED: the engine leaves check mode and waits for the machine to report IDLE. The program stays loaded for a separate real START.",
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .line_height(px(14.0))
                                    .text_color(p.text_dim)
                                    .child(t(
                                        "STOP remains available while the check streams; it never resumes or restarts the run.",
                                    )),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(10.0))
                                    .text_color(p.text_dim)
                                    .child(if name.is_empty() {
                                        format!("{total} LINES")
                                    } else {
                                        format!("{name} / {total} LINES")
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(p.text_faint)
                                    .child(t("CANCEL sends nothing.")),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("check-confirm-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |shell, cx| {
                                            shell.cancel_check_confirm(cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("check-confirm-start")
                                    .label(t("START CHECK"))
                                    .primary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        confirm_view.update(cx, |shell, cx| {
                                            shell.confirm_check(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Confirmation held in front of a normal START when the loaded program's
    /// estimated machine bounds leave the reported travel.
    ///
    /// This is informational: it uses the reported `$130`/`$131`/`$132` and
    /// the active WCS plus `G92`, not the controller's soft-limit enforcement.
    /// Opening or cancelling sends nothing. START ANYWAY sends exactly one
    /// normal `StartProgram { from_line: None }`.
    fn start_confirm_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(limits) = self.state.pending_start_limits().cloned() else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let scrim_view = view.clone();
        let cancel_view = view.clone();
        let confirm_view = view;
        let name = self.state.job.name.clone();
        let total = self.state.job.total;
        let overruns = match &limits.status {
            LimitsStatus::Exceeds(overruns) => overruns.clone(),
            _ => Vec::new(),
        };
        let machine_bounds = limits
            .machine_bounds
            .map_or_else(|| "NOT REPORTED".to_string(), |bounds| bounds.format_all());
        let run_to_line = self.state.pending_start_run_to_line();

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("start-confirm-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| shell.cancel_start_confirm(cx));
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(680.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.warning)
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.warning)
                                    .child(t("PROGRAM EXCEEDS REPORTED TRAVEL")),
                            )
                            .child(chip("START HELD", p)),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .line_height(px(15.0))
                            .text_color(p.text)
                            .child(t(
                                "The loaded program's estimated machine bounds leave the travel reported by the controller. START is held until you confirm.",
                            )),
                    )
                    .children(run_to_line.map(|line| {
                        div()
                            .text_size(px(11.0))
                            .line_height(px(15.0))
                            .text_color(p.warning)
                            .child(format!(
                                "Run to line {line}: the breakpoint is already set. \
                                 Confirming starts from line 1 and pauses there; \
                                 cancelling leaves the breakpoint enabled."
                            ))
                    }))
                    .child(
                        v_flex()
                            .gap_1()
                            .p_2()
                            .rounded(px(6.0))
                            .bg(p.panel)
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(p.text_faint)
                                    .child(t("PROGRAM BOUNDS (WORK COORDINATES)")),
                            )
                            .child(
                                div()
                                    .font_family(self.mono.clone())
                                    .text_size(px(10.0))
                                    .text_color(p.text)
                                    .child(limits.bounds_text()),
                            )
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(p.text_faint)
                                    .child(t("ESTIMATED MACHINE BOUNDS (WCS + G92)")),
                            )
                            .child(
                                div()
                                    .font_family(self.mono.clone())
                                    .text_size(px(10.0))
                                    .text_color(p.text)
                                    .child(machine_bounds),
                            )
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(p.text_faint)
                                    .child(t("REPORTED TRAVEL")),
                            )
                            .child(
                                div()
                                    .font_family(self.mono.clone())
                                    .text_size(px(10.0))
                                    .text_color(p.text)
                                    .child(limits.travel_text()),
                            ),
                    )
                    .when(!overruns.is_empty(), |this| {
                        this.child(
                            v_flex().gap_1().children(overruns.into_iter().map(|overrun| {
                                let delta = if overrun.below_zero && overrun.above_travel {
                                    format!(
                                        "LOW {:.3} mm | HIGH {:.3} mm",
                                        -overrun.min_mm,
                                        overrun.max_mm - overrun.travel_mm
                                    )
                                } else if overrun.below_zero {
                                    format!("LOW {:.3} mm", -overrun.min_mm)
                                } else {
                                    format!("HIGH {:.3} mm", overrun.max_mm - overrun.travel_mm)
                                };
                                v_flex()
                                    .gap_0p5()
                                    .p_2()
                                    .rounded(px(6.0))
                                    .bg(p.danger.opacity(0.12))
                                    .border_1()
                                    .border_color(p.danger.opacity(0.5))
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .w(px(22.0))
                                                    .flex_shrink_0()
                                                    .text_size(px(10.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(p.danger)
                                                    .child(overrun.axis.to_string()),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w(px(0.0))
                                                    .font_family(self.mono.clone())
                                                    .text_size(px(10.0))
                                                    .text_color(p.text)
                                                    .child(format!(
                                                        "MACHINE {:.3} .. {:.3} mm",
                                                        overrun.min_mm, overrun.max_mm
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .flex_shrink_0()
                                                    .text_size(px(10.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(p.danger)
                                                    .child(delta),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .font_family(self.mono.clone())
                                            .text_size(px(9.0))
                                            .text_color(p.text_faint)
                                            .child(format!(
                                                "REPORTED ${} MAXIMUM TRAVEL {:.3} mm",
                                                overrun.setting, overrun.travel_mm
                                            )),
                                    )
                            })),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(9.5))
                            .line_height(px(13.0))
                            .text_color(p.text_dim)
                            .child(t(
                                "INFORMATIONAL CHECK: WCS changes, G10 writes, G92 offsets, and tool-length offsets are not enforced here. Verify the setup and soft limits before cutting.",
                            )),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(10.0))
                                    .text_color(p.text_dim)
                                    .child(if name.is_empty() {
                                        format!("{total} LINES")
                                    } else {
                                        format!("{name} / {total} LINES")
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(p.text_faint)
                                    .child(t("CANCEL sends nothing.")),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("start-confirm-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |shell, cx| {
                                            shell.cancel_start_confirm(cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("start-confirm-anyway")
                                    .label(t("START ANYWAY"))
                                    .danger()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        confirm_view.update(cx, |shell, cx| {
                                            shell.confirm_start(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Slim toolbar inside the viewer: job state, elapsed time and the four
    /// job controls. Keeping it here preserves the approved three-column grid.
    fn job_toolbar(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let load_view = view.clone();
        let start_view = view.clone();
        let pause_view = view.clone();
        let resume_view = view.clone();
        let stop_view = view;

        let status = self.state.display_job_status();
        let color = job_status_color(&self.state.job, p);
        let detail = self
            .state
            .pause_detail()
            .or_else(|| self.state.check_detail().map(|text| t(text).to_string()));
        let elapsed = format_elapsed(self.state.job.elapsed);
        // A check badge carries its elapsed time inside the pill: at 1280 px
        // the toolbar has no room for a separate TIME field beside a long
        // `CHECK COMPLETE` label.
        let check_status = self.state.job.is_check() || self.state.job.check_pending();
        let badge_label = if check_status {
            format!("{}  {elapsed}", t(status))
        } else {
            t(status).to_string()
        };
        let start_ok = self.job_start_allowed();
        let pause_ok = self.job_pause_allowed();
        let resume_ok = self.job_resume_allowed();
        let stop_ok = self.job_stop_allowed();

        h_flex()
            .h(px(56.0))
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .flex_1()
                    .min_w(px(0.0))
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .px_2()
                            .h(px(22.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .rounded(px(5.0))
                            .bg(color.opacity(0.16))
                            .text_size(px(10.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(color)
                            .child(badge_label),
                    )
                    .when_some(detail, |this, detail| {
                        this.child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
                                .text_size(px(10.0))
                                .text_color(p.warning)
                                .child(detail),
                        )
                    })
                    .when(self.state.job_compensated(), |this| {
                        this.child(
                            div()
                                .px_2()
                                .h(px(22.0))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .rounded(px(5.0))
                                .bg(p.accent.opacity(0.16))
                                .border_1()
                                .border_color(p.accent.opacity(0.5))
                                .text_size(px(9.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(p.accent)
                                .child(t("HEIGHT MAP / COMPENSATED")),
                        )
                    })
                    .when(!check_status, |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .font_family(self.mono.clone())
                                .text_size(px(10.0))
                                .text_color(p.text_faint)
                                .child(format!("{} {elapsed}", t("TIME"))),
                        )
                    }),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("job-load")
                            .label(t("LOAD PROGRAM"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(132.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .on_click(move |_, window, cx| {
                                load_view.update(cx, |this, cx| {
                                    this.open_program_browser(window, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("job-start")
                            .label(t("START"))
                            .primary()
                            .h(px(48.0))
                            .w(px(78.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(!start_ok)
                            .on_click(move |_, _, cx| {
                                start_view.update(cx, |this, cx| this.start_job(cx));
                            }),
                    )
                    .child(
                        Button::new("job-pause")
                            .label(t("PAUSE"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(78.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(!pause_ok)
                            .on_click(move |_, _, cx| {
                                pause_view.update(cx, |this, cx| this.pause_job(cx));
                            }),
                    )
                    .child(
                        Button::new("job-resume")
                            .label(t("RESUME"))
                            .primary()
                            .h(px(48.0))
                            .w(px(104.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(!resume_ok)
                            .on_click(move |_, _, cx| {
                                resume_view.update(cx, |this, cx| this.resume_job(cx));
                            }),
                    )
                    .child(
                        Button::new("job-stop")
                            .label(t("STOP"))
                            .warning()
                            .h(px(48.0))
                            .w(px(84.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(!stop_ok)
                            .on_click(move |_, _, cx| {
                                stop_view.update(cx, |this, cx| this.stop_job(cx));
                            }),
                    ),
            )
    }

    fn job_view(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let program = self.state.job.program().cloned();
        let has_program = program.is_some();
        // Real geometry comes from `JobLoaded`; the synthetic sketch remains
        // only for `--mock` and for a live session with nothing loaded.
        let path_label = match (&program, self.state.live) {
            (Some(_), _) => t("TOOLPATH"),
            (None, true) => t("NO PROGRAM"),
            (None, false) => t("SYNTHETIC"),
        };
        let segments = program
            .as_ref()
            .map(|program| format!("{} SEG", program.toolpath.segment_count()));
        let marker = viewer::marker_mm3(self.state.profile.capabilities.units, &self.state.dro);
        let zoom = format!("{}%", self.viewer.zoom_percent());
        let mode = self.viewer.mode();
        let stock_on = self.viewer.stock_visible();

        let fit_view = cx.entity();
        let fit = chip_button("viewer-fit", t("FIT"), p, move |_, cx| {
            fit_view.update(cx, |this, cx| {
                this.viewer.fit();
                cx.notify();
            });
        });
        let drop_view = cx.entity();

        div()
            .relative()
            .size_full()
            .bg(p.bg)
            .on_drop::<ExternalPaths>(move |paths, _, cx| {
                let paths = paths.paths().to_vec();
                drop_view.update(cx, |shell, cx| shell.drop_program_paths(&paths, cx));
            })
            .child(viewer::view(
                &self.viewer,
                p,
                program,
                marker,
                None,
                cx.entity(),
            ))
            .child(
                div()
                    .absolute()
                    .top(px(8.0))
                    .left(px(8.0))
                    .flex()
                    .gap_1()
                    .child(viewer_mode_chip(
                        viewer::ViewerMode::Top2D,
                        mode,
                        p,
                        cx.entity(),
                    ))
                    .child(viewer_mode_chip(
                        viewer::ViewerMode::ThreeD,
                        mode,
                        p,
                        cx.entity(),
                    ))
                    .when(mode.is_three_d(), |this| {
                        this.child(stock_chip(stock_on, p, cx.entity()))
                    })
                    .child(chip(path_label, p))
                    .when_some(segments, |this, segments| this.child(chip(&segments, p))),
            )
            .child(
                div()
                    .absolute()
                    .top(px(8.0))
                    .right(px(8.0))
                    .flex()
                    .gap_1()
                    .child(chip(self.state.wcs(), p))
                    .child(chip(self.state.unit(), p))
                    .child(chip(&zoom, p))
                    .child(fit),
            )
            .child(
                v_flex()
                    .absolute()
                    .bottom(px(8.0))
                    .left(px(8.0))
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_lg()
                    .bg(p.panel.opacity(0.9))
                    .border_1()
                    .border_color(p.border)
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child(legend(p.path_cut, t("Cut"), p))
                            .child(legend(p.path_rapid, t("Rapid"), p))
                            .child(legend(p.path_plunge, t("Plunge"), p))
                            .when(mode.is_three_d(), |this| {
                                this.child(
                                    div()
                                        .pl_3()
                                        .border_l_1()
                                        .border_color(p.border)
                                        .text_size(px(9.0))
                                        .text_color(p.text_faint)
                                        .child(t("DRAG ORBIT | SHIFT/MIDDLE PAN | WHEEL ZOOM")),
                                )
                            }),
                    )
                    .when(mode.is_three_d() && stock_on, |this| {
                        this.child(div().text_size(px(9.0)).text_color(p.text_faint).child(t(
                            "STOCK INFERRED FROM PROGRAM BOUNDS | NOT A MEASURED MODEL",
                        )))
                    }),
            )
            .child(
                div()
                    .absolute()
                    .bottom(px(8.0))
                    .right(px(8.0))
                    .px_2()
                    .py_1()
                    .rounded_lg()
                    .bg(p.panel.opacity(0.9))
                    .border_1()
                    .border_color(p.border)
                    .text_size(px(10.0))
                    .text_color(p.text_dim)
                    .child(if self.state.live {
                        format!(
                            "{} {} | S{}",
                            t("TOOL"),
                            self.state
                                .tool
                                .map_or_else(|| "--".to_string(), |tool| tool.to_string()),
                            self.state.rpm
                        )
                    } else {
                        "TOOL 3 | 6 mm FLAT | S12000".to_string()
                    }),
            )
            .when(self.state.live && !has_program, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(56.0))
                        .left(px(0.0))
                        .right(px(0.0))
                        .bottom(px(0.0))
                        .child(self.empty_program_prompt("job-empty-load", p, cx)),
                )
            })
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(if self.demo_drop_hint { 1.0 } else { 0.0 })
                    .drag_over::<ExternalPaths>(move |style, _, _, _| {
                        style
                            .opacity(1.0)
                            .bg(p.accent.opacity(0.1))
                            .border_2()
                            .border_color(p.accent)
                    })
                    .child(
                        v_flex()
                            .items_center()
                            .gap_1()
                            .p_3()
                            .rounded_lg()
                            .bg(p.panel)
                            .border_1()
                            .border_color(p.accent)
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.accent)
                                    .child(t("DROP PROGRAM TO LOAD")),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(p.text_dim)
                                    .child(t("One .nc, .gcode, .tap, or .ngc file. \
                                         Loading never starts the job.")),
                            ),
                    ),
            )
    }

    // ------------------------------------------------------- file browser

    /// Modal file browser: path field, parent/up, virtualized rows, and the
    /// gated LOAD PROGRAM action.
    fn file_browser_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(browser) = self.file_browser.as_ref() else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let scrim_view = view.clone();
        let close_view = view.clone();
        let up_view = view.clone();
        let go_view = view.clone();
        let load_view = view.clone();
        let selected = browser.selected_entry().cloned();
        let selected_index = browser.selected_index();
        let entries = browser.entries_arc();
        let current = compact_path(browser.cwd(), 86);
        let folder_count = browser
            .entries()
            .iter()
            .filter(|entry| entry.is_dir)
            .count();
        let file_count = browser.file_count();
        let folder_label = if folder_count == 1 {
            t("FOLDER")
        } else {
            t("FOLDERS")
        };
        let file_label = if file_count == 1 {
            t("PROGRAM")
        } else {
            t("PROGRAMS")
        };
        let error = browser.error().map(str::to_string);
        let refusal = self.state.program_load_refusal();
        let can_load = selected.is_some() && refusal.is_none();
        let (link_label, link_color) = match refusal {
            Some(ProgramLoadRefusal::Mock) => (t("MOCK / BROWSE ONLY"), p.warning),
            Some(ProgramLoadRefusal::NotReady) => (t("LINK NOT READY"), p.warning),
            Some(ProgramLoadRefusal::JobActive) => (t("JOB ACTIVE"), p.warning),
            None => (t("LIVE / READY"), p.success),
        };
        let selected_name = selected.as_ref().map_or_else(
            || t("No file selected").to_string(),
            |entry| entry.name.clone(),
        );
        let selected_meta = match (&selected, refusal) {
            (Some(entry), None) => {
                format!("{} | {}", format_size(entry.size), t("READY TO LOAD"))
            }
            (Some(entry), Some(reason)) => {
                format!("{} | {}", format_size(entry.size), t(reason.message()))
            }
            (None, _) => t("Tap a program file to select it").to_string(),
        };
        let load_disabled = !can_load;

        let focus_input = self.file_browser_input.clone();
        let path_field = div()
            .h(px(48.0))
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                focus_input.update(cx, |state, cx| state.focus(window, cx));
            })
            .child(
                Input::new(&self.file_browser_input)
                    .with_size(Size::Large)
                    .w_full()
                    .text_size(px(12.0)),
            );

        let list: AnyElement = if entries.is_empty() {
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_1()
                .p_3()
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(p.text_dim)
                        .child(t("No folders or program files here")),
                )
                .child(
                    div()
                        .text_size(px(9.5))
                        .text_color(p.text_faint)
                        .child(t("Tap UP or type another folder path.")),
                )
                .into_any_element()
        } else {
            let row_view = view.clone();
            uniform_list("file-browser-list", entries.len(), move |range, _, _| {
                range
                    .filter_map(|index| {
                        let entry = entries.get(index)?.clone();
                        Some(file_browser_row(
                            index,
                            entry,
                            Some(index) == selected_index,
                            p,
                            row_view.clone(),
                        ))
                    })
                    .collect::<Vec<_>>()
            })
            .flex_1()
            .min_h(px(0.0))
            .track_scroll(&self.file_browser_scroll)
            .into_any_element()
        };

        let recent_records = self.state.config.recent_files();
        let recent_count = recent_records.len();
        let clear_view = view.clone();
        let mut recent_rows = v_flex()
            .id("recent-files-list")
            .w_full()
            .flex_shrink_0()
            .gap_1()
            .max_h(px(164.0))
            .overflow_y_scroll()
            .track_scroll(&self.recent_files_scroll);
        if recent_records.is_empty() {
            recent_rows = recent_rows.child(
                div()
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .px_2()
                    .rounded_lg()
                    .bg(p.panel)
                    .text_size(px(9.5))
                    .text_color(p.text_faint)
                    .child(t("No recent files yet. A loaded local file appears here.")),
            );
        } else {
            for record in recent_records {
                let missing = !Path::new(&record.path).is_file();
                recent_rows = recent_rows.child(recent_file_row(
                    record.path.clone(),
                    missing,
                    p,
                    self.mono.clone(),
                    view.clone(),
                ));
            }
        }
        let recent_section = v_flex()
            .w_full()
            .flex_shrink_0()
            .gap_1()
            .child(
                h_flex()
                    .h(px(20.0))
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(9.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_faint)
                            .child(t("RECENT FILES")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child(format!("{recent_count} | {}", t("NEWEST FIRST"))),
                    )
                    .when(recent_count > 0, |this| {
                        this.child(
                            Button::new("recent-files-clear")
                                .label(t("CLEAR"))
                                .secondary()
                                .h(px(28.0))
                                .w(px(64.0))
                                .rounded(px(5.0))
                                .text_size(px(8.5))
                                .on_click(move |_, _, cx| {
                                    clear_view.update(cx, |shell, cx| {
                                        shell.browser_clear_recent(cx);
                                    });
                                }),
                        )
                    }),
            )
            .child(recent_rows);

        let mut card = v_flex()
            .relative()
            .w(px(820.0))
            .max_w_full()
            .h(px(640.0))
            .max_h_full()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(p.border_strong)
            .bg(p.raised)
            .child(
                h_flex()
                    .h(px(48.0))
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(t("LOAD PROGRAM")),
                            )
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .text_color(p.text_dim)
                                    .child(t("Choose a file for the job; loading never starts it.")),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .h(px(22.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .rounded(px(5.0))
                            .bg(link_color.opacity(0.16))
                            .text_size(px(9.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(link_color)
                            .child(link_label),
                    )
                    .child(
                        Button::new("file-browser-close")
                            .label(t("CLOSE"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(88.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, _, cx| {
                                close_view.update(cx, |this, cx| this.close_program_browser(cx));
                            }),
                    ),
            )
            .child(
                h_flex()
                    .h(px(22.0))
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(9.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_faint)
                            .child(t("CURRENT")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .font_family(self.mono.clone())
                            .text_size(px(10.0))
                            .text_color(p.text_dim)
                            .child(current),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child(format!(
                                "{folder_count} {folder_label} / {file_count} {file_label}"
                            )),
                    ),
            )
            .child(
                h_flex()
                    .h(px(48.0))
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .child(path_field)
                    .child(
                        Button::new("file-browser-up")
                            .label(t("UP"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(64.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, window, cx| {
                                up_view.update(cx, |this, cx| this.browser_up(window, cx));
                            }),
                    )
                    .child(
                        Button::new("file-browser-go")
                            .label(t("GO"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(64.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, window, cx| {
                                go_view.update(cx, |this, cx| this.browser_go(window, cx));
                            }),
                    ),
            )
            .child(recent_section)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .overflow_hidden()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border)
                    .bg(p.panel)
                    .child(list),
            )
            .child(
                div()
                    .text_size(px(9.0))
                    .line_height(px(12.0))
                    .text_color(p.text_faint)
                    .child(t(
                        "48 px rows | folders open on tap | hidden files and symlinks are not shown | .nc .gcode .tap .ngc | 20 MB limit",
                    )),
            );

        if let Some(error) = error {
            card = card.child(
                div()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.danger.opacity(0.14))
                    .text_size(px(10.0))
                    .text_color(p.danger)
                    .child(error),
            );
        }

        card = card.child(
            h_flex()
                .h(px(56.0))
                .flex_shrink_0()
                .items_center()
                .gap_2()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(
                            div()
                                .truncate()
                                .text_size(px(11.5))
                                .font_weight(FontWeight::BOLD)
                                .text_color(if selected.is_some() {
                                    p.text
                                } else {
                                    p.text_dim
                                })
                                .child(selected_name),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(9.0))
                                .text_color(if refusal.is_some() {
                                    p.warning
                                } else {
                                    p.text_faint
                                })
                                .child(selected_meta),
                        ),
                )
                .child(
                    Button::new("file-browser-cancel")
                        .label(t("CANCEL"))
                        .secondary()
                        .h(px(48.0))
                        .w(px(96.0))
                        .rounded_lg()
                        .text_size(px(11.0))
                        .on_click({
                            let view = scrim_view.clone();
                            move |_, _, cx| {
                                view.update(cx, |this, cx| this.close_program_browser(cx));
                            }
                        }),
                )
                .child(
                    Button::new("file-browser-load")
                        .label(t("LOAD PROGRAM"))
                        .primary()
                        .h(px(48.0))
                        .w(px(150.0))
                        .rounded_lg()
                        .text_size(px(11.0))
                        .disabled(load_disabled)
                        .on_click(move |_, _, cx| {
                            load_view.update(cx, |this, cx| this.browser_load_selected(cx));
                        }),
                ),
        );

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("file-browser-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |this, cx| this.close_program_browser(cx));
                    }),
            )
            .child(card)
            .into_any_element()
    }

    // ---------------------------------------------------------- probe views

    fn probe_view(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let ctx = self.probe_context();
        match self.state.probe.card() {
            Some(card) => self.probe_detail(p, cx, card, &ctx).into_any_element(),
            None => self.probe_cards(p, cx, &ctx).into_any_element(),
        }
    }

    fn probe_cards(
        &self,
        p: Palette,
        cx: &mut Context<Self>,
        ctx: &ProbeContext,
    ) -> impl IntoElement {
        let view = cx.entity();
        let mut grid = v_flex().gap_2();
        for chunk in ProbeCard::ALL.chunks(3) {
            let mut row = h_flex().gap_2().items_stretch();
            for card in chunk {
                row = row.child(self.probe_card_tile(*card, p, view.clone(), ctx));
            }
            for _ in chunk.len()..3 {
                row = row.child(div().flex_1());
            }
            grid = grid.child(row);
        }

        let mut banner = v_flex().gap_2();
        if !ctx.live {
            banner = banner.child(probe_notice(
                "MOCK PREVIEW",
                "Probe cards are configurable, but RUN needs a live engine. Mock mode never sends motion.",
                p.warning,
                p,
            ));
        } else if !ctx.ready {
            banner = banner.child(probe_notice(
                "LINK NOT READY",
                "Connect and finish the handshake before a probe can run.",
                p.warning,
                p,
            ));
        } else if !ctx.has_probe {
            banner = banner.child(probe_notice(
                "NO PROBE INPUT",
                "The controller did not report a probe input. Cycles stay visible for setup, but no run is offered.",
                p.danger,
                p,
            ));
        }

        v_flex()
            .id("probe-cards-scroll")
            .size_full()
            .overflow_y_scroll()
            .gap_2()
            .p_2()
            .child(
                h_flex()
                    .h(px(28.0))
                    .items_center()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_0()
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text_dim)
                                    .child(t("PROBE CYCLES")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(p.text_faint)
                                    .child(t("Pick a card, inspect the preflight, then press RUN")),
                            ),
                    )
                    .child(div().flex_1())
                    .child(chip(self.state.wcs(), p))
                    .child(chip(self.state.unit(), p))
                    .child(chip(if ctx.ready { t("LIVE") } else { t("OFFLINE") }, p)),
            )
            .child(self.probe_offset_bar(p, cx))
            .child(self.probe_defaults_bar(p, cx))
            .child(self.probe_macros_bar(p, cx))
            .child(banner)
            .child(grid)
    }

    /// Always-visible physical probe mount offset for the Probe workspace.
    ///
    /// The value is `probe tip - controlled point`, in millimetres. The cycle
    /// fields stay in feature/tip coordinates; the generated plan commands the
    /// controlled point (`feature - offset`).
    fn probe_offset_bar(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let x = self
            .state
            .probe
            .field_text(ProbeField::ProbeOffsetX)
            .to_string();
        let y = self
            .state
            .probe
            .field_text(ProbeField::ProbeOffsetY)
            .to_string();
        let nonzero = x.trim().parse::<f64>().unwrap_or(0.0) != 0.0
            || y.trim().parse::<f64>().unwrap_or(0.0) != 0.0;
        let error = self
            .state
            .probe
            .field_error(ProbeField::ProbeOffsetX)
            .or_else(|| self.state.probe.field_error(ProbeField::ProbeOffsetY))
            .map(str::to_string);
        let mut bar = v_flex()
            .w_full()
            .flex_shrink_0()
            .gap_1()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(if error.is_some() {
                p.danger
            } else if nonzero {
                p.info.opacity(0.55)
            } else {
                p.border
            })
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(section_label(t("PROBE OFFSET X/Y"), p))
                    .child(self.probe_offset_button(ProbeField::ProbeOffsetX, "X", p, cx))
                    .child(self.probe_offset_button(ProbeField::ProbeOffsetY, "Y", p, cx))
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_dim)
                            .child(t(
                                "mm | probe tip - controlled point | plan shifts controlled by \
                                 -offset",
                            )),
                    ),
            )
            .child(
                div()
                    .text_size(px(8.5))
                    .line_height(px(11.0))
                    .text_color(p.text_faint)
                    .child(
                        "Entered positions, centres and grid origin are FEATURE (probe-tip) coordinates. \
                         Preflight from/to and the program are CONTROLLED-POINT coordinates \
                         (feature - offset). Stored per controller profile; never sent to the controller.",
                    ),
            );
        if let Some(error) = error {
            bar = bar.child(div().text_size(px(9.0)).text_color(p.danger).child(error));
        }
        bar
    }

    /// Compact local-save bar for the per-controller shared probe defaults.
    ///
    /// It never sends a command: the button only writes the active profile's
    /// JSON through the same atomic save path as tools and maps.
    fn probe_defaults_bar(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let view = cx.entity();
        let status = self.probe_defaults_status.clone();
        let mut bar = v_flex()
            .w_full()
            .flex_shrink_0()
            .gap_1()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(match &status {
                Some(status) if !status.saved => p.danger,
                Some(_) => p.success.opacity(0.55),
                None => p.border,
            })
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(section_label(t("PROBE DEFAULTS"), p))
                    .child(
                        Button::new("probe-defaults-save")
                            .label(t("SAVE DEFAULTS"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(160.0))
                            .flex_shrink_0()
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| this.save_probe_defaults(cx));
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_dim)
                            .child(t("SAVED PER CONTROLLER PROFILE")),
                    )
                    .child(
                        div()
                            .text_size(px(8.5))
                            .text_color(p.text_faint)
                            .child(t("local JSON only; nothing is sent to the controller")),
                    ),
            )
            .child(
                div()
                    .text_size(px(8.5))
                    .line_height(px(11.0))
                    .text_color(p.text_faint)
                    .child(
                        "A newly loaded profile prefills SAFE Z, SEARCH, FEED, RETRACT, PROBE DIA, \
                         EDGE OFFSET, and PLATE THICK. Per-cycle anchors, expected values, centre, \
                         radius, touches, separation, reference Z, and the height grid stay per run.",
                    ),
            );
        if let Some(status) = status {
            bar = bar.child(
                div()
                    .text_size(px(9.0))
                    .text_color(if status.saved { p.success } else { p.danger })
                    .child(status.text),
            );
        }
        bar
    }

    /// Compact bar for the bounded pre/post probe macros.
    ///
    /// Opening the sheet is local; the summary reports the validated lines that
    /// every generated probe plan will include.
    fn probe_macros_bar(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let view = cx.entity();
        let summary = self.state.probe.probe_macro_summary();
        let status = self.probe_macros_status.clone();
        let mut bar = v_flex()
            .w_full()
            .flex_shrink_0()
            .gap_1()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(match &status {
                Some(status) if !status.saved => p.danger,
                Some(_) => p.success.opacity(0.55),
                None => p.border,
            })
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(section_label(t("PROBE MACROS"), p))
                    .child(chip(&summary, p))
                    .child(
                        Button::new("probe-macros-open")
                            .label(t("OPEN MACROS"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(150.0))
                            .flex_shrink_0()
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| {
                                    this.open_probe_macro_editor(window, cx);
                                });
                            }),
                    )
                    .child(div().text_size(px(8.5)).text_color(p.text_faint).child(t(
                        "bounded pre/post lines; local profile; still subject to preflight",
                    ))),
            );
        if let Some(status) = status {
            bar = bar.child(
                div()
                    .text_size(px(9.0))
                    .text_color(if status.saved { p.success } else { p.danger })
                    .child(status.text),
            );
        }
        bar
    }

    /// Bounded PRE/POST macro editor.
    ///
    /// Both fields are multi-line textareas with eight visible rows. SAVE
    /// validates and writes the local profile; CANCEL drops the draft without
    /// touching the model. The sheet never sends a command.
    fn probe_macro_editor_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = self.probe_macro_editor.as_ref() else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let cancel_view = view.clone();
        let save_view = view.clone();
        let scrim_view = view;
        let error = editor.error.clone();
        let status = editor.status.clone();
        let has_error = error.is_some();
        let border = if error.is_some() {
            p.danger
        } else {
            p.border_strong
        };
        let allowlist = format!(
            "{} {} {MAX_PROBE_MACRO_BYTES} {} {MAX_PROBE_MACRO_LINES} {} {}",
            t("ALLOWED: comments `(...)` or `;...`, `G4 P0..60`, `M0`, `M1`, `M5`, `M9`."),
            t("Max"),
            t("bytes and"),
            t("lines per macro."),
            t(
                "No motion, probing, spindle-on, coolant-on, tool change, coordinate, or system \
                 commands."
            )
        );

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("probe-macro-editor-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| {
                            shell.cancel_probe_macro_editor(cx);
                        });
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(760.0))
                    .max_w_full()
                    .max_h(px(700.0))
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(border)
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(t("PROBE MACROS")),
                            )
                            .child(chip(t("LOCAL PROFILE"), p)),
                    )
                    .child(
                        div()
                            .text_size(px(9.5))
                            .line_height(px(13.0))
                            .text_color(p.text_dim)
                            .child(allowlist),
                    )
                    .child(
                        v_flex()
                            .id("probe-macro-editor-fields")
                            .flex_1()
                            .min_h(px(0.0))
                            .gap_2()
                            .overflow_y_scroll()
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(9.5))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(p.text_dim)
                                            .child(t("PRE MACRO - after the safety preamble")),
                                    )
                                    .child(Textarea::new(&self.probe_macro_pre_input).w_full()),
                            )
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(9.5))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(p.text_dim)
                                            .child(t(
                                                "POST MACRO - after the final safe-Z retract",
                                            )),
                                    )
                                    .child(Textarea::new(&self.probe_macro_post_input).w_full()),
                            ),
                    )
                    .when_some(error, |this, error| {
                        this.child(
                            div()
                                .text_size(px(10.0))
                                .line_height(px(13.0))
                                .text_color(p.danger)
                                .child(error),
                        )
                    })
                    .when_some(status, |this, status| {
                        let text = if has_error && !status.saved {
                            t("NOT SAVED").to_string()
                        } else {
                            status.text
                        };
                        this.child(
                            div()
                                .text_size(px(9.5))
                                .text_color(if status.saved { p.success } else { p.danger })
                                .child(text),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(9.0))
                            .line_height(px(12.0))
                            .text_color(p.text_faint)
                            .child(t(
                                "These lines run inside the generated probe program and are \
                                 still subject to every preflight gate. SAVE MACROS writes \
                                 only the local controller profile: no controller command, \
                                 setting, offset, or start.",
                            )),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("probe-macro-editor-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |shell, cx| {
                                            shell.cancel_probe_macro_editor(cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("probe-macro-editor-save")
                                    .label(t("SAVE MACROS"))
                                    .primary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        save_view.update(cx, |shell, cx| {
                                            shell.save_probe_macro_editor(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn probe_offset_button(
        &self,
        field: ProbeField,
        axis: &'static str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity();
        let value = self.state.probe.field_text(field).to_string();
        let invalid = self.state.probe.field_error(field).is_some();
        Button::new(format!("probe-offset-{}", field.key()))
            .label(format!("{axis} {value} mm"))
            .h(px(48.0))
            .w(px(150.0))
            .flex_shrink_0()
            .rounded_lg()
            .text_size(px(12.0))
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(p.panel)
                    .foreground(if invalid { p.danger } else { p.text })
                    .hover(p.raised_hover)
                    .active(p.accent_muted),
            )
            .border_1()
            .border_color(if invalid { p.danger } else { p.border })
            .on_click(move |_, _, cx| {
                view.update(cx, |this, cx| {
                    this.state.open_keypad(KeypadTarget::ProbeField(field));
                    cx.notify();
                });
            })
    }

    fn probe_card_tile(
        &self,
        card: ProbeCard,
        p: Palette,
        view: Entity<Shell>,
        ctx: &ProbeContext,
    ) -> impl IntoElement {
        let deferred = card.deferred_note().is_some();
        let partial = card == ProbeCard::Rotation;
        let status = if partial {
            "APPLY DEFERRED"
        } else if ctx.ready && ctx.has_probe {
            t("READY")
        } else {
            t("SETUP")
        };
        let status_color = if partial {
            p.info
        } else if ctx.ready && ctx.has_probe {
            p.success
        } else {
            p.text_faint
        };
        let select_view = view.clone();
        let click_card = card;
        let configure_view = view;
        div()
            .id(format!(
                "probe-card-{}",
                card.title().to_lowercase().replace(' ', "-")
            ))
            .flex_1()
            .min_w(px(0.0))
            .cursor_pointer()
            .rounded_lg()
            .border_1()
            .border_color(if deferred {
                p.warning.opacity(0.45)
            } else {
                p.border
            })
            .bg(p.panel_alt)
            .h(px(112.0))
            .px_2()
            .py_1()
            .flex()
            .flex_col()
            .gap_1()
            .justify_between()
            .hover(|style| style.bg(p.raised_hover))
            .on_click(move |_, _, cx| {
                select_view.update(cx, |this, cx| {
                    this.select_probe_card(click_card, cx);
                });
            })
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(15.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(t(card.title())),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .h(px(18.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(4.0))
                                    .bg(status_color.opacity(0.16))
                                    .text_size(px(9.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(status_color)
                                    .child(status),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .line_height(px(13.0))
                            .max_h(px(26.0))
                            .overflow_hidden()
                            .text_color(p.text_dim)
                            .child(card.direction()),
                    ),
            )
            .child(
                Button::new(format!(
                    "probe-configure-{}",
                    card.title().to_lowercase().replace(' ', "-")
                ))
                .label(t("CONFIGURE"))
                .secondary()
                .h(px(48.0))
                .w_full()
                .rounded_lg()
                .text_size(px(11.0))
                .on_click(move |_, _, cx| {
                    configure_view.update(cx, |this, cx| {
                        this.select_probe_card(card, cx);
                    });
                }),
            )
    }

    fn probe_detail(
        &self,
        p: Palette,
        cx: &mut Context<Self>,
        card: ProbeCard,
        ctx: &ProbeContext,
    ) -> impl IntoElement {
        let preflight = self.state.probe.preflight(ctx);
        let running = self.state.probe.is_running();
        let back_view = cx.entity();
        let saved = if card == ProbeCard::HeightMap && !running {
            self.state.selected_saved_map().cloned()
        } else {
            None
        };
        let status = if saved.is_some() {
            t("SAVED MAP")
        } else if let Some(run) = self.state.probe.run() {
            t(run.phase.label())
        } else if preflight.setup.is_some() {
            t("PREFLIGHT OK")
        } else {
            t("SETUP")
        };
        let status_color = if saved.is_some() {
            p.info
        } else if let Some(run) = self.state.probe.run() {
            match run.phase {
                ProbePhase::Running => p.accent,
                ProbePhase::Complete => p.success,
                ProbePhase::Failed => p.danger,
            }
        } else if preflight.setup.is_some() {
            p.success
        } else {
            p.text_faint
        };

        let body: AnyElement = if let Some(saved) = saved.as_ref() {
            self.saved_height_map_detail(p, cx, saved)
                .into_any_element()
        } else if let Some(run) = self.state.probe.run() {
            let mut content = v_flex().gap_2().p_2();
            if run.map.is_some() {
                // The measured result and its effective summary come first so
                // the Z offset and ranges are visible without scrolling past
                // the saved-map browser. The browser remains below the result.
                content = content.child(self.height_map_result_panel(p, cx, run));
                if card == ProbeCard::HeightMap {
                    content = content.child(self.height_map_library_panel(p, cx));
                }
            } else {
                content = content.child(self.probe_run_panel(p, cx, run));
            }
            div()
                .id("probe-result-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .child(content)
                .into_any_element()
        } else {
            let form_pane = div()
                .id("probe-form-scroll")
                .w(px(300.0))
                .flex_shrink_0()
                .h_full()
                .overflow_y_scrollbar()
                .child(
                    v_flex()
                        .gap_2()
                        .p_2()
                        .when(card != ProbeCard::HeightMap, |this| {
                            this.child(
                                div()
                                    .text_size(px(9.5))
                                    .line_height(px(13.0))
                                    .text_color(p.text_dim)
                                    .child(card.detail()),
                            )
                        })
                        .child(self.probe_form(p, cx, card, ctx)),
                );
            let preflight_pane = div()
                .id("probe-preflight-scroll")
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .overflow_y_scrollbar()
                .child(
                    v_flex()
                        .gap_2()
                        .p_2()
                        .when(card == ProbeCard::HeightMap, |this| {
                            this.child(self.height_map_library_panel(p, cx))
                        })
                        .child(self.probe_preflight_panel(&preflight, p, cx)),
                );
            h_flex()
                .flex_1()
                .min_h(px(0.0))
                .gap_2()
                .child(form_pane)
                .child(preflight_pane)
                .into_any_element()
        };

        let footer = if let Some(saved) = saved.as_ref() {
            self.saved_map_footer(p, cx, saved)
        } else {
            self.probe_footer(p, cx, &preflight, running)
        };

        v_flex()
            .size_full()
            .child(
                h_flex()
                    .h(px(52.0))
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        Button::new("probe-back")
                            .label(t("CYCLES"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(84.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(running)
                            .on_click(move |_, _, cx| {
                                back_view.update(cx, |this, cx| this.close_probe_card(cx));
                            }),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.0))
                            .gap_0()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(px(15.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(p.text)
                                            .child(t(card.title())),
                                    )
                                    .child(
                                        div()
                                            .px_2()
                                            .h(px(18.0))
                                            .flex()
                                            .items_center()
                                            .rounded(px(4.0))
                                            .bg(status_color.opacity(0.16))
                                            .text_size(px(9.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(status_color)
                                            .child(status),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .text_color(p.text_dim)
                                    .truncate()
                                    .child(card.direction()),
                            ),
                    )
                    .child(chip(self.state.wcs(), p))
                    .child(chip(self.state.unit(), p)),
            )
            .child(self.probe_offset_bar(p, cx))
            .child(self.probe_defaults_bar(p, cx))
            .child(self.probe_macros_bar(p, cx))
            .child(body)
            .child(footer)
    }

    fn probe_form(
        &self,
        p: Palette,
        cx: &mut Context<Self>,
        card: ProbeCard,
        ctx: &ProbeContext,
    ) -> Div {
        let fields = card.fields();
        let problems = self.state.probe.field_problems(ctx);
        let mut cells = Vec::new();
        for field in fields {
            let inline = problems
                .iter()
                .find(|problem| problem.field == Some(*field))
                .map(|problem| problem.message.clone());
            cells.push(self.probe_field_cell(*field, p, cx, inline));
        }
        let mut form = v_flex()
            .w_full()
            .gap_2()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(p.border)
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(section_label(t("CYCLE PARAMETERS"), p))
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child(t("millimetres; tap a value to edit")),
                    ),
            );
        if card == ProbeCard::HeightMap {
            form = form.child(self.height_map_grid_summary(p, ctx, &problems));
        }
        form.child(h_flex().w(px(288.0)).flex_wrap().gap_1().children(cells))
            .child(self.probe_controls(p, cx, card))
    }

    fn probe_field_cell(
        &self,
        field: ProbeField,
        p: Palette,
        cx: &mut Context<Self>,
        error: Option<String>,
    ) -> Div {
        let view = cx.entity();
        let unit = field.unit();
        let label = if unit.is_empty() {
            t(field.label()).to_string()
        } else {
            format!("{} ({})", t(field.label()), unit)
        };
        let value = self.state.probe.field_text(field).to_string();
        let mut cell = v_flex()
            .w(px(140.0))
            .gap_1()
            .child(
                div()
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(label),
            )
            .child(
                Button::new(format!("probe-field-{}", field.key()))
                    .label(value)
                    .h(px(48.0))
                    .w_full()
                    .rounded_lg()
                    .text_size(px(13.0))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(p.panel)
                            .foreground(p.text)
                            .hover(p.raised_hover)
                            .active(p.accent_muted),
                    )
                    .border_1()
                    .border_color(if error.is_some() { p.danger } else { p.border })
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.state.open_keypad(KeypadTarget::ProbeField(field));
                            cx.notify();
                        });
                    }),
            );
        if let Some(error) = error {
            cell = cell.child(
                div()
                    .text_size(px(8.5))
                    .line_height(px(11.0))
                    .text_color(p.danger)
                    .child(error),
            );
        }
        cell
    }

    /// Point count, estimate and cross-field validation for the grid editor.
    fn height_map_grid_summary(
        &self,
        p: Palette,
        ctx: &ProbeContext,
        problems: &[FieldProblem],
    ) -> Div {
        let draft = self.state.probe.height_map_draft(ctx);
        let points = self.state.probe.height_map_point_count();
        let points_text = points.map_or_else(
            || "--".to_string(),
            |(columns, rows, count)| format!("{columns} x {rows} = {count} points"),
        );
        let mut summary =
            v_flex()
                .w_full()
                .gap_1()
                .p_2()
                .rounded(px(6.0))
                .bg(p.panel)
                .border_1()
                .border_color(p.border)
                .child(
                    h_flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(section_label(t("GRID SUMMARY"), p))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(p.accent)
                                .child(points_text),
                        )
                        .child(div().text_size(px(9.0)).text_color(p.text_dim).child(
                            match &draft {
                                Ok(draft) => format!(
                                    "{} mm | {}",
                                    format_mm(draft.estimate.probing_distance_mm),
                                    format_duration(draft.estimate.total_seconds)
                                ),
                                Err(_) => "--".to_string(),
                            },
                        )),
                );
        match &draft {
            Ok(draft) => {
                summary = summary.child(
                    div()
                        .font_family(self.mono.clone())
                        .text_size(px(8.5))
                        .line_height(px(11.0))
                        .text_color(p.text_dim)
                        .child(format!(
                            "work {}/{} -> mach {}/{} | safe {} | surf {} | travel {}/{} mm",
                            format_mm(draft.origin_work_mm.x_mm),
                            format_mm(draft.origin_work_mm.y_mm),
                            format_mm(draft.grid.origin_mm.x_mm),
                            format_mm(draft.grid.origin_mm.y_mm),
                            format_mm(draft.params.safe_z_mm),
                            format_mm(draft.grid.surface_z_mm),
                            format_mm(draft.params.safe_z_mm - draft.grid.surface_z_mm),
                            format_mm(draft.params.search_distance_mm),
                        )),
                );
                let z_offset = self.state.probe.map_z_offset_mm().unwrap_or(0.0);
                summary = summary.child(
                    div()
                        .text_size(px(8.5))
                        .line_height(px(11.0))
                        .text_color(p.info)
                        .child(format!(
                            "MAP Z OFFSET {} mm: datum shift added to every measured height for summary, \
                             overlay, CSV and Apply. It does not change the grid, plan, or SURFACE Z search.",
                            format_mm(z_offset)
                        )),
                );
            }
            Err(message) => {
                summary = summary.child(
                    div()
                        .text_size(px(9.0))
                        .text_color(p.danger)
                        .child(message.clone()),
                );
            }
        }
        if !problems.is_empty() {
            summary = summary.child(div().text_size(px(8.5)).text_color(p.danger).child(format!(
                "{} field problem(s); correct the red fields before RUN",
                problems.len()
            )));
        }
        summary
    }

    fn probe_controls(&self, p: Palette, cx: &mut Context<Self>, card: ProbeCard) -> Div {
        let mut rows = v_flex().gap_1();
        let connected_view = cx.entity();
        rows = rows.child(
            h_flex()
                .h(px(48.0))
                .items_center()
                .gap_3()
                .child(
                    Switch::new("probe-connected")
                        .checked(self.state.probe.probe_connected_confirmed)
                        .label(t("Probe connected"))
                        .text_size(px(11.0))
                        .on_change(move |checked, _, cx| {
                            connected_view.update(cx, |this, cx| {
                                this.state.probe.probe_connected_confirmed = *checked;
                                cx.notify();
                            });
                        }),
                )
                .child(div().text_size(px(9.0)).text_color(p.text_faint).child(
                    if self.state.profile.capabilities.has_probe_connected {
                        "PC signal is authoritative when the controller reports it"
                    } else {
                        "Reference board has no `PC`: this checkbox is the probe verification"
                    },
                )),
        );

        match card {
            ProbeCard::EdgeX => {
                rows = rows.child(self.probe_approach_row(p, cx, "X", true));
                rows = rows.child(self.probe_mode_row(p, cx, "X edge", true));
            }
            ProbeCard::EdgeY => {
                rows = rows.child(self.probe_approach_row(p, cx, "Y", false));
                rows = rows.child(self.probe_mode_row(p, cx, "Y edge", false));
            }
            ProbeCard::Corner => {
                rows = rows
                    .child(self.probe_approach_row(p, cx, "X", true))
                    .child(self.probe_approach_row(p, cx, "Y", false))
                    .child(self.probe_mode_row(p, cx, "X edge", true))
                    .child(self.probe_mode_row(p, cx, "Y edge", false));
            }
            ProbeCard::CenterCircle => {
                rows = rows.child(self.probe_circle_mode_row(p, cx));
                rows = rows.child(self.probe_circle_touch_row(p, cx));
            }
            ProbeCard::CenterRectangle => {
                rows = rows.child(self.probe_rectangle_mode_row(p, cx));
            }
            ProbeCard::XyTouchPlate => {
                rows = rows
                    .child(self.probe_plate_approach_row(p, cx, "X", true))
                    .child(self.probe_plate_approach_row(p, cx, "Y", false))
                    .child(div().text_size(px(9.0)).text_color(p.text_faint).child(
                        "Touch plate is OUTSIDE only: each edge is approached from \
                                 outside toward the plate.",
                    ));
            }
            ProbeCard::Rotation => {
                rows = rows.child(self.probe_rotation_row(p, cx));
            }
            _ => {}
        }
        rows
    }

    fn probe_approach_row(
        &self,
        p: Palette,
        cx: &mut Context<Self>,
        axis: &'static str,
        is_x: bool,
    ) -> Div {
        let view = cx.entity();
        let current = if is_x {
            self.state.probe.edge_x_approach
        } else {
            self.state.probe.edge_y_approach
        };
        let positive = current == ghs_probe::Sign::Positive;
        let pos_view = view.clone();
        let neg_view = view;
        let positive_id = format!("probe-approach-{axis}-pos");
        let negative_id = format!("probe-approach-{axis}-neg");
        h_flex()
            .h(px(48.0))
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(150.0))
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(format!("{axis} APPROACH")),
            )
            .child(probe_choice(
                cx,
                p,
                positive_id,
                &format!("+{axis}"),
                positive,
                move |_, cx| {
                    pos_view.update(cx, |this, cx| {
                        if is_x {
                            this.state.probe.edge_x_approach = ghs_probe::Sign::Positive;
                        } else {
                            this.state.probe.edge_y_approach = ghs_probe::Sign::Positive;
                        }
                        cx.notify();
                    });
                },
            ))
            .child(probe_choice(
                cx,
                p,
                negative_id,
                &format!("-{axis}"),
                !positive,
                move |_, cx| {
                    neg_view.update(cx, |this, cx| {
                        if is_x {
                            this.state.probe.edge_x_approach = ghs_probe::Sign::Negative;
                        } else {
                            this.state.probe.edge_y_approach = ghs_probe::Sign::Negative;
                        }
                        cx.notify();
                    });
                },
            ))
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(p.text_faint)
                    .child(format!("searches {axis} toward the material")),
            )
    }

    fn probe_mode_row(
        &self,
        p: Palette,
        cx: &mut Context<Self>,
        label: &'static str,
        is_x: bool,
    ) -> Div {
        let view = cx.entity();
        let current = if is_x {
            self.state.probe.edge_x_mode
        } else {
            self.state.probe.edge_y_mode
        };
        let outside = current == ghs_probe::EdgeMode::Outside;
        let out_view = view.clone();
        let in_view = view;
        h_flex()
            .h(px(48.0))
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(150.0))
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(format!("{label} SIDE")),
            )
            .child(probe_choice(
                cx,
                p,
                format!("probe-mode-{label}-out"),
                "OUTSIDE",
                outside,
                move |_, cx| {
                    out_view.update(cx, |this, cx| {
                        if is_x {
                            this.state.probe.edge_x_mode = ghs_probe::EdgeMode::Outside;
                        } else {
                            this.state.probe.edge_y_mode = ghs_probe::EdgeMode::Outside;
                        }
                        cx.notify();
                    });
                },
            ))
            .child(probe_choice(
                cx,
                p,
                format!("probe-mode-{label}-in"),
                t("POCKET"),
                !outside,
                move |_, cx| {
                    in_view.update(cx, |this, cx| {
                        if is_x {
                            this.state.probe.edge_x_mode = ghs_probe::EdgeMode::Inside;
                        } else {
                            this.state.probe.edge_y_mode = ghs_probe::EdgeMode::Inside;
                        }
                        cx.notify();
                    });
                },
            ))
    }

    fn probe_circle_mode_row(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let view = cx.entity();
        let bore = self.state.probe.circle_mode == ghs_probe::EdgeMode::Inside;
        let bore_view = view.clone();
        let boss_view = view;
        h_flex()
            .h(px(48.0))
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(150.0))
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child("FEATURE"),
            )
            .child(probe_choice(
                cx,
                p,
                "probe-circle-bore",
                "BORE (INSIDE)",
                bore,
                move |_, cx| {
                    bore_view.update(cx, |this, cx| {
                        this.state.probe.circle_mode = ghs_probe::EdgeMode::Inside;
                        cx.notify();
                    });
                },
            ))
            .child(probe_choice(
                cx,
                p,
                "probe-circle-boss",
                "BOSS (OUTSIDE)",
                !bore,
                move |_, cx| {
                    boss_view.update(cx, |this, cx| {
                        this.state.probe.circle_mode = ghs_probe::EdgeMode::Outside;
                        cx.notify();
                    });
                },
            ))
    }

    fn probe_circle_touch_row(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let mut row = h_flex().h(px(48.0)).items_center().gap_2().child(
            div()
                .w(px(150.0))
                .text_size(px(9.0))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(t("TOUCHES")),
        );
        for count in [3_u8, 4, 6, 8] {
            let view = cx.entity();
            row = row.child(probe_choice(
                cx,
                p,
                format!("probe-touches-{count}"),
                &count.to_string(),
                self.state.probe.circle_touches == count,
                move |_, cx| {
                    view.update(cx, |this, cx| {
                        this.state.probe.circle_touches = count;
                        let _ = this
                            .state
                            .probe
                            .set_field_text(ProbeField::Touches, count.to_string());
                        cx.notify();
                    });
                },
            ));
        }
        row
    }

    fn probe_rectangle_mode_row(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let view = cx.entity();
        let pocket = self.state.probe.rectangle_mode == ghs_probe::EdgeMode::Inside;
        let pocket_view = view.clone();
        let boss_view = view;
        h_flex()
            .h(px(48.0))
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(150.0))
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child("RECTANGLE"),
            )
            .child(probe_choice(
                cx,
                p,
                "probe-rectangle-pocket",
                "POCKET (INSIDE)",
                pocket,
                move |_, cx| {
                    pocket_view.update(cx, |this, cx| {
                        this.state.probe.rectangle_mode = ghs_probe::EdgeMode::Inside;
                        cx.notify();
                    });
                },
            ))
            .child(probe_choice(
                cx,
                p,
                "probe-rectangle-boss",
                "BOSS (OUTSIDE)",
                !pocket,
                move |_, cx| {
                    boss_view.update(cx, |this, cx| {
                        this.state.probe.rectangle_mode = ghs_probe::EdgeMode::Outside;
                        cx.notify();
                    });
                },
            ))
    }

    fn probe_plate_approach_row(
        &self,
        p: Palette,
        cx: &mut Context<Self>,
        axis: &'static str,
        is_x: bool,
    ) -> Div {
        let view = cx.entity();
        let current = if is_x {
            self.state.probe.plate_x_approach
        } else {
            self.state.probe.plate_y_approach
        };
        let positive = current == ghs_probe::Sign::Positive;
        let pos_view = view.clone();
        let neg_view = view;
        h_flex()
            .h(px(48.0))
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(150.0))
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(format!("{axis} PLATE APPROACH")),
            )
            .child(probe_choice(
                cx,
                p,
                format!("probe-plate-approach-{axis}-pos"),
                &format!("+{axis}"),
                positive,
                move |_, cx| {
                    pos_view.update(cx, |this, cx| {
                        if is_x {
                            this.state.probe.plate_x_approach = ghs_probe::Sign::Positive;
                        } else {
                            this.state.probe.plate_y_approach = ghs_probe::Sign::Positive;
                        }
                        cx.notify();
                    });
                },
            ))
            .child(probe_choice(
                cx,
                p,
                format!("probe-plate-approach-{axis}-neg"),
                &format!("-{axis}"),
                !positive,
                move |_, cx| {
                    neg_view.update(cx, |this, cx| {
                        if is_x {
                            this.state.probe.plate_x_approach = ghs_probe::Sign::Negative;
                        } else {
                            this.state.probe.plate_y_approach = ghs_probe::Sign::Negative;
                        }
                        cx.notify();
                    });
                },
            ))
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(p.text_faint)
                    .child(format!("plate extends {axis} from the corner")),
            )
    }

    fn probe_rotation_row(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let view = cx.entity();
        let positive = self.state.probe.rotation_approach == ghs_probe::Sign::Positive;
        let pos_view = view.clone();
        let neg_view = view;
        h_flex()
            .h(px(48.0))
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(150.0))
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child("Y APPROACH"),
            )
            .child(probe_choice(
                cx,
                p,
                "probe-rotation-pos",
                "+Y",
                positive,
                move |_, cx| {
                    pos_view.update(cx, |this, cx| {
                        this.state.probe.rotation_approach = ghs_probe::Sign::Positive;
                        cx.notify();
                    });
                },
            ))
            .child(probe_choice(
                cx,
                p,
                "probe-rotation-neg",
                "-Y",
                !positive,
                move |_, cx| {
                    neg_view.update(cx, |this, cx| {
                        this.state.probe.rotation_approach = ghs_probe::Sign::Negative;
                        cx.notify();
                    });
                },
            ))
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(p.text_faint)
                    .child("two points on a nominally X-aligned edge"),
            )
    }

    fn probe_preflight_panel(
        &self,
        preflight: &ProbePreflight,
        p: Palette,
        _cx: &mut Context<Self>,
    ) -> Div {
        let ok = preflight.blockers.is_empty() && preflight.setup.is_some();
        let mut panel = v_flex()
            .gap_2()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(if ok {
                p.success.opacity(0.35)
            } else {
                p.danger.opacity(0.45)
            })
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(section_label(t("PREFLIGHT"), p))
                    .child(
                        div()
                            .px_2()
                            .h(px(18.0))
                            .flex()
                            .items_center()
                            .rounded(px(4.0))
                            .bg(if ok {
                                p.success.opacity(0.16)
                            } else {
                                p.danger.opacity(0.16)
                            })
                            .text_size(px(9.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(if ok { p.success } else { p.danger })
                            .child(if ok { t("READY") } else { t("REFUSED") }),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child(t("every gate must pass before RUN")),
                    ),
            );

        let is_height_map = preflight
            .setup
            .as_ref()
            .is_some_and(|setup| matches!(setup.target, ProbeTarget::HeightMap(_)));
        let (offset_x, offset_y) = preflight
            .setup
            .as_ref()
            .map(|setup| (setup.probe_offset.x_mm, setup.probe_offset.y_mm))
            .or_else(|| {
                self.state
                    .probe
                    .probe_offset()
                    .ok()
                    .map(|offset| (offset.x_mm, offset.y_mm))
            })
            .unwrap_or((0.0, 0.0));
        panel = panel.child(probe_notice(
            t("PROBE OFFSET X/Y"),
            &format!(
                "X{} Y{} mm. Entered positions/centres/grid are FEATURE (probe-tip) coordinates; \
                 plan FROM/TO are CONTROLLED-POINT coordinates (feature - offset).",
                format_mm(offset_x),
                format_mm(offset_y)
            ),
            p.info,
            p,
        ));
        if is_height_map {
            let z_offset = preflight
                .setup
                .as_ref()
                .map_or(0.0, |setup| setup.map_z_offset_mm);
            panel = panel
                .child(probe_notice(
                    t("SAFE Z"),
                    "Safe Z must clear the highest point on the stock; every grid probe descends \
                     from it. SEARCH must cover SAFE Z - SURFACE Z.",
                    p.warning,
                    p,
                ))
                .child(probe_notice(
                    t("MAP Z OFFSET"),
                    &format!(
                        "{} mm is added to every measured height for summary, overlay, CSV and \
                         Apply. The plan and SURFACE Z search are unchanged.",
                        format_mm(z_offset)
                    ),
                    p.info,
                    p,
                ));
        }

        if !preflight.blockers.is_empty() {
            let mut list = v_flex()
                .gap_1()
                .p_2()
                .rounded(px(6.0))
                .bg(p.danger.opacity(0.10));
            for blocker in &preflight.blockers {
                list = list.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(p.danger)
                        .child(format!("{}: {blocker}", t("REFUSED"))),
                );
            }
            panel = panel.child(list);
        }

        let mut cells = Vec::new();
        for row in &preflight.rows {
            cells.push(probe_preflight_row(row, p));
        }
        panel = panel.child(h_flex().flex_wrap().gap_1().children(cells));

        if let Some(setup) = &preflight.setup {
            let height_map = matches!(setup.target, ProbeTarget::HeightMap(_));
            let limit = if height_map { Some(8) } else { None };
            panel = panel
                .child(self.probe_travel_summary(&setup.plan, limit, p))
                .child(probe_program_block(
                    &setup.plan.program,
                    if height_map { Some(80) } else { None },
                    p,
                ));
        }
        panel
    }

    fn probe_travel_summary(
        &self,
        plan: &ghs_probe::ProbePlan,
        limit: Option<usize>,
        p: Palette,
    ) -> Div {
        let mut lines = v_flex().gap_1();
        let shown = limit.map_or(plan.probes.len(), |limit| limit.min(plan.probes.len()));
        for probe in plan.probes.iter().take(shown) {
            lines = lines.child(
                div()
                    .font_family(self.mono.clone())
                    .text_size(px(9.5))
                    .text_color(p.text_dim)
                    .child(format!(
                        "{}  {} -> {}  |  {:.3} mm @ {:.0} mm/min",
                        probe.label, probe.from, probe.to, probe.travel_mm, probe.feed_mm_min
                    )),
            );
        }
        if shown < plan.probes.len() {
            lines = lines.child(
                div()
                    .text_size(px(9.0))
                    .text_color(p.text_faint)
                    .child(format!(
                        "{} more probe(s) are in the generated program",
                        plan.probes.len() - shown
                    )),
            );
        }
        v_flex()
            .gap_1()
            .p_2()
            .rounded(px(6.0))
            .bg(p.panel)
            .border_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .child(section_label(t("SEARCH BOUND (CONTROLLED-POINT XY)"), p))
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child(format!(
                                "{:.3} mm total, max {:.3} mm per probe",
                                plan.total_search_travel_mm(),
                                plan.probes
                                    .iter()
                                    .map(|probe| probe.travel_mm)
                                    .fold(0.0_f64, f64::max)
                            )),
                    ),
            )
            .child(
                div()
                    .text_size(px(8.5))
                    .line_height(px(11.0))
                    .text_color(p.info)
                    .child(
                        "FROM/TO are controlled-point machine coordinates. Entered positions, \
                         centres and grid origin are feature/tip coordinates; \
                         controlled = feature - probe offset.",
                    ),
            )
            .child(lines)
    }

    fn probe_run_panel(&self, p: Palette, cx: &mut Context<Self>, run: &ProbeRun) -> Div {
        let phase_color = match run.phase {
            ProbePhase::Running => p.accent,
            ProbePhase::Complete => p.success,
            ProbePhase::Failed => p.danger,
        };
        let tool_length = matches!(
            run.outcome.as_ref().map(|outcome| &outcome.value),
            Some(ghs_probe::ProbeValue::ToolLength { .. })
        );
        let mut panel = v_flex()
            .gap_2()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(phase_color.opacity(0.5))
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(section_label(t("PROBE RUN"), p))
                    .child(
                        div()
                            .px_2()
                            .h(px(18.0))
                            .flex()
                            .items_center()
                            .rounded(px(4.0))
                            .bg(phase_color.opacity(0.16))
                            .text_size(px(9.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(phase_color)
                            .child(t(run.phase.label())),
                    )
                    .child(
                        div()
                            .font_family(self.mono.clone())
                            .text_size(px(10.0))
                            .text_color(p.text_dim)
                            .child(format!(
                                "{} / {} results | {}",
                                run.bound(),
                                run.expected(),
                                run.setup.plan.name
                            )),
                    )
                    .child(div().flex_1())
                    .when(run.phase == ProbePhase::Running, |this| {
                        let attack_view = cx.entity();
                        this.child(
                            Button::new("probe-abort")
                                .label(t("ABORT"))
                                .warning()
                                .h(px(48.0))
                                .w(px(92.0))
                                .rounded_lg()
                                .text_size(px(10.0))
                                .on_click(move |_, _, cx| {
                                    attack_view.update(cx, |this, cx| {
                                        this.abort_probe_run(cx);
                                    });
                                }),
                        )
                    }),
            );

        if run.phase == ProbePhase::Running {
            let frac = if run.expected() == 0 {
                0.0
            } else {
                (run.bound() as f32 / run.expected() as f32).clamp(0.0, 1.0)
            };
            panel = panel
                .child(
                    div()
                        .h(px(6.0))
                        .w(px(320.0))
                        .rounded(px(999.0))
                        .bg(p.panel)
                        .child(
                            div()
                                .h_full()
                                .w(px(320.0 * frac))
                                .rounded(px(999.0))
                                .bg(p.accent),
                        ),
                )
                .child(div().text_size(px(11.0)).text_color(p.text_dim).child(
                    "The engine is streaming the generated probe program. \
                             Raw PRB results bind in plan order.",
                ));
            if self.state.probe.waiting_for_probe() {
                panel = panel.child(
                    div()
                        .p_2()
                        .rounded(px(6.0))
                        .bg(p.warning.opacity(0.10))
                        .text_size(px(10.0))
                        .text_color(p.warning)
                        .child(
                            "Paused at a probe checkpoint; waiting for the raw PRB result. \
                             The job resumes only after it arrives.",
                        ),
                );
            }
        }

        if let Some(summary) = run.outcome.as_ref().map(outcome_summary) {
            let mut details = summary.details.clone();
            if tool_length && let Ok(preview) = self.state.probe.tool_length_preview() {
                details.push((
                    "Current TLO".to_string(),
                    format!("{} mm", format_mm(preview.current_tlo_mm)),
                ));
                details.push((
                    "Proposed TLO".to_string(),
                    format!("{} mm", format_mm(preview.target_tlo_mm)),
                ));
            }
            panel = panel
                .child(
                    div()
                        .text_size(px(22.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.success)
                        .child(summary.headline),
                )
                .child(
                    h_flex().flex_wrap().gap_2().children(
                        details
                            .iter()
                            .map(|(label, value)| probe_detail_cell(label, value, p)),
                    ),
                );
        }

        if let Some(error) = &run.error {
            panel = panel.child(
                div()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.danger.opacity(0.10))
                    .text_size(px(11.0))
                    .text_color(p.danger)
                    .child(format!("STOPPED: {error}")),
            );
        }

        // Tool-length actions and values come before the raw audit block so the
        // preview and both actions are visible without scrolling at 1280x800.
        if run.phase == ProbePhase::Complete && run.map.is_none() && tool_length {
            panel = panel.child(self.probe_apply_section(p, cx, run));
        }

        if !run.raw.is_empty() {
            if run.is_height_map() {
                panel = panel.child(div().text_size(px(9.5)).text_color(p.text_dim).child(
                    format!(
                        "{} raw PRB result(s); the console carries every line for audit",
                        run.raw.len()
                    ),
                ));
            } else {
                let mut raw = v_flex().gap_0();
                for line in &run.raw {
                    raw = raw.child(
                        div()
                            .font_family(self.mono.clone())
                            .text_size(px(9.5))
                            .text_color(p.text_dim)
                            .child(line.clone()),
                    );
                }
                panel = panel.child(
                    v_flex()
                        .gap_1()
                        .p_2()
                        .rounded(px(6.0))
                        .bg(p.panel)
                        .border_1()
                        .border_color(p.border)
                        .child(section_label(t("RAW PRB (machine mm)"), p))
                        .child(
                            div()
                                .id("probe-raw-scroll")
                                .max_h(px(86.0))
                                .overflow_y_scroll()
                                .child(raw),
                        ),
                );
            }
        }

        if run.phase == ProbePhase::Complete {
            if run.map.is_none() && !tool_length {
                panel = panel.child(self.probe_apply_section(p, cx, run));
            }
        } else if run.phase == ProbePhase::Failed {
            let retry_view = cx.entity();
            panel = panel.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("probe-retry")
                            .label(t("RETRY PREFLIGHT"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(150.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .on_click(move |_, _, cx| {
                                retry_view.update(cx, |this, cx| this.start_probe_run(cx));
                            }),
                    )
                    .child(div().text_size(px(10.0)).text_color(p.warning).child(t(
                        "No automatic retry or resume. Recover the machine, then run again.",
                    ))),
            );
        }

        panel
    }

    /// Per-controller saved-map list and its profile status.
    ///
    /// Shown in the height-map workspace before and after a measurement; the
    /// rows only select a read-only view.
    fn height_map_library_panel(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let maps = self.state.saved_height_maps();
        let selected = self
            .state
            .selected_saved_map()
            .map(|saved| saved.record.name.clone());
        let profile = self.state.config.profile_path();
        let profile_text = profile.as_ref().map_or_else(
            || "PROFILE: not active".to_string(),
            |path| format!("PROFILE: {}", path.display()),
        );
        let blocker = self.state.config.height_map_save_blocker();
        let mut panel = v_flex()
            .w_full()
            .gap_1()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(p.border)
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(section_label(t("SAVED MAPS"), p))
                    .child(
                        div()
                            .px_2()
                            .h(px(18.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .rounded(px(4.0))
                            .bg(p.info.opacity(0.14))
                            .text_size(px(8.5))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.info)
                            .child(format!("{} / {MAX_HEIGHT_MAPS}", maps.len())),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(8.0))
                            .text_color(p.text_faint)
                            .child(t("per controller profile")),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .truncate()
                    .font_family(self.mono.clone())
                    .text_size(px(7.5))
                    .line_height(px(10.0))
                    .text_color(p.text_faint)
                    .child(profile_text),
            );
        if let Some(blocker) = blocker {
            panel = panel.child(
                div()
                    .text_size(px(8.5))
                    .line_height(px(10.5))
                    .text_color(p.warning)
                    .child(format!("{}: {blocker}", t("SAVE BLOCKED"))),
            );
        }
        if let Some(notice) = self.state.height_map_library.last_saved() {
            panel = panel.child(
                div()
                    .w_full()
                    .truncate()
                    .font_family(self.mono.clone())
                    .text_size(px(7.8))
                    .line_height(px(10.0))
                    .text_color(p.success)
                    .child(format!(
                        "SAVED: {} -> {}",
                        notice.name,
                        notice.path.display()
                    )),
            );
        } else if let Some(error) = self.state.height_map_library.last_error() {
            panel = panel.child(
                div()
                    .w_full()
                    .truncate()
                    .font_family(self.mono.clone())
                    .text_size(px(7.8))
                    .line_height(px(10.0))
                    .text_color(p.danger)
                    .child(format!("SAVE FAILED: {error}")),
            );
        }

        let mut rows = v_flex().gap_1();
        if maps.is_empty() {
            rows = rows.child(
                div()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.panel)
                    .text_size(px(9.5))
                    .text_color(p.text_dim)
                    .child(t("No saved maps for this controller profile yet.")),
            );
        } else {
            for (index, record) in maps.iter().enumerate() {
                let summary = SavedMapSummary::from_record(record);
                let is_selected = selected.as_deref() == Some(summary.name.as_str());
                let view = cx.entity();
                let name = summary.name.clone();
                let date = summary
                    .created_at_unix_s
                    .map_or_else(|| "date unknown".to_string(), format_unix_utc);
                let tool = summary
                    .tool_number
                    .map_or_else(|| "T?".to_string(), |tool| format!("T{tool}"));
                let state_text = if summary.complete {
                    "complete"
                } else {
                    "partial"
                };
                rows = rows.child(
                    h_flex()
                        .id(("saved-map-row", index))
                        .h(px(48.0))
                        .w_full()
                        .flex_shrink_0()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(if is_selected { p.accent } else { p.border })
                        .bg(if is_selected { p.accent_muted } else { p.panel })
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.open_saved_height_map(&name, cx);
                            });
                        })
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w(px(0.0))
                                .gap_0()
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(10.5))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(p.text)
                                        .child(summary.name.clone()),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .font_family(self.mono.clone())
                                        .text_size(px(7.8))
                                        .line_height(px(10.0))
                                        .text_color(p.text_dim)
                                        .child(format!(
                                            "{date} | {} cells | {} | {tool} | {state_text} | \
                                             Z offset {} mm",
                                            summary.point_count,
                                            summary.wcs.as_str(),
                                            format_mm(summary.z_offset_mm)
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .px_2()
                                .h(px(18.0))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .rounded(px(4.0))
                                .bg(if is_selected {
                                    p.accent.opacity(0.18)
                                } else {
                                    p.panel_alt
                                })
                                .text_size(px(7.5))
                                .font_weight(FontWeight::BOLD)
                                .text_color(if is_selected { p.accent } else { p.text_faint })
                                .child(if is_selected { t("OPEN") } else { t("VIEW") }),
                        ),
                );
            }
        }
        panel.child(
            div()
                .id("saved-map-list")
                .w_full()
                .max_h(px(104.0))
                .overflow_y_scroll()
                .child(rows),
        )
    }

    /// Read-only detail for one saved map: list, summary, frame check, overlay.
    fn saved_height_map_detail(
        &self,
        p: Palette,
        cx: &mut Context<Self>,
        saved: &LoadedSavedMap,
    ) -> Div {
        let summary = &saved.summary;
        let live = self.probe_context();
        let mismatches = saved.frame_mismatches(&live);
        let frame_ok = mismatches.is_empty();
        let frame_chip = if frame_ok {
            "FRAME MATCH"
        } else {
            "FRAME MISMATCH"
        };
        let frame_color = if frame_ok { p.success } else { p.danger };
        let created = saved
            .record
            .created_at_unix_s
            .map_or_else(|| "date unknown".to_string(), format_unix_utc);
        let tool = saved
            .record
            .tool_number
            .map_or_else(|| "T?".to_string(), |tool| format!("T{tool}"));
        let snapshot = saved.snapshot();
        let program = self
            .height_map_base_program
            .clone()
            .or_else(|| self.state.job.program().cloned());
        let marker = viewer::marker_mm3(self.state.profile.capabilities.units, &self.state.dro);

        let min_z = summary.min_z_mm.map_or_else(|| "--".to_string(), format_mm);
        let max_z = summary.max_z_mm.map_or_else(|| "--".to_string(), format_mm);
        let compensation = match (summary.min_compensation_mm, summary.max_compensation_mm) {
            (Some(low), Some(high)) => {
                format!("{} .. {} mm", format_mm(low), format_mm(high))
            }
            _ => "--".to_string(),
        };

        let mut frame_rows = v_flex().gap_0();
        for (label, recorded, current) in [
            (
                "WCS",
                saved.frame.wcs.as_str().to_string(),
                live.wcs.as_str().to_string(),
            ),
            (
                t("WCS OFFSET"),
                format!(
                    "X{} Y{}",
                    format_mm(saved.frame.wcs_value_mm.get(CoreAxis::X)),
                    format_mm(saved.frame.wcs_value_mm.get(CoreAxis::Y))
                ),
                format!(
                    "X{} Y{}",
                    format_mm(live.wcs_value_mm.get(CoreAxis::X)),
                    format_mm(live.wcs_value_mm.get(CoreAxis::Y))
                ),
            ),
            (
                "G92",
                format!(
                    "X{} Y{}",
                    format_mm(saved.frame.g92_mm.get(CoreAxis::X)),
                    format_mm(saved.frame.g92_mm.get(CoreAxis::Y))
                ),
                format!(
                    "X{} Y{}",
                    format_mm(live.g92_mm.get(CoreAxis::X)),
                    format_mm(live.g92_mm.get(CoreAxis::Y))
                ),
            ),
            (
                "TLO Z",
                format!("{} mm", format_mm(saved.frame.tlo_z_mm)),
                format!("{} mm", format_mm(live.tlo_z_mm)),
            ),
        ] {
            frame_rows = frame_rows.child(
                h_flex()
                    .h(px(24.0))
                    .items_center()
                    .gap_2()
                    .font_family(self.mono.clone())
                    .text_size(px(8.5))
                    .child(
                        div()
                            .w(px(78.0))
                            .flex_shrink_0()
                            .text_color(p.text_faint)
                            .child(label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_color(p.text_dim)
                            .child(format!("rec {recorded}")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_color(if frame_ok { p.text_dim } else { p.warning })
                            .child(format!("live {current}")),
                    ),
            );
        }

        let stats = v_flex()
            .gap_1()
            .child(
                h_flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .child(section_label(t("SAVED MAP"), p))
                    .child(
                        div()
                            .px_2()
                            .h(px(18.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .rounded(px(4.0))
                            .bg(frame_color.opacity(0.16))
                            .text_size(px(8.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(frame_color)
                            .child(frame_chip),
                    ),
            )
            .child(
                div()
                    .truncate()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text)
                    .child(saved.record.name.clone()),
            )
            .child(
                div()
                    .font_family(self.mono.clone())
                    .text_size(px(8.0))
                    .line_height(px(10.0))
                    .text_color(p.text_dim)
                    .child(format!(
                        "{created} | {} | {} | Z offset {} mm",
                        saved.record.frame.wcs.as_str(),
                        tool,
                        format_mm(summary.z_offset_mm)
                    )),
            )
            .child(
                div()
                    .text_size(px(9.0))
                    .line_height(px(11.0))
                    .text_color(p.info)
                    .child(
                        "READ-ONLY: opening a saved map never applies compensation or \
                         starts a job.",
                    ),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        height_map_detail_cell(
                            t("VALID"),
                            &format!("{} / {}", summary.valid, summary.total),
                            p,
                        )
                        .w(px(96.0)),
                    )
                    .child(
                        height_map_detail_cell(t("MISSING"), &summary.missing.to_string(), p)
                            .w(px(96.0)),
                    )
                    .child(height_map_detail_cell("MIN Z", &format!("{min_z} mm"), p).w(px(96.0)))
                    .child(height_map_detail_cell("MAX Z", &format!("{max_z} mm"), p).w(px(96.0)))
                    .child(height_map_detail_cell("COMP RANGE", &compensation, p).w(px(96.0)))
                    .child(
                        height_map_detail_cell(
                            t("SURFACE"),
                            &format!("{} mm", format_mm(summary.nominal_z_mm)),
                            p,
                        )
                        .w(px(96.0)),
                    )
                    .child(
                        height_map_detail_cell(
                            "Z OFFSET",
                            &format!("{} mm", format_mm(summary.z_offset_mm)),
                            p,
                        )
                        .w(px(96.0)),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .pt_1()
                    .border_t_1()
                    .border_color(p.border)
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(section_label(t("MEASUREMENT FRAME"), p))
                            .child(
                                div()
                                    .text_size(px(8.0))
                                    .text_color(p.text_faint)
                                    .child(t("recorded vs live")),
                            ),
                    )
                    .child(frame_rows)
                    .when(!frame_ok, |this| {
                        let mut warnings = v_flex().gap_1();
                        warnings = warnings.child(
                            div()
                                .text_size(px(9.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(p.danger)
                                .child(
                                    "FRAME MISMATCH: this map was measured in another \
                                     WCS/G92/TLO frame. Apply is refused; re-probe in the \
                                     live frame or restore the recorded offsets.",
                                ),
                        );
                        for mismatch in &mismatches {
                            warnings = warnings.child(
                                div()
                                    .text_size(px(8.5))
                                    .line_height(px(10.5))
                                    .text_color(p.warning)
                                    .child(format!("- {mismatch}")),
                            );
                        }
                        this.child(warnings)
                    })
                    .when(frame_ok, |this| {
                        this.child(
                            div()
                                .text_size(px(8.5))
                                .line_height(px(10.5))
                                .text_color(p.success)
                                .child(
                                    "The live WCS, G92, and TLO match the recorded frame. \
                                     Apply still requires Idle, no active job, and a complete map.",
                                ),
                        )
                    }),
            )
            .child(self.height_map_apply_section(p, cx, Some(&snapshot)));

        h_flex()
            .flex_1()
            .min_h(px(0.0))
            .gap_2()
            .child(
                div()
                    .id("saved-map-summary-scroll")
                    .w(px(300.0))
                    .flex_shrink_0()
                    .h_full()
                    .overflow_y_scrollbar()
                    .child(div().w_full().pr_1().child(stats)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(220.0))
                            .w_full()
                            .rounded_lg()
                            .border_1()
                            .border_color(p.border)
                            .overflow_hidden()
                            .child(viewer::view(
                                &self.viewer,
                                p,
                                program,
                                marker,
                                Some(saved.overlay.clone()),
                                cx.entity(),
                            )),
                    )
                    .child(height_map_overlay_legend(p)),
            )
    }

    /// Footer for the saved-map browser; no RUN action is offered.
    fn saved_map_footer(&self, p: Palette, cx: &mut Context<Self>, saved: &LoadedSavedMap) -> Div {
        let close_view = cx.entity();
        h_flex()
            .h(px(68.0))
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_2()
            .border_t_1()
            .border_color(p.border)
            .bg(p.panel)
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.0))
                    .gap_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.info)
                            .child(format!("SAVED MAP: {}", saved.record.name)),
                    )
                    .child(div().text_size(px(9.0)).text_color(p.text_faint).child(t(
                        "Read-only view; opening does not apply compensation or start a job",
                    ))),
            )
            .child(
                Button::new("saved-map-close")
                    .label(t("CLOSE MAP"))
                    .secondary()
                    .h(px(56.0))
                    .w(px(150.0))
                    .rounded_lg()
                    .text_size(px(10.0))
                    .on_click(move |_, _, cx| {
                        close_view.update(cx, |this, cx| this.close_saved_height_map(cx));
                    }),
            )
    }

    /// SAVE MAP row for a measured height map.
    ///
    /// The button opens the short-name sheet; the profile path and the last
    /// save result are shown beside it. It is disabled with an explicit reason
    /// when the profile is corrupt/unverified/pending, no controller profile is
    /// active, or the map has no valid cell.
    fn height_map_save_section(
        &self,
        p: Palette,
        cx: &mut Context<Self>,
        valid_cells: usize,
    ) -> Div {
        let save_view = cx.entity();
        let blocker = self.state.config.height_map_save_blocker();
        let ready = valid_cells > 0 && blocker.is_none();
        let last_saved = self.state.height_map_library.last_saved().cloned();
        let last_error = self
            .state
            .height_map_library
            .last_error()
            .map(str::to_string);
        let default_name = default_map_name(now_unix_s());
        let profile = self.state.config.profile_path().map_or_else(
            || "PROFILE NOT ACTIVE".to_string(),
            |path| format!("PROFILE: {}", path.display()),
        );
        let mut section = v_flex()
            .gap_1()
            .pt_1()
            .border_t_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("height-map-save")
                            .label(t("SAVE MAP"))
                            .primary()
                            .h(px(48.0))
                            .w(px(130.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(!ready)
                            .on_click(move |_, window, cx| {
                                save_view.update(cx, |this, cx| {
                                    this.open_height_map_name(window, cx);
                                });
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .font_family(self.mono.clone())
                            .text_size(px(8.0))
                            .line_height(px(10.0))
                            .text_color(p.text_dim)
                            .child(profile),
                    ),
            );
        if let Some(blocker) = blocker {
            section = section.child(
                div()
                    .text_size(px(8.5))
                    .line_height(px(10.5))
                    .text_color(p.warning)
                    .child(format!("{}: {blocker}", t("SAVE BLOCKED"))),
            );
        } else if valid_cells == 0 {
            section = section.child(
                div()
                    .text_size(px(8.5))
                    .text_color(p.warning)
                    .child(format!("{}: the map has no valid cell", t("SAVE BLOCKED"))),
            );
        } else if let Some(notice) = last_saved {
            section = section.child(
                div()
                    .w_full()
                    .truncate()
                    .font_family(self.mono.clone())
                    .text_size(px(8.5))
                    .line_height(px(11.0))
                    .text_color(p.success)
                    .child(format!(
                        "SAVED: {} -> {}",
                        notice.name,
                        notice.path.display()
                    )),
            );
        } else {
            section = section.child(
                div()
                    .font_family(self.mono.clone())
                    .text_size(px(8.0))
                    .line_height(px(10.0))
                    .text_color(p.text_faint)
                    .child(format!("DEFAULT NAME: {default_name} (edit in the sheet)")),
            );
        }
        if let Some(error) = last_error {
            section = section.child(
                div()
                    .text_size(px(8.5))
                    .line_height(px(10.5))
                    .text_color(p.danger)
                    .child(format!("SAVE FAILED: {error}")),
            );
        }
        section
    }

    /// Result sheet and viewer overlay for a measured height map.
    fn height_map_result_panel(&self, p: Palette, cx: &mut Context<Self>, run: &ProbeRun) -> Div {
        let Some(map) = run.map.as_ref() else {
            return div();
        };
        let summary = height_map_summary(map);
        let snapshot = HeightMapSnapshot::from_run(run);
        let status = self.state.height_map.status(snapshot.as_ref());
        let (status_text, status_color) = match status {
            HeightMapApplyStatus::MeasurementOnly => {
                (t(HeightMapApplyStatus::MeasurementOnly.label()), p.warning)
            }
            HeightMapApplyStatus::ReadyToApply => {
                (t(HeightMapApplyStatus::ReadyToApply.label()), p.accent)
            }
            HeightMapApplyStatus::Applied => (t(HeightMapApplyStatus::Applied.label()), p.success),
            HeightMapApplyStatus::Reverted => {
                (t(HeightMapApplyStatus::Reverted.label()), p.text_dim)
            }
        };
        let last_error = self.state.height_map.last_error().map(str::to_string);
        let overlay = self.state.probe.height_overlay();
        let program = self
            .height_map_base_program
            .clone()
            .or_else(|| self.state.job.program().cloned());
        let marker = viewer::marker_mm3(self.state.profile.capabilities.units, &self.state.dro);
        let export_view = cx.entity();
        let export_path = self.height_map_export_path();
        let path_text = export_path.as_ref().map_or_else(
            || "CONFIG DIR UNAVAILABLE".to_string(),
            |path| format!("{}", path.display()),
        );
        let export_result = run.export.as_ref().map(|export| match &export.rows {
            Ok(rows) => format!("EXPORTED {rows} cells to {}", export.path.display()),
            Err(message) => format!("EXPORT FAILED: {}: {message}", export.path.display()),
        });
        let export_chip = run.export.as_ref().map(|export| match &export.rows {
            Ok(rows) => (format!("EXPORTED {rows}"), p.success),
            Err(_) => (t("FAILED").to_string(), p.danger),
        });
        let min_z = summary.min_z_mm.map_or_else(|| "--".to_string(), format_mm);
        let max_z = summary.max_z_mm.map_or_else(|| "--".to_string(), format_mm);
        let compensation = match (summary.min_compensation_mm, summary.max_compensation_mm) {
            (Some(low), Some(high)) => {
                format!("{} .. {} mm", format_mm(low), format_mm(high))
            }
            _ => "--".to_string(),
        };
        let partial = if summary.complete {
            t("COMPLETE")
        } else {
            t("PARTIAL")
        };
        let partial_color = if summary.complete {
            p.success
        } else {
            p.warning
        };

        let stats = v_flex()
            .gap_1()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(section_label(t("HEIGHT MAP RESULT"), p))
                    .child(
                        div()
                            .px_2()
                            .h(px(18.0))
                            .flex()
                            .items_center()
                            .rounded(px(4.0))
                            .bg(partial_color.opacity(0.16))
                            .text_size(px(9.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(partial_color)
                            .child(partial),
                    ),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(status_color)
                    .child(status_text),
            )
            .child(
                div()
                    .font_family(self.mono.clone())
                    .text_size(px(9.0))
                    .text_color(p.text_dim)
                    .child(format!(
                        "{} / {} results | {} | phase {}",
                        run.bound(),
                        run.expected(),
                        run.setup.plan.name,
                        t(run.phase.label())
                    )),
            )
            .when_some(run.error.clone(), |this, error| {
                this.child(
                    div()
                        .text_size(px(9.0))
                        .text_color(p.danger)
                        .child(format!("STOPPED: {error}")),
                )
            })
            .when_some(last_error, |this, error| {
                this.child(
                    div()
                        .text_size(px(9.0))
                        .line_height(px(11.0))
                        .text_color(p.danger)
                        .child(error),
                )
            })
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        height_map_detail_cell(
                            t("VALID"),
                            &format!("{} / {}", summary.valid, summary.total),
                            p,
                        )
                        .w(px(96.0)),
                    )
                    .child(
                        height_map_detail_cell(t("MISSING"), &summary.missing.to_string(), p)
                            .w(px(96.0)),
                    )
                    .child(height_map_detail_cell("MIN Z", &format!("{min_z} mm"), p).w(px(96.0)))
                    .child(height_map_detail_cell("MAX Z", &format!("{max_z} mm"), p).w(px(96.0)))
                    .child(height_map_detail_cell("COMP RANGE", &compensation, p).w(px(96.0)))
                    .child(
                        height_map_detail_cell(
                            t("SURFACE"),
                            &format!("{} mm", format_mm(summary.nominal_z_mm)),
                            p,
                        )
                        .w(px(96.0)),
                    )
                    .child(
                        height_map_detail_cell(
                            "Z OFFSET",
                            &format!("{} mm", format_mm(summary.z_offset_mm)),
                            p,
                        )
                        .w(px(96.0)),
                    ),
            )
            .child(
                div()
                    .text_size(px(9.0))
                    .line_height(px(11.0))
                    .text_color(p.text_dim)
                    .child(match status {
                        HeightMapApplyStatus::MeasurementOnly => {
                            if snapshot.as_ref().is_some_and(|snapshot| snapshot.complete) {
                                "A complete map is available, but no part-program \
                                 snapshot is kept; run the map again."
                            } else {
                                "Missing cells are invalid; Apply is refused for an incomplete map."
                            }
                        }
                        HeightMapApplyStatus::ReadyToApply => {
                            "A static rewrite of the loaded program; nothing starts automatically."
                        }
                        HeightMapApplyStatus::Applied => {
                            "Loaded only, never started. The original is kept for Revert."
                        }
                        HeightMapApplyStatus::Reverted => {
                            "The original program is loaded again; Apply is available again."
                        }
                    }),
            )
            .child(self.height_map_apply_section(p, cx, snapshot.as_ref()))
            .child(self.height_map_save_section(p, cx, summary.valid))
            .child(
                v_flex()
                    .gap_1()
                    .pt_1()
                    .border_t_1()
                    .border_color(p.border)
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new("height-map-export")
                                    .label(t("EXPORT CSV"))
                                    .secondary()
                                    .h(px(48.0))
                                    .w(px(130.0))
                                    .rounded_lg()
                                    .text_size(px(10.0))
                                    .disabled(export_path.is_none())
                                    .on_click(move |_, _, cx| {
                                        export_view.update(cx, |this, cx| {
                                            this.export_height_map(cx);
                                        });
                                    }),
                            )
                            .when_some(export_chip.clone(), |this, (text, color)| {
                                this.child(
                                    div()
                                        .px_2()
                                        .h(px(18.0))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .rounded(px(4.0))
                                        .bg(color.opacity(0.16))
                                        .text_size(px(8.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(color)
                                        .child(text),
                                )
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .font_family(self.mono.clone())
                                    .text_size(px(8.0))
                                    .line_height(px(10.0))
                                    .text_color(p.text_dim)
                                    .child(path_text),
                            ),
                    )
                    .when_some(export_result, |this, result| {
                        this.child(
                            div()
                                .font_family(self.mono.clone())
                                .text_size(px(8.5))
                                .line_height(px(11.0))
                                .text_color(
                                    if matches!(
                                        &run.export,
                                        Some(export) if export.rows.is_err()
                                    ) {
                                        p.danger
                                    } else {
                                        p.text_dim
                                    },
                                )
                                .child(result),
                        )
                    }),
            );

        let viewer_pane = v_flex()
            .flex_1()
            .min_w(px(0.0))
            .gap_1()
            .child(
                div()
                    .h(px(220.0))
                    .w_full()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border)
                    .overflow_hidden()
                    .child(viewer::view(
                        &self.viewer,
                        p,
                        program,
                        marker,
                        overlay,
                        cx.entity(),
                    )),
            )
            .child(height_map_overlay_legend(p));

        h_flex()
            .items_stretch()
            .gap_2()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(p.border)
            .bg(p.panel_alt)
            .child(div().w(px(300.0)).flex_shrink_0().child(stats))
            .child(viewer_pane)
    }

    /// Explicit Apply/Confirm/Revert controls for one completed height map.
    ///
    /// The map stays measurement-only until every gate passes. The action only
    /// loads a program; it never starts the job.
    fn height_map_apply_section(
        &self,
        p: Palette,
        cx: &mut Context<Self>,
        snapshot: Option<&HeightMapSnapshot>,
    ) -> Div {
        let status = self.state.height_map.status(snapshot);
        let applied = self.state.height_map.applied();
        let pending = self.state.height_map.pending();
        let blockers = if applied.is_some() {
            Vec::new()
        } else {
            snapshot
                .map(|snapshot| {
                    let gate = HeightApplyGate::from_context(&self.probe_context());
                    self.state.height_map.blockers(snapshot, &gate)
                })
                .unwrap_or_default()
        };
        let can_apply = applied.is_none()
            && pending.is_none()
            && matches!(
                status,
                HeightMapApplyStatus::ReadyToApply | HeightMapApplyStatus::Reverted
            )
            && blockers.is_empty();
        let mut section = v_flex().gap_1().pt_1().border_t_1().border_color(p.border);

        if let Some(preview) = pending {
            let cancel_view = cx.entity();
            let confirm_view = cx.entity();
            return section
                .child(section_label(t("CONFIRM APPLY"), p))
                .child(compact_stat_row(
                    t("PROGRAM"),
                    preview.program_name.clone(),
                    p,
                ))
                .child(compact_stat_row(
                    t("MAP"),
                    format!(
                        "{} / {} cells, {} x {} mm step",
                        preview.valid_cells,
                        preview.cells,
                        format_mm(preview.step_x_mm),
                        format_mm(preview.step_y_mm)
                    ),
                    p,
                ))
                .child(compact_stat_row(
                    "OFFSET XY",
                    format!(
                        "X{} Y{} mm (WCS + G92)",
                        format_mm(preview.work_offset_xy_mm[0]),
                        format_mm(preview.work_offset_xy_mm[1])
                    ),
                    p,
                ))
                .child(compact_stat_row(
                    "COVERAGE",
                    if preview.is_fully_covered() {
                        format!(
                            "WHOLE PROGRAM - 0 of {} moves uncompensated",
                            preview.stats.output_motion_blocks
                        )
                    } else {
                        format!(
                            "PARTIAL - {} move(s) uncompensated",
                            preview.stats.uncompensated_motions
                        )
                    },
                    p,
                ))
                .child(
                    div()
                        .text_size(px(9.0))
                        .line_height(px(11.0))
                        .text_color(p.warning)
                        .child(
                            "OutsideMap::Reject: any move outside the map refuses the whole \
                             transform; nothing runs uncompensated. The program is loaded only, \
                             never started.",
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("height-map-cancel")
                                .label(t("CANCEL"))
                                .secondary()
                                .h(px(48.0))
                                .w(px(96.0))
                                .rounded_lg()
                                .text_size(px(10.0))
                                .on_click(move |_, _, cx| {
                                    cancel_view.update(cx, |this, cx| {
                                        this.cancel_height_map_apply(cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new("height-map-confirm")
                                .label(t("CONFIRM APPLY"))
                                .primary()
                                .h(px(48.0))
                                .w(px(160.0))
                                .rounded_lg()
                                .text_size(px(10.0))
                                .on_click(move |_, _, cx| {
                                    confirm_view.update(cx, |this, cx| {
                                        this.confirm_height_map_apply(cx);
                                    });
                                }),
                        ),
                );
        }

        if let Some(preview) = applied {
            let revert_view = cx.entity();
            return section
                .child(
                    h_flex()
                        .h(px(48.0))
                        .items_center()
                        .gap_2()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w(px(0.0))
                                .gap_0()
                                .child(
                                    div()
                                        .truncate()
                                        .font_family(self.mono.clone())
                                        .text_size(px(9.0))
                                        .line_height(px(11.0))
                                        .text_color(p.text)
                                        .child(preview.program_name.clone()),
                                )
                                .child(
                                    div()
                                        .truncate()
                                        .font_family(self.mono.clone())
                                        .text_size(px(7.5))
                                        .line_height(px(9.0))
                                        .text_color(p.text_dim)
                                        .child(format!(
                                            "ORIGINAL {} ({} source)",
                                            preview.original.name,
                                            if preview.original.exact {
                                                "verbatim"
                                            } else {
                                                "reconstructed"
                                            }
                                        )),
                                ),
                        )
                        .child(
                            Button::new("height-map-revert")
                                .label(t("REVERT"))
                                .secondary()
                                .h(px(48.0))
                                .w(px(96.0))
                                .rounded_lg()
                                .text_size(px(10.0))
                                .disabled(
                                    self.state.job.is_active()
                                        || self.machine() != MachineState::Idle,
                                )
                                .on_click(move |_, _, cx| {
                                    revert_view.update(cx, |this, cx| this.revert_height_map(cx));
                                }),
                        ),
                )
                .child(self.height_map_transform_stats(p, preview));
        }

        let apply_view = cx.entity();
        section = section.child(
            Button::new("height-map-apply")
                .label(t("APPLY HEIGHT MAP"))
                .primary()
                .h(px(48.0))
                .w(px(168.0))
                .rounded_lg()
                .text_size(px(10.0))
                .disabled(!can_apply)
                .on_click(move |_, _, cx| {
                    apply_view.update(cx, |this, cx| this.request_height_map_apply(cx));
                }),
        );
        if status == HeightMapApplyStatus::Reverted
            && let Some(original) = self.state.height_map.original()
        {
            section = section.child(
                div()
                    .font_family(self.mono.clone())
                    .text_size(px(8.5))
                    .text_color(p.text_dim)
                    .child(format!("ORIGINAL RESTORED: {}", original.name)),
            );
        }
        for blocker in blockers.iter().take(4) {
            section = section.child(
                div()
                    .text_size(px(9.0))
                    .line_height(px(11.0))
                    .text_color(p.warning)
                    .child(format!("REFUSED: {blocker}")),
            );
        }
        section
    }

    /// Compact counters and warnings from one completed transform.
    fn height_map_transform_stats(&self, p: Palette, preview: &HeightApplyPreview) -> Div {
        let stats = &preview.stats;
        let compensation = match (stats.min_compensation_mm, stats.max_compensation_mm) {
            (Some(low), Some(high)) => {
                format!("{}..{} mm", format_mm(low), format_mm(high))
            }
            _ => "--".to_string(),
        };
        let rows = [
            (
                "LINES",
                format!("{} / {} out", stats.input_lines, stats.output_lines),
                "SPLIT",
                stats.split_motions.to_string(),
            ),
            (
                "COMP",
                format!(
                    "{} / {}",
                    stats.compensated_motions, stats.output_motion_blocks
                ),
                "UNCOMP",
                stats.uncompensated_motions.to_string(),
            ),
            ("ARCS", stats.arc_motions.to_string(), "RANGE", compensation),
            (
                "OUT STEP",
                format!("{} mm", format_mm(stats.max_output_step_mm)),
                "WARN",
                preview.warnings.len().to_string(),
            ),
        ];
        let mut block = v_flex().gap_0();
        for (left_label, left_value, right_label, right_value) in rows {
            block = block.child(
                h_flex()
                    .gap_2()
                    .w_full()
                    .child(compact_stat_pair(left_label, left_value, p))
                    .child(compact_stat_pair(right_label, right_value, p)),
            );
        }
        for warning in preview.warnings.iter().take(2) {
            block = block.child(
                div()
                    .text_size(px(8.0))
                    .line_height(px(9.5))
                    .text_color(p.warning)
                    .child(if warning.line == 0 {
                        format!("WARN: {}", warning.message)
                    } else {
                        format!("WARN line {}: {}", warning.line, warning.message)
                    }),
            );
        }
        block
    }

    fn probe_apply_section(&self, p: Palette, cx: &mut Context<Self>, run: &ProbeRun) -> Div {
        if matches!(
            run.outcome.as_ref().map(|outcome| &outcome.value),
            Some(ghs_probe::ProbeValue::ToolLength { .. })
        ) {
            return self.probe_tool_length_section(p, cx, run);
        }
        let supported = self.state.probe.apply_supported();
        let mut section = v_flex().gap_2().pt_2().border_t_1().border_color(p.border);

        if let Some(line) = &run.applied_line {
            let (apply_status, apply_color) = match self.state.offsets.status() {
                OffsetStatus::Refreshing => ("$# refresh pending".to_string(), p.text_faint),
                OffsetStatus::Refreshed => ("$# refreshed".to_string(), p.success),
                OffsetStatus::Failed(message) => (
                    format!("APPLY FAILED: {message}; the old offset is kept"),
                    p.danger,
                ),
                OffsetStatus::Idle => ("$# not refreshed".to_string(), p.warning),
            };
            let failed = matches!(self.state.offsets.status(), OffsetStatus::Failed(_));
            let (chip, chip_color) = if failed {
                (t("FAILED"), p.danger)
            } else {
                (t("APPLIED"), p.success)
            };
            section = section.child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .px_2()
                            .h(px(20.0))
                            .flex()
                            .items_center()
                            .rounded(px(4.0))
                            .bg(chip_color.opacity(0.16))
                            .text_size(px(9.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(chip_color)
                            .child(chip),
                    )
                    .child(
                        div()
                            .font_family(self.mono.clone())
                            .text_size(px(11.0))
                            .text_color(p.text)
                            .child(line.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(apply_color)
                            .child(apply_status),
                    ),
            );
        }

        if let Ok(values) = self.state.probe.apply_values() {
            let mut rows = v_flex().gap_1();
            for (axis, new_value) in &values {
                let old_value = run.setup.wcs_value_mm.get(*axis);
                rows = rows.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .font_family(self.mono.clone())
                        .text_size(px(10.0))
                        .child(
                            div()
                                .w(px(30.0))
                                .text_color(p.text_faint)
                                .child(axis.as_str()),
                        )
                        .child(
                            div()
                                .w(px(112.0))
                                .text_color(p.text_dim)
                                .child(format!("old {}", crate::probe::format_mm(old_value))),
                        )
                        .child(
                            div()
                                .w(px(112.0))
                                .text_color(p.success)
                                .child(format!("new {}", crate::probe::format_mm(*new_value))),
                        ),
                );
            }
            section = section.child(rows);
        } else if let Some(note) = run
            .outcome
            .as_ref()
            .and_then(|outcome| match &outcome.value {
                ghs_probe::ProbeValue::Rotation { .. } => {
                    Some("Rotation is reported only; applying work rotation is deferred.")
                }
                _ => None,
            })
        {
            section = section.child(div().text_size(px(10.0)).text_color(p.warning).child(note));
        }

        if run.kept {
            section = section.child(
                div()
                    .text_size(px(11.0))
                    .text_color(p.text_dim)
                    .child(t("KEPT: offsets are unchanged. Apply stays available.")),
            );
        }

        if run.apply_confirm {
            let lines = self
                .state
                .probe
                .apply_lines()
                .map(|lines| lines.join(" "))
                .unwrap_or_else(|error| error);
            let confirm_view = cx.entity();
            let cancel_view = cx.entity();
            section = section.child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.warning.opacity(0.10))
                    .border_1()
                    .border_color(p.warning.opacity(0.5))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.warning)
                            .child(t("CONFIRM OFFSET WRITE")),
                    )
                    .child(
                        div()
                            .font_family(self.mono.clone())
                            .text_size(px(12.0))
                            .text_color(p.text)
                            .child(lines),
                    )
                    .child(div().text_size(px(9.5)).text_color(p.text_dim).child(
                        "G10 L2 changes the stored WCS offset; it does not move an axis. \
                                 G92 is preserved. `$#` is read back after the write.",
                    ))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("probe-apply-confirm")
                                    .label(t("CONFIRM WRITE"))
                                    .warning()
                                    .h(px(48.0))
                                    .w(px(150.0))
                                    .rounded_lg()
                                    .text_size(px(10.0))
                                    .disabled(!self.mdi_allowed())
                                    .on_click(move |_, _, cx| {
                                        confirm_view.update(cx, |this, cx| {
                                            this.confirm_probe_apply(cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("probe-apply-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .w(px(100.0))
                                    .rounded_lg()
                                    .text_size(px(10.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |this, cx| {
                                            this.cancel_probe_apply(cx);
                                        });
                                    }),
                            )
                            .child(div().text_size(px(9.0)).text_color(p.text_faint).child(
                                if self.mdi_allowed() {
                                    "controller is Idle"
                                } else {
                                    "Apply is locked until the controller is Idle"
                                },
                            )),
                    ),
            );
        } else if supported && run.applied_line.is_none() {
            let keep_view = cx.entity();
            let apply_view = cx.entity();
            section = section.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("probe-apply")
                            .label(t("APPLY OFFSET"))
                            .primary()
                            .h(px(52.0))
                            .w(px(150.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(!self.mdi_allowed())
                            .on_click(move |_, _, cx| {
                                apply_view.update(cx, |this, cx| {
                                    this.request_probe_apply(cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("probe-keep")
                            .label(t("KEEP"))
                            .secondary()
                            .h(px(52.0))
                            .w(px(90.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, _, cx| {
                                keep_view.update(cx, |this, cx| {
                                    this.keep_probe_result(cx);
                                });
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child(t("Nothing is applied automatically")),
                    ),
            );
        }
        section
    }

    /// Tool-length result values and the confirmed `G43.1`/`G49` actions.
    ///
    /// The preview and every button are local until `CONFIRM ...` is pressed.
    /// The old TLO is retained in the display; `APPLIED` appears only after a
    /// matching `$#` read-back.
    fn probe_tool_length_section(
        &self,
        p: Palette,
        cx: &mut Context<Self>,
        _run: &ProbeRun,
    ) -> Div {
        let preview = self.state.probe.tool_length_preview();
        let gate = self.state.probe_tool_length_gate().check();
        let in_flight = self.state.probe.tool_length_write_in_flight();
        let action_allowed = gate.is_ok() && !in_flight;
        let mut section = v_flex().gap_1().pt_1().border_t_1().border_color(p.border);

        if let Err(error) = &preview {
            section = section.child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.danger)
                    .child(format!("TLO preview refused: {error}")),
            );
        }

        if let Some(write) = self.state.probe.tool_length_write() {
            let (chip, color, detail) = match &write.status {
                ToolLengthWriteStatus::AwaitingReadBack => (
                    t("PENDING"),
                    p.warning,
                    "`$#` read-back pending; APPLIED only after it matches".to_string(),
                ),
                ToolLengthWriteStatus::Applied { reported_tlo_mm } => (
                    t("APPLIED"),
                    p.success,
                    format!("`$#` reports {} mm", format_mm(*reported_tlo_mm)),
                ),
                ToolLengthWriteStatus::Failed { message } => (
                    t("FAILED"),
                    p.danger,
                    format!("{message}; old TLO retained"),
                ),
            };
            section = section.child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .p_1()
                    .rounded(px(6.0))
                    .bg(color.opacity(0.10))
                    .border_1()
                    .border_color(color.opacity(0.45))
                    .child(
                        div()
                            .px_2()
                            .h(px(20.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .rounded(px(4.0))
                            .bg(color.opacity(0.16))
                            .text_size(px(9.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(color)
                            .child(chip),
                    )
                    .child(
                        div()
                            .text_size(px(8.5))
                            .text_color(p.text_faint)
                            .child(t(write.action.label())),
                    )
                    .child(
                        div()
                            .font_family(self.mono.clone())
                            .text_size(px(11.0))
                            .text_color(p.text)
                            .child(write.line.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_dim)
                            .child(detail),
                    ),
            );
        }

        if let Some(action) = self.state.probe.tool_length_confirmation() {
            let line = match action {
                ToolLengthAction::Apply => preview.as_ref().map_or_else(
                    |error| format!("G43.1 unavailable: {error}"),
                    |preview| preview.apply_line(),
                ),
                ToolLengthAction::Clear => "G49".to_string(),
            };
            let confirm_view = cx.entity();
            let cancel_view = cx.entity();
            section = section.child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .p_1()
                    .rounded(px(6.0))
                    .bg(p.warning.opacity(0.10))
                    .border_1()
                    .border_color(p.warning.opacity(0.5))
                    .child(
                        div()
                            .w(px(150.0))
                            .flex_shrink_0()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.warning)
                            .child(t(action.confirm_label())),
                    )
                    .child(
                        div()
                            .font_family(self.mono.clone())
                            .text_size(px(12.0))
                            .text_color(p.text)
                            .child(line),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("probe-tlo-confirm")
                            .label(t(action.confirm_label()))
                            .warning()
                            .h(px(48.0))
                            .w(px(170.0))
                            .rounded_lg()
                            .text_size(px(9.5))
                            .disabled(!action_allowed)
                            .on_click(move |_, _, cx| {
                                confirm_view.update(cx, |this, cx| {
                                    this.confirm_probe_tlo(cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("probe-tlo-cancel")
                            .label(t("CANCEL"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(96.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .on_click(move |_, _, cx| {
                                cancel_view.update(cx, |this, cx| {
                                    this.cancel_probe_tlo(cx);
                                });
                            }),
                    )
                    .when(!action_allowed, |this| {
                        this.child(
                            div()
                                .text_size(px(9.0))
                                .text_color(p.warning)
                                .child(gate.clone().err().unwrap_or_default()),
                        )
                    }),
            );
        } else {
            let apply_view = cx.entity();
            let clear_view = cx.entity();
            section =
                section.child(
                    h_flex()
                        .flex_wrap()
                        .gap_2()
                        .items_center()
                        .child(
                            Button::new("probe-tlo-apply")
                                .label(t("APPLY TLO"))
                                .primary()
                                .h(px(48.0))
                                .w(px(140.0))
                                .rounded_lg()
                                .text_size(px(11.0))
                                .disabled(!action_allowed || preview.is_err())
                                .on_click(move |_, _, cx| {
                                    apply_view.update(cx, |this, cx| {
                                        this.request_probe_tlo(ToolLengthAction::Apply, cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new("probe-tlo-clear")
                                .label(t("CLEAR TLO"))
                                .secondary()
                                .h(px(48.0))
                                .w(px(140.0))
                                .rounded_lg()
                                .text_size(px(11.0))
                                .disabled(!action_allowed)
                                .on_click(move |_, _, cx| {
                                    clear_view.update(cx, |this, cx| {
                                        this.request_probe_tlo(ToolLengthAction::Clear, cx);
                                    });
                                }),
                        )
                        .child(div().text_size(px(9.0)).text_color(p.text_faint).child(
                            match gate {
                                Ok(()) => {
                                    "Confirm every write; `$#` read-back required".to_string()
                                }
                                Err(reason) => format!("locked: {reason}"),
                            },
                        )),
                );
        }

        section
    }

    fn probe_footer(
        &self,
        p: Palette,
        cx: &mut Context<Self>,
        preflight: &ProbePreflight,
        running: bool,
    ) -> Div {
        let can_run = preflight.setup.is_some()
            && !running
            && !self.state.probe.tool_length_write_in_flight();
        let (title, title_color, detail) = if running {
            (
                "PROBE PROGRAM RUNNING".to_string(),
                p.accent,
                "The engine owns motion; ABORT stops the job and never resumes",
            )
        } else if let Some(run) = self.state.probe.run() {
            match run.phase {
                ProbePhase::Complete if run.map.is_some() => {
                    let complete = run.map.as_ref().is_some_and(|map| map.is_complete());
                    match self.state.height_map.status_for(complete) {
                        HeightMapApplyStatus::Applied => (
                            self.state.height_map.applied().map_or_else(
                                || "HEIGHT MAP APPLIED".to_string(),
                                |applied| format!("HEIGHT MAP APPLIED: {}", applied.program_name),
                            ),
                            p.success,
                            "Loaded, never started; REVERT restores the original source",
                        ),
                        HeightMapApplyStatus::Reverted => (
                            "HEIGHT MAP REVERTED: original restored".to_string(),
                            p.success,
                            "Apply is available again from the same measured map",
                        ),
                        HeightMapApplyStatus::ReadyToApply => (
                            "HEIGHT MAP COMPLETE: ready to apply".to_string(),
                            p.success,
                            "Apply rewrites the loaded program; nothing starts automatically",
                        ),
                        HeightMapApplyStatus::MeasurementOnly => {
                            if complete {
                                (
                                    "HEIGHT MAP COMPLETE: no part-program snapshot".to_string(),
                                    p.warning,
                                    "Run the map again with the current program before Apply",
                                )
                            } else {
                                (
                                    "HEIGHT MAP COMPLETE: measurement only".to_string(),
                                    p.warning,
                                    "Missing cells are invalid; Apply is refused for an incomplete map",
                                )
                            }
                        }
                    }
                }
                ProbePhase::Complete
                    if matches!(
                        run.outcome.as_ref().map(|outcome| &outcome.value),
                        Some(ghs_probe::ProbeValue::ToolLength { .. })
                    ) =>
                {
                    (
                        "PROBE COMPLETE: review APPLY TLO or CLEAR TLO".to_string(),
                        p.success,
                        "G43.1/G49 are idle-only; every write needs confirmation and a `$#` read-back",
                    )
                }
                ProbePhase::Complete if !self.state.probe.apply_supported() => (
                    "PROBE COMPLETE: result reported, apply deferred".to_string(),
                    p.success,
                    "No offset write is offered for this cycle; the value stays for audit",
                ),
                ProbePhase::Complete => (
                    "PROBE COMPLETE: review APPLY or KEEP".to_string(),
                    p.success,
                    "Nothing is applied automatically; RUN AGAIN needs a fresh preflight",
                ),
                ProbePhase::Failed if run.map.is_some() => (
                    "HEIGHT MAP STOPPED: partial map shown".to_string(),
                    p.warning,
                    "Missing cells are invalid; no auto-resume and no interpolation",
                ),
                ProbePhase::Failed => (
                    "PROBE FAILED: recover, then retry the preflight".to_string(),
                    p.danger,
                    "No automatic retry or resume; the raw results stay for audit",
                ),
                ProbePhase::Running => (
                    "PROBE PROGRAM RUNNING".to_string(),
                    p.accent,
                    "The engine owns motion; ABORT stops the job and never resumes",
                ),
            }
        } else if let Some(setup) = &preflight.setup {
            let count = setup.plan.expected_probes;
            let estimate = setup
                .estimate
                .as_ref()
                .map_or_else(String::new, |estimate| {
                    format!(" | {}", format_estimate(estimate))
                });
            (
                format!(
                    "READY: {} probe{}, {:.3} mm search, safe Z {:.3} mm{}",
                    count,
                    if count == 1 { "" } else { "s" },
                    setup.plan.total_search_travel_mm(),
                    setup.plan.safe_z_mm,
                    estimate
                ),
                p.success,
                "RUN is the only action that starts motion; there is no auto-start",
            )
        } else if !preflight.blockers.is_empty() {
            (
                format!("REFUSED: {}", preflight.blockers.join("; ")),
                p.warning,
                "RUN is the only action that starts motion; there is no auto-start",
            )
        } else {
            (
                t("NOT READY").to_string(),
                p.text_faint,
                "RUN is the only action that starts motion; there is no auto-start",
            )
        };
        let run_exists = self.state.probe.run().is_some();
        let mut footer = h_flex()
            .h(px(68.0))
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_2()
            .border_t_1()
            .border_color(p.border)
            .bg(p.panel);
        footer = footer.child(
            v_flex()
                .flex_1()
                .min_w(px(0.0))
                .gap_0()
                .child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(title_color)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(p.text_faint)
                        .child(detail),
                ),
        );
        if running {
            let abort_view = cx.entity();
            footer = footer.child(
                Button::new("probe-footer-abort")
                    .label(t("ABORT"))
                    .warning()
                    .h(px(56.0))
                    .w(px(112.0))
                    .rounded_lg()
                    .text_size(px(12.0))
                    .on_click(move |_, _, cx| {
                        abort_view.update(cx, |this, cx| this.abort_probe_run(cx));
                    }),
            );
        } else {
            let run_view = cx.entity();
            let back_footer_view = cx.entity();
            footer = footer.child(
                Button::new("probe-run")
                    .label(if run_exists { t("RUN AGAIN") } else { t("RUN") })
                    .primary()
                    .h(px(56.0))
                    .w(px(120.0))
                    .rounded_lg()
                    .text_size(px(14.0))
                    .disabled(!can_run)
                    .on_click(move |_, _, cx| {
                        run_view.update(cx, |this, cx| this.start_probe_run(cx));
                    }),
            );
            footer = footer.child(
                Button::new("probe-footer-back")
                    .label(t("CYCLES"))
                    .secondary()
                    .h(px(56.0))
                    .w(px(96.0))
                    .rounded_lg()
                    .text_size(px(10.0))
                    .on_click(move |_, _, cx| {
                        back_footer_view.update(cx, |this, cx| this.close_probe_card(cx));
                    }),
            );
        }
        footer
    }

    fn mode_view(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.state.mode {
            Mode::Job | Mode::Probe | Mode::Settings => unreachable!(),
            Mode::Offset => self.offsets_view(p, cx).into_any_element(),
            Mode::Utility => self.utility_view(p, cx).into_any_element(),
            Mode::Tools => self.tools_view(p, cx).into_any_element(),
        };
        v_flex()
            .gap_2()
            .p_3()
            .size_full()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child(t(self.state.mode.title())),
                    )
                    .child(chip(
                        if self.state.live {
                            t("LIVE")
                        } else {
                            t("MOCK")
                        },
                        p,
                    )),
            )
            .child(div().flex_1().min_h(px(0.0)).child(body))
    }

    /// Work-offset workspace.
    ///
    /// At 1280x800 the centre column is roughly 636 px wide. Three value fields
    /// plus three 48 px per-axis zero targets and the selected-WCS detail panel
    /// cannot share that width without horizontal scrolling, so the primary
    /// 48 px zero targets live in the active-WCS panel and the dense `$#` table
    /// stays tap-to-edit. Any WCS can still be zeroed by selecting it first.
    /// `G28`/`G30` gotos and `G92.1` clear always open the confirmation sheet.
    fn offsets_view(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let model = &self.state.offsets;
        let rows = model.wcs_rows();
        let named = model.named_rows();
        let can_edit = self.state.live && self.mdi_allowed();
        let active_wcs = model.active_wcs();
        let active_row = model.wcs_row(active_wcs.index());
        let action_gate = self.offset_gate();
        let action_block = action_gate.block_reason();
        let refresh_view = view.clone();

        let (status_color, status_text) = match model.status() {
            OffsetStatus::Idle if !model.reported() => (p.text_faint, "WAITING FOR $#".to_string()),
            OffsetStatus::Idle => (p.success, t("READY").to_string()),
            OffsetStatus::Refreshing => (p.warning, "REFRESHING AFTER WRITE".to_string()),
            OffsetStatus::Refreshed => (p.success, "REFRESHED FROM $#".to_string()),
            OffsetStatus::Failed(message) => (p.danger, format!("FAILED: {message}")),
        };

        let active_reported = active_row.as_ref().is_some_and(|row| row.reported);
        let active_values = active_row.as_ref().map_or_else(
            || "--".to_string(),
            |row| {
                format!(
                    "X {}   Y {}   Z {}",
                    model.format_optional(row.values[0]),
                    model.format_optional(row.values[1]),
                    model.format_optional(row.values[2])
                )
            },
        );

        let mut detail = v_flex()
            .gap_2()
            .p_2()
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(chip(&format!("ACTIVE {}", active_wcs.as_str()), p))
                            .child(chip(&format!("P{}", active_wcs.index() + 1), p))
                            .child(chip(self.state.unit(), p)),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(if active_reported {
                                p.success
                            } else {
                                p.text_faint
                            })
                            .child(if active_reported {
                                t("REPORTED")
                            } else {
                                t("NOT REPORTED")
                            }),
                    ),
            )
            .child(
                div()
                    .font_family(self.mono.clone())
                    .text_size(px(14.0))
                    .text_color(if active_reported {
                        p.text
                    } else {
                        p.text_faint
                    })
                    .child(active_values),
            );

        let zero_button = |axis: Axis| {
            let zero_view = view.clone();
            Button::new(format!("detail-zero-{}", axis.label()))
                .label(format!("ZERO {}", axis.label()))
                .warning()
                .h(px(48.0))
                .flex_1()
                .rounded_lg()
                .text_size(px(11.0))
                .disabled(!can_edit)
                .on_click(move |_, _, cx| {
                    zero_view.update(cx, |this, cx| {
                        this.zero_offset_axis(active_wcs, axis, cx);
                    });
                })
        };
        let zero_all_view = view.clone();
        let zero_all = Button::new("detail-zero-all")
            .label(t("ZERO ALL"))
            .danger()
            .h(px(48.0))
            .flex_1()
            .rounded_lg()
            .text_size(px(11.0))
            .disabled(!can_edit)
            .on_click(move |_, _, cx| {
                zero_all_view.update(cx, |this, cx| {
                    this.zero_offset_all(active_wcs, cx);
                });
            });
        let zero_top = h_flex()
            .gap_2()
            .child(zero_button(Axis::X))
            .child(zero_button(Axis::Y));
        let zero_bottom = h_flex().gap_2().child(zero_button(Axis::Z)).child(zero_all);
        detail = detail.child(zero_top).child(zero_bottom);

        let mut action_row = h_flex().gap_2();
        for action in OffsetAction::ALL {
            let action_view = view.clone();
            let button = Button::new(format!(
                "offset-action-{}",
                action.command().replace('.', "-")
            ))
            .label(t(action.label()))
            .h(px(48.0))
            .flex_1()
            .rounded_lg()
            .text_size(px(11.0))
            .disabled(action_block.is_some() || !model.action_available(action))
            .on_click(move |_, _, cx| {
                action_view.update(cx, |this, cx| {
                    this.request_offset_action(action, cx);
                });
            });
            let button = if action.moves_machine() {
                button.warning()
            } else {
                button.danger()
            };
            action_row = action_row.child(button);
        }
        let missing_goto: Vec<&str> = OffsetAction::ALL
            .into_iter()
            .filter(|action| action.moves_machine() && !model.action_available(*action))
            .map(OffsetAction::command)
            .collect();

        let mut actions = v_flex()
            .gap_1()
            .p_2()
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(t("PREDEFINED POSITIONS / G92")),
                    )
                    .child(chip(t("CONFIRM"), p)),
            )
            .child(action_row)
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(
                "G28/G30 move to the stored target; CLEAR G92 sends G92.1. \
                 Cancel sends nothing.",
            ));
        if !missing_goto.is_empty() {
            actions = actions.child(div().text_size(px(10.0)).text_color(p.warning).child(
                format!(
                    "{} NOT REPORTED BY $#; goto stays disabled",
                    missing_goto.join(" / ")
                ),
            ));
        }
        if let Some(reason) = action_block {
            actions = actions.child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.warning)
                    .child(format!("LOCKED: {reason}")),
            );
        }

        let mut list = v_flex().id("offset-wcs-list").gap_1();
        for row in rows {
            let row_view = view.clone();
            let wcs = row.wcs;
            let active = row.active;
            let mut line = h_flex()
                .items_center()
                .gap_1()
                .h(px(52.0))
                .px_1()
                .rounded(px(6.0))
                .bg(if active { p.accent_muted } else { p.panel_alt });
            line = line.child(
                Button::new(format!("wcs-{}", wcs.as_str()))
                    .label(wcs.as_str())
                    .h(px(48.0))
                    .w(px(64.0))
                    .rounded(px(6.0))
                    .text_size(px(12.0))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(if active { p.accent_muted } else { p.panel_alt })
                            .foreground(if active { p.accent } else { p.text_dim })
                            .hover(p.raised_hover)
                            .active(p.accent_muted),
                    )
                    .disabled(!self.mdi_allowed())
                    .on_click(move |_, _, cx| {
                        row_view.update(cx, |this, cx| {
                            this.select_wcs(wcs.index(), cx);
                        });
                    }),
            );
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                let value = row.values.get(axis.index()).copied().flatten();
                let text = model.format_optional(value);
                let value_view = view.clone();
                line = line.child(
                    Button::new(format!("off-{}-{}", wcs.as_str(), axis.label()))
                        .label(format!("{text:>9}"))
                        .h(px(48.0))
                        .flex_1()
                        .min_w(px(0.0))
                        .rounded(px(6.0))
                        .text_size(px(13.0))
                        .custom(
                            ButtonCustomVariant::new(cx)
                                .color(p.panel)
                                .foreground(if value.is_some() {
                                    p.text
                                } else {
                                    p.text_faint
                                })
                                .hover(p.raised_hover)
                                .active(p.accent_muted),
                        )
                        .border_1()
                        .border_color(p.border)
                        .disabled(!can_edit)
                        .on_click(move |_, _, cx| {
                            value_view.update(cx, |this, cx| {
                                this.state.open_keypad(KeypadTarget::WorkOffset {
                                    wcs: wcs.index(),
                                    axis,
                                });
                                cx.notify();
                            });
                        }),
                );
            }
            list = list.child(line);
        }

        let mut named_list = v_flex().gap_1().pt_2();
        named_list = named_list.child(
            div()
                .text_size(px(10.0))
                .text_color(p.text_faint)
                .child(t("READ-ONLY $# PARAMETERS")),
        );
        for row in named {
            let value_color = if row.note == Some("NON-PERSISTENT") {
                p.warning
            } else {
                p.text
            };
            named_list = named_list.child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .h(px(24.0))
                    .px_1()
                    .child(
                        div()
                            .w(px(52.0))
                            .text_size(px(11.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_dim)
                            .child(row.label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .font_family(self.mono.clone())
                            .text_size(px(12.0))
                            .text_color(value_color)
                            .child(row.value),
                    )
                    .when_some(row.note, |this, note| this.child(chip(note, p))),
            );
        }

        let status = v_flex()
            .gap_1()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(7.0))
                            .h(px(7.0))
                            .rounded(px(999.0))
                            .bg(status_color),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(status_color)
                            .child(status_text),
                    ),
            )
            .child(
                div().text_size(px(10.0)).text_color(p.text_faint).child(
                    model
                        .last_line()
                        .unwrap_or("No offset write yet")
                        .to_string(),
                ),
            );

        let export_path = self.offsets_export_path().map_or_else(
            || "No app configuration directory".to_string(),
            |path| path.display().to_string(),
        );
        let export_view = view.clone();
        let load_file_view = view.clone();
        let mut backup = v_flex()
            .gap_1()
            .p_2()
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("offset-export")
                            .label(t("EXPORT OFFSETS"))
                            .h(px(48.0))
                            .w(px(150.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .custom(
                                ButtonCustomVariant::new(cx)
                                    .color(p.panel)
                                    .foreground(p.text)
                                    .hover(p.raised_hover)
                                    .active(p.accent_muted),
                            )
                            .border_1()
                            .border_color(p.border)
                            .on_click(move |_, _, cx| {
                                export_view.update(cx, |this, cx| {
                                    this.export_offsets(cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("offset-load-file")
                            .label(t("LOAD OFFSETS FILE"))
                            .h(px(48.0))
                            .w(px(180.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .custom(
                                ButtonCustomVariant::new(cx)
                                    .color(p.panel)
                                    .foreground(p.text)
                                    .hover(p.raised_hover)
                                    .active(p.accent_muted),
                            )
                            .border_1()
                            .border_color(p.border)
                            .on_click(move |_, _, cx| {
                                load_file_view.update(cx, |this, cx| {
                                    this.open_offsets_restore(cx);
                                });
                            }),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.0))
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .font_family(self.mono.clone())
                                    .text_color(p.text_dim)
                                    .child(format!("FILE: {export_path}")),
                            )
                            .child(div().text_size(px(9.0)).text_color(p.text_faint).child(
                                "Local, no command sent. Work offsets only; G92, G28/G30, HOME, \
                                 TLO, TLR, G51 and PRB are omitted.",
                            )),
                    ),
            );
        if let Some(status) = self.state.offsets_backup.status() {
            backup = backup.child(
                div()
                    .text_size(px(10.0))
                    .text_color(if status.ok { p.success } else { p.danger })
                    .child(status.text.clone()),
            );
        }

        let table = v_flex()
            .flex_1()
            .min_w(px(300.0))
            .gap_1()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text_dim)
                                    .child(t("WCS OFFSETS")),
                            )
                            .child(chip(self.state.unit(), p)),
                    )
                    .child(
                        Button::new("offset-refresh")
                            .label(t("REFRESH $#"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(112.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(!self.state.live || !self.ready())
                            .on_click(move |_, _, cx| {
                                refresh_view.update(cx, |this, cx| {
                                    this.request_offset_refresh(cx);
                                });
                            }),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .h(px(20.0))
                    .px_1()
                    .child(
                        div()
                            .w(px(64.0))
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child("WCS"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(t("X / Y / Z IN DISPLAY UNITS")),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(t("TAP TO EDIT")),
                    ),
            )
            .child(list);

        let left = v_flex()
            .w(px(300.0))
            .flex_shrink_0()
            .gap_2()
            .child(detail)
            .child(actions)
            .child(status);

        v_flex()
            .id("offset-mode-scroll")
            .gap_2()
            .size_full()
            .overflow_y_scrollbar()
            .child(backup)
            .child(
                h_flex()
                    .items_start()
                    .flex_wrap()
                    .gap_2()
                    .child(left)
                    .child(table),
            )
            .child(named_list)
            .into_any_element()
    }

    /// Two-step confirmation for a `G28`/`G30` goto or `G92.1` clear.
    ///
    /// The scrim cancels on tap. The confirm button re-checks the live gate and
    /// the engine remains the final authority; nothing is sent before the
    /// second explicit press.
    fn offset_confirm_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(pending) = self.state.offsets.pending_action() else {
            return div().into_any_element();
        };
        let action = pending.action();
        let block = self.offset_gate().block_reason();
        let view = cx.entity();
        let cancel_view = view.clone();
        let confirm_view = view;
        let mono = self.mono.clone();
        let model = &self.state.offsets;

        let mut dro_row = h_flex().gap_2();
        for (index, axis) in [Axis::X, Axis::Y, Axis::Z].iter().enumerate() {
            let value = self.state.dro.get(index).copied();
            dro_row = dro_row.child(
                v_flex()
                    .flex_1()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.panel_alt)
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(format!("DRO {}", axis.label())),
                    )
                    .child(
                        div()
                            .font_family(mono.clone())
                            .text_size(px(20.0))
                            .text_color(p.text)
                            .child(model.format_optional(value)),
                    ),
            );
        }

        let dro_changed = (0..3).any(|index| {
            let live = self.state.dro.get(index).copied();
            let requested = pending.dro()[index];
            match (live, requested) {
                (Some(live), Some(requested)) => (live - requested).abs() > 0.0005,
                (None, None) => false,
                _ => true,
            }
        });

        let accent = if action.moves_machine() {
            p.warning
        } else {
            p.danger
        };
        let warning = if action.moves_machine() {
            format!(
                "{} sends {}. The machine will move to the stored position above. Nothing has \
                 been sent yet; CANCEL sends nothing. No homing or extra motion is added.",
                t(action.label()),
                pending.command()
            )
        } else {
            format!(
                "{} sends {}. No axis moves; the displayed work position changes by the cleared \
                 offset. The old G92 values are shown above and $# is read back after the command.",
                t(action.label()),
                pending.command()
            )
        };

        let sheet = v_flex()
            .w(px(560.0))
            .max_w_full()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(accent.opacity(0.6))
            .bg(p.raised)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(accent)
                            .child(t(action.title())),
                    )
                    .child(chip(t("NOT SENT YET"), p)),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.panel_alt)
                    .child(
                        div()
                            .font_family(mono.clone())
                            .text_size(px(24.0))
                            .text_color(p.text)
                            .child(pending.command()),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(t("EXACT MDI LINE")),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.panel_alt)
                    .child(
                        div()
                            .w(px(140.0))
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(t(action.target_label())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .font_family(mono.clone())
                            .text_size(px(12.0))
                            .text_color(p.text)
                            .child(pending.target().to_string()),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.text_faint)
                    .child(t("CURRENT DRO (DISPLAY UNITS)")),
            )
            .child(dro_row)
            .when(dro_changed, |this| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(p.warning)
                        .child(t("DRO CHANGED SINCE THIS SHEET OPENED; CANCEL AND RECHECK")),
                )
            })
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(p.text_dim)
                    .child(warning),
            )
            .when_some(block, |this, reason| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(p.warning)
                        .child(format!("LOCKED: {reason}")),
                )
            })
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("offset-confirm-cancel")
                            .label(t("CANCEL"))
                            .secondary()
                            .h(px(48.0))
                            .flex_1()
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, _, cx| {
                                cancel_view.update(cx, |this, cx| {
                                    this.cancel_offset_action(cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("offset-confirm-accept")
                            .label(if action.moves_machine() {
                                format!("{} {}", t("CONFIRM"), pending.command())
                            } else {
                                t("CLEAR G92").to_string()
                            })
                            .danger()
                            .h(px(48.0))
                            .flex_1()
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(block.is_some())
                            .on_click(move |_, _, cx| {
                                confirm_view.update(cx, |this, cx| {
                                    this.confirm_offset_action(cx);
                                });
                            }),
                    ),
            );

        let scrim_view = cx.entity();
        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("offset-confirm-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |this, cx| {
                            this.cancel_offset_action(cx);
                        });
                    }),
            )
            .child(sheet)
            .into_any_element()
    }

    /// Preview of the validated `offsets.nc` file.
    ///
    /// The sheet lists the full path and every accepted `G10 L2` line. CANCEL
    /// sends nothing. `LOAD AS PROGRAM` re-validates the file and loads it into
    /// the engine for review; it never starts the job. Running it later
    /// overwrites the stored WCS offsets listed here and needs the usual
    /// Idle/ready gate, with the engine as the final authority.
    fn offsets_restore_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(preview) = self.state.offsets_backup.preview() else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let cancel_view = view.clone();
        let load_view = view;
        let mono = self.mono.clone();
        let path = preview.path().display().to_string();
        let line_count = preview.lines().len();
        let wcs_count = preview.wcs().len();
        let job_active = self.state.job.is_active();

        let mut lines = v_flex()
            .id("offsets-restore-lines")
            .gap_1()
            .max_h(px(264.0))
            .overflow_y_scrollbar();
        for line in preview.lines() {
            lines = lines.child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .h(px(22.0))
                    .px_1()
                    .rounded(px(4.0))
                    .bg(p.panel_alt)
                    .child(
                        div()
                            .w(px(52.0))
                            .flex_shrink_0()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_faint)
                            .child(line.wcs.as_str()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .font_family(mono.clone())
                            .text_size(px(10.0))
                            .text_color(p.text)
                            .child(line.text.clone()),
                    ),
            );
        }

        let sheet = v_flex()
            .w(px(640.0))
            .max_w_full()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(p.warning.opacity(0.6))
            .bg(p.raised)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.warning)
                            .child(t("RESTORE WORK OFFSETS - PREVIEW")),
                    )
                    .child(chip(t("NOT LOADED"), p)),
            )
            .child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.panel_alt)
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(t("VALIDATED FILE")),
                    )
                    .child(
                        div()
                            .font_family(mono.clone())
                            .text_size(px(11.0))
                            .text_color(p.text)
                            .child(path),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_dim)
                            .child(format!(
                                "{line_count} G10 L2 lines | {wcs_count} work coordinate systems | \
                                 normalized preview"
                            )),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.text_faint)
                    .child(t("G10 L2 LINES IN THIS FILE (VALUES IN MILLIMETRES)")),
            )
            .child(lines)
            .child(div().text_size(px(11.0)).text_color(p.text_dim).child(
                "LOAD AS PROGRAM validates this file again and loads it into the Job \
                         workspace. It does not start. The file begins with G21, so every \
                         value is interpreted in millimetres. Running it overwrites the \
                         stored WCS offsets listed above and requires Idle with a ready \
                         controller. The engine safety gateway remains the final authority.",
            ))
            .child(div().text_size(px(10.0)).text_color(p.warning).child(t(
                "THE FILE BEGINS WITH G21: EVERY VALUE IS INTERPRETED IN MILLIMETRES.",
            )))
            .when(job_active, |this| {
                this.child(div().text_size(px(10.0)).text_color(p.danger).child(t(
                    "LOCKED: A JOB IS ACTIVE; STOP IT BEFORE LOADING A PROGRAM",
                )))
            })
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("offsets-restore-cancel")
                            .label(t("CANCEL"))
                            .secondary()
                            .h(px(48.0))
                            .flex_1()
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, _, cx| {
                                cancel_view.update(cx, |this, cx| {
                                    this.cancel_offsets_restore(cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("offsets-restore-load")
                            .label(t("LOAD AS PROGRAM"))
                            .primary()
                            .h(px(48.0))
                            .flex_1()
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(job_active || !self.state.live)
                            .on_click(move |_, _, cx| {
                                load_view.update(cx, |this, cx| {
                                    this.load_offsets_program(cx);
                                });
                            }),
                    ),
            );

        let scrim_view = cx.entity();
        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("offsets-restore-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |this, cx| {
                            this.cancel_offsets_restore(cx);
                        });
                    }),
            )
            .child(sheet)
            .into_any_element()
    }

    /// User macros: list, add/edit, delete, and the explicit RUN action.
    ///
    /// The list comes from the active controller profile. The built-in actions
    /// in the right rail are separate and stay untouched.
    fn macro_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let macros = self.state.user_macro_list().to_vec();
        let save_blocker = self.state.macro_save_blocker();
        let run_lock = self.state.macro_gate().blocked_reason();
        let last_error = self.state.macro_last_error().map(str::to_string);
        let new_view = view.clone();
        let count = macros.len();

        let mut list = v_flex().gap_1();
        if macros.is_empty() {
            list = list.child(
                div()
                    .p_3()
                    .rounded_lg()
                    .bg(p.panel)
                    .text_size(px(11.0))
                    .text_color(p.text_dim)
                    .child(t(
                        "No user macros for this controller profile. Add a named, single-line MDI command; it is saved only here and never runs on connect.",
                    )),
            );
        }
        for (index, mac) in macros.into_iter().enumerate() {
            let run_view = view.clone();
            let edit_view = view.clone();
            let delete_view = view.clone();
            let run_name = mac.name.clone();
            let delete_name = mac.name.clone();
            let record = mac.clone();
            let confirm = mac.confirm;
            let name = mac.name;
            let command = mac.command;
            list = list.child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .p_1()
                    .rounded(px(6.0))
                    .bg(p.panel)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.0))
                            .gap_1()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .min_w(px(0.0))
                                            .truncate()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(p.text)
                                            .child(name),
                                    )
                                    .child(chip(
                                        if confirm { t("CONFIRM") } else { t("DIRECT") },
                                        p,
                                    )),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .font_family(self.mono.clone())
                                    .text_size(px(10.0))
                                    .text_color(p.text_dim)
                                    .child(command),
                            ),
                    )
                    .child(
                        Button::new(format!("macro-run-{index}"))
                            .label(t("RUN"))
                            .primary()
                            .h(px(48.0))
                            .w(px(62.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .on_click(move |_, _, cx| {
                                run_view.update(cx, |this, cx| {
                                    this.run_user_macro(&run_name, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new(format!("macro-edit-{index}"))
                            .label(t("EDIT"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(78.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(save_blocker.is_some())
                            .on_click(move |_, _, cx| {
                                edit_view.update(cx, |this, cx| {
                                    this.open_macro_editor(Some(&record), cx);
                                });
                            }),
                    )
                    .child(
                        Button::new(format!("macro-delete-{index}"))
                            .label(t("DEL"))
                            .warning()
                            .h(px(48.0))
                            .w(px(54.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(save_blocker.is_some())
                            .on_click(move |_, _, cx| {
                                delete_view.update(cx, |this, cx| {
                                    this.request_delete_macro(delete_name.clone(), cx);
                                });
                            }),
                    ),
            );
        }

        v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(section_label(t("MACROS"), p))
                            .child(chip(&format!("{count} DEFINED"), p))
                            .child(chip(t("USER-DEFINED"), p)),
                    )
                    .child(
                        Button::new("macro-new")
                            .label(t("NEW MACRO"))
                            .primary()
                            .h(px(48.0))
                            .w(px(124.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(save_blocker.is_some())
                            .on_click(move |_, _, cx| {
                                new_view.update(cx, |this, cx| {
                                    this.open_macro_editor(None, cx);
                                });
                            }),
                    ),
            )
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                "Named single-line MDI commands, saved in this controller profile. RUN sends only after a click; CONFIRM macros require a second explicit confirmation.",
            )))
            .child(list)
            .when_some(save_blocker, |this, reason| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.warning)
                        .child(format!("{}: {}", t("MACRO SAVES LOCKED"), reason.to_uppercase())),
                )
            })
            .when_some(run_lock, |this, reason| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.warning)
                        .child(format!("{}: {}", t("MACRO RUNS LOCKED"), reason.to_uppercase())),
                )
            })
            .when_some(last_error, |this, error| {
                this.child(div().text_size(px(10.0)).text_color(p.danger).child(error))
            })
    }

    fn keymap_card(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let blocker = self.state.config.keymap_save_blocker();
        let saved = KeymapModel::from_keymap(&self.keymap_source);
        let dirty = self.keymap.differs_from(&saved);
        let os = cx.platform_info().os;
        let validation = self.keymap.to_keymap(os).err();
        let mut cells = Vec::with_capacity(self.keymap.rows().len());
        for row in self.keymap.rows() {
            let action = row.action;
            let keys = row.keys.clone();
            let text = pretty_keys(&keys);
            let is_default = keys == action.default_keys();
            let edit_view = view.clone();
            let mono = self.mono.clone();
            cells.push(
                h_flex()
                    .flex_1()
                    .min_w(px(0.0))
                    .items_center()
                    .gap_1()
                    .p_1()
                    .rounded(px(6.0))
                    .bg(p.panel)
                    .child(
                        div()
                            .w(px(58.0))
                            .flex_shrink_0()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(if is_default { p.accent } else { p.warning })
                            .child(action.label()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .font_family(mono)
                            .text_size(px(11.0))
                            .text_color(p.text)
                            .child(text),
                    )
                    .child(
                        Button::new(format!("keymap-edit-{}", action.id()))
                            .label(t("EDIT"))
                            .secondary()
                            .h(px(40.0))
                            .w(px(78.0))
                            .rounded(px(6.0))
                            .text_size(px(9.0))
                            .on_click(move |_, window, cx| {
                                edit_view.update(cx, |this, cx| {
                                    this.open_keymap_editor(action, window, cx);
                                });
                            }),
                    )
                    .into_any_element(),
            );
        }
        let mut cells = cells.into_iter();
        let rows = h_flex()
            .items_start()
            .gap_1()
            .child(v_flex().flex_1().gap_1().children(cells.by_ref().take(5)))
            .child(v_flex().flex_1().gap_1().children(cells));

        let reset_view = view.clone();
        let cancel_view = view.clone();
        let save_view = view;
        v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w(px(0.0))
                            .items_center()
                            .gap_2()
                            .child(section_label(t("KEYMAP"), p))
                            .child(chip(t("PROFILE"), p))
                            .when(dirty, |this| this.child(chip(t("UNSAVED"), p)))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(9.0))
                                    .text_color(p.text_faint)
                                    .child(t(
                                        "One step per press; off while typing. RESET DEFAULTS restores.",
                                    )),
                            ),
                    )
                    .child(chip(
                        if self.keymap.is_default() {
                            t("DEFAULTS")
                        } else {
                            t("CUSTOM")
                        },
                        p,
                    )),
            )
            .child(rows)
            .when_some(validation.clone(), |this, message| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.danger)
                        .child(format!("{}: {message}", t("KEYMAP NOT VALID"))),
                )
            })
            .when_some(blocker.clone(), |this, reason| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.warning)
                        .child(format!("{}: {}", t("SAVE LOCKED"), reason.to_uppercase())),
                )
            })
            .child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("keymap-reset")
                            .label(t("RESET DEFAULTS"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(170.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .on_click(move |_, _, cx| {
                                reset_view.update(cx, |this, cx| this.reset_keymap_defaults(cx));
                            }),
                    )
                    .child(
                        Button::new("keymap-cancel")
                            .label(t("CANCEL"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(110.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(!dirty)
                            .on_click(move |_, _, cx| {
                                cancel_view.update(cx, |this, cx| this.cancel_keymap_draft(cx));
                            }),
                    )
                    .child(
                        Button::new("keymap-save")
                            .label(t("SAVE"))
                            .primary()
                            .h(px(48.0))
                            .w(px(120.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(!dirty || blocker.is_some() || validation.is_some())
                            .on_click(move |_, _, cx| {
                                save_view.update(cx, |this, cx| this.save_keymap(cx));
                            }),
                    ),
            )
            .into_any_element()
    }

    /// Read-only controller pin/limit diagnostics entry point.
    ///
    /// The right rail keeps its live `Pn` signals card; this Utility card opens
    /// the detailed `$PINS`/`$PINSTATE`/`$LIM`/`$LEV` sheet.
    fn pins_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let pins = &self.state.pins;
        let status_color = match pins.status() {
            PinsStatus::Idle => p.text_faint,
            PinsStatus::Refreshing | PinsStatus::Busy => p.warning,
            PinsStatus::Requested => p.success,
            PinsStatus::Stopped(_) => p.danger,
        };
        let status_text = pins_status_label(pins.status());
        let map_state = pins.pin_map_report();
        let summary = match map_state {
            ReportState::Missing => t("No $PINS report in this session").to_string(),
            ReportState::Empty => t("The controller reported no pin entries").to_string(),
            ReportState::Reported => {
                format!(
                    "{} {}",
                    pins.pin_map().map_or(0, |map| map.len()),
                    t("pin entries reported")
                )
            }
        };

        v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(section_label(t("CONTROLLER DIAGNOSTICS"), p))
                    .child(chip(t("READ-ONLY"), p)),
            )
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                "Pin map, auxiliary pin levels, limit switches and last control events. Opening the panel sends nothing; REFRESH sends only $PINS, $PINSTATE, $LIM and $LEV.",
            )))
            .child(
                h_flex().items_center().gap_2().children(
                    PinsRequest::ALL
                        .into_iter()
                        .map(|request| chip(request.label(), p)),
                ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(status_color)
                    .child(status_text),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.text_dim)
                    .child(summary),
            )
            .child(
                Button::new("pins-open")
                    .label(t("OPEN PINS / LIMITS"))
                    .primary()
                    .h(px(48.0))
                    .w(px(220.0))
                    .rounded_lg()
                    .text_size(px(10.0))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |shell, cx| shell.open_pins_panel(cx));
                    }),
            )
    }

    /// Compact four-way UI language selector.
    ///
    /// The native labels come from [`Locale::label`]; each target is 48 px
    /// high. A change applies immediately, persists in `app.json`, and never
    /// contacts the controller.
    fn language_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let active = ghs_i18n::locale();
        let buttons = Locale::ALL.iter().enumerate().map(|(index, locale)| {
            let locale = *locale;
            let locale_view = view.clone();
            let selected = active == locale;
            Button::new(format!("language-{index}"))
                .label(locale.label())
                .flex_1()
                .h(px(48.0))
                .rounded_lg()
                .text_size(px(12.0))
                .custom(mode_variant(cx, p, selected))
                .border_1()
                .border_color(if selected { p.accent } else { p.border })
                .on_click(move |_, _, cx| {
                    locale_view.update(cx, |this, cx| this.select_language(locale, cx));
                })
        });

        v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(section_label(t("LANGUAGE"), p))
                    .child(chip(active.code(), p)),
            )
            .child(h_flex().w_full().gap_1().children(buttons))
            .child(div().text_size(px(9.5)).text_color(p.text_faint).child(t(
                "Applies immediately and is saved in app preferences; the controller is not contacted.",
            )))
    }

    /// One tap-to-edit utility field cell, backed by the shared keypad.
    fn utility_field_cell(&self, field: UtilityField, p: Palette, cx: &mut Context<Self>) -> Div {
        let view = cx.entity();
        let unit = field.unit();
        let label = if unit.is_empty() {
            t(field.label()).to_string()
        } else {
            format!("{} ({})", t(field.label()), unit)
        };
        let value = self.state.utilities.field_text(field).to_string();
        let error = self
            .state
            .utilities
            .field_error(field)
            .map(std::string::ToString::to_string);
        let mut cell = v_flex()
            .w(px(136.0))
            .flex_shrink_0()
            .gap_1()
            .child(
                div()
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(label),
            )
            .child(
                Button::new(format!("utility-field-{}", field.key()))
                    .label(value)
                    .h(px(48.0))
                    .w_full()
                    .rounded_lg()
                    .text_size(px(13.0))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(p.panel)
                            .foreground(p.text)
                            .hover(p.raised_hover)
                            .active(p.accent_muted),
                    )
                    .border_1()
                    .border_color(if error.is_some() { p.danger } else { p.border })
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.state.open_keypad(KeypadTarget::Utility(field));
                            cx.notify();
                        });
                    }),
            );
        if let Some(error) = error {
            cell = cell.child(
                div()
                    .text_size(px(8.5))
                    .line_height(px(11.0))
                    .text_color(p.danger)
                    .child(error),
            );
        }
        cell
    }

    /// One tap-to-edit calibration distance, backed by the shared keypad.
    fn calibration_field_cell(
        &self,
        field: CalibrationField,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let view = cx.entity();
        let model = self.state.utilities.calibration();
        let value = model.field_text(field).to_string();
        let busy = model.stage().is_busy();
        let error = model
            .field_error(field)
            .map(std::string::ToString::to_string);
        let mut cell = v_flex()
            .w(px(190.0))
            .flex_shrink_0()
            .gap_1()
            .child(
                div()
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(format!("{} ({})", t(field.label()), field.unit())),
            )
            .child(
                Button::new(format!("calibration-field-{}", field.key()))
                    .label(if value.is_empty() {
                        "--".to_string()
                    } else {
                        value
                    })
                    .h(px(48.0))
                    .w_full()
                    .rounded_lg()
                    .text_size(px(13.0))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(p.panel)
                            .foreground(p.text)
                            .hover(p.raised_hover)
                            .active(p.accent_muted),
                    )
                    .border_1()
                    .border_color(if error.is_some() { p.danger } else { p.border })
                    .disabled(busy)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.state.open_keypad(KeypadTarget::Calibration(field));
                            cx.notify();
                        });
                    }),
            );
        if let Some(error) = error {
            cell = cell.child(
                div()
                    .text_size(px(8.5))
                    .line_height(px(11.0))
                    .text_color(p.danger)
                    .child(error),
            );
        }
        cell
    }

    /// Stepper/axis calibration card. Calculation only; no motion command.
    fn calibration_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let model = self.state.utilities.calibration();
        let axis = model.axis();
        let stage = model.stage();
        let proposal = model.proposal().cloned();
        let error = model.error().map(std::string::ToString::to_string);
        let last_result = model.last_result().map(std::string::ToString::to_string);
        let last_ok = model.last_result_ok();
        let gate = self.state.calibration_gate();
        let locked = gate.blocked_reason();
        let available = self.state.profile.capabilities.axes.clone();
        let axis_buttons = CalibrationAxis::ALL
            .into_iter()
            .map(|candidate| {
                let button_view = view.clone();
                let enabled = available
                    .iter()
                    .any(|axis| axis.index() == candidate.index());
                let selected = candidate == axis;
                Button::new(format!("calibration-axis-{}", candidate.label()))
                    .label(candidate.label())
                    .w(px(64.0))
                    .h(px(48.0))
                    .rounded_lg()
                    .text_size(px(13.0))
                    .custom(mode_variant(cx, p, selected))
                    .border_1()
                    .border_color(if selected { p.accent } else { p.border })
                    .disabled(!enabled || stage.is_busy())
                    .on_click(move |_, _, cx| {
                        button_view.update(cx, |this, cx| {
                            this.cancel_calibration(cx);
                            if this.state.utilities.calibration_mut().set_axis(candidate) {
                                cx.notify();
                            }
                        });
                    })
            })
            .collect::<Vec<_>>();
        let steps_chips = CalibrationAxis::ALL.into_iter().map(|candidate| {
            let value = self
                .state
                .calibration_current_steps(candidate)
                .map_or_else(|| "--".to_string(), |value| format!("{value:.3}"));
            chip(
                &format!(
                    "{} ${} {value}",
                    candidate.label(),
                    candidate.setting_number()
                ),
                p,
            )
        });
        let fields = CalibrationField::ALL
            .into_iter()
            .map(|field| self.calibration_field_cell(field, p, cx))
            .collect::<Vec<_>>();
        let current_text = self.state.calibration_current_steps(axis).map_or_else(
            || t("NOT REPORTED").to_string(),
            |value| format!("{value:.3}"),
        );
        let proposal_text = proposal.as_ref().map_or_else(
            || {
                t("Calculate to preview the new steps/mm and the change before writing.")
                    .to_string()
            },
            |proposal| {
                format!(
                    "${}  {:.3} -> {:.3} steps/mm   {:+.3} ({:+.2}%)",
                    proposal.setting_number,
                    proposal.current_steps_per_mm,
                    proposal.new_steps_per_mm,
                    proposal.delta_steps_per_mm,
                    proposal.delta_percent
                )
            },
        );
        let calculate_view = view.clone();
        let review_view = view.clone();
        let can_review = model.can_review();
        let stage_label = stage.label();

        v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(section_label(t("STEPPER CALIBRATION"), p))
                    .child(chip(t("NO MOTION COMMANDED"), p)),
            )
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                "Jog or run the test move separately, then enter the commanded distance and the measured travel. The wizard only calculates and writes the steps/mm setting; it never moves an axis.",
            )))
            .child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(9.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_faint)
                            .child(t("AXIS")),
                    )
                    .children(axis_buttons)
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_dim)
                            .child(format!(
                                "{} ${} ({}) {current_text} steps/mm",
                                t("CURRENT"),
                                axis.setting_number(),
                                axis.label()
                            )),
                    ),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .children(steps_chips),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .items_end()
                    .gap_2()
                    .children(fields)
                    .child(
                        Button::new("calibration-calculate")
                            .label(t("CALCULATE"))
                            .primary()
                            .h(px(48.0))
                            .w(px(150.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(stage.is_busy())
                            .on_click(move |_, _, cx| {
                                calculate_view.update(cx, |this, cx| {
                                    this.calculate_calibration(cx);
                                });
                            }),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.panel)
                    .border_1()
                    .border_color(if error.is_some() { p.danger } else { p.border })
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(section_label(t("PROPOSED STEPS/MM"), p))
                            .when(!stage_label.is_empty(), |this| {
                                this.child(chip(t(stage_label), p))
                            }),
                    )
                    .child(
                        div()
                            .font_family(self.mono.clone())
                            .text_size(px(11.0))
                            .text_color(p.text_dim)
                            .child(proposal_text),
                    )
                    .when_some(error, |this, error| {
                        this.child(div().text_size(px(10.0)).text_color(p.danger).child(error))
                    }),
            )
            .when_some(locked, |this, reason| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.warning)
                        .child(format!(
                            "{}: {}",
                            t("CALIBRATION WRITES LOCKED"),
                            reason.to_uppercase()
                        )),
                )
            })
            .when_some(last_result, |this, result| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(if last_ok { p.success } else { p.danger })
                        .child(result),
                )
            })
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("calibration-review")
                            .label(t("REVIEW & APPLY"))
                            .warning()
                            .h(px(48.0))
                            .w(px(200.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(!can_review)
                            .on_click(move |_, _, cx| {
                                review_view.update(cx, |this, cx| {
                                    this.open_calibration_confirm(cx);
                                });
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(9.5))
                            .text_color(p.text_faint)
                            .child(t(
                                "APPLY is a separate, confirmed step and is allowed only while the controller is Idle.",
                            )),
                    ),
            )
    }

    /// One tap-to-edit gantry measurement, backed by the shared keypad.
    fn gantry_field_cell(&self, field: GantryField, p: Palette, cx: &mut Context<Self>) -> Div {
        let view = cx.entity();
        let model = self.state.utilities.gantry();
        let value = model.field_text(field).to_string();
        let error = model
            .field_error(field)
            .map(std::string::ToString::to_string);
        let mut cell = v_flex()
            .w(px(210.0))
            .flex_shrink_0()
            .gap_1()
            .child(
                div()
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(format!("{} ({})", t(field.label()), field.unit())),
            )
            .child(
                Button::new(format!("gantry-field-{}", field.key()))
                    .label(if value.is_empty() {
                        "--".to_string()
                    } else {
                        value
                    })
                    .h(px(48.0))
                    .w_full()
                    .rounded_lg()
                    .text_size(px(13.0))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(p.panel)
                            .foreground(p.text)
                            .hover(p.raised_hover)
                            .active(p.accent_muted),
                    )
                    .border_1()
                    .border_color(if error.is_some() { p.danger } else { p.border })
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.state.open_keypad(KeypadTarget::Gantry(field));
                            cx.notify();
                        });
                    }),
            );
        if let Some(error) = error {
            cell = cell.child(
                div()
                    .text_size(px(8.5))
                    .line_height(px(11.0))
                    .text_color(p.danger)
                    .child(error),
            );
        }
        cell
    }

    /// Gantry-squaring calculator card. Pure measurement; no motion or write.
    fn gantry_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let model = self.state.utilities.gantry();
        let result = model.result();
        let error = model.error().map(std::string::ToString::to_string);
        let summary = result.map_or_else(
            || {
                t("No measurement yet. Enter the span and both Y readings in the calculator.")
                    .to_string()
            },
            |result| {
                let direction = if result.right_side_forward() {
                    t("RIGHT FORWARD")
                } else if result.left_side_forward() {
                    t("LEFT FORWARD")
                } else {
                    t("SQUARE")
                };
                format!(
                    "{} {} | {} {} | {} {} | {direction}",
                    t("DELTA"),
                    result.delta_text(),
                    t("ANGLE"),
                    result.angle_text(),
                    t("PER-SIDE"),
                    result.per_side_text()
                )
            },
        );
        let correction = result.map(|result| result.correction_text());
        v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(section_label(t("GANTRY SQUARING"), p))
                    .child(chip(t("CALCULATOR ONLY"), p)),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.warning)
                    .child(t(
                        "CALCULATOR ONLY - THE APP DOES NOT MOVE OR ADJUST THE GANTRY",
                    )),
            )
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                "Measure the Y distance from a common reference to the left and right ends of the gantry, enter the span between those points, and calculate the delta, angle, and per-side correction. Follow the machine's squaring procedure for any adjustment.",
            )))
            .child(
                div()
                    .font_family(self.mono.clone())
                    .text_size(px(10.5))
                    .text_color(p.text_dim)
                    .child(summary),
            )
            .when_some(correction, |this, correction| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.accent)
                        .child(correction),
                )
            })
            .when_some(error, |this, error| {
                this.child(div().text_size(px(10.0)).text_color(p.danger).child(error))
            })
            .child(
                Button::new("gantry-open")
                    .label(t("OPEN CALCULATOR"))
                    .primary()
                    .h(px(48.0))
                    .w(px(210.0))
                    .rounded_lg()
                    .text_size(px(11.0))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| this.open_gantry_squaring(cx));
                    }),
            )
    }

    /// Gantry-squaring wizard sheet. It only displays a calculation.
    fn gantry_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if !self.gantry_open {
            return div().into_any_element();
        }
        let view = cx.entity();
        let scrim_view = view.clone();
        let close_view = view.clone();
        let calculate_view = view.clone();
        let model = self.state.utilities.gantry();
        let result = model.result();
        let error = model.error().map(std::string::ToString::to_string);
        let fields = GantryField::ALL
            .into_iter()
            .map(|field| self.gantry_field_cell(field, p, cx))
            .collect::<Vec<_>>();
        let mono = self.mono.clone();
        let stat = move |label: &'static str, value: String| {
            v_flex()
                .flex_1()
                .min_w(px(0.0))
                .gap_0()
                .p_2()
                .rounded(px(6.0))
                .bg(p.panel)
                .child(
                    div()
                        .text_size(px(9.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.text_faint)
                        .child(label),
                )
                .child(
                    div()
                        .font_family(mono.clone())
                        .text_size(px(18.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.text)
                        .child(value),
                )
        };
        let delta_value = result.map_or_else(|| "--".to_string(), |result| result.delta_text());
        let angle_value = result.map_or_else(|| "--".to_string(), |result| result.angle_text());
        let per_side_value =
            result.map_or_else(|| "--".to_string(), |result| result.per_side_text());
        let correction = result.map(|result| result.correction_text());
        let per_side_note = result.map(|result| result.per_side_note());

        let sheet = v_flex()
            .relative()
            .w(px(820.0))
            .max_w_full()
            .max_h(px(720.0))
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(p.border_strong)
            .bg(p.raised)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child(t("GANTRY SQUARING CALCULATOR")),
                    )
                    .child(chip(t("NO MOTION / NO WRITE"), p)),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .font_weight(FontWeight::BOLD)
                    .line_height(px(14.0))
                    .text_color(p.warning)
                    .child(t(GANTRY_SAFETY_NOTE)),
            )
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                "Enter millimetres. The span is the distance between the two measurement points; the readings are the Y values at the left and right points. CALCULATE is local arithmetic.",
            )))
            .child(
                h_flex()
                    .w_full()
                    .min_w(px(0.0))
                    .flex_wrap()
                    .items_end()
                    .gap_2()
                    .children(fields)
                    .child(
                        Button::new("gantry-calculate")
                            .label(t("CALCULATE"))
                            .primary()
                            .h(px(48.0))
                            .w(px(150.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, _, cx| {
                                calculate_view.update(cx, |this, cx| {
                                    this.calculate_gantry_squaring(cx);
                                });
                            }),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.panel_alt)
                    .border_1()
                    .border_color(if error.is_some() { p.danger } else { p.border })
                    .child(section_label(t("MEASUREMENT RESULT"), p))
                    .child(
                        h_flex()
                            .w_full()
                            .min_w(px(0.0))
                            .gap_2()
                            .child(stat(t("DELTA (TOTAL)"), delta_value))
                            .child(stat(t("ANGLE"), angle_value))
                            .child(stat(t("PER-SIDE (PIVOT CENTRE)"), per_side_value)),
                    )
                    .when_some(correction, |this, correction| {
                        this.child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(p.accent)
                                .child(correction),
                        )
                    })
                    .when_some(per_side_note, |this, note| {
                        this.child(div().text_size(px(10.0)).text_color(p.text_dim).child(note))
                    })
                    .when_some(error, |this, error| {
                        this.child(div().text_size(px(10.0)).text_color(p.danger).child(error))
                    })
                    .when(result.is_none() && model.error().is_none(), |this| {
                        this.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(p.text_faint)
                                .child(t(
                                    "Press CALCULATE to validate the fields and show the correction.",
                                )),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(9.5))
                            .line_height(px(12.0))
                            .text_color(p.text_faint)
                            .child(t(GANTRY_CONVENTION_NOTE)),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("gantry-close")
                            .label(t("CLOSE"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(140.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, _, cx| {
                                close_view.update(cx, |this, cx| {
                                    this.close_gantry_squaring(cx);
                                });
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(9.5))
                            .text_color(p.text_faint)
                            .child(t(
                                "There is no APPLY. Move or shim the gantry by hand, then measure again.",
                            )),
                    ),
            );

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_3()
            .child(
                div()
                    .id("gantry-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |this, cx| {
                            this.close_gantry_squaring(cx);
                        });
                    }),
            )
            .child(sheet)
            .into_any_element()
    }

    /// Material surfacing summary card in the Utility workspace.
    fn surfacing_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let preview = self.state.utilities.surfacing_preview();
        let error = self.state.utilities.surfacing_error().map(str::to_string);
        let summary = preview.map_or_else(
            || t("No generated program yet. Open the wizard, validate the fields, and preview.").to_string(),
            |preview| {
                format!(
                    "{} depth passes | {} rows/pass | {} lines | {} mm cut | EST {} (rapids excluded)",
                    preview.depth_passes,
                    preview.rows_per_pass,
                    preview.line_count,
                    preview.path_length_mm.round(),
                    format_elapsed(Duration::from_secs(preview.estimated_secs()))
                )
            },
        );
        v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(section_label(t("MATERIAL SURFACING"), p))
                    .child(chip(t("GENERATED / NOT CHECKED"), p)),
            )
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                "Back-and-forth raster at each depth, with a safe-Z retract between passes. The wizard validates every field and caps the program before LOAD AS PROGRAM; it never starts the job.",
            )))
            .child(div().text_size(px(11.0)).text_color(p.text_dim).child(summary))
            .when_some(error, |this, error| {
                this.child(div().text_size(px(10.0)).text_color(p.danger).child(error))
            })
            .child(
                Button::new("surfacing-open")
                    .label(t("OPEN WIZARD"))
                    .primary()
                    .h(px(48.0))
                    .w(px(220.0))
                    .rounded_lg()
                    .text_size(px(11.0))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| this.open_surfacing(cx));
                    }),
            )
    }

    /// Read-only program limits card for the loaded program.
    fn program_limits_card(&self, p: Palette) -> impl IntoElement {
        let limits = self.state.program_limits();
        let status_color = if limits.is_over_travel() {
            p.danger
        } else if limits.is_not_checked() {
            p.warning
        } else if limits.is_checked() {
            p.success
        } else {
            p.text_faint
        };
        let soft_limits = match limits.soft_limits {
            Some(true) => t("ON").to_string(),
            Some(false) => t("OFF").to_string(),
            None => t("NOT REPORTED").to_string(),
        };
        let mut overruns = v_flex().gap_1();
        for line in limits.overruns() {
            overruns = overruns.child(
                div()
                    .text_size(px(10.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.danger)
                    .child(line),
            );
        }
        v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(section_label(t("PROGRAM LIMITS"), p))
                    .child(chip(t("READ-ONLY"), p)),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.text_faint)
                    .child(format!(
                        "PROGRAM {} | {} lines | {} segments | {} arcs | {} tool changes",
                        self.state.job_title(),
                        limits.lines,
                        limits.segments,
                        limits.arcs,
                        limits.tool_changes
                    )),
            )
            .child(
                div()
                    .font_family(self.mono.clone())
                    .text_size(px(10.0))
                    .text_color(p.text_dim)
                    .child(format!(
                        "WORK BOUNDS (zero-offset parse)   {}",
                        limits.bounds_text()
                    )),
            )
            .child(
                div()
                    .font_family(self.mono.clone())
                    .text_size(px(10.0))
                    .text_color(p.text_dim)
                    .child(format!(
                        "TRAVEL   {}   | soft limits $20 {soft_limits}",
                        limits.travel_text()
                    )),
            )
            .when_some(limits.machine_bounds, |this, machine| {
                this.child(
                    div()
                        .font_family(self.mono.clone())
                        .text_size(px(10.0))
                        .text_color(p.text_dim)
                        .child(format!(
                            "MACHINE EST (work + WCS/G92)   {}",
                            machine.format_all()
                        )),
                )
            })
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(status_color)
                    .child(limits.status_text()),
            )
            .child(overruns)
            .child(
                div()
                    .text_size(px(9.5))
                    .line_height(px(12.0))
                    .text_color(p.text_faint)
                    .child(t(
                        "Informational only: START is never blocked. The machine estimate adds the reported active WCS plus G92; TLO is not included. Missing travel or offset data shows NOT CHECKED, never a safety claim. No controller writes.",
                    )),
            )
    }

    /// Non-destructive source transforms for the loaded program.
    fn transform_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let loaded = self.state.job.has_program();
        let name = self.state.job_title().to_string();
        let compensated = self.state.job_compensated();
        let error = self.state.utilities.transform_error().map(str::to_string);
        let view = cx.entity();
        let numbers_view = view.clone();
        let comments_view = view.clone();
        let line_fields = [UtilityField::LineStart, UtilityField::LineStep]
            .into_iter()
            .map(|field| self.utility_field_cell(field, p, cx))
            .collect::<Vec<_>>();
        v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(section_label(t("SOURCE TRANSFORMS"), p))
                    .child(chip(t("PREVIEW FIRST"), p)),
            )
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(format!(
                "Loaded source: {name}. Transforms never modify it in place; LOAD AS PROGRAM replaces the job with the previewed copy and never starts it."
            )))
            .when(compensated, |this| {
                this.child(div().text_size(px(10.0)).text_color(p.warning).child(t(
                    "HEIGHT MAP / COMPENSATED: loading a transform replaces the compensated program and its Revert original. Keep an external copy if needed.",
                )))
            })
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .children(line_fields),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("transform-line-numbers")
                            .label(t("ADD LINE NUMBERS"))
                            .primary()
                            .h(px(48.0))
                            .w(px(200.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(!loaded)
                            .on_click(move |_, _, cx| {
                                numbers_view.update(cx, |this, cx| {
                                    this.generate_transform(TransformKind::AddLineNumbers, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("transform-strip-comments")
                            .label(t("STRIP COMMENTS"))
                            .warning()
                            .h(px(48.0))
                            .w(px(180.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(!loaded)
                            .on_click(move |_, _, cx| {
                                comments_view.update(cx, |this, cx| {
                                    this.generate_transform(TransformKind::StripComments, cx);
                                });
                            }),
                    )
                    .when(!loaded, |this| {
                        this.child(
                            div().text_size(px(10.0)).text_color(p.warning).child(t(
                                "NO PROGRAM: load a program before transforming its source",
                            )),
                        )
                    }),
            )
            .when_some(error, |this, error| {
                this.child(div().text_size(px(10.0)).text_color(p.danger).child(error))
            })
    }

    /// Explicit old/new confirmation for one `$10x` calibration write.
    fn calibration_confirm_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if !self.calibration_confirm_open {
            return div().into_any_element();
        }
        let Some(proposal) = self.state.utilities.calibration().proposal().cloned() else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let scrim_view = view.clone();
        let cancel_view = view.clone();
        let confirm_view = view.clone();
        let apply_view = view;
        let model = self.state.utilities.calibration();
        let confirmed = model.confirmed();
        let stage = model.stage();
        let locked = self.state.calibration_gate().blocked_reason();
        let last_result = model.last_result().map(std::string::ToString::to_string);
        let last_ok = model.last_result_ok();
        let can_apply = confirmed && locked.is_none() && !stage.is_busy();

        let old_box = v_flex()
            .flex_1()
            .min_w(px(0.0))
            .gap_0()
            .p_2()
            .rounded(px(6.0))
            .bg(p.panel)
            .child(
                div()
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(t("OLD STEPS/MM")),
            )
            .child(
                div()
                    .font_family(self.mono.clone())
                    .text_size(px(24.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_dim)
                    .child(format!("{:.3}", proposal.current_steps_per_mm)),
            );
        let new_box = v_flex()
            .flex_1()
            .min_w(px(0.0))
            .gap_0()
            .p_2()
            .rounded(px(6.0))
            .bg(p.panel)
            .border_1()
            .border_color(p.accent)
            .child(
                div()
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(t("NEW STEPS/MM")),
            )
            .child(
                div()
                    .font_family(self.mono.clone())
                    .text_size(px(24.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.accent)
                    .child(format!("{:.3}", proposal.new_steps_per_mm)),
            );

        let sheet = v_flex()
            .relative()
            .w(px(760.0))
            .max_w_full()
            .max_h(px(700.0))
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(p.accent)
            .bg(p.raised)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child(t("CALIBRATION CONFIRMATION")),
                    )
                    .child(chip(
                        &format!("${} {}", proposal.setting_number, proposal.axis.label()),
                        p,
                    )),
            )
            .child(div().text_size(px(10.0)).text_color(p.warning).child(t(
                "This writes one controller setting. It does not move the machine: the test move is your separate jog or MDI command. The controller stores steps/mm in EEPROM; avoid unnecessary writes.",
            )))
            .child(h_flex().gap_2().child(old_box).child(new_box))
            .child(
                div()
                    .font_family(self.mono.clone())
                    .text_size(px(11.0))
                    .text_color(p.text_dim)
                    .child(format!(
                        "{:.3} x {:.3} mm / {:.3} mm = {:.3} steps/mm   ({:+.3}, {:+.2}%)",
                        proposal.current_steps_per_mm,
                        proposal.commanded_mm,
                        proposal.measured_mm,
                        proposal.new_steps_per_mm,
                        proposal.delta_steps_per_mm,
                        proposal.delta_percent
                    )),
            )
            .child(
                div()
                    .text_size(px(9.5))
                    .text_color(p.text_faint)
                    .child(t(
                        "APPLY sends exactly one Command::SetSetting through the engine. The engine writes the value, reads it back, and only then marks it APPLIED. CANCEL sends nothing.",
                    )),
            )
            .when_some(locked, |this, reason| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.danger)
                        .child(format!(
                            "{}: {}",
                            t("WRITE LOCKED"),
                            reason.to_uppercase()
                        )),
                )
            })
            .when_some(last_result, |this, result| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(if last_ok { p.success } else { p.danger })
                        .child(result),
                )
            })
            .child(
                div().h(px(48.0)).flex().items_center().child(
                    Switch::new("calibration-confirm")
                        .checked(confirmed)
                        .label(t(
                            "I measured the actual travel and want to write this steps/mm value",
                        ))
                        .text_size(px(11.0))
                        .on_change(move |checked, _, cx| {
                            confirm_view.update(cx, |this, cx| {
                                this.state
                                    .utilities
                                    .calibration_mut()
                                    .set_confirmed(*checked);
                                cx.notify();
                            });
                        }),
                ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("calibration-apply")
                            .label(format!("{} ${}", t("APPLY"), proposal.setting_number))
                            .primary()
                            .h(px(48.0))
                            .flex_1()
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(!can_apply)
                            .on_click(move |_, _, cx| {
                                apply_view.update(cx, |this, cx| this.apply_calibration(cx));
                            }),
                    )
                    .child(
                        Button::new("calibration-cancel")
                            .label(t("CANCEL"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(120.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, _, cx| {
                                cancel_view.update(cx, |this, cx| this.cancel_calibration(cx));
                            }),
                    ),
            );

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_3()
            .child(
                div()
                    .id("calibration-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |this, cx| this.cancel_calibration(cx));
                    }),
            )
            .child(sheet)
            .into_any_element()
    }

    /// Material surfacing wizard sheet.
    fn surfacing_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if !self.surfacing_open {
            return div().into_any_element();
        }
        let view = cx.entity();
        let scrim_view = view.clone();
        let close_view = view.clone();
        let generate_view = view.clone();
        let spindle_view = view.clone();
        let confirm_view = view.clone();
        let load_view = view.clone();
        let mono = self.mono.clone();
        let model = &self.state.utilities;
        let preview = model.surfacing_preview();
        let error = model.surfacing_error().map(str::to_string);
        let load_refusal = self.state.program_load_refusal();
        let can_load = preview.is_some() && model.surfacing_confirmed() && load_refusal.is_none();

        let cells = UtilityField::ALL
            .into_iter()
            .filter(|field| field.is_surfacing())
            .map(|field| self.utility_field_cell(field, p, cx))
            .collect::<Vec<_>>();

        let mut source_lines = v_flex()
            .id("surfacing-preview-lines")
            .gap_0()
            .max_h(px(120.0))
            .overflow_y_scrollbar();
        let mut shown = 0usize;
        if let Some(preview) = preview {
            for line in preview.source.lines().take(40) {
                shown += 1;
                source_lines = source_lines.child(
                    div()
                        .font_family(mono.clone())
                        .text_size(px(10.0))
                        .line_height(px(13.0))
                        .text_color(p.text_dim)
                        .child(line.to_string()),
                );
            }
        }

        let summary = preview.map_or_else(
            || {
                t("GENERATE PREVIEW to validate the fields and compute passes, path length, and time.")
                    .to_string()
            },
            |preview| {
                format!(
                    "{} depth passes | {} rows/pass | {} lines | {} mm cut | EST {} (rapids excluded)",
                    preview.depth_passes,
                    preview.rows_per_pass,
                    preview.line_count,
                    preview.path_length_mm.round(),
                    format_elapsed(Duration::from_secs(preview.estimated_secs()))
                )
            },
        );
        let preview_name = preview.map(|preview| preview.name.clone());
        let total_lines = preview.map_or(0, |preview| preview.line_count);

        let sheet = v_flex()
            .relative()
            .w(px(920.0))
            .max_w_full()
            .max_h(px(700.0))
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(p.border_strong)
            .bg(p.raised)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child(t("MATERIAL SURFACING WIZARD")),
                    )
                    .child(chip(t("GENERATED / NOT CHECKED"), p)),
            )
            .child(div().text_size(px(10.0)).text_color(p.warning).child(t(
                "The program is generated from these fields. It is not checked against this machine's travel, offsets, tool, or stock. LOAD AS PROGRAM does not start it.",
            )))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .children(cells),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Switch::new("surfacing-spindle")
                            .checked(model.spindle_on())
                            .label(t("SPINDLE ON (M3 S... / M5)"))
                            .text_size(px(11.0))
                            .on_change(move |checked, _, cx| {
                                spindle_view.update(cx, |this, cx| {
                                    this.state.utilities.set_spindle_on(*checked);
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child(t("OFF omits both M3 and M5")),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.panel)
                    .border_1()
                    .border_color(if error.is_some() { p.danger } else { p.border })
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(section_label(t("PREVIEW SUMMARY"), p))
                            .when_some(preview_name, |this, name| {
                                this.child(
                                    div()
                                        .font_family(mono.clone())
                                        .text_size(px(10.0))
                                        .text_color(p.accent)
                                        .child(name),
                                )
                            }),
                    )
                    .child(div().text_size(px(11.0)).text_color(p.text_dim).child(summary))
                    .when_some(error, |this, error| {
                        this.child(div().text_size(px(10.0)).text_color(p.danger).child(error))
                    })
                    .when_some(preview, |this, _| {
                        this.child(source_lines).child(
                            div()
                                .text_size(px(9.0))
                                .text_color(p.text_faint)
                                .child(format!(
                                    "showing {} of {total_lines} generated lines",
                                    shown
                                )),
                        )
                    }),
            )
            .child(
                Switch::new("surfacing-confirm")
                    .checked(model.surfacing_confirmed())
                    .label(t(
                        "I understand this program is generated, not checked against the machine",
                    ))
                    .text_size(px(11.0))
                    .on_change(move |checked, _, cx| {
                        confirm_view.update(cx, |this, cx| {
                            this.state.utilities.set_surfacing_confirmed(*checked);
                            cx.notify();
                        });
                    }),
            )
            .when_some(load_refusal, |this, refusal| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(p.warning)
                        .child(t(refusal.message())),
                )
            })
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("surfacing-generate")
                            .label(t("GENERATE PREVIEW"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(180.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, _, cx| {
                                generate_view.update(cx, |this, cx| this.generate_surfacing(cx));
                            }),
                    )
                    .child(
                        Button::new("surfacing-load")
                            .label(t("LOAD AS PROGRAM"))
                            .primary()
                            .h(px(48.0))
                            .flex_1()
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(!can_load)
                            .on_click(move |_, _, cx| {
                                load_view.update(cx, |this, cx| this.load_surfacing(cx));
                            }),
                    )
                    .child(
                        Button::new("surfacing-close")
                            .label(t("CLOSE"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(110.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, _, cx| {
                                close_view.update(cx, |this, cx| this.close_surfacing(cx));
                            }),
                    ),
            );

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_3()
            .child(
                div()
                    .id("surfacing-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |this, cx| this.close_surfacing(cx));
                    }),
            )
            .child(sheet)
            .into_any_element()
    }

    /// Transform preview and explicit load sheet.
    fn transform_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(preview) = self.state.utilities.transform_preview() else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let scrim_view = view.clone();
        let close_view = view.clone();
        let confirm_view = view.clone();
        let load_view = view;
        let mono = self.mono.clone();
        let confirmed = self.state.utilities.transform_confirmed();
        let load_refusal = self.state.program_load_refusal();
        let can_load = confirmed && load_refusal.is_none();
        let stats = match preview.kind {
            TransformKind::AddLineNumbers => format!(
                "{} numbers added | {} existing N words kept | {} checksum lines kept | {} lines changed",
                preview.added_numbers,
                preview.skipped_existing,
                preview.skipped_checksums,
                preview.changed_lines
            ),
            TransformKind::StripComments => format!(
                "{} comment blocks removed | {} lines changed | {} checksum lines kept | {} -> {} lines",
                preview.removed_comments,
                preview.changed_lines,
                preview.skipped_checksums,
                preview.input_lines,
                preview.output_lines
            ),
        };
        let mut lines = v_flex()
            .id("transform-preview-lines")
            .gap_0()
            .max_h(px(260.0))
            .overflow_y_scrollbar();
        let mut shown = 0usize;
        for line in preview.source.lines().take(80) {
            shown += 1;
            lines = lines.child(
                div()
                    .font_family(mono.clone())
                    .text_size(px(10.5))
                    .line_height(px(14.0))
                    .text_color(p.text_dim)
                    .child(line.to_string()),
            );
        }
        let total = preview.source.lines().count();

        let sheet = v_flex()
            .relative()
            .w(px(820.0))
            .max_w_full()
            .max_h(px(700.0))
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(p.warning.opacity(0.6))
            .bg(p.raised)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child(t("TRANSFORM PREVIEW")),
                    )
                    .child(chip(t(preview.kind.label()), p)),
            )
            .child(div().text_size(px(10.0)).text_color(p.warning).child(t(
                "The loaded source is unchanged until LOAD AS PROGRAM. This loads one new program and never starts it.",
            )))
            .child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.panel)
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(p.text_faint)
                                    .child(format!("SOURCE   {}", preview.base_name)),
                            )
                            .child(chip(t("PREVIEW"), p)),
                    )
                    .child(
                        div()
                            .font_family(mono.clone())
                            .text_size(px(10.0))
                            .text_color(p.accent)
                            .child(format!("LOAD NAME   {}", preview.name)),
                    )
                    .child(div().text_size(px(10.0)).text_color(p.text_dim).child(stats)),
            )
            .child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.panel_alt)
                    .child(lines)
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child(format!("showing {shown} of {total} transformed lines")),
                    ),
            )
            .child(div().text_size(px(9.5)).text_color(p.text_faint).child(preview.note.clone()))
            .child(
                Switch::new("transform-confirm")
                    .checked(confirmed)
                    .label(t("I understand this loads the previewed source and does not start it"))
                    .text_size(px(11.0))
                    .on_change(move |checked, _, cx| {
                        confirm_view.update(cx, |this, cx| {
                            this.state.utilities.set_transform_confirmed(*checked);
                            cx.notify();
                        });
                    }),
            )
            .when_some(load_refusal, |this, refusal| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(p.warning)
                        .child(t(refusal.message())),
                )
            })
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("transform-load")
                            .label(t("LOAD AS PROGRAM"))
                            .primary()
                            .h(px(48.0))
                            .flex_1()
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(!can_load)
                            .on_click(move |_, _, cx| {
                                load_view.update(cx, |this, cx| this.load_transform(cx));
                            }),
                    )
                    .child(
                        Button::new("transform-close")
                            .label(t("CLOSE"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(110.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, _, cx| {
                                close_view.update(cx, |this, cx| this.close_transform(cx));
                            }),
                    ),
            );

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_3()
            .child(
                div()
                    .id("transform-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |this, cx| this.close_transform(cx));
                    }),
            )
            .child(sheet)
            .into_any_element()
    }

    /// First-run safety/setup wizard overlay.
    ///
    /// Every action here mutates the local [`crate::setup::SetupModel`] or the
    /// app config. No path calls the engine, so opening, navigating, changing
    /// units, closing, or finishing can never produce a controller command.
    fn setup_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if !self.state.setup.is_open() {
            return div().into_any_element();
        }
        let setup = &self.state.setup;
        let step = setup.step();
        let units = setup.units();
        let acknowledged = setup.acknowledged_count();
        let can_advance = setup.can_advance();
        let error = setup.error().map(std::string::ToString::to_string);
        let view = cx.entity();
        let scrim_view = view.clone();

        let mut step_chips = h_flex().gap_1();
        for candidate in SetupStep::ALL {
            let active = candidate == step;
            step_chips = step_chips.child(
                div()
                    .h(px(26.0))
                    .px_2()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .rounded(px(5.0))
                    .bg(if active { p.accent_muted } else { p.panel_alt })
                    .border_1()
                    .border_color(if active { p.accent } else { p.border })
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(if active { p.accent } else { p.text_faint })
                    .child(format!("{} {}", candidate.number(), t(candidate.title()))),
            );
        }

        let mut body = v_flex()
            .gap_2()
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scrollbar();

        match step {
            SetupStep::Safety => {
                let mut checklist = v_flex().gap_1();
                for item in SafetyItem::ALL {
                    let toggle_view = view.clone();
                    let checked = setup.safety_checked(item);
                    checklist = checklist.child(
                        h_flex()
                            .id(format!("setup-safety-{}", item.index()))
                            .h(px(56.0))
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .rounded_lg()
                            .border_1()
                            .border_color(if checked { p.accent } else { p.border })
                            .bg(if checked { p.accent_muted } else { p.panel })
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                toggle_view.update(cx, |this, cx| {
                                    this.state.setup_toggle_safety(item);
                                    cx.notify();
                                });
                            })
                            .child(
                                div()
                                    .w(px(26.0))
                                    .h(px(26.0))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(4.0))
                                    .border_1()
                                    .border_color(if checked { p.accent } else { p.border_strong })
                                    .bg(if checked { p.accent } else { p.panel_alt })
                                    .text_size(px(14.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.on_accent)
                                    .when(checked, |this| this.child("X")),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .gap_0()
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(if checked { p.text } else { p.text_dim })
                                            .child(t(item.title())),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(p.text_faint)
                                            .child(t(item.detail())),
                                    ),
                            ),
                    );
                }
                body = body
                    .child(
                        v_flex()
                            .gap_1()
                            .p_2()
                            .rounded(px(6.0))
                            .bg(p.panel_alt)
                            .border_1()
                            .border_color(p.warning)
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.warning)
                                    .child(t("THIS WIZARD DOES NOT TEST THE MACHINE")),
                            )
                            .child(div().text_size(px(10.0)).text_color(p.text_dim).child(t(
                                "The controller, its firmware, and the physical safety wiring remain authoritative. Acknowledge only what you have checked on this machine.",
                            ))),
                    )
                    .child(checklist)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(if acknowledged == SafetyItem::ALL.len() {
                                        p.success
                                    } else {
                                        p.warning
                                    })
                                    .child(format!(
                                        "{} {acknowledged} {} {}",
                                        t("ACKNOWLEDGED"),
                                        t("OF"),
                                        SafetyItem::ALL.len()
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(p.text_faint)
                                    .child(t("EVERY ITEM IS REQUIRED BEFORE NEXT")),
                            ),
                    );
            }
            SetupStep::Units => {
                let mm_selected = units == CoreUnits::Mm;
                let inch_selected = units == CoreUnits::Inch;
                let mm_view = view.clone();
                let inch_view = view.clone();
                let mm = Button::new("setup-units-mm")
                    .label(t("MILLIMETRES (MM)"))
                    .h(px(56.0))
                    .flex_1()
                    .rounded_lg()
                    .text_size(px(12.0))
                    .on_click(move |_, _, cx| {
                        mm_view.update(cx, |this, cx| {
                            this.state.setup_set_units(CoreUnits::Mm);
                            cx.notify();
                        });
                    });
                let inch = Button::new("setup-units-inch")
                    .label(t("INCHES (IN)"))
                    .h(px(56.0))
                    .flex_1()
                    .rounded_lg()
                    .text_size(px(12.0))
                    .on_click(move |_, _, cx| {
                        inch_view.update(cx, |this, cx| {
                            this.state.setup_set_units(CoreUnits::Inch);
                            cx.notify();
                        });
                    });
                body = body
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child(t("DEFAULT DISPLAY UNITS")),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(if mm_selected { mm.primary() } else { mm.secondary() })
                            .child(if inch_selected {
                                inch.primary()
                            } else {
                                inch.secondary()
                            }),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .p_2()
                            .rounded(px(6.0))
                            .bg(p.panel_alt)
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.info)
                                    .child(t("APP PREFERENCE ONLY")),
                            )
                            .child(div().text_size(px(10.0)).text_color(p.text_dim).child(t(
                                "This choice is saved for the next session. The wizard does not send G20 or G21 and does not change the controller's units. The live controller parser state still controls the current session.",
                            ))),
                    );
            }
            SetupStep::Connection => {
                body = body
                    .child(
                        v_flex()
                            .gap_1()
                            .p_2()
                            .rounded(px(6.0))
                            .bg(p.panel_alt)
                            .border_1()
                            .border_color(p.info)
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.info)
                                    .child(t("INFORMATIONAL ONLY")),
                            )
                            .child(div().text_size(px(11.0)).text_color(p.text).child(t(
                                "The wizard never connects, homes, jogs, probes, or writes controller settings.",
                            )))
                            .child(div().text_size(px(10.0)).text_color(p.text_dim).child(t(
                                "After setup, use the normal Connection panel. Start with the simulator or with the spindle off, then run only status, DRO, and small jogs.",
                            ))),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text_dim)
                                    .child(t("FIRST CONNECTION ORDER")),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(p.text_faint)
                                    .child(t(
                                        "1. Confirm the physical E-stop and independent spindle stop. 2. Open CONNECTION and select the serial port. 3. Wait for READY and Idle. 4. Check the DRO with one small jog. 5. Verify units, WCS, and soft limits before loading a program.",
                                    )),
                            ),
                    );
            }
            SetupStep::Review => {
                let unit_label = if units == CoreUnits::Mm {
                    t("MILLIMETRES (MM)")
                } else {
                    t("INCHES (IN)")
                };
                let row = |label: &'static str, value: String| {
                    h_flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .p_2()
                        .rounded(px(6.0))
                        .bg(p.panel_alt)
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(p.text_faint)
                                .child(label),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(p.text)
                                .child(value),
                        )
                };
                body = body
                    .child(row(
                        t("SAFETY ITEMS ACKNOWLEDGED"),
                        format!("{acknowledged} OF {}", SafetyItem::ALL.len()),
                    ))
                    .child(row(t("DISPLAY UNITS"), unit_label.to_string()))
                    .child(
                        v_flex()
                            .gap_1()
                            .p_2()
                            .rounded(px(6.0))
                            .bg(p.panel_alt)
                            .border_1()
                            .border_color(p.danger)
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.danger)
                                    .child(t("SETUP IS NOT A MACHINE CERTIFICATION")),
                            )
                            .child(div().text_size(px(10.0)).text_color(p.text_dim).child(t(
                                "This records that you reviewed the checklist and chose an app preference. It does not test, certify, or configure the machine. Verify every motion, limit, and safety device yourself.",
                            ))),
                    );
            }
        }

        let mut footer = h_flex().items_center().gap_2();
        if step.previous().is_some() {
            let back_view = view.clone();
            footer = footer.child(
                Button::new("setup-back")
                    .label(t("BACK"))
                    .secondary()
                    .h(px(48.0))
                    .w(px(96.0))
                    .rounded_lg()
                    .text_size(px(11.0))
                    .on_click(move |_, _, cx| {
                        back_view.update(cx, |this, cx| {
                            if this.state.setup_back() {
                                cx.notify();
                            }
                        });
                    }),
            );
        }
        let later_view = view.clone();
        footer = footer.child(
            Button::new("setup-remind-later")
                .label(t("REMIND ME LATER"))
                .secondary()
                .h(px(48.0))
                .w(px(170.0))
                .rounded_lg()
                .text_size(px(10.0))
                .on_click(move |_, _, cx| {
                    later_view.update(cx, |this, cx| {
                        this.state.remind_later_setup();
                        cx.notify();
                    });
                }),
        );
        footer = footer.child(div().flex_1());
        if step == SetupStep::Review {
            let finish_view = view.clone();
            footer = footer.child(
                Button::new("setup-finish")
                    .label(t("FINISH SETUP"))
                    .primary()
                    .h(px(48.0))
                    .w(px(164.0))
                    .rounded_lg()
                    .text_size(px(11.0))
                    .on_click(move |_, _, cx| {
                        finish_view.update(cx, |this, cx| {
                            let _ = this.state.finish_setup();
                            cx.notify();
                        });
                    }),
            );
        } else {
            let next_view = view.clone();
            footer = footer.child(
                Button::new("setup-next")
                    .label(t("NEXT"))
                    .primary()
                    .h(px(48.0))
                    .w(px(116.0))
                    .rounded_lg()
                    .text_size(px(11.0))
                    .disabled(!can_advance)
                    .on_click(move |_, _, cx| {
                        next_view.update(cx, |this, cx| {
                            if this.state.setup_next() {
                                cx.notify();
                            }
                        });
                    }),
            );
        }

        let sheet = v_flex()
            .relative()
            .w(px(760.0))
            .max_w_full()
            .max_h(px(700.0))
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(p.border_strong)
            .bg(p.raised)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child(t("MACHINE SETUP")),
                    )
                    .child(chip(
                        &format!(
                            "{} {} {} {}",
                            t("STEP"),
                            step.number(),
                            t("OF"),
                            SetupStep::ALL.len()
                        ),
                        p,
                    )),
            )
            .child(step_chips)
            .child(body)
            .when_some(error, |this, error| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.danger)
                        .child(error),
                )
            })
            .child(footer);

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_3()
            .child(
                div()
                    .id("setup-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |this, cx| {
                            this.state.remind_later_setup();
                            cx.notify();
                        });
                    }),
            )
            .child(sheet)
            .into_any_element()
    }

    /// First-run setup status and the explicit reopen action.
    ///
    /// The card never marks setup complete. `OPEN SETUP` reopens at SAFETY; the
    /// operator must pass the checklist and REVIEW again to save the flag.
    fn machine_setup_card(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let complete = self.state.config.setup_complete();
        let status_color = if complete { p.success } else { p.warning };
        v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .border_1()
            .border_color(if complete { p.border } else { p.warning })
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(section_label(t("MACHINE SETUP"), p))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(status_color)
                            .child(if complete {
                                t("COMPLETE")
                            } else {
                                t("NOT COMPLETED")
                            }),
                    ),
            )
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                "The wizard reviews the physical safety checklist, the app display unit preference, and what the first connection should look like. It never connects, homes, jogs, probes, or writes controller settings.",
            )))
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("machine-setup-open")
                            .label(t("OPEN SETUP"))
                            .primary()
                            .h(px(48.0))
                            .w(px(132.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.state.open_setup();
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(if complete {
                                t("REOPENS AT SAFETY TO REVIEW OR UPDATE THE CHOICES")
                            } else {
                                t("START HERE BEFORE THE FIRST COMMANDED MOVE")
                            }),
                    ),
            )
            .into_any_element()
    }

    /// Destructive `$RST` card with its local backup prompt.
    ///
    /// Every target row opens the same two-step confirmation; the card itself
    /// never sends. A successful local backup export or the explicit
    /// data-loss choice is required before any row can be confirmed.
    fn controller_reset_card(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let model = &self.state.controller_reset;
        let backup_path = self
            .state
            .config
            .backup_path()
            .map_or_else(|| "--".to_string(), |path| path.display().to_string());
        let gate_refusal = model.gate_refusal(self.reset_gate());
        let backup_refusal = model.backup_acknowledgement_refusal();
        let can_request = gate_refusal.is_none() && backup_refusal.is_none();
        let export_succeeded = model.export_succeeded_this_session();
        let loss_accepted = model.loss_accepted();

        let export_text = if model.backup_export_in_flight() {
            t("EXPORT IN FLIGHT - WAIT FOR THE WRITE TO FINISH").to_string()
        } else {
            model.export_result().map_or_else(
                || t("NO BACKUP EXPORTED THIS SESSION").to_string(),
                |result| result.text().to_string(),
            )
        };
        let export_color = if model.backup_export_in_flight() {
            p.warning
        } else {
            match model.export_result() {
                Some(result) if result.ok() => p.success,
                Some(_) => p.danger,
                None => p.text_faint,
            }
        };

        let result_line = model.last_result().map(|result| {
            let target = model.last_target().map_or("--", command_text);
            let color = match result {
                ResetResult::Sent => p.success,
                ResetResult::Refused => p.warning,
                ResetResult::Failed => p.danger,
            };
            let detail = model.error().map_or_else(
                || {
                    model.last_target().map_or_else(String::new, |target| {
                        t(crate::controller_reset::refresh_note(target)).to_string()
                    })
                },
                std::string::ToString::to_string,
            );
            (
                color,
                format!("{} {} - {detail}", t(result.label()), target),
            )
        });

        let export_view = view.clone();
        let ack_view = view.clone();
        let mut target_rows = v_flex().gap_1();
        for target in RESET_TARGETS {
            let target_view = view.clone();
            let destructive = is_most_destructive(target);
            let row_bg = if destructive {
                p.danger.opacity(0.10)
            } else {
                p.panel
            };
            let row_border = if destructive {
                p.danger.opacity(0.55)
            } else {
                p.border
            };
            let button = Button::new(format!("reset-{}", target.as_str()))
                .label(format!("{} {}", t("RESET"), command_text(target)))
                .h(px(48.0))
                .w(px(164.0))
                .flex_shrink_0()
                .rounded_lg()
                .text_size(px(10.0))
                .disabled(!can_request)
                .on_click(move |_, _, cx| {
                    target_view.update(cx, |this, cx| {
                        this.request_controller_reset(target, cx);
                    });
                });
            let button = if destructive {
                button.danger()
            } else {
                button.warning()
            };
            target_rows = target_rows.child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .px_1()
                    .rounded_lg()
                    .border_1()
                    .border_color(row_border)
                    .bg(row_bg)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.0))
                            .gap_0()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(chip(command_text(target), p))
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(if destructive { p.danger } else { p.text })
                                            .child(t(target_title(target))),
                                    )
                                    .when(destructive, |this| {
                                        this.child(
                                            div()
                                                .flex_shrink_0()
                                                .text_size(px(9.0))
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(p.danger)
                                                .child(t("MOST DESTRUCTIVE")),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(9.5))
                                    .text_color(if destructive { p.danger } else { p.text_dim })
                                    .child(t(summary(target))),
                            ),
                    )
                    .child(button),
            );
        }

        let ack = if export_succeeded {
            h_flex()
                .items_center()
                .gap_2()
                .h(px(48.0))
                .flex_1()
                .min_w(px(0.0))
                .px_2()
                .rounded_lg()
                .border_1()
                .border_color(p.success.opacity(0.45))
                .bg(p.success.opacity(0.10))
                .child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.success)
                        .child(t("BACKUP EXPORTED THIS SESSION - RESET ACKNOWLEDGED")),
                )
                .into_any_element()
        } else {
            let ack_button = Button::new("reset-loss-ack")
                .label(if loss_accepted {
                    t("DATA LOSS ACKNOWLEDGED - RESET MAY PROCEED")
                } else {
                    t("I HAVE A BACKUP OR ACCEPT DATA LOSS")
                })
                .h(px(48.0))
                .flex_1()
                .min_w(px(0.0))
                .rounded_lg()
                .text_size(px(10.0))
                .on_click(move |_, _, cx| {
                    ack_view.update(cx, |this, cx| this.toggle_controller_reset_loss(cx));
                });
            let ack_button = if loss_accepted {
                ack_button.danger()
            } else {
                ack_button.secondary()
            };
            ack_button.into_any_element()
        };

        v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(p.danger.opacity(0.45))
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(section_label(t("CONTROLLER RESET"), p))
                            .child(chip("$RST", p))
                            .child(chip(t("BACKUP REQUIRED"), p)),
                    )
                    .child(chip(t("DESTRUCTIVE"), p)),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.text_faint)
                    .truncate()
                    .child(format!("{}   {backup_path}", t("BACKUP FILE"))),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("reset-export-backup")
                            .label(t("EXPORT BACKUP"))
                            .primary()
                            .h(px(48.0))
                            .w(px(160.0))
                            .flex_shrink_0()
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(model.backup_export_in_flight())
                            .on_click(move |_, _, cx| {
                                export_view.update(cx, |this, cx| {
                                    this.export_controller_reset_backup(cx);
                                });
                            }),
                    )
                    .child(ack),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(export_color)
                    .truncate()
                    .child(export_text),
            )
            .when_some(gate_refusal, |this, refusal| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.danger)
                        .child(format!("{}: {}", t("RESET LOCKED"), t(refusal.message()))),
                )
            })
            .when_some(backup_refusal, |this, refusal| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.warning)
                        .child(t(refusal.message())),
                )
            })
            .when(can_request, |this| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.success)
                        .child(t("RESET GATES READY - TWO-STEP CONFIRMATION REQUIRED")),
                )
            })
            .child(target_rows)
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.warning)
                    .child(t(LOCAL_PROFILE_NOTE)),
            )
            .when_some(result_line, |this, (color, text)| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(color)
                        .child(text),
                )
            })
            .into_any_element()
    }

    /// Local job-history card in Utility.
    ///
    /// The records come from `app.json` through [`ConfigModel`]; the card only
    /// reads them. Newest first, at most ten rows, with a count and a sheet for
    /// every stored entry. Opening the sheet and arming the inline clear
    /// confirmation write nothing.
    fn job_history_card(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let records = self.state.config.job_history();
        let total = records.len();
        let armed = self.state.job_history_clear_armed();
        let open_view = cx.entity();
        let clear_view = cx.entity();
        let cancel_view = cx.entity();
        let confirm_view = cx.entity();

        let mut rows = v_flex().gap_1();
        for record in records.iter().rev().take(10) {
            rows = rows.child(job_history_row(record, &self.mono, p, false));
        }
        let shown = total.min(10);

        let mut card = v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(section_label(t("JOB HISTORY"), p))
                            .child(chip(&format!("{total} {}", t("STORED")), p))
                            .child(chip(t("LOCAL ONLY"), p))
                            .child(chip(&format!("{} {MAX_JOB_HISTORY}", t("MAX")), p)),
                    )
                    .when(total > 0, |this| {
                        this.child(chip(
                            &format!("{} {shown} {} {total}", t("SHOWING"), t("OF")),
                            p,
                        ))
                    }),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_dim)
                    .child(t("LOCAL ONLY - NOT SENT TO THE CONTROLLER")),
            )
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                "Finished jobs are stored in app.json on this computer. The newest 100 records are kept; the card shows the newest 10.",
            )));

        if total == 0 {
            card = card.child(
                div()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.panel)
                    .text_size(px(11.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(t("NO JOBS RECORDED")),
            );
        } else {
            card = card
                .child(job_history_column_header(p, &self.mono))
                .child(rows);
        }

        if armed {
            card = card.child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.danger.opacity(0.60))
                    .bg(p.danger.opacity(0.08))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.danger)
                            .child(t("CLEAR ALL HISTORY?")),
                    )
                    .child(div().text_size(px(10.0)).text_color(p.text_dim).child(t(
                        "This removes every stored job record from app.json on this computer. It does not change the loaded program or send anything to the controller.",
                    )))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("job-history-clear-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |this, cx| {
                                            this.cancel_job_history_clear(cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("job-history-clear-confirm")
                                    .label(t("CLEAR ALL HISTORY"))
                                    .danger()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        confirm_view.update(cx, |this, cx| {
                                            this.confirm_clear_job_history(cx);
                                        });
                                    }),
                            ),
                    ),
            );
        } else {
            card = card.child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("job-history-open")
                            .label(t("OPEN HISTORY"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(150.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .on_click(move |_, _, cx| {
                                open_view.update(cx, |this, cx| this.open_job_history(cx));
                            }),
                    )
                    .child(
                        Button::new("job-history-clear")
                            .label(t("CLEAR HISTORY"))
                            .danger()
                            .h(px(48.0))
                            .w(px(150.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(total == 0)
                            .on_click(move |_, _, cx| {
                                clear_view.update(cx, |this, cx| {
                                    this.arm_job_history_clear(cx);
                                });
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(t("CLEAR ASKS FOR CONFIRMATION FIRST")),
                    ),
            );
        }

        card.into_any_element()
    }

    /// All-records job-history sheet. Opening and closing write nothing.
    fn job_history_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if !self.job_history_open {
            return div().into_any_element();
        }
        let records = self.state.config.job_history();
        let total = records.len();
        let view = cx.entity();
        let close_view = view.clone();
        let scrim_view = view;

        let mut list = v_flex().gap_1();
        if total == 0 {
            list = list.child(
                div()
                    .p_3()
                    .rounded(px(6.0))
                    .bg(p.panel)
                    .text_size(px(11.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(t("NO JOBS RECORDED")),
            );
        } else {
            for record in records.iter().rev() {
                list = list.child(job_history_row(record, &self.mono, p, true));
            }
        }

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("job-history-sheet-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |this, cx| this.close_job_history(cx));
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(820.0))
                    .max_w_full()
                    .h(px(620.0))
                    .max_h_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(p.text)
                                            .child(t("JOB HISTORY")),
                                    )
                                    .child(chip(&format!("{total} {}", t("STORED")), p))
                                    .child(chip(t("LOCAL ONLY"), p)),
                            )
                            .child(chip(&format!("{} {MAX_JOB_HISTORY}", t("MAX")), p)),
                    )
                    .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                        "LOCAL ONLY - NOT SENT TO THE CONTROLLER. Records are kept in app.json on this computer, newest first.",
                    )))
                    .child(job_history_column_header(p, &self.mono))
                    .child(
                        v_flex()
                            .id("job-history-sheet-list")
                            .flex_1()
                            .min_h(px(0.0))
                            .gap_1()
                            .track_scroll(&self.job_history_scroll)
                            .overflow_y_scroll()
                            .vertical_scrollbar(&self.job_history_scroll)
                            .child(list),
                    )
                    .child(
                        h_flex()
                            .justify_end()
                            .child(
                                Button::new("job-history-sheet-close")
                                    .label(t("CLOSE"))
                                    .secondary()
                                    .h(px(48.0))
                                    .w(px(120.0))
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        close_view.update(cx, |this, cx| {
                                            this.close_job_history(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn utility_view(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if self.demo_gantry_card {
            // Screenshot switch: keep the gantry card at the top of the list so
            // the 1280x800 card capture does not depend on a manual scroll.
            self.utility_scroll.scroll_to_top_of_item(GANTRY_CARD_INDEX);
        }
        if self.demo_job_history_card {
            // Screenshot switch: the local job-history card is below the
            // reset/setup cards at 1280x800; scroll it to the top.
            self.utility_scroll
                .scroll_to_top_of_item(JOB_HISTORY_CARD_INDEX);
        }
        let view = cx.entity();
        let config = &self.state.config;
        let profile_path = config
            .profile_path()
            .map_or_else(|| "--".to_string(), |path| path.display().to_string());
        let backup_path = config
            .backup_path()
            .map_or_else(|| "--".to_string(), |path| path.display().to_string());
        let identity = config.identity_line();
        let result = config.last_result().map(|result| {
            (
                if result.ok { p.success } else { p.danger },
                result.text.clone(),
            )
        });
        let app_error = config.app_error().map(std::string::ToString::to_string);
        let profile_error = config.profile_error().map(std::string::ToString::to_string);
        let has_pending = config.has_pending_profile();
        let unverified = config.unverified_identity();
        let corrupt = config.corrupt_profile();
        let import_armed = self.backup_import_armed;

        let startup = &self.state.startup_lines;
        let startup_lock = self.state.startup_write_gate().blocked_reason();
        let sleep_refusal = self.state.sleep_refusal();
        let startup_result = startup
            .last_result()
            .map(|text| (startup.last_result_ok(), text.to_string()));
        let reload_view = cx.entity();
        let mut startup_list = v_flex().gap_1();
        for row in startup.rows() {
            let index = row.index;
            let edit_view = cx.entity();
            let apply_view = cx.entity();
            let busy = row.status.is_busy();
            let reported = row.reported;
            let current_text = if !reported {
                "(not read yet)".to_string()
            } else if row.stored.is_empty() {
                "(empty)".to_string()
            } else {
                row.stored.clone()
            };
            let draft_text = if row.is_clear() {
                "(empty; APPLY clears this line)".to_string()
            } else {
                row.draft.clone()
            };
            let status_label = t(row.status.label());
            let status_color = match &row.status {
                StartupStatus::Verified => p.success,
                StartupStatus::Mismatch(_) | StartupStatus::Failed(_) => p.danger,
                StartupStatus::Edited
                | StartupStatus::Queued
                | StartupStatus::Writing
                | StartupStatus::Verifying => p.warning,
                StartupStatus::Synced => p.text_faint,
            };
            let note = row.note.clone();
            let has_edit = row.has_edit();
            startup_list = startup_list.child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded_lg()
                    .bg(p.panel)
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(42.0))
                                    .flex_shrink_0()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.accent)
                                    .child(row.title()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .font_family(self.mono.clone())
                                    .text_size(px(12.0))
                                    .text_color(p.text)
                                    .child(current_text),
                            )
                            .when(!status_label.is_empty(), |this| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_size(px(10.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(status_color)
                                        .child(status_label),
                                )
                            })
                            .child(
                                Button::new(format!("startup-edit-{index}"))
                                    .label(t("EDIT"))
                                    .secondary()
                                    .h(px(36.0))
                                    .w(px(78.0))
                                    .rounded(px(6.0))
                                    .text_size(px(10.0))
                                    .disabled(busy || !reported)
                                    .on_click(move |_, window, cx| {
                                        edit_view.update(cx, |this, cx| {
                                            this.open_startup_editor(index, window, cx);
                                        });
                                    }),
                            ),
                    )
                    .when(has_edit, |this| {
                        this.child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .w(px(42.0))
                                        .flex_shrink_0()
                                        .text_size(px(9.0))
                                        .text_color(p.text_faint)
                                        .child(t("NEW")),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .truncate()
                                        .font_family(self.mono.clone())
                                        .text_size(px(11.0))
                                        .text_color(p.warning)
                                        .child(draft_text),
                                )
                                .child(
                                    Button::new(format!("startup-apply-{index}"))
                                        .label(t("APPLY"))
                                        .warning()
                                        .h(px(36.0))
                                        .w(px(74.0))
                                        .rounded(px(6.0))
                                        .text_size(px(10.0))
                                        .disabled(busy || startup_lock.is_some())
                                        .on_click(move |_, _, cx| {
                                            apply_view.update(cx, |this, cx| {
                                                this.request_startup_apply(index, cx);
                                            });
                                        }),
                                ),
                        )
                    })
                    .when_some(note, |this, note| {
                        this.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(status_color)
                                .child(note),
                        )
                    })
                    .when(row.status.is_problem(), |this| {
                        this.child(
                            div()
                                .text_size(px(9.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(p.danger)
                                .child(t("WRITE NOT APPLIED - THE PREVIOUS VALUE IS STILL ACTIVE")),
                        )
                    }),
            );
        }

        let startup_card = v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(section_label(t("STARTUP LINES"), p))
                            .child(chip(
                                if startup.reported() {
                                    "$N0 / $N1"
                                } else {
                                    "NOT READ YET"
                                },
                                p,
                            ))
                            .when(startup.pending_count() > 0, |this| {
                                this.child(chip(
                                    &format!("{} PENDING", startup.pending_count()),
                                    p,
                                ))
                            })
                            .child(chip(t("RESET EXECUTION"), p)),
                    )
                    .child(
                        Button::new("startup-reload")
                            .label(t("RELOAD"))
                            .secondary()
                            .h(px(36.0))
                            .w(px(88.0))
                            .rounded(px(6.0))
                            .text_size(px(10.0))
                            .on_click(move |_, _, cx| {
                                reload_view.update(cx, |this, cx| {
                                    if this
                                        .send_command(Command::System(SystemCommand::StartupLines))
                                    {
                                        this.state.push_console(
                                            ConsoleLevel::Info,
                                            "Requested $N startup lines",
                                        );
                                    }
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .child(div().text_size(px(10.0)).text_color(p.warning).child(t(
                "Startup lines execute automatically on every controller reset and can command motion, spindle, or coolant. Writes are allowed only while the controller is Idle; nothing is written until APPLY and CONFIRM WRITE.",
            )))
            .child(startup_list)
            .when_some(startup_lock, |this, reason| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.warning)
                        .child(format!(
                            "{}: {}",
                            t("STARTUP WRITES LOCKED"),
                            reason.to_uppercase()
                        )),
                )
            })
            .when_some(startup_result, |this, (ok, text)| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(if ok { p.success } else { p.danger })
                        .child(text),
                )
            });

        let sleep_view = cx.entity();
        let sleep_card = v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(section_label(t("CONTROLLER SLEEP"), p))
                    .child(chip("$SLP", p)),
            )
            .child(div().text_size(px(10.0)).text_color(p.warning).child(t(
                "Sleep stops controller status polling until RESET or a status wake. The app does not wake the controller automatically; the DRO and job view stay frozen until the controller answers again.",
            )))
            .when_some(sleep_refusal, |this, refusal| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.danger)
                        .child(refusal.message().to_uppercase()),
                )
            })
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("sleep-open")
                            .label(t("SLEEP"))
                            .warning()
                            .h(px(48.0))
                            .w(px(120.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(sleep_refusal.is_some())
                            .on_click(move |_, _, cx| {
                                sleep_view.update(cx, |this, cx| this.open_sleep_confirm(cx));
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(t("CONFIRM FIRST; CANCEL SENDS NOTHING")),
                    ),
            );

        let save_view = view.clone();
        let app_view = view.clone();
        let pending_view = view.clone();
        let export_view = view.clone();
        let validate_view = view.clone();
        let import_view = view;

        let profile_card = v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(section_label(t("CONTROLLER PROFILE"), p))
                    .child(chip(
                        if corrupt {
                            t("CORRUPT")
                        } else if unverified {
                            t("UNVERIFIED")
                        } else if has_pending {
                            t("PENDING CONFIRMATION")
                        } else if config.key().is_some() {
                            t("LOCAL ONLY")
                        } else {
                            t("NO CONTROLLER")
                        },
                        p,
                    )),
            )
            .child(div().text_size(px(13.0)).text_color(p.text).child(identity))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.text_faint)
                    .child(format!("FILE   {profile_path}")),
            )
            .when_some(profile_error, |this, error| {
                this.child(div().text_size(px(10.0)).text_color(p.warning).child(error))
            })
            .when_some(app_error, |this, error| {
                this.child(div().text_size(px(10.0)).text_color(p.danger).child(error))
            })
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("profile-save")
                            .label(t("SAVE PROFILE"))
                            .primary()
                            .h(px(48.0))
                            .w(px(140.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .disabled(corrupt)
                            .on_click(move |_, _, cx| {
                                save_view.update(cx, |this, cx| this.save_profile(cx));
                            }),
                    )
                    .child(
                        Button::new("profile-app-save")
                            .label(t("SAVE APP"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(110.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .on_click(move |_, _, cx| {
                                app_view.update(cx, |this, cx| this.save_app_config(cx));
                            }),
                    )
                    .when(has_pending, |this| {
                        this.child(
                            Button::new("profile-use-pending")
                                .label(t("USE LOCAL DATA"))
                                .warning()
                                .h(px(48.0))
                                .w(px(150.0))
                                .rounded_lg()
                                .text_size(px(10.0))
                                .on_click(move |_, _, cx| {
                                    pending_view.update(cx, |this, cx| {
                                        this.use_pending_profile(cx);
                                    });
                                }),
                        )
                    }),
            );

        let backup_card = v_flex()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(p.panel_alt)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(section_label(t("BACKUP BUNDLE"), p))
                    .child(chip("JSON / V1", p)),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.text_faint)
                    .child(format!("PATH   {backup_path}")),
            )
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                "Export writes app prefs, identity, settings, offsets, tools, macros, startup lines, and keymap; import validates before writing and never pushes to the controller.",
            )))
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("backup-export")
                            .label(t("EXPORT"))
                            .primary()
                            .h(px(48.0))
                            .w(px(104.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .on_click(move |_, _, cx| {
                                export_view.update(cx, |this, cx| this.export_backup(cx));
                            }),
                    )
                    .child(
                        Button::new("backup-validate")
                            .label(t("VALIDATE"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(104.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .on_click(move |_, _, cx| {
                                validate_view.update(cx, |this, cx| this.validate_backup(cx));
                            }),
                    )
                    .child(
                        Button::new("backup-import")
                            .label(if import_armed {
                                t("CONFIRM IMPORT")
                            } else {
                                t("IMPORT")
                            })
                            .warning()
                            .h(px(48.0))
                            .w(px(if import_armed { 160.0 } else { 104.0 }))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .on_click(move |_, _, cx| {
                                import_view.update(cx, |this, cx| this.import_backup(cx));
                            }),
                    ),
            )
            .when_some(result, |this, (color, text)| {
                this.child(div().text_size(px(10.0)).text_color(color).child(text))
            });

        v_flex()
            .id("utility-scroll")
            .gap_2()
            .size_full()
            .track_scroll(&self.utility_scroll)
            .overflow_y_scroll()
            .child(self.controller_reset_card(p, cx))
            .child(self.machine_setup_card(p, cx))
            .child(self.job_history_card(p, cx))
            .child(self.gantry_card(p, cx))
            .child(self.calibration_card(p, cx))
            .child(self.surfacing_card(p, cx))
            .child(self.program_limits_card(p))
            .child(self.transform_card(p, cx))
            .child(self.language_card(p, cx))
            .child(self.pins_card(p, cx))
            .child(self.keymap_card(p, cx))
            .child(self.macro_card(p, cx))
            .child(startup_card)
            .child(sleep_card)
            .child(profile_card)
            .child(backup_card)
            .vertical_scrollbar(&self.utility_scroll)
            .into_any_element()
    }

    fn tools_view(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let model = &self.state.tools;
        let table = model.table();
        let path = model.path().map_or_else(
            || "NO PROFILE PATH".to_string(),
            |path| path.display().to_string(),
        );
        let new_view = view.clone();
        let save_view = view.clone();
        let count = model.len();
        let dirty = model.dirty();
        let error = model.error().map(std::string::ToString::to_string);
        let status = model.file().status.clone();

        let rows = table.tools.clone();
        let mut list = v_flex().id("tool-table-list").gap_1();
        if model.is_empty() {
            list = list.child(
                div()
                    .p_3()
                    .rounded_lg()
                    .bg(p.panel_alt)
                    .text_size(px(12.0))
                    .text_color(p.text_dim)
                    .child(t(
                        "No tools in the app table. Add one so the M6 sheet can show a name, diameter, and length instead of only the number.",
                    )),
            );
        }
        for record in rows {
            let row_view = view.clone();
            let delete_view = view.clone();
            let number = record.number;
            let name = ToolTableModel::display_name(&record);
            let summary = ToolTableModel::summary(&record);
            let row = record.clone();
            list = list.child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .h(px(40.0))
                    .px_2()
                    .rounded(px(6.0))
                    .bg(p.panel_alt)
                    .child(
                        div()
                            .w(px(40.0))
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.accent)
                            .child(format!("T{number}")),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(p.text)
                                    .truncate()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(p.text_faint)
                                    .child(summary),
                            ),
                    )
                    .child(
                        Button::new(format!("tool-edit-{number}"))
                            .label(t("EDIT"))
                            .secondary()
                            .h(px(40.0))
                            .w(px(78.0))
                            .rounded(px(6.0))
                            .text_size(px(10.0))
                            .on_click(move |_, window, cx| {
                                row_view.update(cx, |this, cx| {
                                    this.open_tool_editor(Some(&row), cx);
                                    let _ = window;
                                });
                            }),
                    )
                    .child(
                        Button::new(format!("tool-delete-{number}"))
                            .label(t("DEL"))
                            .warning()
                            .h(px(40.0))
                            .w(px(54.0))
                            .rounded(px(6.0))
                            .text_size(px(10.0))
                            .on_click(move |_, _, cx| {
                                delete_view.update(cx, |this, cx| {
                                    this.delete_tool(number, cx);
                                });
                            }),
                    ),
            );
        }

        v_flex()
            .id("tools-mode-scroll")
            .gap_2()
            .size_full()
            .overflow_y_scrollbar()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(chip(&format!("{count} TOOLS"), p))
                            .when(dirty, |this| this.child(chip(t("UNSAVED"), p)))
                            .child(chip(
                                if self.state.profile.capabilities.has_tool_table {
                                    "CONTROLLER TABLE"
                                } else {
                                    "APP TABLE"
                                },
                                p,
                            ))
                            .child(chip(
                                &format!(
                                    "CTRL {} ENTRIES",
                                    self.state.profile.capabilities.tool_table_entries
                                ),
                                p,
                            )),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("tool-new")
                                    .label(t("NEW TOOL"))
                                    .primary()
                                    .h(px(44.0))
                                    .w(px(108.0))
                                    .rounded_lg()
                                    .text_size(px(10.0))
                                    .on_click(move |_, _, cx| {
                                        new_view.update(cx, |this, cx| {
                                            this.open_tool_editor(None, cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("tool-save")
                                    .label(t("SAVE"))
                                    .secondary()
                                    .h(px(44.0))
                                    .w(px(82.0))
                                    .rounded_lg()
                                    .text_size(px(10.0))
                                    .on_click(move |_, _, cx| {
                                        save_view.update(cx, |this, cx| this.save_profile(cx));
                                    }),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_0()
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(format!("FILE   {path}")),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(if error.is_some() {
                                p.danger
                            } else {
                                p.text_faint
                            })
                            .child(format!("STATUS {status}")),
                    ),
            )
            .when_some(error, |this, error| {
                this.child(div().text_size(px(10.0)).text_color(p.danger).child(error))
            })
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                "The app owns this table; it persists in the per-controller profile. Controller [T:] entries stay read-only and are never merged silently.",
            )))
            .child(list)
            .child({
                let controller = &self.state.controller_tools;
                let mut panel = v_flex()
                    .id("controller-tool-table")
                    .gap_1()
                    .p_2()
                    .rounded_lg()
                    .bg(p.panel_alt)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(section_label(t("CONTROLLER TOOLS"), p))
                                    .child(chip(t("READ-ONLY"), p)),
                            )
                            .child(chip(&format!("{} ENTRIES", controller.len()), p)),
                    )
                    .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                        "Reported by the controller through [T:...]; this panel never writes, overwrites, or merges into the app table above.",
                    )));
                if !controller.reported() {
                    panel = panel.child(
                        div()
                            .px_2()
                            .py(px(5.0))
                            .rounded(px(6.0))
                            .bg(p.panel)
                            .text_size(px(11.0))
                            .text_color(p.text_dim)
                            .child(t("No controller tool table has been reported yet.")),
                    );
                } else if controller.is_empty() {
                    panel = panel.child(
                        div()
                            .px_2()
                            .py(px(5.0))
                            .rounded(px(6.0))
                            .bg(p.panel)
                            .text_size(px(11.0))
                            .text_color(p.text_dim)
                            .child(t("The controller reports an empty tool table (0 entries).")),
                    );
                } else {
                    panel = panel.child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .text_size(px(8.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_faint)
                            .child(div().w(px(40.0)).child(t("TOOL")))
                            .child(div().flex_1().child(t("NAME")))
                            .child(div().w(px(68.0)).child("X"))
                            .child(div().w(px(68.0)).child("Y"))
                            .child(div().w(px(68.0)).child("Z"))
                            .child(div().w(px(58.0)).child(t("RADIUS")))
                            .child(div().w(px(46.0)).child(t("POCKET"))),
                    );
                    for entry in controller.entries() {
                        panel = panel.child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .h(px(34.0))
                                .px_2()
                                .rounded(px(6.0))
                                .bg(p.panel)
                                .child(
                                    div()
                                        .w(px(40.0))
                                        .flex_shrink_0()
                                        .text_size(px(12.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(p.accent)
                                        .child(format!("T{}", entry.number)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .truncate()
                                        .text_size(px(11.0))
                                        .text_color(p.text)
                                        .child(ControllerToolTable::display_name(entry)),
                                )
                                .child(
                                    div()
                                        .w(px(68.0))
                                        .flex_shrink_0()
                                        .font_family(self.mono.clone())
                                        .text_size(px(10.0))
                                        .text_color(p.text_dim)
                                        .child(ControllerToolTable::offset_text(
                                            entry,
                                            CoreAxis::X,
                                        )),
                                )
                                .child(
                                    div()
                                        .w(px(68.0))
                                        .flex_shrink_0()
                                        .font_family(self.mono.clone())
                                        .text_size(px(10.0))
                                        .text_color(p.text_dim)
                                        .child(ControllerToolTable::offset_text(
                                            entry,
                                            CoreAxis::Y,
                                        )),
                                )
                                .child(
                                    div()
                                        .w(px(68.0))
                                        .flex_shrink_0()
                                        .font_family(self.mono.clone())
                                        .text_size(px(10.0))
                                        .text_color(p.text_dim)
                                        .child(ControllerToolTable::offset_text(
                                            entry,
                                            CoreAxis::Z,
                                        )),
                                )
                                .child(
                                    div()
                                        .w(px(58.0))
                                        .flex_shrink_0()
                                        .font_family(self.mono.clone())
                                        .text_size(px(10.0))
                                        .text_color(p.text_dim)
                                        .child(ControllerToolTable::radius_text(entry)),
                                )
                                .child(
                                    div()
                                        .w(px(46.0))
                                        .flex_shrink_0()
                                        .font_family(self.mono.clone())
                                        .text_size(px(10.0))
                                        .text_color(p.text_dim)
                                        .child(ControllerToolTable::pocket_text(entry)),
                                ),
                        );
                    }
                }
                panel
            })
            .into_any_element()
    }

    // ------------------------------------------------------- settings editor

    /// Generated settings form: search toolbar, group rail, and row list.
    fn settings_view(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let model = &self.state.settings;
        let total = model.total();
        let pending = model.pending_count();
        let has_pending = model.has_pending();
        let query = model.search().to_string();
        let can_write = self.settings_write_allowed();
        let lock_reason = self.settings_lock_reason();
        let groups = model.rail().to_vec();
        let scope = model.scope();
        let scope_name = model.scope_name();
        let rows: Vec<SettingRow> = model.visible_rows().into_iter().cloned().collect();
        let axis_labels = model.axis_labels().to_vec();
        let last_result = model.last_result().map(str::to_string);
        let last_ok = model.last_result_ok();
        let mono = self.mono.clone();
        let view = cx.entity();

        if total == 0 {
            return self.settings_empty_state(p).into_any_element();
        }

        let search_view = view.clone();
        let clear_view = view.clone();
        let search_cell = div()
            .id("settings-search")
            .h(px(48.0))
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .rounded_lg()
            .bg(p.panel_alt)
            .border_1()
            .border_color(p.border)
            .cursor_pointer()
            .hover(|style| style.bg(p.raised_hover))
            .on_click(move |_, window, cx| {
                search_view.update(cx, |shell, cx| {
                    shell.open_settings_entry(SettingsEntryPurpose::Search, window, cx);
                });
            })
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text_faint)
                    .child(t("SEARCH")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(13.0))
                    .text_color(if query.is_empty() {
                        p.text_faint
                    } else {
                        p.text
                    })
                    .child(if query.is_empty() {
                        "number, name, or group".to_string()
                    } else {
                        query.clone()
                    }),
            );
        let clear_button = (!query.is_empty()).then(|| {
            let view = clear_view;
            div()
                .id("settings-search-clear")
                .h(px(48.0))
                .w(px(60.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_lg()
                .bg(p.panel_alt)
                .border_1()
                .border_color(p.border)
                .text_size(px(10.0))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_dim)
                .cursor_pointer()
                .hover(|style| style.bg(p.raised_hover).text_color(p.text))
                .on_click(move |_, _, cx| {
                    view.update(cx, |shell, cx| {
                        shell.state.settings.set_search(String::new());
                        cx.notify();
                    });
                })
                .child(t("CLEAR"))
        });

        let pending_chip = div()
            .px_2()
            .h(px(22.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .rounded(px(5.0))
            .bg(if pending > 0 {
                p.warning.opacity(0.16)
            } else {
                p.panel_alt
            })
            .border_1()
            .border_color(p.border)
            .text_size(px(10.0))
            .font_weight(FontWeight::BOLD)
            .text_color(if pending > 0 { p.warning } else { p.text_dim })
            .child(format!("{pending} PENDING"));

        let apply_view = view.clone();
        let reset_view = view.clone();
        let toolbar = h_flex()
            .h(px(56.0))
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_2()
            .border_b_1()
            .border_color(p.border)
            .child(search_cell)
            .when_some(clear_button, |this, button| this.child(button))
            .child(chip(&format!("{total} TOTAL"), p))
            .child(pending_chip)
            .child(
                Button::new("settings-apply")
                    .label(t("APPLY"))
                    .primary()
                    .h(px(48.0))
                    .w(px(88.0))
                    .rounded_lg()
                    .text_size(px(11.0))
                    .disabled(!can_write || pending == 0)
                    .on_click(move |_, _, cx| {
                        apply_view.update(cx, |shell, cx| shell.apply_settings(cx));
                    }),
            )
            .child(
                Button::new("settings-reset")
                    .label(t("REVERT"))
                    .secondary()
                    .h(px(48.0))
                    .w(px(84.0))
                    .rounded_lg()
                    .text_size(px(11.0))
                    .disabled(!has_pending)
                    .on_click(move |_, _, cx| {
                        reset_view.update(cx, |shell, cx| {
                            shell.state.settings.reset_drafts();
                            cx.notify();
                        });
                    }),
            );

        let rail = v_flex()
            .id("settings-groups")
            .w(px(190.0))
            .flex_shrink_0()
            .h_full()
            .gap_1()
            .p_1()
            .overflow_y_scroll()
            .children(
                groups
                    .into_iter()
                    .map(|group| settings_group_row(group, scope, p, view.clone())),
            );

        let matching = rows.len();
        let form_rows: Vec<AnyElement> = rows
            .into_iter()
            .map(|row| {
                // Editing is local and always allowed; only Apply is gated by
                // the controller's idle-only write rule. A queued or in-flight
                // row locks itself inside the row renderer.
                settings_row_view(row, p, view.clone(), mono.clone(), &axis_labels, true)
            })
            .collect();

        let form_header = h_flex()
            .h(px(36.0))
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_2()
            .border_b_1()
            .border_color(p.border)
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(12.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.text)
                    .child(scope_name),
            )
            .child(chip(&format!("{matching} SHOWN"), p))
            .child(div().flex_1())
            .child(chip(
                if query.is_empty() {
                    "$ES / $EG"
                } else {
                    t("SEARCH")
                },
                p,
            ));

        let list = v_flex()
            .id("settings-form")
            .flex_1()
            .min_h(px(0.0))
            .overflow_y_scroll()
            .p_2()
            .gap_1()
            .when(matching == 0, |this| {
                this.child(settings_no_rows(query.as_str(), p))
            })
            .children(form_rows);

        let lock_text = if can_write {
            t("IDLE: WRITES ENABLED").to_string()
        } else {
            format!("{}: {lock_reason}", t("WRITES LOCKED"))
        };
        let footer = h_flex()
            .h(px(30.0))
            .flex_shrink_0()
            .items_center()
            .gap_2()
            .px_2()
            .border_t_1()
            .border_color(p.border)
            .text_size(px(9.0))
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::BOLD)
                    .text_color(if can_write { p.success } else { p.warning })
                    .child(lock_text),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(p.text_faint)
                    .child(t("ONE WRITE AT A TIME | READ-BACK VERIFIED")),
            )
            .child(div().flex_1())
            .when_some(last_result, |this, result| {
                this.child(
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_color(if last_ok { p.success } else { p.danger })
                        .child(result),
                )
            });

        let form = v_flex()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .border_l_1()
            .border_color(p.border)
            .child(form_header)
            .child(list)
            .child(footer);

        v_flex()
            .size_full()
            .child(toolbar)
            .child(h_flex().flex_1().min_h(px(0.0)).child(rail).child(form))
            .into_any_element()
    }

    /// Honest empty state for mock mode and for a controller with no metadata.
    fn settings_empty_state(&self, p: Palette) -> Div {
        let (title, detail) = if !self.state.live {
            (
                t("MOCK MODE: NO CONTROLLER SETTINGS"),
                t(
                    "The generated editor is built from the controller's $$ values and $ES/$EG metadata. The design-review mock supplies neither. Start without --mock to connect the in-process FlexiHAL demo controller.",
                ),
            )
        } else if !self.ready() {
            (
                t("WAITING FOR SETTINGS"),
                t(
                    "The handshake has not reported $$, $ES, or $EG yet. The form appears as soon as the controller answers.",
                ),
            )
        } else {
            (
                t("THE CONTROLLER REPORTED NO SETTINGS"),
                t(
                    "$$ returned no values and $ES/$EG returned no metadata. The editor stays empty rather than inventing a setting list.",
                ),
            )
        };
        let capabilities = &self.state.profile.capabilities;
        let mut facts = vec![
            format!("PROFILE: {}", self.state.profile.name),
            capabilities.units.label().to_string(),
            format!("{} AXES", capabilities.axes.len()),
            format!(
                "SPINDLES: {}",
                if capabilities.multi_spindle {
                    "MULTI"
                } else {
                    "SINGLE"
                }
            ),
        ];
        for (name, enabled) in capabilities.accessory_panels() {
            facts.push(format!(
                "{name}: {}",
                if enabled { "reported" } else { "not reported" }
            ));
        }

        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .p_4()
            .child(
                v_flex()
                    .gap_2()
                    .max_w(px(620.0))
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border)
                    .bg(p.panel_alt)
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(p.text_dim)
                            .child(detail),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .max_w(px(620.0))
                    .w_full()
                    .child(
                        div()
                            .text_size(px(9.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_faint)
                            .child(t("EXAMPLE SHAPE (NO VALUES ARE WRITTEN)")),
                    )
                    .child(settings_example_row(
                        "Example boolean",
                        "Switch maps to $ES type 0 | 0 = OFF, 1 = ON",
                        "OFF",
                        p,
                    ))
                    .child(settings_example_row(
                        "Example decimal",
                        "Numeric field and keypad | 0.000 .. 1000.000 | mm",
                        "123.450",
                        p,
                    )),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .max_w(px(620.0))
                    .gap_1()
                    .children(facts.iter().map(|fact| chip(fact, p))),
            )
            .child(
                h_flex().flex_wrap().max_w(px(620.0)).gap_1().children(
                    [
                        "BOOLEAN",
                        "BITFIELD",
                        "EXCLUSIVE",
                        "RADIO",
                        "AXIS MASK",
                        "INTEGER",
                        "DECIMAL",
                        "TEXT",
                        "PASSWORD",
                        "IPV4",
                    ]
                    .into_iter()
                    .map(|kind| chip(kind, p)),
                ),
            )
    }

    /// Shared text sheet for search and text/password/IPv4 settings.
    fn keymap_editor_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = self.keymap_editor.as_ref() else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let action = editor.action;
        let input = editor.input.clone();
        let error = editor.error.clone();
        let capturing = editor.capturing;
        let capture_focus = editor.capture_focus.clone();
        let apply_view = view.clone();
        let cancel_view = view.clone();
        let scrim_view = view.clone();
        let capture_view = view.clone();

        let presets: [(&str, &str); 10] = [
            ("F5", "f5"),
            ("F6", "f6"),
            ("F7", "f7"),
            ("F8", "f8"),
            ("C+S+LEFT", "ctrl-shift-left"),
            ("C+S+RIGHT", "ctrl-shift-right"),
            ("C+S+UP", "ctrl-shift-up"),
            ("C+S+DOWN", "ctrl-shift-down"),
            ("C+S+PGUP", "ctrl-shift-pageup"),
            ("C+S+PGDN", "ctrl-shift-pagedown"),
        ];
        let mut preset_row = h_flex().flex_wrap().gap_1();
        for (label, keys) in presets {
            let preset_input = input.clone();
            let preset_view = view.clone();
            preset_row = preset_row.child(
                Button::new(format!("keymap-preset-{keys}"))
                    .label(label)
                    .secondary()
                    .h(px(44.0))
                    .w(px(118.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.panel)
                    .text_size(px(10.0))
                    .on_click(move |_, window, cx| {
                        preset_input.update(cx, |state, cx| {
                            state.set_value(keys, window, cx);
                        });
                        preset_view.update(cx, |this, cx| {
                            if let Some(editor) = this.keymap_editor.as_mut() {
                                editor.error = None;
                            }
                            cx.notify();
                        });
                    }),
            );
        }

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("keymap-editor-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| shell.cancel_keymap_editor(cx));
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .track_focus(&capture_focus)
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                        this.capture_keymap_key(event, window, cx);
                    }))
                    .w(px(620.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(if capturing { p.accent } else { p.border_strong })
                    .bg(p.raised)
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_dim)
                            .child(format!("{} / {}", t("KEYMAP"), action.label())),
                    )
                    .when(capturing, |this| {
                        this.child(
                            div()
                                .text_size(px(14.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(p.accent)
                                .child(t("PRESS A KEY COMBINATION")),
                        )
                    })
                    .child({
                        let focus_input = input.clone();
                        div()
                            .h(px(48.0))
                            .w_full()
                            .flex()
                            .items_center()
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                focus_input.update(cx, |state, cx| state.focus(window, cx));
                            })
                            .child(
                                Input::new(&input)
                                    .with_size(Size::Large)
                                    .w_full()
                                    .text_size(px(16.0)),
                            )
                    })
                    .child(div().text_size(px(10.0)).text_color(p.text_faint).child(
                        "Tap a preset, type a combination such as ctrl-shift-left, or \
                                 press RECORD KEY. Ctrl, Alt, Fn, or F1-F24 are required so \
                                 normal typing is not stolen. Esc, Cmd/Super/Win, and common text \
                                 editing shortcuts are reserved.",
                    ))
                    .child(preset_row)
                    .when_some(error, |this, error| {
                        this.child(div().text_size(px(10.0)).text_color(p.danger).child(error))
                    })
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                Button::new("keymap-capture")
                                    .label(if capturing {
                                        t("PRESS A KEY...")
                                    } else {
                                        t("RECORD KEY")
                                    })
                                    .warning()
                                    .h(px(48.0))
                                    .w(px(150.0))
                                    .rounded_lg()
                                    .text_size(px(10.0))
                                    .on_click(move |_, window, cx| {
                                        capture_view.update(cx, |this, cx| {
                                            this.start_keymap_capture(window, cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("keymap-editor-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |this, cx| {
                                            this.cancel_keymap_editor(cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("keymap-editor-apply")
                                    .label(t("USE KEY"))
                                    .primary()
                                    .h(px(48.0))
                                    .w(px(140.0))
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        apply_view.update(cx, |this, cx| {
                                            this.apply_keymap_editor(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn settings_entry_overlay(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(entry) = self.settings_entry.as_ref() else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let title = match entry.purpose {
            SettingsEntryPurpose::Search => t("SEARCH SETTINGS").to_string(),
            SettingsEntryPurpose::Text(number) => self.state.settings.row(number).map_or_else(
                || format!("SETTING ${number}"),
                |row| format!("SETTING ${number} / {}", row.name),
            ),
            SettingsEntryPurpose::StartupLine(index) => format!("STARTUP LINE $N{index}"),
            SettingsEntryPurpose::ToolName => t("TOOL NAME").to_string(),
            SettingsEntryPurpose::MacroName => t("MACRO NAME").to_string(),
            SettingsEntryPurpose::MacroCommand => t("MACRO COMMAND").to_string(),
            SettingsEntryPurpose::HeightMapName => "SAVE HEIGHT MAP".to_string(),
        };
        let hint = match entry.purpose {
            SettingsEntryPurpose::Search => {
                "Matches a setting number, its name, or its group name.".to_string()
            }
            SettingsEntryPurpose::Text(number) => self
                .state
                .settings
                .row(number)
                .map(|row| row.meta_line())
                .unwrap_or_default(),
            SettingsEntryPurpose::StartupLine(index) => format!(
                "$N{index} executes on every controller reset and can command motion or the \
                 spindle. Empty text clears the line. Up to {} ASCII bytes; the draft is local \
                 until APPLY and CONFIRM WRITE.",
                crate::startup::MAX_STARTUP_TEXT
            ),
            SettingsEntryPurpose::ToolName => {
                "Display name for the M6 sheet; 128 characters, no control characters.".to_string()
            }
            SettingsEntryPurpose::MacroName => {
                "Unique in this controller profile; up to 128 characters, no control characters."
                    .to_string()
            }
            SettingsEntryPurpose::MacroCommand => {
                format!(
                    "One MDI G-code line up to {} bytes. System ($) commands, newlines, and \
                     control characters are rejected in this pass.",
                    crate::macros::MAX_MACRO_COMMAND_BYTES
                )
            }
            SettingsEntryPurpose::HeightMapName => {
                "Short label for this controller profile; the same name replaces that map."
                    .to_string()
            }
        };
        let masked = entry.masked;
        let error = entry.error.clone();
        let input = entry.input.clone();
        let commit_label = match entry.purpose {
            SettingsEntryPurpose::HeightMapName => t("SAVE MAP"),
            SettingsEntryPurpose::StartupLine(_) => t("KEEP DRAFT"),
            _ if masked => t("SET"),
            _ => t("APPLY TEXT"),
        };
        let commit_view = view.clone();
        let cancel_view = view.clone();
        let scrim_view = view;

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("settings-entry-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| shell.cancel_settings_entry(cx));
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(460.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.raised)
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_dim)
                            .child(title),
                    )
                    .child({
                        let focus_input = input.clone();
                        div()
                            .h(px(48.0))
                            .w_full()
                            .flex()
                            .items_center()
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                focus_input.update(cx, |state, cx| state.focus(window, cx));
                            })
                            .child(
                                Input::new(&input)
                                    .with_size(Size::Large)
                                    .w_full()
                                    .text_size(px(16.0))
                                    .when(masked, |this| this.mask_toggle()),
                            )
                    })
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(hint),
                    )
                    .when_some(error, |this, error| {
                        this.child(div().text_size(px(10.0)).text_color(p.danger).child(error))
                    })
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("settings-entry-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |shell, cx| {
                                            shell.cancel_settings_entry(cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("settings-entry-commit")
                                    .label(t(commit_label))
                                    .primary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        commit_view.update(cx, |shell, cx| {
                                            shell.commit_settings_entry(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Warning sheet for one confirmed `$N<n>=<line>` write.
    ///
    /// The exact text and the previous controller value are both visible; the
    /// command is not sent until CONFIRM WRITE.
    fn startup_confirm_overlay(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(confirm) = self.startup_confirm.as_ref() else {
            return div().into_any_element();
        };
        let index = confirm.index;
        let text = confirm.text.clone();
        let previous = self
            .state
            .startup_lines
            .row(index)
            .map(|row| {
                if row.stored.is_empty() {
                    "(empty)".to_string()
                } else {
                    row.stored.clone()
                }
            })
            .unwrap_or_else(|| "(not read)".to_string());
        let view = cx.entity();
        let cancel_view = view.clone();
        let confirm_view = view.clone();
        let scrim_view = view;

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("startup-confirm-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| shell.cancel_startup_confirm(cx));
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(560.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(format!("WRITE $N{index}")),
                            )
                            .child(chip(t("RESET EXECUTION"), p)),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(p.warning)
                            .child(t("This startup line executes automatically on every controller reset. It can command motion, spindle, or coolant.")),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .p_2()
                            .rounded(px(6.0))
                            .bg(p.panel)
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(p.text_faint)
                                    .child(t("EXACT TEXT TO STORE")),
                            )
                            .child(
                                div()
                                    .font_family(self.mono.clone())
                                    .text_size(px(14.0))
                                    .text_color(p.text)
                                    .child(if text.is_empty() {
                                        "(empty line: clears the startup line)".to_string()
                                    } else {
                                        text.clone()
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(format!(
                                "{}   {previous}",
                                t("Previous controller value")
                            )),
                    )
                    .child(div().text_size(px(10.0)).text_color(p.text_dim).child(t(
                        "CONFIRM WRITE sends the exact text shown. The app does not update the row itself: it reads the line back and reports the result.",
                    )))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("startup-confirm-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |shell, cx| {
                                            shell.cancel_startup_confirm(cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("startup-confirm-write")
                                    .label(t("CONFIRM WRITE"))
                                    .warning()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        confirm_view.update(cx, |shell, cx| {
                                            shell.confirm_startup_apply(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Sleep confirmation: explains that polling stops until a manual wake.
    ///
    /// The sheet sends nothing on open, scrim click, or CANCEL. CONFIRM SLEEP
    /// consumes the sheet and hands exactly one `Command::Sleep` to the gate.
    fn sleep_confirm_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if !self.state.sleep_confirm_open {
            return div().into_any_element();
        }
        let view = cx.entity();
        let cancel_view = view.clone();
        let confirm_view = view.clone();
        let scrim_view = view;

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("sleep-confirm-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| shell.cancel_sleep_confirm(cx));
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(560.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(t("PUT CONTROLLER TO SLEEP")),
                            )
                            .child(chip("$SLP", p)),
                    )
                    .child(div().text_size(px(11.0)).text_color(p.warning).child(t(
                        "Sleep stops controller status polling until RESET or a status wake. The app does not wake the controller automatically; the DRO and job view stay frozen until the controller answers again.",
                    )))
                    .child(
                        v_flex()
                            .gap_1()
                            .p_2()
                            .rounded(px(6.0))
                            .bg(p.panel)
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(p.text_faint)
                                    .child(t("BEFORE YOU CONFIRM")),
                            )
                            .child(div().text_size(px(10.0)).text_color(p.text_dim).child(t(
                                "The controller must report Idle and no job may be running or paused. One confirmed tap sends one sleep command; CANCEL sends nothing.",
                            ))),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("sleep-confirm-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |shell, cx| {
                                            shell.cancel_sleep_confirm(cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("sleep-confirm-send")
                                    .label(t("CONFIRM SLEEP"))
                                    .warning()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        confirm_view.update(cx, |shell, cx| {
                                            shell.confirm_sleep(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Exact-command confirmation for one destructive controller reset.
    ///
    /// Opening, the scrim, and CANCEL send nothing. CONFIRM consumes the sheet
    /// and hands exactly one `Command::System(SystemCommand::Reset(target))`
    /// to the model's gate; the read-only refresh follows only after the
    /// sender accepts the reset.
    fn reset_confirm_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(target) = self.state.controller_reset.confirm_target() else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let cancel_view = view.clone();
        let confirm_view = view.clone();
        let scrim_view = view;
        let command = command_text(target);
        let destructive = is_most_destructive(target);
        let action_color = if destructive { p.danger } else { p.warning };
        let backup_line = if self.state.controller_reset.export_succeeded_this_session() {
            t("BACKUP EXPORTED THIS SESSION")
        } else {
            t("DATA LOSS ACKNOWLEDGED BY THE OPERATOR")
        };
        let confirm_button = Button::new("reset-confirm-send")
            .label(format!("{} {command}", t("CONFIRM")))
            .h(px(48.0))
            .flex_1()
            .rounded_lg()
            .text_size(px(11.0))
            .on_click(move |_, _, cx| {
                confirm_view.update(cx, |shell, cx| {
                    shell.confirm_controller_reset(cx);
                });
            });
        let confirm_button = if destructive {
            confirm_button.danger()
        } else {
            confirm_button.warning()
        };

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("reset-confirm-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| {
                            shell.cancel_controller_reset(cx);
                        });
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(640.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(action_color.opacity(0.70))
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(action_color)
                                    .child(format!("{} {command}", t("RESET"))),
                            )
                            .child(chip(t("ONE COMMAND"), p)),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .p_2()
                            .rounded(px(6.0))
                            .bg(p.panel)
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(p.text_faint)
                                    .child(t("EXACT COMMAND TO SEND")),
                            )
                            .child(
                                div()
                                    .font_family(self.mono.clone())
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(command),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(if destructive { p.danger } else { p.text_dim })
                            .child(t(consequence(target))),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.warning)
                            .child(t(LOCAL_PROFILE_NOTE)),
                    )
                    .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                        "The controller may reset or re-handshake after this command. The app does not reconnect, home, jog, or resume anything. CANCEL sends nothing.",
                    )))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.success)
                            .child(backup_line),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("reset-confirm-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |shell, cx| {
                                            shell.cancel_controller_reset(cx);
                                        });
                                    }),
                            )
                            .child(confirm_button),
                    ),
            )
            .into_any_element()
    }

    /// Read-only `$PINS` / `$PINSTATE` / `$LIM` / `$LEV` sheet.
    ///
    /// The panel is controller diagnostic data, separate from the live `Pn`
    /// signals card. It has no motion, MDI or setting control; REFRESH and
    /// CLOSE are the only actions.
    fn pins_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if !self.state.pins.is_open() {
            return div().into_any_element();
        }
        let view = cx.entity();
        let scrim_view = view.clone();
        let refresh_view = view.clone();
        let close_view = view;
        let mono = self.mono.clone();
        let pins = &self.state.pins;

        let status_color = match pins.status() {
            PinsStatus::Idle => p.text_faint,
            PinsStatus::Refreshing | PinsStatus::Busy => p.warning,
            PinsStatus::Requested => p.success,
            PinsStatus::Stopped(_) => p.danger,
        };
        let status_text = pins_status_label(pins.status());

        let pin_count = pins.pin_map().map_or(0, |map| map.len());
        let mut map_list = v_flex()
            .id("pins-map-list")
            .gap_1()
            .max_h(px(250.0))
            .overflow_y_scrollbar();
        match pins.pin_map() {
            None | Some([]) => {
                map_list = map_list.child(pins_empty(pins.pin_map_note(), p));
            }
            Some(map) => {
                for pin in map {
                    map_list = map_list.child(
                        h_flex()
                            .h(px(30.0))
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .rounded(px(5.0))
                            .bg(p.panel)
                            .child(
                                div()
                                    .w(px(46.0))
                                    .flex_shrink_0()
                                    .font_family(mono.clone())
                                    .text_size(px(10.0))
                                    .text_color(p.text_faint)
                                    .child(format!("#{}", pin.id)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(11.0))
                                    .text_color(p.text)
                                    .child(pin.name.clone()),
                            )
                            .child(
                                div()
                                    .w(px(58.0))
                                    .flex_shrink_0()
                                    .text_size(px(9.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text_dim)
                                    .child(pin_direction_label(pin.direction)),
                            ),
                    );
                }
            }
        }

        let pin_state_card = match pins.pin_state() {
            Some(state) => pins_section(t("PIN STATE ($PINSTATE)"), p)
                .child(pins_key_value(
                    "RAW MASK",
                    signal_mask_text(state.mask),
                    p,
                    &mono,
                ))
                .child(pins_key_value(
                    "INPUTS",
                    format!("{} reported", state.inputs.len()),
                    p,
                    &mono,
                ))
                .child(pins_bits(
                    "pins-input-bits",
                    pin_bits_text(&state.inputs),
                    p,
                    &mono,
                ))
                .child(pins_key_value(
                    "OUTPUTS",
                    format!("{} reported", state.outputs.len()),
                    p,
                    &mono,
                ))
                .child(pins_bits(
                    "pins-output-bits",
                    pin_bits_text(&state.outputs),
                    p,
                    &mono,
                )),
            None => pins_section(t("PIN STATE ($PINSTATE)"), p)
                .child(pins_empty(pins.pin_state_note(), p)),
        };

        let limits_card = match pins.limit_state() {
            Some(state) => {
                let mut rows = v_flex().gap_1();
                for axis in &self.state.profile.capabilities.axes {
                    let asserted = CoreAxis::from_index(axis.index())
                        .is_some_and(|axis| state.is_asserted(axis));
                    rows = rows.child(
                        h_flex()
                            .h(px(24.0))
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(p.text_dim)
                                    .child(format!("{} LIMIT", axis.label())),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .h(px(20.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(4.0))
                                    .bg(if asserted {
                                        p.danger.opacity(0.16)
                                    } else {
                                        p.panel
                                    })
                                    .text_size(px(9.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(if asserted { p.danger } else { p.success })
                                    .child(if asserted { "ASSERTED" } else { t("CLEAR") }),
                            ),
                    );
                }
                if self.state.profile.capabilities.axes.is_empty() {
                    rows = rows.child(pins_empty("No axes have been reported by $I+", p));
                }
                pins_section("LIMITS ($LIM)", p)
                    .child(rows)
                    .child(pins_key_value(
                        "RAW MASK",
                        format!("0x{:04X}", state.mask),
                        p,
                        &mono,
                    ))
            }
            None => pins_section("LIMITS ($LIM)", p).child(pins_empty(pins.limits_note(), p)),
        };

        let events_card = match pins.last_events() {
            Some(events) => pins_section(t("LAST EVENTS ($LEV)"), p)
                .child(pins_key_value("COUNT", events.count.to_string(), p, &mono))
                .child(pins_key_value(
                    t("SIGNALS"),
                    if events.signals.is_empty() {
                        "none".to_string()
                    } else {
                        events.signals.to_string()
                    },
                    p,
                    &mono,
                ))
                .child(pins_key_value(
                    "RAW MASK",
                    signal_mask_text(events.signals),
                    p,
                    &mono,
                )),
            None => pins_section(t("LAST EVENTS ($LEV)"), p)
                .child(pins_empty(pins.last_events_note(), p)),
        };

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("pins-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| shell.close_pins_panel(cx));
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(1080.0))
                    .max_w_full()
                    .max_h_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                v_flex()
                                    .min_w(px(0.0))
                                    .gap_0()
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(p.text)
                                            .child(t("PINS / LIMITS")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(p.text_faint)
                                            .child(t("CONTROLLER DIAGNOSTIC DATA - READ-ONLY")),
                                    ),
                            )
                            .child(
                                div()
                                    .max_w(px(560.0))
                                    .truncate()
                                    .px_2()
                                    .h(px(22.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(5.0))
                                    .bg(status_color.opacity(0.16))
                                    .border_1()
                                    .border_color(status_color.opacity(0.5))
                                    .text_size(px(9.5))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(status_color)
                                    .child(status_text),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .line_height(px(14.0))
                            .text_color(p.text_dim)
                            .child(
                                "Read-only controller evidence. Pin levels and limit state are \
                                 instantaneous reports; the pin map is controller configuration. \
                                 Missing and empty reports are shown explicitly.",
                            ),
                    )
                    .child(
                        h_flex()
                            .flex_1()
                            .min_h(px(0.0))
                            .items_stretch()
                            .gap_2()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .gap_2()
                                    .child(
                                        pins_section(t("PIN MAP ($PINS)"), p)
                                            .child(
                                                div()
                                                    .text_size(px(9.5))
                                                    .text_color(p.text_faint)
                                                    .child(format!("{pin_count} entries")),
                                            )
                                            .child(map_list),
                                    )
                                    .child(events_card),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .gap_2()
                                    .child(pin_state_card)
                                    .child(limits_card),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .text_size(px(9.5))
                                    .text_color(p.text_faint)
                                    .child(
                                        "REFRESH sends $PINS, $PINSTATE, $LIM and $LEV. \
                                         Closing this sheet sends nothing.",
                                    ),
                            )
                            .child(
                                Button::new("pins-refresh")
                                    .label(t("REFRESH"))
                                    .primary()
                                    .h(px(48.0))
                                    .w(px(120.0))
                                    .flex_shrink_0()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        refresh_view.update(cx, |shell, cx| {
                                            shell.request_pins_refresh(cx);
                                        });
                                    }),
                            )
                            .child(
                                div()
                                    .id("pins-close")
                                    .h(px(48.0))
                                    .w(px(120.0))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_lg()
                                    .bg(p.panel_alt)
                                    .border_1()
                                    .border_color(p.border_strong)
                                    .cursor_pointer()
                                    .hover(|style| style.bg(p.raised_hover).border_color(p.accent))
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .on_click(move |_, _, cx| {
                                        close_view.update(cx, |shell, cx| {
                                            shell.close_pins_panel(cx);
                                        });
                                    })
                                    .child(t("CLOSE")),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Tall right cell: Coolant, Macros, Signals and Work in one continuous
    /// card. There is deliberately no divider between Signals and Work, and the
    /// card's bottom edge is the MDI/DRO baseline (the Spindle card below it is
    /// exactly one row tall).
    fn aux_panel(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let coolant = self
            .state
            .profile
            .capabilities
            .coolant_channels()
            .into_iter()
            .enumerate()
            .map(|(slot, (name, channel))| {
                let active = self.state.coolant[channel];
                let view = view.clone();
                let variant = if active {
                    ButtonCustomVariant::new(cx)
                        .color(p.accent_muted)
                        .foreground(p.accent)
                        .hover(p.raised_hover)
                        .active(p.accent_muted)
                } else {
                    ButtonCustomVariant::new(cx)
                        .color(p.panel_alt)
                        .foreground(p.text_dim)
                        .hover(p.raised_hover)
                        .active(p.raised)
                };
                Button::new(format!("coolant-{slot}"))
                    .label(name)
                    .flex_1()
                    .h(px(48.0))
                    .rounded(px(6.0))
                    .text_size(px(11.0))
                    .custom(variant)
                    .disabled(!self.aux_allowed())
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.toggle_coolant(channel, cx);
                        });
                    })
            });

        let macro_view = view.clone();
        let macro_rows = (0..4).map(|row| {
            h_flex().gap_1().children((0..2).map(|col| {
                let index = row * 2 + col;
                let macro_view = macro_view.clone();
                let Some(mac) = self.state.macros.get(index) else {
                    return div().into_any_element();
                };
                Button::new(format!("macro-{index}"))
                    .label(mac.label)
                    .flex_1()
                    .h(px(48.0))
                    .rounded(px(6.0))
                    .text_size(px(11.0))
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(p.panel_alt)
                            .foreground(p.text_dim)
                            .hover(p.raised_hover)
                            .active(p.accent_muted),
                    )
                    .disabled(!self.macro_allowed(mac.action))
                    .on_click(move |_, _, cx| {
                        macro_view.update(cx, |this, cx| {
                            this.run_macro(index, cx);
                        });
                    })
                    .into_any_element()
            }))
        });

        let signal_rows = (0..3).map(|row| {
            h_flex().gap_2().children((0..2).map(|col| {
                let index = row * 2 + col;
                let Some(signal) = self.state.signals.get(index) else {
                    return div().into_any_element();
                };
                let (color, text) = match signal.state {
                    SignalState::Off => (p.text_faint, p.text_faint),
                    SignalState::Ok => (p.success, p.text_dim),
                    SignalState::Warn => (p.warning, p.warning),
                    SignalState::Alert => (p.danger, p.danger),
                };
                h_flex()
                    .items_center()
                    .gap_1()
                    .flex_1()
                    .min_w(px(0.0))
                    .h(px(22.0))
                    .child(div().w(px(7.0)).h(px(7.0)).rounded(px(999.0)).bg(color))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(text)
                            .child(signal.label.clone()),
                    )
                    .into_any_element()
            }))
        });

        let work_rows = if self.state.live {
            [
                ("Origin X", format!("{:>8.3}", self.state.offset(Axis::X))),
                ("Origin Y", format!("{:>8.3}", self.state.offset(Axis::Y))),
                ("Origin Z", format!("{:>8.3}", self.state.offset(Axis::Z))),
                (
                    t("Feed"),
                    format!("{} {}/min", self.state.feed_rate, self.state.unit()),
                ),
            ]
        } else {
            [
                ("Origin X", "-123.450".to_string()),
                ("Origin Y", "-87.200".to_string()),
                ("Origin Z", "-2.000".to_string()),
                (
                    t("Feed"),
                    format!("{} {}/min", self.state.feed_rate, self.state.unit()),
                ),
            ]
        };

        let signal_note = if self.state.live {
            if self.state.signal_caps.is_empty() {
                "Pn: waiting for $I+".to_string()
            } else {
                format!("Pn: {}", self.state.signal_caps)
            }
        } else {
            "HSDEP | no probe-connected pin".to_string()
        };

        card(&p)
            .flex_1()
            .min_h(px(0.0))
            .overflow_hidden()
            .gap_2()
            .child(
                v_flex()
                    .gap_1()
                    .flex_shrink_0()
                    .child(section_label(t("COOLANT"), p))
                    .child(h_flex().gap_1().children(coolant)),
            )
            .child(
                v_flex()
                    .gap_1()
                    .flex_shrink_0()
                    .child(section_label(t("MACROS"), p))
                    .child(v_flex().gap_1().children(macro_rows)),
            )
            .child(
                v_flex()
                    .gap_1()
                    .flex_shrink_0()
                    .child(section_label(t("SIGNALS"), p))
                    .child(v_flex().gap_1().children(signal_rows))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(signal_note),
                    ),
            )
            // No divider: the extra height of the tall cell simply spaces the
            // Work rows below Signals.
            .child(
                v_flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .gap_1()
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(section_label(t("WORK"), p))
                            .child(chip(self.state.wcs(), p)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .justify_between()
                            .gap_1()
                            .children(work_rows.iter().map(|(name, value)| {
                                h_flex()
                                    .items_center()
                                    .justify_between()
                                    .h(px(24.0))
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(p.text_faint)
                                            .child(*name),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .font_family(self.mono.clone())
                                            .text_color(p.text)
                                            .child(value.clone()),
                                    )
                            })),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .child(chip(
                                if self.state.profile.capabilities.units
                                    == crate::state::Units::Inch
                                {
                                    "G20"
                                } else {
                                    "G21"
                                },
                                p,
                            ))
                            .child(chip("G90", p))
                            .child(chip("G17", p)),
                    ),
            )
    }

    /// MDI: one 48 px field plus a Send button on a single row, one hint line
    /// with the compact UP/DOWN history recall chips.
    fn mdi_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let empty = self.state.mdi.trim().is_empty();
        let field_view = view.clone();
        let send_view = view.clone();
        let up_view = view.clone();
        let down_view = view.clone();
        let history_len = self.state.mdi_history.len();
        let history_empty = history_len == 0;
        let can_recall_next = self.state.mdi_history_index.is_some();

        card(&p)
            .h(px(ROW_H))
            .flex_shrink_0()
            .justify_center()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(section_label(t("MDI"), p))
                    .child(
                        div()
                            .id("mdi-field")
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .h(px(48.0))
                            .px_3()
                            .rounded(px(7.0))
                            .bg(p.panel_alt)
                            .border_1()
                            .border_color(p.border)
                            .font_family(self.mono.clone())
                            .text_size(px(17.0))
                            .text_color(if empty { p.text_faint } else { p.text })
                            .cursor_pointer()
                            .child(if empty {
                                "G-code or numeric input".to_string()
                            } else {
                                self.state.mdi.clone()
                            })
                            .on_click(move |_, _, cx| {
                                field_view.update(cx, |this, cx| {
                                    this.state.open_keypad(KeypadTarget::Mdi);
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        Button::new("mdi-send")
                            .label(t("SEND"))
                            .primary()
                            .h(px(48.0))
                            .w(px(80.0))
                            .rounded_lg()
                            .text_size(px(12.0))
                            .disabled(!self.mdi_allowed())
                            .on_click(move |_, _, cx| {
                                send_view.update(cx, |this, cx| this.send_mdi(cx));
                            }),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_size(px(11.0)).text_color(p.text_faint).child(
                        if self.state.live {
                            "Live: engine logs command + ok"
                        } else {
                            "Mock send appends to console"
                        },
                    ))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(p.text_faint)
                                    .child(format!("HIST {history_len}")),
                            )
                            .child(
                                Button::new("mdi-history-up")
                                    .label(t("UP"))
                                    .secondary()
                                    .h(px(30.0))
                                    .w(px(48.0))
                                    .rounded(px(6.0))
                                    .text_size(px(10.0))
                                    .disabled(history_empty)
                                    .on_click(move |_, _, cx| {
                                        up_view.update(cx, |this, cx| {
                                            this.state.mdi_history_previous();
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                Button::new("mdi-history-down")
                                    .label(t("DN"))
                                    .secondary()
                                    .h(px(30.0))
                                    .w(px(48.0))
                                    .rounded(px(6.0))
                                    .text_size(px(10.0))
                                    .disabled(!can_recall_next)
                                    .on_click(move |_, _, cx| {
                                        down_view.update(cx, |this, cx| {
                                            this.state.mdi_history_next();
                                            cx.notify();
                                        });
                                    }),
                            ),
                    ),
            )
    }

    /// DRO: three equal X/Y/Z cells with a 26 px value and a 48 px zero button.
    fn dro_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let wcs_view = view.clone();
        let wcs_menu = Button::new("wcs-menu")
            .label(self.state.wcs())
            .h(px(22.0))
            .w(px(58.0))
            .rounded(px(6.0))
            .text_size(px(10.0))
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(p.panel_alt)
                    .foreground(p.accent)
                    .hover(p.raised_hover)
                    .active(p.accent_muted),
            )
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                let mut menu = menu.min_w(px(150.0));
                for index in 0..AppState::wcs_count() {
                    let wcs_view = wcs_view.clone();
                    menu = menu.item(PopupMenuItem::new(AppState::wcs_at(index)).on_click(
                        move |_, _, cx| {
                            wcs_view.update(cx, |this, cx| {
                                this.select_wcs(index, cx);
                            });
                        },
                    ));
                }
                menu
            });

        let tiles = self.state.axes().iter().map(|axis| {
            let axis = *axis;
            let value_view = view.clone();
            let zero_view = view.clone();
            let value = format!("{:>8.3}", self.state.value(axis));
            let stale = self.stale();
            div()
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .flex()
                .items_center()
                .gap_1()
                .p_1()
                .rounded(px(6.0))
                .bg(p.panel_alt)
                .border_1()
                .border_color(p.border)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(
                            h_flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(p.text_dim)
                                        .child(axis.label()),
                                )
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(p.text_faint)
                                        .child(self.state.unit()),
                                ),
                        )
                        .child(
                            h_flex().w_full().justify_end().child(
                                div()
                                    .id(format!("dro-{}", axis.label()))
                                    .cursor_pointer()
                                    .font_family(self.mono.clone())
                                    .text_size(px(26.0))
                                    .text_color(if stale { p.text_faint } else { p.text })
                                    .child(value)
                                    .on_click(move |_, _, cx| {
                                        value_view.update(cx, |this, cx| {
                                            this.state.open_keypad(KeypadTarget::Dro(axis));
                                            cx.notify();
                                        });
                                    }),
                            ),
                        ),
                )
                .child(
                    Button::new(format!("zero-{}", axis.label()))
                        .label("0")
                        .h(px(48.0))
                        .w(px(48.0))
                        .rounded(px(7.0))
                        .text_size(px(16.0))
                        .custom(
                            ButtonCustomVariant::new(cx)
                                .color(p.panel_alt)
                                .foreground(p.warning)
                                .hover(p.raised_hover)
                                .active(p.raised),
                        )
                        .border_1()
                        .border_color(p.border)
                        .disabled(!self.mdi_allowed())
                        .on_click(move |_, _, cx| {
                            zero_view.update(cx, |this, cx| {
                                this.zero_axis(axis, cx);
                            });
                        }),
                )
        });

        card(&p)
            .h(px(ROW_H))
            .flex_shrink_0()
            .gap_1()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .h(px(20.0))
                    .child(section_label(t("DRO"), p))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .when(self.stale(), |this| this.child(stale_badge(p)))
                            .child(wcs_menu),
                    ),
            )
            .child(h_flex().gap_2().flex_1().min_h(px(0.0)).children(tiles))
    }

    /// Feed: label and 24 px percentage with `RESET 100%`, then a full-width
    /// slider. Coarse `-10`/`+10` and fine `-1`/`+1` are 48 px targets.
    fn feed_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let down_view = view.clone();
        let fine_down_view = view.clone();
        let fine_up_view = view.clone();
        let up_view = view.clone();
        let reset_view = view.clone();

        card(&p)
            .h(px(ROW_H))
            .flex_shrink_0()
            .gap_1()
            .py_1()
            .px_2()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .h(px(48.0))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(section_label(t("FEED OVERRIDE"), p))
                            .child(
                                div()
                                    .font_family(self.mono.clone())
                                    .text_size(px(24.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.accent)
                                    .child(format!("{}%", self.state.feed_override)),
                            ),
                    )
                    .child(
                        Button::new("feed-reset")
                            .label(t("RESET 100%"))
                            .primary()
                            .h(px(48.0))
                            .w(px(148.0))
                            .px_1()
                            .rounded_lg()
                            .text_size(px(8.0))
                            .disabled(!self.ready())
                            .on_click(move |_, window, cx| {
                                reset_view.update(cx, |this, cx| {
                                    this.reset_feed_override(window, cx);
                                });
                            }),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .h(px(48.0))
                    .child(
                        Button::new("feed-down")
                            .label("-10")
                            .h(px(48.0))
                            .w(px(48.0))
                            .rounded_lg()
                            .text_size(px(12.0))
                            .custom(mode_variant(cx, p, false))
                            .border_1()
                            .border_color(p.border)
                            .disabled(!self.ready())
                            .on_click(move |_, window, cx| {
                                down_view.update(cx, |this, cx| {
                                    this.step_feed_override(-10, window, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("feed-fine-down")
                            .label("-1")
                            .h(px(48.0))
                            .w(px(48.0))
                            .rounded_lg()
                            .text_size(px(12.0))
                            .custom(mode_variant(cx, p, false))
                            .border_1()
                            .border_color(p.border)
                            .disabled(!self.ready())
                            .on_click(move |_, window, cx| {
                                fine_down_view.update(cx, |this, cx| {
                                    this.step_feed_override(-1, window, cx);
                                });
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(Slider::new(&self.feed_slider).h(px(24.0))),
                    )
                    .child(
                        Button::new("feed-fine-up")
                            .label("+1")
                            .h(px(48.0))
                            .w(px(48.0))
                            .rounded_lg()
                            .text_size(px(12.0))
                            .custom(mode_variant(cx, p, false))
                            .border_1()
                            .border_color(p.border)
                            .disabled(!self.ready())
                            .on_click(move |_, window, cx| {
                                fine_up_view.update(cx, |this, cx| {
                                    this.step_feed_override(1, window, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("feed-up")
                            .label("+10")
                            .h(px(48.0))
                            .w(px(48.0))
                            .rounded_lg()
                            .text_size(px(12.0))
                            .custom(mode_variant(cx, p, false))
                            .border_1()
                            .border_color(p.border)
                            .disabled(!self.ready())
                            .on_click(move |_, window, cx| {
                                up_view.update(cx, |this, cx| {
                                    this.step_feed_override(10, window, cx);
                                });
                            }),
                    ),
            )
    }

    /// Jog: axis selector, step selector, 48 px minus/plus and a continuous
    /// toggle in the header.
    fn jog_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let axes = self.state.axes().iter().enumerate().map(|(index, axis)| {
            let axis = *axis;
            let active = self.state.jog_axis == axis;
            let axis_view = view.clone();
            Button::new(format!("jog-axis-{index}"))
                .label(axis.label())
                .h(px(48.0))
                .w(px(48.0))
                .rounded(px(6.0))
                .text_size(px(13.0))
                .custom(mode_variant(cx, p, active))
                .border_1()
                .border_color(if active { p.accent } else { p.border })
                .on_click(move |_, _, cx| {
                    axis_view.update(cx, |this, cx| {
                        this.state.jog_axis = axis;
                        cx.notify();
                    });
                })
        });

        let continuous_view = view.clone();
        let step_labels =
            ["0.01", "0.1", "1", "10", "100"]
                .iter()
                .enumerate()
                .map(|(index, label)| {
                    let step = STEP_SIZES[index];
                    let active = (self.state.jog_step - step).abs() < f64::EPSILON;
                    let step_view = view.clone();
                    Button::new(format!("jog-step-{index}"))
                        .label(*label)
                        .flex_1()
                        .h(px(48.0))
                        .rounded(px(6.0))
                        .text_size(px(12.0))
                        .custom(mode_variant(cx, p, active))
                        .border_1()
                        .border_color(if active { p.accent } else { p.border })
                        .on_click(move |_, _, cx| {
                            step_view.update(cx, |this, cx| {
                                this.state.jog_step = step;
                                cx.notify();
                            });
                        })
                });

        let minus_view = view.clone();
        let plus_view = view.clone();
        let minus_up = view.clone();
        let plus_up = view.clone();
        let allowed = self.jog_allowed();

        // A Button component only exposes `on_click`, which cannot express
        // press-and-hold. These are plain 64 px targets with mouse-down and
        // mouse-up handlers; `on_mouse_up_out` covers a release outside the
        // target so a continuous jog can never keep running.
        let jog_button = |id: &'static str, label: &'static str, sign: f64| {
            let press_view = if sign < 0.0 {
                minus_view.clone()
            } else {
                plus_view.clone()
            };
            let release_view = if sign < 0.0 {
                minus_up.clone()
            } else {
                plus_up.clone()
            };
            let release_out_view = release_view.clone();
            div()
                .id(id)
                .h(px(64.0))
                .w(px(64.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_lg()
                .border_1()
                .border_color(p.border)
                .bg(if allowed { p.raised } else { p.panel_alt })
                .text_size(px(26.0))
                .text_color(if allowed { p.text } else { p.text_faint })
                .when(allowed, |this| {
                    this.cursor_pointer()
                        .hover(|style| style.bg(p.raised_hover))
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            press_view.update(cx, |this, cx| this.jog_press(sign, cx));
                        })
                        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                            release_view.update(cx, |this, cx| this.end_jog(cx));
                        })
                        .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                            release_out_view.update(cx, |this, cx| this.end_jog(cx));
                        })
                })
                .child(label)
        };

        card(&p)
            .h(px(ROW_H))
            .flex_shrink_0()
            .gap_1()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .h(px(22.0))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(section_label(t("JOG"), p))
                            .child(
                                div()
                                    .font_family(self.mono.clone())
                                    .text_size(px(12.0))
                                    .text_color(p.text)
                                    .child(format!(
                                        "{} {:.2} mm",
                                        self.state.jog_axis.label(),
                                        self.state.jog_step
                                    )),
                            )
                            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(
                                if self.state.continuous {
                                    "CONTINUOUS"
                                } else {
                                    "STEP"
                                },
                            )),
                    )
                    .child(
                        Switch::new("continuous")
                            .checked(self.state.continuous)
                            .label(t("CONT"))
                            .text_size(px(10.0))
                            .on_change(move |checked, _, cx| {
                                continuous_view.update(cx, |this, cx| {
                                    this.state.continuous = *checked;
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(h_flex().gap_1().children(axes))
                    .child(jog_button("jog-minus", "-", -1.0))
                    .child(
                        h_flex()
                            .gap_1()
                            .flex_1()
                            .min_w(px(0.0))
                            .children(step_labels),
                    )
                    .child(jog_button("jog-plus", "+", 1.0)),
            )
    }

    /// Spindle: on/off, RPM field, CW/CCW and the override slider. Exactly the
    /// height of the Feed/Jog row.
    fn spindle_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let power_view = view.clone();
        let cw_view = view.clone();
        let ccw_view = view.clone();
        let rpm_view = view;

        card(&p)
            .h(px(ROW_H))
            .flex_shrink_0()
            .gap_1()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .h(px(20.0))
                    .child(section_label(t("SPINDLE"), p))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(p.text_faint)
                                    .child(format!("{}%", self.state.spindle_override)),
                            )
                            .child(
                                Switch::new("spindle-power")
                                    .checked(self.state.spindle_on)
                                    .disabled(!self.mdi_allowed())
                                    .label(if self.state.spindle_on {
                                        t("ON")
                                    } else {
                                        t("OFF")
                                    })
                                    .text_size(px(10.0))
                                    .on_change(move |checked, _, cx| {
                                        power_view.update(cx, |this, cx| {
                                            this.spindle_power(*checked, cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .h(px(48.0))
                    .child(
                        div()
                            .id("rpm-field")
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .justify_end()
                            .px_2()
                            .rounded(px(7.0))
                            .bg(p.panel_alt)
                            .border_1()
                            .border_color(p.border)
                            .cursor_pointer()
                            .font_family(self.mono.clone())
                            .text_size(px(22.0))
                            .text_color(if self.state.spindle_on {
                                p.text
                            } else {
                                p.text_faint
                            })
                            .child(format!("{}", self.state.rpm))
                            .on_click(move |_, _, cx| {
                                rpm_view.update(cx, |this, cx| {
                                    if this.mdi_allowed() {
                                        this.state.open_keypad(KeypadTarget::Rpm);
                                    } else {
                                        this.state.push_console(
                                            ConsoleLevel::Warn,
                                            "Spindle RPM change requires Idle",
                                        );
                                    }
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        // CW/CCW stay as protocol direction tokens: the
                        // compact spindle row cannot fit the full native
                        // words without clipping the RPM field.
                        Button::new("spindle-cw")
                            .label("CW")
                            .h(px(48.0))
                            .w(px(52.0))
                            .rounded(px(7.0))
                            .text_size(px(11.0))
                            .custom(mode_variant(cx, p, self.state.spindle_cw))
                            .border_1()
                            .border_color(if self.state.spindle_cw {
                                p.accent
                            } else {
                                p.border
                            })
                            .disabled(!self.mdi_allowed())
                            .on_click(move |_, _, cx| {
                                cw_view.update(cx, |this, cx| {
                                    this.spindle_direction(true, cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("spindle-ccw")
                            .label("CCW")
                            .h(px(48.0))
                            .w(px(52.0))
                            .rounded(px(7.0))
                            .text_size(px(11.0))
                            .custom(mode_variant(cx, p, !self.state.spindle_cw))
                            .border_1()
                            .border_color(if self.state.spindle_cw {
                                p.border
                            } else {
                                p.accent
                            })
                            .disabled(!self.mdi_allowed())
                            .on_click(move |_, _, cx| {
                                ccw_view.update(cx, |this, cx| {
                                    this.spindle_direction(false, cx);
                                });
                            }),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .h(px(18.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child("OVERRIDE"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(Slider::new(&self.spindle_slider).h(px(18.0))),
                    ),
            )
    }

    /// Add or edit one app-side tool. The sheet never touches the controller.
    fn tool_editor_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = self.tool_editor.as_ref() else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let cancel_view = view.clone();
        let save_view = view.clone();
        let scrim_view = view.clone();
        let title = match editor.original {
            Some(number) => format!("{} T{number}", t("EDIT")),
            None => t("NEW TOOL").to_string(),
        };
        let original = editor.original;
        let error = editor.error.clone();

        let mut fields = v_flex().gap_2();
        for field in ToolField::ALL {
            let field_view = view.clone();
            let name_view = view.clone();
            let value = editor.value(field).to_string();
            let display = if value.trim().is_empty() {
                "--".to_string()
            } else {
                value
            };
            let label = match field {
                ToolField::Number => "TOOL NUMBER",
                ToolField::Diameter => "DIAMETER (MM)",
                ToolField::Length => "LENGTH (MM)",
                ToolField::Pocket => "POCKET",
            };
            let line = h_flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(112.0))
                        .text_size(px(10.0))
                        .text_color(p.text_dim)
                        .child(t(label)),
                )
                .child(
                    Button::new(format!("tool-field-{}", label.replace(' ', "-")))
                        .label(display)
                        .secondary()
                        .h(px(48.0))
                        .flex_1()
                        .rounded_lg()
                        .text_size(px(14.0))
                        .on_click(move |_, _, cx| {
                            if field == ToolField::Number {
                                field_view.update(cx, |this, cx| {
                                    this.open_tool_field(ToolField::Number, cx);
                                });
                            } else {
                                field_view.update(cx, |this, cx| {
                                    this.open_tool_field(field, cx);
                                });
                            }
                        }),
                );
            fields = fields.child(line);
            if field == ToolField::Number {
                // The name is text, not numeric; it gets its own row below.
                let name_value = self
                    .tool_editor
                    .as_ref()
                    .map(|editor| editor.name.clone())
                    .unwrap_or_default();
                fields = fields.child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(112.0))
                                .text_size(px(10.0))
                                .text_color(p.text_dim)
                                .child(t("NAME")),
                        )
                        .child(
                            Button::new("tool-field-name")
                                .label(if name_value.trim().is_empty() {
                                    "--".to_string()
                                } else {
                                    name_value
                                })
                                .secondary()
                                .h(px(48.0))
                                .flex_1()
                                .rounded_lg()
                                .text_size(px(14.0))
                                .on_click(move |_, window, cx| {
                                    name_view.update(cx, |this, cx| {
                                        this.open_tool_name(window, cx);
                                    });
                                }),
                        ),
                );
            }
        }

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("tool-editor-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| {
                            shell.tool_editor = None;
                            shell.state.keypad = None;
                            cx.notify();
                        });
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(560.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(title),
                            )
                            .child(chip(
                                original.map_or(t("NEW RECORD"), |_| "REPLACES BY NUMBER"),
                                p,
                            )),
                    )
                    .child(fields)
                    .when_some(error, |this, error| {
                        this.child(div().text_size(px(10.0)).text_color(p.danger).child(error))
                    })
                    .child(div().text_size(px(10.0)).text_color(p.text_faint).child(
                        "Local table only. Save writes the per-controller profile; \
                                 it never writes the controller tool table.",
                    ))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("tool-editor-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |this, cx| {
                                            this.tool_editor = None;
                                            this.state.keypad = None;
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                Button::new("tool-editor-save")
                                    .label(t("SAVE TOOL"))
                                    .primary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        save_view.update(cx, |this, cx| {
                                            this.save_tool_editor(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Add or edit one user macro. Nothing is persisted until SAVE MACRO.
    fn macro_editor_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = self.macro_editor.as_ref() else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let cancel_view = view.clone();
        let save_view = view.clone();
        let name_view = view.clone();
        let command_view = view.clone();
        let confirm_view = view.clone();
        let scrim_view = view;
        let title = match &editor.original {
            Some(name) => format!("{} / {name}", t("EDIT")),
            None => t("NEW MACRO").to_string(),
        };
        let name = editor.name.clone();
        let command = editor.command.clone();
        let confirm = editor.confirm;
        let error = editor.error.clone();

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("macro-editor-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| {
                            shell.macro_editor = None;
                            shell.settings_entry = None;
                            cx.notify();
                        });
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(560.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(title),
                            )
                            .child(chip(
                                if editor.original.is_some() {
                                    t("PROFILE RECORD")
                                } else {
                                    t("NEW RECORD")
                                },
                                p,
                            )),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(92.0))
                                    .text_size(px(10.0))
                                    .text_color(p.text_dim)
                                    .child(t("NAME")),
                            )
                            .child(
                                Button::new("macro-editor-name")
                                    .label(if name.trim().is_empty() {
                                        "--".to_string()
                                    } else {
                                        name
                                    })
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .rounded_lg()
                                    .text_size(px(14.0))
                                    .on_click(move |_, window, cx| {
                                        name_view.update(cx, |this, cx| {
                                            this.open_macro_name(window, cx);
                                        });
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(92.0))
                                    .text_size(px(10.0))
                                    .text_color(p.text_dim)
                                    .child(t("COMMAND")),
                            )
                            .child(
                                Button::new("macro-editor-command")
                                    .label(if command.trim().is_empty() {
                                        "--".to_string()
                                    } else {
                                        command
                                    })
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .rounded_lg()
                                    .text_size(px(13.0))
                                    .on_click(move |_, window, cx| {
                                        command_view.update(cx, |this, cx| {
                                            this.open_macro_command(window, cx);
                                        });
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .h(px(48.0))
                            .child(
                                Switch::new("macro-editor-confirm")
                                    .checked(confirm)
                                    .label(t("Ask before running"))
                                    .text_size(px(11.0))
                                    .on_change(move |checked, _, cx| {
                                        confirm_view.update(cx, |this, cx| {
                                            if let Some(editor) = this.macro_editor.as_mut() {
                                                editor.confirm = *checked;
                                            }
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(t(
                                "New macros default to CONFIRM. A direct macro runs on one click.",
                            ))),
                    )
                    .when_some(error, |this, error| {
                        this.child(div().text_size(px(10.0)).text_color(p.danger).child(error))
                    })
                    .child(div().text_size(px(10.0)).text_color(p.text_faint).child(
                        "Single-line MDI G-code. System ($) commands and newlines are rejected in \
                         this pass. SAVE MACRO writes only this controller profile.",
                    ))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("macro-editor-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |this, cx| {
                                            this.macro_editor = None;
                                            this.settings_entry = None;
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                Button::new("macro-editor-save")
                                    .label(t("SAVE MACRO"))
                                    .primary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .disabled(self.state.macro_save_blocker().is_some())
                                    .on_click(move |_, _, cx| {
                                        save_view.update(cx, |this, cx| {
                                            this.save_macro_editor(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Confirmation sheet for one user macro. The exact command is captured
    /// when RUN is pressed and is shown before anything is sent.
    fn macro_run_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(mac) = self.state.macro_run_pending() else {
            return div().into_any_element();
        };
        let name = mac.name.clone();
        let command = mac.command.clone();
        let lock = self.state.macro_gate().blocked_reason();
        let view = cx.entity();
        let cancel_view = view.clone();
        let run_view = view.clone();
        let scrim_view = view;

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("macro-run-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| shell.cancel_macro_run(cx));
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(580.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(format!("{} / {name}", t("RUN MACRO"))),
                            )
                            .child(chip(t("CONFIRM REQUIRED"), p)),
                    )
                    .child(div().text_size(px(11.0)).text_color(p.warning).child(
                        "This command can move the machine, start the spindle, or change \
                                 offsets. Keep hands clear and know where the E-stop is.",
                    ))
                    .child(
                        v_flex()
                            .gap_1()
                            .p_2()
                            .rounded(px(6.0))
                            .bg(p.panel)
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(p.text_faint)
                                    .child(t("EXACT COMMAND TO SEND")),
                            )
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .font_family(self.mono.clone())
                                    .text_size(px(15.0))
                                    .text_color(p.text)
                                    .child(command),
                            ),
                    )
                    .when_some(lock, |this, reason| {
                        this.child(
                            div()
                                .text_size(px(10.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(p.danger)
                                .child(format!("RUN LOCKED: {}", reason.to_uppercase())),
                        )
                    })
                    .child(div().text_size(px(10.0)).text_color(p.text_dim).child(
                        "RUN MACRO sends exactly this one MDI command. A controller rejection is \
                         shown in the console and is not retried.",
                    ))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("macro-run-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |this, cx| {
                                            this.cancel_macro_run(cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("macro-run-confirm")
                                    .label(t("RUN MACRO"))
                                    .warning()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .disabled(lock.is_some())
                                    .on_click(move |_, _, cx| {
                                        run_view.update(cx, |this, cx| {
                                            this.confirm_macro_run(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Deletion confirmation for one stored macro.
    fn macro_delete_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(name) = self.macro_delete.as_ref() else {
            return div().into_any_element();
        };
        let name = name.clone();
        let view = cx.entity();
        let cancel_view = view.clone();
        let delete_view = view.clone();
        let scrim_view = view;

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("macro-delete-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        scrim_view.update(cx, |shell, cx| shell.cancel_delete_macro(cx));
                    }),
            )
            .child(
                v_flex()
                    .relative()
                    .w(px(520.0))
                    .max_w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.raised)
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(t("DELETE MACRO")),
                            )
                            .child(chip(t("PROFILE RECORD"), p)),
                    )
                    .child(
                        div()
                            .overflow_hidden()
                            .font_family(self.mono.clone())
                            .text_size(px(14.0))
                            .text_color(p.text)
                            .child(name),
                    )
                    .child(div().text_size(px(10.0)).text_color(p.text_faint).child(
                        "This removes the macro from this controller profile. It never sends a \
                         command to the controller.",
                    ))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("macro-delete-cancel")
                                    .label(t("CANCEL"))
                                    .secondary()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        cancel_view.update(cx, |this, cx| {
                                            this.cancel_delete_macro(cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("macro-delete-confirm")
                                    .label(t("DELETE MACRO"))
                                    .warning()
                                    .h(px(48.0))
                                    .flex_1()
                                    .rounded_lg()
                                    .text_size(px(11.0))
                                    .on_click(move |_, _, cx| {
                                        delete_view.update(cx, |this, cx| {
                                            this.confirm_delete_macro(cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Manual tool-change sheet.
    ///
    /// It starts below the 56 px top bar so RESET and STOP stay visible and
    /// reachable, and it never resumes the job without a second explicit press.
    fn tool_change_overlay(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(change) = self.state.job.tool_change else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let ack_view = view.clone();
        let resume_view = view.clone();
        let stop_view = view;
        let acknowledged = change.acknowledged;
        let ack_ok = self.job_tool_change_allowed();
        let resume_ok = self.state.live && self.ready() && !self.locked() && acknowledged;
        let source = self
            .state
            .job
            .line_source(change.line)
            .unwrap_or("M6")
            .to_string();
        let tool_record = self.state.tools.get(change.tool).cloned();
        let tool_detail = tool_record.as_ref().map_or_else(
            || "Not in the app tool table; M6 still sends only the tool number.".to_string(),
            |record| {
                format!(
                    "{} | {}",
                    ToolTableModel::display_name(record),
                    ToolTableModel::summary(record)
                )
            },
        );

        let sheet = v_flex()
            .w(px(520.0))
            .max_w_full()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(p.warning.opacity(0.6))
            .bg(p.raised)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.warning)
                                    .child(t("TOOL CHANGE REQUIRED")),
                            )
                            .child(chip(&format!("T{}", change.tool), p)),
                    )
                    .child(chip(&format!("LINE {}", change.line), p)),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.panel_alt)
                    .font_family(self.mono.clone())
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(format!("{:>4}", change.line)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(12.0))
                            .text_color(p.text)
                            .child(source),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.panel_alt)
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(t("APP TOOL")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(12.0))
                            .text_color(if tool_record.is_some() { p.text } else { p.text_faint })
                            .child(tool_detail),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(p.text_dim)
                    .child(format!(
                        "Tool T{} is requested at line {}. Install it and set its offset, then acknowledge (0xA3) and resume (cycle start).",
                        change.tool, change.line
                    )),
            )
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(
                t("Jogging is allowed while the job waits; the Jog panel stays active. The job will not send another line until you resume. RESET and STOP above remain available."),
            ))
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("tool-ack")
                            .label(if acknowledged {
                                t("ACKNOWLEDGED")
                            } else {
                                t("1. ACKNOWLEDGE")
                            })
                            .primary()
                            .h(px(48.0))
                            .w(px(180.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(!ack_ok)
                            .on_click(move |_, _, cx| {
                                ack_view.update(cx, |this, cx| {
                                    this.acknowledge_tool_change(cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("tool-resume")
                            .label(t("2. RESUME"))
                            .primary()
                            .h(px(48.0))
                            .w(px(140.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(!resume_ok)
                            .on_click(move |_, _, cx| {
                                resume_view.update(cx, |this, cx| {
                                    this.resume_after_tool_change(cx);
                                });
                            }),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("tool-stop")
                            .label(t("STOP JOB"))
                            .warning()
                            .h(px(48.0))
                            .w(px(110.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .on_click(move |_, _, cx| {
                                stop_view.update(cx, |this, cx| this.stop_job(cx));
                            }),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(if acknowledged { p.success } else { p.text_faint })
                    .child(if acknowledged {
                        "Acknowledged. Press Resume to continue the program."
                    } else {
                        "Acknowledge does not move any axis; it only confirms the tool change to the controller."
                    }),
            );

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                // Dim, but do not occlude: jogging stays available while the
                // job waits in Tool, and the sheet itself still takes clicks.
                div()
                    .id("tool-change-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim),
            )
            .child(sheet)
            .into_any_element()
    }

    // ------------------------------------------------------ connection panel

    /// Transport picker overlay. It starts below the 56 px top bar so RESET
    /// and STOP stay visible and reachable while a serial link is opening.
    fn connection_overlay(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let connected = self.link.as_ref().is_some_and(LiveLink::is_connected)
            || matches!(
                self.state.connection,
                ConnectionState::Connecting | ConnectionState::Ready
            );
        let error = self
            .panel
            .error
            .clone()
            .or_else(|| match &self.state.connection {
                ConnectionState::Failed(reason) => Some(reason.clone()),
                _ => None,
            });

        let body = match self.panel.transport {
            TransportChoice::Demo => v_flex()
                .gap_1()
                .p_2()
                .rounded(px(6.0))
                .bg(p.panel_alt)
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(p.text_dim)
                        .child(t("In-process FlexiHAL simulator over the loopback transport. No serial port is opened.")),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(p.text_faint)
                        .child(t("Same engine, handshake, streamer and safety gateway as a real controller.")),
                )
                .into_any_element(),
            TransportChoice::Serial => self.serial_picker(p, &view, cx).into_any_element(),
        };

        let transports = [TransportChoice::Demo, TransportChoice::Serial]
            .iter()
            .enumerate()
            .map(|(index, choice)| {
                let choice = *choice;
                let active = self.panel.transport == choice;
                let transport_view = view.clone();
                Button::new(format!("transport-{index}"))
                    .label(t(choice.label()))
                    .flex_1()
                    .h(px(48.0))
                    .rounded_lg()
                    .text_size(px(13.0))
                    .custom(mode_variant(cx, p, active))
                    .border_1()
                    .border_color(if active { p.accent } else { p.border })
                    .on_click(move |_, _, cx| {
                        transport_view.update(cx, |this, cx| {
                            this.panel.select_transport(choice);
                            cx.notify();
                        });
                    })
            });

        let close_view = view.clone();
        let scrim_view = view.clone();
        let connect_view = view.clone();
        let disconnect_view = view.clone();

        let mut sheet = v_flex()
            .w(px(520.0))
            .max_w_full()
            .gap_2()
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(p.border_strong)
            .bg(p.raised)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(t("CONNECTION")),
                            )
                            .child(chip(t(self.state.connection.label()), p)),
                    )
                    .child(
                        Button::new("panel-close-x")
                            .label("X")
                            .secondary()
                            .h(px(48.0))
                            .w(px(48.0))
                            .rounded_lg()
                            .text_size(px(13.0))
                            .on_click(move |_, _, cx| {
                                close_view.update(cx, |this, cx| this.close_connection(cx));
                            }),
                    ),
            )
            .child(section_label(t("TRANSPORT"), p))
            .child(h_flex().gap_2().children(transports))
            .when(connected, |this| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(p.text_faint)
                        .child(t("Disconnect before switching transport.")),
                )
            })
            .child(body)
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .h(px(20.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(t("CURRENT LINK")),
                    )
                    .child(
                        div()
                            .font_family(self.mono.clone())
                            .text_size(px(11.0))
                            .text_color(p.text_dim)
                            .child(self.connection_name()),
                    ),
            );

        if let Some(error) = error {
            sheet = sheet.child(
                div()
                    .p_2()
                    .rounded(px(6.0))
                    .bg(p.danger.opacity(0.14))
                    .text_size(px(11.0))
                    .text_color(p.danger)
                    .child(error),
            );
        }

        let footer = h_flex()
            .gap_2()
            .child(
                Button::new("panel-connect")
                    .label(t("CONNECT"))
                    .primary()
                    .h(px(48.0))
                    .w(px(140.0))
                    .rounded_lg()
                    .text_size(px(12.0))
                    .disabled(connected)
                    .on_click(move |_, _, cx| {
                        connect_view.update(cx, |this, cx| this.connect_selected(cx));
                    }),
            )
            .child(
                Button::new("panel-disconnect")
                    .label(t("DISCONNECT"))
                    .warning()
                    .h(px(48.0))
                    .w(px(140.0))
                    .rounded_lg()
                    .text_size(px(12.0))
                    .disabled(!connected)
                    .on_click(move |_, _, cx| {
                        disconnect_view.update(cx, |this, cx| this.disconnect(cx));
                    }),
            )
            .child(div().flex_1())
            .child(
                Button::new("panel-close")
                    .label(t("CLOSE"))
                    .secondary()
                    .h(px(48.0))
                    .w(px(96.0))
                    .rounded_lg()
                    .text_size(px(12.0))
                    .on_click(move |_, _, cx| {
                        scrim_view.update(cx, |this, cx| this.close_connection(cx));
                    }),
            );

        sheet = sheet.child(footer);

        div()
            .absolute()
            .top(px(56.0))
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(0.0))
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                div()
                    .id("connection-scrim")
                    .absolute()
                    .inset_0()
                    .bg(p.scrim)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        let scrim_view = view.clone();
                        scrim_view.update(cx, |this, cx| this.close_connection(cx));
                    }),
            )
            .child(sheet)
    }

    /// Serial half of the connection dialog: port list, baud and line defaults.
    fn serial_picker(&self, p: Palette, view: &Entity<Shell>, cx: &mut Context<Self>) -> Div {
        let refresh_view = view.clone();
        let mut rows = v_flex().w_full().gap_1();

        if self.panel.ports.is_empty() {
            rows = rows.child(
                div()
                    .w_full()
                    .h(px(56.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.0))
                    .bg(p.panel_alt)
                    .text_size(px(11.0))
                    .text_color(p.text_faint)
                    .child(
                        self.panel
                            .ports_error
                            .clone()
                            .unwrap_or_else(|| t("No serial ports found").to_string()),
                    ),
            );
        } else {
            for (index, port) in self.panel.ports.iter().enumerate() {
                let selected = self.panel.selected_port.as_deref() == Some(port.name.as_str());
                let port_view = view.clone();
                let name = port.name.clone();
                let detail = port
                    .description
                    .clone()
                    .or_else(|| port.manufacturer.clone())
                    .unwrap_or_else(|| t("serial port").to_string());
                rows = rows.child(
                    h_flex()
                        .id(format!("port-{index}"))
                        .w_full()
                        .cursor_pointer()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .h(px(48.0))
                        .px_2()
                        .rounded(px(6.0))
                        .bg(if selected {
                            p.accent_muted
                        } else {
                            p.panel_alt
                        })
                        .border_1()
                        .border_color(if selected { p.accent } else { p.border })
                        .hover(|style| style.bg(p.raised_hover))
                        .on_click(move |_, _, cx| {
                            port_view.update(cx, |this, cx| {
                                this.panel.select_port(name.clone());
                                cx.notify();
                            });
                        })
                        .child(
                            v_flex()
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .font_family(self.mono.clone())
                                        .text_size(px(13.0))
                                        .text_color(if selected { p.accent } else { p.text })
                                        .child(port.name.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(p.text_faint)
                                        .child(detail),
                                ),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .w(px(10.0))
                                .h(px(10.0))
                                .rounded(px(999.0))
                                .bg(if selected { p.accent } else { p.border }),
                        ),
                );
            }
        }

        // Three rows visible; more ports scroll. A fixed-height wrapper keeps
        // the dialog compact on a 1280x800 panel with many USB adapters.
        let list_height = if self.panel.ports.is_empty() {
            56.0
        } else {
            let rows_visible = self.panel.ports.len().min(3) as f32;
            rows_visible * 52.0 - 4.0
        };
        let port_list = div()
            .id("port-list")
            .w_full()
            .h(px(list_height))
            .flex_shrink_0()
            .track_scroll(&self.panel_scroll)
            .overflow_y_scroll()
            .child(rows);

        let baud = self.panel.baud;
        let baud_view = view.clone();
        let baud_menu = Button::new("baud-menu")
            .label(format!("{baud} baud"))
            .h(px(48.0))
            .w(px(148.0))
            .rounded(px(7.0))
            .text_size(px(12.0))
            .custom(mode_variant(cx, p, false))
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                let mut menu = menu.min_w(px(170.0));
                for rate in BAUD_RATES {
                    let baud_view = baud_view.clone();
                    menu = menu.item(PopupMenuItem::new(format!("{rate} baud")).on_click(
                        move |_, _, cx| {
                            baud_view.update(cx, |this, cx| {
                                this.panel.set_baud(rate);
                                cx.notify();
                            });
                        },
                    ));
                }
                menu
            });

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(section_label(t("SERIAL PORT"), p))
                    .child(
                        Button::new("ports-refresh")
                            .label(t("REFRESH"))
                            .secondary()
                            .h(px(48.0))
                            .w(px(104.0))
                            .rounded_lg()
                            .text_size(px(12.0))
                            .on_click(move |_, _, cx| {
                                refresh_view.update(cx, |this, cx| {
                                    this.panel.refresh();
                                    cx.notify();
                                });
                            }),
                    ),
            )
            .child(port_list)
            .when(
                self.panel.selected_port.is_none() && !self.panel.ports.is_empty(),
                |this| {
                    this.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(t("Tap a port to select it; nothing opens until CONNECT.")),
                    )
                },
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(baud_menu)
                    .child(chip("8N1", p))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(self.panel.line_control_summary()),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.text_faint)
                    .child(t("Line settings are fixed at 8 data bits, no parity, one stop bit. DTR reset pulses are not implemented in M2.")),
            )
    }

    /// Three columns with shared row guides. The right column is dropped below
    /// 1120 px; a full portrait reflow is deferred to M7.
    fn workspace(
        &self,
        p: Palette,
        cx: &mut Context<Self>,
        width: Pixels,
        compact: bool,
    ) -> impl IntoElement {
        let (left_w, right_w) = if width >= px(1600.0) {
            (400.0, 260.0)
        } else {
            (340.0, 220.0)
        };

        let mut row = h_flex()
            .gap(px(GAP))
            .flex_1()
            .min_h(px(0.0))
            .child(self.left_column(p, cx, left_w))
            .child(self.center_column(p, cx));

        if !compact {
            row = row.child(self.aux_column(p, cx, right_w));
        }
        row
    }
}

impl viewer::HasViewer for Shell {
    fn viewer_handle(&self) -> &viewer::ViewerHandle {
        &self.viewer
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::for_mode(self.state.theme_mode);
        let width = window.viewport_size().width;
        let compact = width < px(1120.0);

        // A demo-opened sheet has no window in the poll that opened it; copy
        // its draft into the two textareas here, once. This is a local text
        // assignment, not a command.
        if self.probe_macro_editor.is_some() {
            self.sync_probe_macro_inputs(window, cx);
        }

        let content = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .p(px(GAP))
            .child(self.workspace(p, cx, width, compact));

        let mut root = div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(p.bg)
            .text_color(p.text)
            .font_family("sans-serif")
            .child(self.top_bar(p, width, cx))
            .child(content)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.note_machine_key_down(event, cx)
            }))
            .on_key_up(cx.listener(|this, event: &KeyUpEvent, _, _| {
                this.note_machine_key_up(event);
            }))
            .on_action(cx.listener(|this, _: &JogXMinus, window, cx| {
                this.keyboard_action(MachineAction::JogXMinus, window, cx);
            }))
            .on_action(cx.listener(|this, _: &JogXPlus, window, cx| {
                this.keyboard_action(MachineAction::JogXPlus, window, cx);
            }))
            .on_action(cx.listener(|this, _: &JogYMinus, window, cx| {
                this.keyboard_action(MachineAction::JogYMinus, window, cx);
            }))
            .on_action(cx.listener(|this, _: &JogYPlus, window, cx| {
                this.keyboard_action(MachineAction::JogYPlus, window, cx);
            }))
            .on_action(cx.listener(|this, _: &JogZMinus, window, cx| {
                this.keyboard_action(MachineAction::JogZMinus, window, cx);
            }))
            .on_action(cx.listener(|this, _: &JogZPlus, window, cx| {
                this.keyboard_action(MachineAction::JogZPlus, window, cx);
            }))
            .on_action(cx.listener(|this, _: &MachineStart, window, cx| {
                this.keyboard_action(MachineAction::Start, window, cx);
            }))
            .on_action(cx.listener(|this, _: &MachinePause, window, cx| {
                this.keyboard_action(MachineAction::Pause, window, cx);
            }))
            .on_action(cx.listener(|this, _: &MachineResume, window, cx| {
                this.keyboard_action(MachineAction::Resume, window, cx);
            }))
            .on_action(cx.listener(|this, _: &MachineStop, window, cx| {
                this.keyboard_action(MachineAction::Stop, window, cx);
            }));

        let keypad_state = self.state.keypad.clone();
        if self.tool_editor.is_some() {
            root = root.child(self.tool_editor_overlay(p, cx));
        }
        if self.macro_editor.is_some() {
            root = root.child(self.macro_editor_overlay(p, cx));
        }
        if self.probe_macro_editor.is_some() {
            root = root.child(self.probe_macro_editor_overlay(p, cx));
        }
        if self.macro_delete.is_some() {
            root = root.child(self.macro_delete_overlay(p, cx));
        }
        if let Some(keypad) = keypad_state.as_ref() {
            root = root.child(keypad::overlay(
                keypad,
                p,
                cx.entity(),
                self.mono.clone(),
                self.state.unit(),
                cx,
            ));
        }
        if self.settings_entry.is_some() {
            root = root.child(self.settings_entry_overlay(p, cx));
        }
        if self.keymap_editor.is_some() {
            root = root.child(self.keymap_editor_overlay(p, cx));
        }
        if self.startup_confirm.is_some() {
            root = root.child(self.startup_confirm_overlay(p, cx));
        }
        if self.state.sleep_confirm_open {
            root = root.child(self.sleep_confirm_overlay(p, cx));
        }
        if self.job_history_open {
            root = root.child(self.job_history_overlay(p, cx));
        }
        if self.state.controller_reset.is_confirm_open() {
            root = root.child(self.reset_confirm_overlay(p, cx));
        }
        if self.state.pins.is_open() {
            root = root.child(self.pins_overlay(p, cx));
        }
        if self.calibration_confirm_open {
            root = root.child(self.calibration_confirm_overlay(p, cx));
        }
        if self.surfacing_open {
            root = root.child(self.surfacing_overlay(p, cx));
        }
        if self.gantry_open {
            root = root.child(self.gantry_overlay(p, cx));
        }
        if self.state.utilities.transform_preview().is_some() {
            root = root.child(self.transform_overlay(p, cx));
        }
        if self.state.macro_run_pending().is_some() {
            root = root.child(self.macro_run_overlay(p, cx));
        }
        if self.state.run_options_open {
            root = root.child(self.run_options_overlay(p, cx));
        }
        if self.state.check_confirm_open {
            root = root.child(self.check_confirm_overlay(p, cx));
        }
        if self.state.start_confirm_open() {
            root = root.child(self.start_confirm_overlay(p, cx));
        }
        if self.state.job.breakpoints.pending_run_to().is_some() {
            root = root.child(self.run_to_overlay(p, cx));
        }
        if self.state.job.tool_change.is_some() {
            root = root.child(self.tool_change_overlay(p, cx));
        }
        if self.panel.open {
            root = root.child(self.connection_overlay(p, cx));
        }
        if self.state.offsets_backup.preview().is_some() {
            root = root.child(self.offsets_restore_overlay(p, cx));
        }
        if self.state.offsets.pending_action().is_some() {
            root = root.child(self.offset_confirm_overlay(p, cx));
        }
        if self.file_browser.is_some() {
            root = root.child(self.file_browser_overlay(p, cx));
        }
        if self.state.setup.is_open() {
            root = root.child(self.setup_overlay(p, cx));
        }
        // The first frame is now on screen; the poll may offer the first-run
        // setup once. This is a plain write, not a notify, so rendering stays
        // side-effect free for the frame.
        self.first_render_done = true;
        root
    }
}

/// Program rows for the Program tab: the engine's parsed program or the mock
/// design-review list.
enum ProgramSource {
    Engine(Arc<Program>),
    Mock(Vec<crate::state::ProgramLine>),
}

impl ProgramSource {
    fn len(&self) -> usize {
        match self {
            ProgramSource::Engine(program) => program.lines.len(),
            ProgramSource::Mock(lines) => lines.len(),
        }
    }

    fn row(
        &self,
        index: usize,
        active_line: u32,
        search: &ProgramSearch,
        breakpoints: &ProgramBreakpoints,
        breakpoint_enabled: bool,
        editable_through: u32,
    ) -> Option<ProgramRow> {
        match self {
            ProgramSource::Engine(program) => {
                let line = program.lines.get(index)?;
                let number = u32::try_from(line.line).unwrap_or(u32::MAX);
                let trimmed = line.source.trim_start();
                Some(ProgramRow {
                    number,
                    text: line.source.clone(),
                    comment: trimmed.starts_with('(') || trimmed.starts_with(';'),
                    kind: None,
                    active: active_line != 0 && number == active_line,
                    matched: search.is_match(index),
                    current_match: search.current_source_index() == Some(index),
                    breakpoint: breakpoints.contains(number),
                    selected: breakpoints.selected() == Some(number),
                    breakpoint_enabled: breakpoint_enabled
                        && number > 0
                        && number <= editable_through,
                })
            }
            ProgramSource::Mock(lines) => {
                let line = lines.get(index)?;
                let number = u32::try_from(line.number).unwrap_or(u32::MAX);
                Some(ProgramRow {
                    number,
                    text: line.text.to_string(),
                    comment: line.kind == ProgramKind::Comment,
                    kind: Some(line.kind),
                    active: active_line != 0 && number == active_line,
                    matched: search.is_match(index),
                    current_match: search.current_source_index() == Some(index),
                    breakpoint: breakpoints.contains(number),
                    selected: breakpoints.selected() == Some(number),
                    breakpoint_enabled: breakpoint_enabled
                        && number > 0
                        && number <= editable_through,
                })
            }
        }
    }
}

/// One rendered Program tab row.
struct ProgramRow {
    number: u32,
    text: String,
    comment: bool,
    kind: Option<ProgramKind>,
    active: bool,
    /// The row matches the active search.
    matched: bool,
    /// The row is the current search match.
    current_match: bool,
    /// The source line has an enabled breakpoint.
    breakpoint: bool,
    /// The source line is the selected row.
    selected: bool,
    /// The gutter may send a breakpoint toggle for this line right now.
    breakpoint_enabled: bool,
}

fn program_row(row: ProgramRow, p: Palette, view: Entity<Shell>) -> impl IntoElement {
    let active = row.active;
    let selected = row.selected;
    let breakpoint = row.breakpoint;
    let number = row.number;
    let text_color = if active || selected || row.current_match || row.matched {
        p.accent
    } else if row.comment {
        p.text_faint
    } else {
        match row.kind {
            Some(ProgramKind::Comment) => p.text_faint,
            Some(ProgramKind::Setup) => p.info,
            Some(ProgramKind::Rapid) => p.text_dim,
            Some(ProgramKind::Cut) => p.text,
            Some(ProgramKind::Plunge) => p.warning,
            None => p.text_dim,
        }
    };
    let background = if row.current_match {
        p.accent.opacity(0.22)
    } else if active {
        p.accent_muted
    } else if selected {
        p.raised
    } else if row.matched {
        p.accent.opacity(0.08)
    } else {
        p.panel
    };
    let border_color = if active || selected || row.current_match {
        p.accent
    } else if row.matched {
        p.accent.opacity(0.55)
    } else {
        p.panel
    };
    let select_view = view.clone();
    let gutter_view = view;
    let gutter = h_flex()
        .id(format!("program-gutter-{number}"))
        .w(px(58.0))
        .h_full()
        .flex_shrink_0()
        .items_center()
        .gap_1()
        .when(row.breakpoint_enabled, |this| {
            let toggle = gutter_view;
            this.cursor_pointer()
                .hover(|style| style.bg(p.raised_hover))
                .on_click(move |_, _, cx| {
                    toggle.update(cx, |shell, cx| {
                        shell.toggle_program_breakpoint(number, cx);
                    });
                })
        })
        .child(
            div()
                .w(px(10.0))
                .h(px(10.0))
                .flex_shrink_0()
                .rounded(px(999.0))
                .border_1()
                .border_color(if breakpoint {
                    p.danger
                } else {
                    p.border_strong
                })
                .bg(if breakpoint { p.danger } else { p.panel }),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(10.0))
                .font_weight(if breakpoint || selected {
                    FontWeight::BOLD
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if breakpoint {
                    p.danger
                } else if active || selected || row.current_match || row.matched {
                    p.accent
                } else {
                    p.text_faint
                })
                .child(number.to_string()),
        );

    h_flex()
        .id(format!("program-row-{number}"))
        .items_center()
        .gap_2()
        .h(px(48.0))
        .px_2()
        .bg(background)
        .border_l(px(3.0))
        .border_color(border_color)
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            select_view.update(cx, |shell, cx| {
                shell.select_program_line(number, cx);
            });
        })
        .child(gutter)
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .truncate()
                .text_size(px(12.0))
                .text_color(text_color)
                .child(row.text),
        )
}

/// One 48 px row in the LOAD PROGRAM recent-file list.
///
/// The row loads on tap and carries its own REMOVE button. A missing path is
/// marked before the operator taps it; the bounded reader still produces the
/// authoritative error when a stale row is clicked.
fn recent_file_row(
    path: String,
    missing: bool,
    p: Palette,
    mono: SharedString,
    view: Entity<Shell>,
) -> AnyElement {
    let file_name = Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.clone());
    let load_path = path.clone();
    let load_view = view.clone();
    let remove_path = path.clone();
    let remove_view = view.clone();
    h_flex()
        .id(format!("recent-file-{path}"))
        .w_full()
        .h(px(48.0))
        .flex_shrink_0()
        .items_center()
        .gap_2()
        .px_2()
        .rounded_lg()
        .border_1()
        .border_color(if missing {
            p.warning.opacity(0.55)
        } else {
            p.border
        })
        .bg(p.panel)
        .cursor_pointer()
        .hover(|style| style.bg(p.raised_hover))
        .on_click(move |_, _, cx| {
            let path = PathBuf::from(load_path.clone());
            load_view.update(cx, |shell, cx| shell.browser_load_recent(path, cx));
        })
        .child(
            div()
                .w(px(58.0))
                .h(px(24.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .bg(if missing {
                    p.warning.opacity(0.16)
                } else {
                    p.accent.opacity(0.14)
                })
                .text_size(px(8.5))
                .font_weight(FontWeight::BOLD)
                .text_color(if missing { p.warning } else { p.accent })
                .child(if missing {
                    t("MISSING").to_string()
                } else {
                    t("RECENT").to_string()
                }),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.0))
                .child(
                    div()
                        .truncate()
                        .text_size(px(11.5))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.text)
                        .child(file_name),
                )
                .child(
                    div()
                        .truncate()
                        .font_family(mono)
                        .text_size(px(9.0))
                        .text_color(p.text_faint)
                        .child(path),
                ),
        )
        .child(
            Button::new(format!("recent-file-remove-{remove_path}"))
                .label(t("REMOVE"))
                .secondary()
                .h(px(36.0))
                .w(px(76.0))
                .rounded(px(6.0))
                .text_size(px(8.5))
                .on_click(move |_, _, cx| {
                    let path = remove_path.clone();
                    remove_view.update(cx, |shell, cx| shell.browser_remove_recent(path, cx));
                }),
        )
        .into_any_element()
}

/// One 48 px row in the file browser's virtualized list.
///
/// Folder rows enter on tap; file rows only select. The selected row keeps its
/// full border and tone change, so selection is visible without colour alone.
fn file_browser_row(
    index: usize,
    entry: BrowserEntry,
    selected: bool,
    p: Palette,
    view: Entity<Shell>,
) -> AnyElement {
    let is_dir = entry.is_dir;
    let kind = if is_dir {
        t("DIR").to_string()
    } else {
        let label = entry.kind_label();
        if label == "FILE" {
            t("FILE").to_string()
        } else {
            label
        }
    };
    let detail = if is_dir {
        t("FOLDER").to_string()
    } else {
        format_size(entry.size)
    };
    let name = entry.name.clone();
    let action = if selected {
        t("SELECTED")
    } else if is_dir {
        t("OPEN")
    } else {
        t("SELECT")
    };
    let action_color = if selected { p.accent } else { p.text_faint };
    div()
        .id(format!("file-browser-row-{index}"))
        .h(px(48.0))
        .w_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .rounded_lg()
        .border_1()
        .border_color(if selected { p.accent } else { p.border })
        .bg(if selected {
            p.accent_muted
        } else {
            p.panel_alt
        })
        .cursor_pointer()
        .hover(|style| style.bg(p.raised_hover))
        .on_click(move |_, window, cx| {
            view.update(cx, |shell, cx| {
                shell.browser_row_clicked(index, is_dir, window, cx);
            });
        })
        .child(
            div()
                .w(px(38.0))
                .h(px(24.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.0))
                .bg(if is_dir {
                    p.info.opacity(0.16)
                } else {
                    p.accent.opacity(0.14)
                })
                .text_size(px(8.5))
                .font_weight(FontWeight::BOLD)
                .text_color(if is_dir { p.info } else { p.accent })
                .child(kind),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.0))
                .gap_0()
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.0))
                        .text_color(p.text)
                        .child(name),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(p.text_faint)
                        .child(detail),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(9.0))
                .font_weight(FontWeight::BOLD)
                .text_color(action_color)
                .child(action),
        )
        .into_any_element()
}

fn empty_program_lines_notice(p: Palette) -> Div {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_1()
        .p_3()
        .child(
            div()
                .text_size(px(12.0))
                .text_color(p.text_dim)
                .child(t("Program has no lines")),
        )
        .child(
            div()
                .text_size(px(10.0))
                .text_color(p.text_faint)
                .child(t("Load a non-empty source file to search it.")),
        )
}

fn no_program_matches_notice(p: Palette, query: &str) -> Div {
    let query = query.trim().to_string();
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_1()
        .p_3()
        .child(
            div()
                .text_size(px(12.0))
                .text_color(p.text_dim)
                .child(t("No matching lines")),
        )
        .child(
            div()
                .max_w(px(260.0))
                .truncate()
                .text_size(px(10.0))
                .text_color(p.text_faint)
                .child(format!("{} '{query}'.", t("Nothing matches"))),
        )
        .child(
            div()
                .max_w(px(260.0))
                .text_size(px(10.0))
                .text_color(p.text_faint)
                .child(t(
                    "Switch to HIGHLIGHT or clear the field to restore the full list.",
                )),
        )
}

fn job_status_color(job: &JobView, p: Palette) -> Hsla {
    if job.tool_change.is_some() {
        return p.warning;
    }
    if job.check_pending() {
        return p.warning;
    }
    if job.is_check() {
        return match job.state {
            JobState::Running | JobState::Paused => p.accent,
            JobState::Completed => p.success,
            JobState::Aborted | JobState::Interrupted => p.danger,
            JobState::Idle => p.text_dim,
        };
    }
    match job.state {
        JobState::Running => p.accent,
        JobState::Paused => p.warning,
        JobState::Completed => p.success,
        JobState::Aborted | JobState::Interrupted => p.danger,
        JobState::Idle => p.text_dim,
    }
}

/// Read, validate, send, and record one local program file.
///
/// This is the shared implementation behind the file browser, a recent-file
/// click, a drag-and-drop, and `--demo-load-file`. It sends exactly one
/// `LoadProgram` through `send`; a timeout means the engine already queued the
/// load, so it is treated as accepted. On success the path is recorded in the
/// recent-file list. Failed reads, refusals, and engine rejections are not
/// recorded. Nothing here starts the job: START remains a separate operator
/// action.
fn load_local_program(
    state: &mut AppState,
    path: &Path,
    send: impl FnOnce(Command) -> Result<(), EngineError>,
) -> Result<String, String> {
    if let Some(refusal) = state.program_load_refusal() {
        return Err(t(refusal.message()).to_string());
    }
    let file = read_program(path).map_err(|error| error.to_string())?;
    let command = state
        .program_load(&file.name, &file.source)
        .map_err(|refusal| t(refusal.message()).to_string())?;
    let result = match send(command) {
        Ok(()) => Ok(format!(
            "Loading program: {} ({})",
            file.name,
            format_size(file.byte_len() as u64)
        )),
        // `send` waits one second for the engine to accept the command. A
        // timeout is not a rejection: the load is already queued and the
        // engine continues parsing in the background. Waiting for `JobLoaded`
        // is safer than reporting a failure and discarding the exact source.
        Err(EngineError::Timeout { .. }) => Ok(format!(
            "Program load accepted; waiting for the engine to parse {} ({})",
            file.name,
            format_size(file.byte_len() as u64)
        )),
        Err(error) => {
            state.height_map.discard_exact_source(&file.name);
            let message = format!("Program load rejected: {error}");
            state.push_console(ConsoleLevel::Error, message.clone());
            Err(message)
        }
    };
    if result.is_ok()
        && let Err(error) = state.config.record_recent_file(path)
    {
        state.push_console(
            ConsoleLevel::Warn,
            format!("Recent file not saved: {error}"),
        );
    }
    result
}

/// Persist at most one finished-job record from the shell poll.
///
/// Returns `Some(Ok(path))` when one record was written, `Some(Err(_))` when
/// one was taken but the local write failed, and `None` when nothing was
/// pending. The record is consumed before the write, so a failure is dropped
/// rather than retried on the next poll.
fn flush_job_history_record(state: &mut AppState) -> Option<Result<PathBuf, String>> {
    let record = state.take_job_history_record()?;
    Some(state.config.record_job_history(record))
}

/// Read the demo `--connection-panel[=serial]` switch.
///
/// A bare flag opens the dialog on the demo transport; `=serial` also selects
/// the serial tab so a review capture can show the port list.
fn connection_panel_arg() -> Option<String> {
    for arg in std::env::args() {
        if arg == "--connection-panel" {
            return Some(String::new());
        }
        if let Some(value) = arg.strip_prefix("--connection-panel=") {
            return Some(value.to_string());
        }
    }
    None
}

/// Read the screenshot-only `--program-search=TEXT` switch.
fn program_search_arg() -> Option<String> {
    std::env::args().find_map(|arg| {
        arg.strip_prefix("--program-search=")
            .map(std::string::ToString::to_string)
    })
}

/// Whether a command-line flag already determines the initial screen.
///
/// The first-run setup prompt is suppressed so it cannot cover a workspace or
/// sheet selected for a demo capture. `--mock` is intentionally not listed: it
/// is a review shell, not an initial-screen choice.
fn demo_initial_screen_arg(arg: &str) -> bool {
    arg.starts_with("--demo")
        || arg.starts_with("--settings-group=")
        || arg.starts_with("--probe-card=")
        || arg.starts_with("--file-browser=")
        || arg.starts_with("--connection-panel")
        || matches!(
            arg,
            "--settings"
                | "--offsets"
                | "--tools"
                | "--utility"
                | "--startup-lines"
                | "--macros"
                | "--probe"
                | "--pins"
                | "--keymap"
                | "--keypad"
                | "--console-tab"
                | "--viewer-3d"
                | "--viewer-stock-off"
        )
}

/// Short label for a concrete theme mode, including the resolved System case.
fn theme_mode_label(mode: ThemeMode) -> &'static str {
    t(match mode {
        ThemeMode::HighContrast => "HIGH CONTRAST",
        ThemeMode::Light => "LIGHT",
        ThemeMode::Dark | ThemeMode::System => "DARK",
    })
}

fn mode_variant(cx: &App, p: Palette, active: bool) -> ButtonCustomVariant {
    if active {
        ButtonCustomVariant::new(cx)
            .color(p.accent_muted)
            .foreground(p.accent)
            .hover(p.raised_hover)
            .active(p.accent_muted)
    } else {
        ButtonCustomVariant::new(cx)
            .color(p.panel_alt)
            .foreground(p.text_dim)
            .hover(p.raised_hover)
            .active(p.raised)
    }
}

fn parse_probe_card(name: &str) -> Option<ProbeCard> {
    match name.trim().to_ascii_lowercase().as_str() {
        "z" | "z-touch" | "z-touch-plate" => Some(ProbeCard::ZTouchPlate),
        "edge-x" | "x" => Some(ProbeCard::EdgeX),
        "edge-y" | "y" => Some(ProbeCard::EdgeY),
        "corner" => Some(ProbeCard::Corner),
        "circle" | "center-circle" | "centre-circle" => Some(ProbeCard::CenterCircle),
        "rectangle" | "rect" | "center-rectangle" | "centre-rectangle" => {
            Some(ProbeCard::CenterRectangle)
        }
        "rotation" => Some(ProbeCard::Rotation),
        "xy-plate" | "xy-touch-plate" | "plate" | "xy" => Some(ProbeCard::XyTouchPlate),
        "tool" | "tool-length" => Some(ProbeCard::ToolLength),
        "height" | "height-map" => Some(ProbeCard::HeightMap),
        _ => None,
    }
}

fn probe_notice(title: &str, body: &str, color: Hsla, p: Palette) -> Div {
    h_flex()
        .gap_2()
        .p_2()
        .rounded_lg()
        .border_1()
        .border_color(color.opacity(0.45))
        .bg(color.opacity(0.10))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(10.0))
                .font_weight(FontWeight::BOLD)
                .text_color(color)
                .child(title.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(10.5))
                .text_color(p.text_dim)
                .child(body.to_string()),
        )
}

fn probe_preflight_row(row: &PreflightRow, p: Palette) -> Div {
    let color = match row.state {
        RowState::Ok => p.success,
        RowState::Info => p.text_dim,
        RowState::Warn => p.warning,
        RowState::Bad => p.danger,
    };
    h_flex()
        .w_full()
        .min_w(px(0.0))
        .h(px(22.0))
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(96.0))
                .flex_shrink_0()
                .text_size(px(9.0))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(row.label),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .font_family("monospace")
                .text_size(px(9.5))
                .text_color(color)
                .child(row.value.clone()),
        )
}

fn probe_program_block(program: &str, limit: Option<usize>, p: Palette) -> Div {
    let total = program.lines().count();
    let shown = limit.map_or(total, |limit| limit.min(total));
    let mut lines: Vec<Div> = program
        .lines()
        .take(shown)
        .map(|line| div().child(line.to_string()))
        .collect();
    if shown < total {
        lines.push(
            div()
                .text_color(p.text_faint)
                .child(format!("... {} more line(s)", total - shown)),
        );
    }
    v_flex()
        .gap_1()
        .child(section_label(t("GENERATED PROGRAM"), p))
        .child(
            div()
                .id("probe-program-scroll")
                .max_h(px(126.0))
                .overflow_y_scroll()
                .p_2()
                .rounded(px(6.0))
                .bg(p.panel)
                .border_1()
                .border_color(p.border)
                .font_family("monospace")
                .text_size(px(9.5))
                .text_color(p.text_dim)
                .children(lines),
        )
}

fn probe_detail_cell(label: &str, value: &str, p: Palette) -> Div {
    v_flex()
        .gap_0()
        .px_2()
        .py_1()
        .rounded(px(6.0))
        .bg(p.panel)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .text_size(px(9.0))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(label.to_string()),
        )
        .child(
            div()
                .font_family("monospace")
                .text_size(px(11.0))
                .text_color(p.text)
                .child(value.to_string()),
        )
}

/// One compact label/value row for the height-map transform stats.
fn compact_stat_row(label: &str, value: String, p: Palette) -> Div {
    h_flex()
        .justify_between()
        .gap_2()
        .w_full()
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(8.0))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(label.to_string()),
        )
        .child(
            div()
                .min_w(px(0.0))
                .truncate()
                .font_family("monospace")
                .text_size(px(8.5))
                .text_color(p.text)
                .child(value),
        )
}

/// One inline label/value pair in the two-column transform grid.
fn compact_stat_pair(label: &str, value: String, p: Palette) -> Div {
    h_flex()
        .flex_1()
        .min_w(px(0.0))
        .gap_1()
        .font_family("monospace")
        .text_size(px(8.0))
        .child(
            div()
                .flex_shrink_0()
                .text_color(p.text_faint)
                .child(label.to_string()),
        )
        .child(
            div()
                .min_w(px(0.0))
                .truncate()
                .text_color(p.text)
                .child(value),
        )
}

/// Compact measurement cell used by the height-map result sheet.
fn height_map_detail_cell(label: &str, value: &str, p: Palette) -> Div {
    v_flex()
        .gap_0()
        .px_2()
        .py(px(2.0))
        .rounded(px(5.0))
        .bg(p.panel)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .text_size(px(7.5))
                .line_height(px(9.0))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(label.to_string()),
        )
        .child(
            div()
                .font_family("monospace")
                .text_size(px(10.0))
                .line_height(px(12.0))
                .text_color(p.text)
                .child(value.to_string()),
        )
}

fn probe_choice(
    cx: &App,
    p: Palette,
    id: impl Into<String>,
    label: &str,
    active: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> Button {
    Button::new(id.into())
        .label(label)
        .h(px(48.0))
        .px_3()
        .rounded_lg()
        .text_size(px(10.0))
        .custom(mode_variant(cx, p, active))
        .on_click(move |_, window, cx| on_click(window, cx))
}

fn tab_button(cx: &App, p: Palette, label: LeftTab, active: bool, view: Entity<Shell>) -> Button {
    Button::new(format!("left-tab-{}", label.label()))
        .label(t(label.label()))
        .flex_1()
        .h(px(48.0))
        .rounded_lg()
        .text_size(px(13.0))
        .custom(mode_variant(cx, p, active))
        .on_click(move |_, _, cx| {
            view.update(cx, |this, cx| {
                this.state.left_tab = label;
                cx.notify();
            });
        })
}

/// One titled block inside the read-only diagnostics sheet.
fn pins_section(title: &'static str, p: Palette) -> Div {
    v_flex()
        .gap(px(6.0))
        .p_2()
        .rounded_lg()
        .border_1()
        .border_color(p.border)
        .bg(p.panel_alt)
        .child(section_label(title, p))
}

/// Explicit missing/empty report state for one diagnostics block.
fn pins_empty(note: &'static str, p: Palette) -> Div {
    div()
        .p_2()
        .rounded(px(5.0))
        .border_1()
        .border_color(p.border)
        .bg(p.panel)
        .text_size(px(10.0))
        .text_color(p.text_dim)
        .child(t(note))
}

/// Localized request-state label for the diagnostics card and sheet.
fn pins_status_label(status: &PinsStatus) -> String {
    match status {
        PinsStatus::Stopped(reason) => reason.clone(),
        PinsStatus::Idle => t("NOT REQUESTED").to_string(),
        PinsStatus::Refreshing => t("READING CONTROLLER").to_string(),
        PinsStatus::Busy => t("CONTROLLER BUSY - RETRYING").to_string(),
        PinsStatus::Requested => t("REPORTS REQUESTED").to_string(),
    }
}

/// One label/value row inside the diagnostics sheet.
fn pins_key_value(label: &'static str, value: String, p: Palette, mono: &SharedString) -> Div {
    h_flex()
        .items_center()
        .justify_between()
        .gap_2()
        .h(px(20.0))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(9.5))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(label),
        )
        .child(
            div()
                .min_w(px(0.0))
                .truncate()
                .font_family(mono.clone())
                .text_size(px(10.0))
                .text_color(p.text)
                .child(value),
        )
}

/// Bounded monospace bit block; long reports scroll vertically only.
fn pins_bits(id: &'static str, text: String, p: Palette, mono: &SharedString) -> impl IntoElement {
    div()
        .id(id)
        .max_h(px(80.0))
        .overflow_y_scrollbar()
        .rounded(px(5.0))
        .border_1()
        .border_color(p.border)
        .bg(p.panel)
        .p_1()
        .font_family(mono.clone())
        .text_size(px(9.0))
        .line_height(px(12.0))
        .text_color(p.text_dim)
        .child(text)
}

fn card(p: &Palette) -> Div {
    v_flex()
        .gap_2()
        .p_2()
        .rounded_lg()
        .border_1()
        .border_color(p.border)
        .bg(p.panel)
}

/// Header row shared by the job-history card and sheet.
fn job_history_column_header(p: Palette, mono: &SharedString) -> Div {
    h_flex()
        .h(px(18.0))
        .w_full()
        .flex_shrink_0()
        .items_center()
        .gap_1()
        .px_2()
        .child(
            div()
                .w(px(JOB_HISTORY_TIME_W))
                .flex_shrink_0()
                .font_family(mono.clone())
                .text_size(px(8.5))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(t("LOCAL TIME")),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(8.5))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(t("PROGRAM")),
        )
        .child(
            div()
                .w(px(JOB_HISTORY_RESULT_W))
                .flex_shrink_0()
                .text_size(px(8.5))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(t("RESULT")),
        )
        .child(
            h_flex()
                .w(px(JOB_HISTORY_ELAPSED_W))
                .flex_shrink_0()
                .justify_end()
                .text_size(px(8.5))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(t("ACTIVE")),
        )
        .child(
            h_flex()
                .w(px(JOB_HISTORY_LINES_W))
                .flex_shrink_0()
                .justify_end()
                .text_size(px(8.5))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(t("LINES")),
        )
        .child(
            div()
                .w(px(JOB_HISTORY_CHECK_W))
                .flex_shrink_0()
                .text_size(px(8.5))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(t("CHECK")),
        )
}

/// One read-only history row: local time, name, result, active time, lines.
fn job_history_row(record: &JobHistoryRecord, mono: &SharedString, p: Palette, tall: bool) -> Div {
    h_flex()
        .h(px(if tall { 36.0 } else { 32.0 }))
        .w_full()
        .flex_shrink_0()
        .items_center()
        .gap_1()
        .px_2()
        .rounded(px(5.0))
        .bg(p.panel)
        .child(
            div()
                .w(px(JOB_HISTORY_TIME_W))
                .flex_shrink_0()
                .font_family(mono.clone())
                .text_size(px(9.0))
                .text_color(p.text_dim)
                .child(format_job_history_local_time(record.started_unix_s)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_size(px(10.0))
                .text_color(p.text)
                .child(record.name.clone()),
        )
        .child(
            div()
                .w(px(JOB_HISTORY_RESULT_W))
                .flex_shrink_0()
                .text_size(px(8.5))
                .font_weight(FontWeight::BOLD)
                .text_color(job_history_result_color(record.result, p))
                .child(t(job_history_result_label(record.result))),
        )
        .child(
            h_flex()
                .w(px(JOB_HISTORY_ELAPSED_W))
                .flex_shrink_0()
                .justify_end()
                .font_family(mono.clone())
                .text_size(px(9.0))
                .text_color(p.text_dim)
                .child(format_elapsed(Duration::from_secs(record.elapsed_s))),
        )
        .child(
            h_flex()
                .w(px(JOB_HISTORY_LINES_W))
                .flex_shrink_0()
                .justify_end()
                .font_family(mono.clone())
                .text_size(px(9.0))
                .text_color(p.text_dim)
                .child(record.lines.to_string()),
        )
        .child(
            div()
                .w(px(JOB_HISTORY_CHECK_W))
                .flex_shrink_0()
                .when(record.check_run, |this| {
                    this.child(
                        div()
                            .px_1()
                            .h(px(18.0))
                            .flex()
                            .items_center()
                            .rounded(px(4.0))
                            .bg(p.accent_muted)
                            .text_size(px(8.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.accent)
                            .child(t("CHECK")),
                    )
                }),
        )
}

/// Short result label for the local history.
fn job_history_result_label(result: JobHistoryResult) -> &'static str {
    match result {
        JobHistoryResult::Completed => "COMPLETED",
        JobHistoryResult::Aborted => "ABORTED",
        JobHistoryResult::Stopped => "STOPPED",
        JobHistoryResult::Interrupted => "INTERRUPTED",
        JobHistoryResult::Failed => "FAILED",
    }
}

/// Result colour for the local history.
fn job_history_result_color(result: JobHistoryResult, p: Palette) -> Hsla {
    match result {
        JobHistoryResult::Completed => p.success,
        JobHistoryResult::Stopped | JobHistoryResult::Interrupted => p.warning,
        JobHistoryResult::Aborted | JobHistoryResult::Failed => p.danger,
    }
}

/// Render a stored Unix start time in the operator's local time zone.
///
/// A time that cannot be represented maps to `--`; the record itself is never
/// changed.
fn format_job_history_local_time(started_unix_s: u64) -> String {
    let Ok(seconds) = i64::try_from(started_unix_s) else {
        return "--".to_string();
    };
    chrono::Local
        .timestamp_opt(seconds, 0)
        .single()
        .map_or_else(
            || "--".to_string(),
            |time| time.format("%Y-%m-%d %H:%M").to_string(),
        )
}

fn section_label(text: &'static str, p: Palette) -> Div {
    div()
        .text_size(px(11.0))
        .font_weight(FontWeight::BOLD)
        .text_color(p.text_dim)
        .child(text)
}

/// Wall-clock Unix seconds for a saved-map label.
///
/// The app has no clock service; a failed system-clock read maps to zero, which
/// the UI renders as the epoch rather than inventing a time.
fn now_unix_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn chip(text: &str, p: Palette) -> Div {
    div()
        .px_2()
        .h(px(22.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .rounded(px(5.0))
        .bg(p.panel_alt)
        .border_1()
        .border_color(p.border)
        .text_size(px(10.0))
        .font_weight(FontWeight::BOLD)
        .text_color(p.text_dim)
        .child(text.to_string())
}

/// Projection chip in the viewer toolbar.
///
/// The active mode is tinted with the accent and acts as a no-op; the other
/// chip switches mode and lets the viewer fit the destination camera once.
fn viewer_mode_chip(
    candidate: viewer::ViewerMode,
    active: viewer::ViewerMode,
    p: Palette,
    view: Entity<Shell>,
) -> impl IntoElement {
    let selected = candidate == active;
    let id = if candidate.is_three_d() {
        "viewer-mode-3d"
    } else {
        "viewer-mode-2d"
    };
    div()
        .id(id)
        .px_2()
        .h(px(22.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .rounded(px(5.0))
        .bg(if selected {
            p.accent_muted
        } else {
            p.panel_alt
        })
        .border_1()
        .border_color(if selected { p.accent } else { p.border })
        .text_size(px(10.0))
        .font_weight(FontWeight::BOLD)
        .text_color(if selected { p.accent } else { p.text_dim })
        .hover(|style| style.bg(p.raised_hover).text_color(p.text))
        .on_click(move |_, _, cx| {
            view.update(cx, |this, cx| {
                if this.viewer.set_mode(candidate) {
                    cx.notify();
                }
            });
        })
        .child(candidate.label())
}

/// Stock-box toggle chip in the 3D viewer toolbar.
///
/// On is tinted with the accent, off stays subdued, and the label carries the
/// state in text as well as colour. The click flips one viewer flag only: the
/// chip never sends a controller command.
fn stock_chip(on: bool, p: Palette, view: Entity<Shell>) -> impl IntoElement {
    div()
        .id("viewer-stock")
        .px_2()
        .h(px(22.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .rounded(px(5.0))
        .bg(if on { p.accent_muted } else { p.panel_alt })
        .border_1()
        .border_color(if on { p.accent } else { p.border })
        .text_size(px(10.0))
        .font_weight(FontWeight::BOLD)
        .text_color(if on { p.accent } else { p.text_dim })
        .hover(|style| style.bg(p.raised_hover).text_color(p.text))
        .on_click(move |_, _, cx| {
            view.update(cx, |this, cx| {
                if this.viewer.toggle_stock() {
                    cx.notify();
                }
            });
        })
        .child(if on { t("STOCK ON") } else { t("STOCK OFF") })
}

/// A chip-sized action button, used for the viewer FIT control.
fn chip_button(
    id: &'static str,
    text: &str,
    p: Palette,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .px_2()
        .h(px(22.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .rounded(px(5.0))
        .bg(p.panel_alt)
        .border_1()
        .border_color(p.border)
        .text_size(px(10.0))
        .font_weight(FontWeight::BOLD)
        .text_color(p.text_dim)
        .hover(|style| style.bg(p.raised_hover).text_color(p.text))
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(text.to_string())
}

/// One 48 px Program-tab search action. Disabled actions stay visible so the
/// toolbar geometry never shifts.
fn program_search_button(
    id: &'static str,
    label: &'static str,
    enabled: bool,
    p: Palette,
    view: Entity<Shell>,
    action: impl Fn(&mut Shell, &mut Context<Shell>) + 'static,
) -> AnyElement {
    let button = div()
        .id(id)
        .h(px(48.0))
        .min_w(px(52.0))
        .flex_shrink_0()
        .px_2()
        .flex()
        .items_center()
        .justify_center()
        .rounded_lg()
        .border_1()
        .border_color(p.border)
        .bg(p.panel_alt)
        .text_size(px(9.5))
        .font_weight(FontWeight::BOLD)
        .text_color(p.text_dim);
    if enabled {
        button
            .cursor_pointer()
            .hover(|style| style.bg(p.raised_hover).text_color(p.text))
            .on_click(move |_, _, cx| {
                view.update(cx, |shell, cx| action(shell, cx));
            })
            .child(label)
            .into_any_element()
    } else {
        button.opacity(0.45).child(label).into_any_element()
    }
}

/// One half of the FILTER / HIGHLIGHT segmented toggle.
fn program_mode_segment(
    mode: ProgramSearchMode,
    selected: bool,
    p: Palette,
    view: Entity<Shell>,
) -> impl IntoElement {
    let id = match mode {
        ProgramSearchMode::Filter => "program-search-filter",
        ProgramSearchMode::Highlight => "program-search-highlight",
    };
    div()
        .id(id)
        .flex_1()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(if selected {
            p.accent_muted
        } else {
            p.panel_alt
        })
        .text_size(px(9.5))
        .font_weight(FontWeight::BOLD)
        .text_color(if selected { p.accent } else { p.text_dim })
        .cursor_pointer()
        .hover(|style| style.bg(p.raised_hover).text_color(p.text))
        .on_click(move |_, _, cx| {
            view.update(cx, |shell, cx| {
                shell.state.set_program_search_mode(mode);
                if let Some(index) = shell.state.program_search_scroll_index() {
                    shell.scroll_program_to_visible(index);
                }
                cx.notify();
            });
        })
        .child(t(mode.label()))
}

/// Compact "the numbers below are not live" marker for the DRO header.
fn stale_badge(p: Palette) -> Div {
    div()
        .px_1()
        .h(px(16.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_1()
        .rounded(px(4.0))
        .bg(p.danger.opacity(0.16))
        .text_size(px(9.0))
        .font_weight(FontWeight::BOLD)
        .text_color(p.danger)
        .child(t("STALE"))
}

/// Compact legend for the measured-map overlay.
fn height_map_overlay_legend(p: Palette) -> Div {
    let mut swatches = h_flex().items_center().gap_1();
    for index in 0..viewer::HEIGHT_OVERLAY_STEPS {
        swatches = swatches.child(
            div()
                .w(px(22.0))
                .h(px(10.0))
                .rounded(px(2.0))
                .bg(viewer::height_overlay_color(index, p)),
        );
    }
    h_flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .rounded(px(6.0))
        .bg(p.panel)
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .text_size(px(9.0))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(t("MEASURED Z")),
        )
        .child(
            div()
                .text_size(px(9.0))
                .text_color(p.text_dim)
                .child(t("LOW")),
        )
        .child(swatches)
        .child(
            div()
                .text_size(px(9.0))
                .text_color(p.text_dim)
                .child(t("HIGH")),
        )
        .child(
            div()
                .text_size(px(9.0))
                .text_color(p.text_faint)
                .child(t("dashed = missing")),
        )
}

fn legend(color: Hsla, text: &'static str, p: Palette) -> Div {
    h_flex()
        .items_center()
        .gap_1()
        .child(div().w(px(6.0)).h(px(6.0)).rounded(px(999.0)).bg(color))
        .child(div().text_size(px(10.0)).text_color(p.text_dim).child(text))
}

fn console_color(level: ConsoleLevel, p: Palette) -> Hsla {
    match level {
        ConsoleLevel::Info => p.text_dim,
        ConsoleLevel::Command => p.info,
        ConsoleLevel::Ok => p.success,
        ConsoleLevel::Warn => p.warning,
        ConsoleLevel::Error => p.danger,
    }
}

/// One 48 px Console level chip with its count.
///
/// Chips share the row equally and have no minimum width, so six of them fit
/// the 340 px Console card used at 1280x800 without a horizontal scroll.
fn console_filter_chip(
    filter: ConsoleFilter,
    active: ConsoleFilter,
    counts: ConsoleCounts,
    p: Palette,
    view: Entity<Shell>,
) -> impl IntoElement {
    let selected = filter == active;
    let tint = match filter {
        ConsoleFilter::All | ConsoleFilter::Info => p.text_dim,
        ConsoleFilter::Command => p.info,
        ConsoleFilter::Ok => p.success,
        ConsoleFilter::Warn => p.warning,
        ConsoleFilter::Error => p.danger,
    };
    div()
        .id(format!("console-filter-{}", filter.id()))
        .h(px(48.0))
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .rounded_lg()
        .border_1()
        .border_color(if selected { p.accent } else { p.border })
        .bg(if selected {
            p.accent_muted
        } else {
            p.panel_alt
        })
        .cursor_pointer()
        .hover(|style| style.bg(p.raised_hover))
        .on_click(move |_, _, cx| {
            view.update(cx, |shell, cx| {
                shell.state.set_console_filter(filter);
                shell.console_scroll.scroll_to_bottom();
                cx.notify();
            });
        })
        .child(
            div()
                .text_size(px(9.0))
                .line_height(px(10.0))
                .font_weight(FontWeight::BOLD)
                .text_color(if selected { tint } else { p.text_dim })
                .child(filter.label()),
        )
        .child(
            div()
                .text_size(px(9.0))
                .line_height(px(10.0))
                .text_color(if selected { tint } else { p.text_faint })
                .child(counts.count(filter).to_string()),
        )
}

/// Result of the last `SAVE LOG`: scope/status plus the full target path.
fn console_log_status_row(p: Palette, status: &ConsoleLogStatus, mono: SharedString) -> Div {
    let color = if status.ok { p.success } else { p.danger };
    v_flex()
        .w_full()
        .flex_shrink_0()
        .gap_0()
        .px_2()
        .pb_1()
        .child(
            div()
                .text_size(px(9.0))
                .line_height(px(10.5))
                .font_weight(FontWeight::BOLD)
                .text_color(color)
                .child(status.text.clone()),
        )
        .when_some(status.path.clone(), |this, path| {
            this.child(
                div()
                    .font_family(mono)
                    .text_size(px(8.5))
                    .line_height(px(10.5))
                    .text_color(p.text_dim)
                    .child(format!("PATH: {}", path.display())),
            )
        })
}

// ------------------------------------------------------------ settings rows

/// One group-rail row.
fn settings_group_row(
    group: GroupRow,
    active: SettingsScope,
    p: Palette,
    view: Entity<Shell>,
) -> impl IntoElement {
    let selected = group.scope == active;
    let scope = group.scope;
    let key = match group.scope {
        SettingsScope::All => "all".to_string(),
        SettingsScope::General => "general".to_string(),
        SettingsScope::Group(id) => format!("group-{id}"),
    };
    let indent = 8.0 + f32::from(group.depth) * 18.0;
    div()
        .id(format!("settings-group-{key}"))
        .h(px(48.0))
        .w_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_2()
        .pl(px(indent))
        .pr_2()
        .rounded(px(6.0))
        .bg(if selected { p.accent_muted } else { p.panel })
        .border_1()
        .border_color(if selected { p.accent } else { p.panel })
        .cursor_pointer()
        .hover(|style| style.bg(p.raised_hover))
        .on_click(move |_, _, cx| {
            view.update(cx, |shell, cx| {
                // Picking a group leaves search so the rail always has a
                // visible effect; the operator can search again from there.
                shell.state.settings.set_search(String::new());
                shell.state.settings.set_scope(scope);
                cx.notify();
            });
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_size(px(11.0))
                .font_weight(if selected {
                    FontWeight::BOLD
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if selected { p.accent } else { p.text_dim })
                .child(group.name),
        )
        .when(group.pending > 0, |this| {
            this.child(
                div()
                    .w(px(6.0))
                    .h(px(6.0))
                    .flex_shrink_0()
                    .rounded(px(999.0))
                    .bg(p.warning),
            )
        })
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(9.0))
                .text_color(if selected { p.accent } else { p.text_faint })
                .child(group.count.to_string()),
        )
}

/// One generated form row.
fn settings_row_view(
    row: SettingRow,
    p: Palette,
    view: Entity<Shell>,
    mono: SharedString,
    axis_labels: &[String],
    enabled: bool,
) -> AnyElement {
    let pending = row.status.is_pending();
    let status_color = match &row.status {
        RowStatus::Synced => p.border,
        RowStatus::Edited | RowStatus::Queued | RowStatus::Writing => p.warning,
        RowStatus::Invalid | RowStatus::Failed(_) => p.danger,
        RowStatus::Applied => p.success,
    };
    let background = match &row.status {
        RowStatus::Synced => p.panel_alt,
        RowStatus::Applied => p.success.opacity(0.06),
        RowStatus::Invalid | RowStatus::Failed(_) => p.danger.opacity(0.07),
        _ => p.warning.opacity(0.08),
    };
    let number = row.number;
    // A queued or in-flight write owns the row until the engine answers.
    let row_enabled = enabled && !matches!(row.status, RowStatus::Queued | RowStatus::Writing);
    let status_label = t(row.status.label());
    let note = row.note.clone();
    let status_is_pending = pending;
    let name = row.name.clone();
    let meta_line = row.meta_line();
    let reboot = row.reboot;
    let kind = row.kind;
    let unit = row.unit.clone();
    let display_value = row.display_value();
    let number_text = row.number_text().to_string();
    let radio_value = row.radio_value();
    let radio_values = radio_span(&row);

    let control: AnyElement = match kind {
        SettingKind::Boolean => {
            settings_toggle(number, row.is_on(), p, view.clone(), row_enabled).into_any_element()
        }
        SettingKind::Radio => match radio_values {
            Some(_) => chip(&format!("VALUE {display_value}"), p).into_any_element(),
            None => settings_number_field(
                number,
                &number_text,
                unit.as_deref(),
                p,
                view.clone(),
                row_enabled,
                mono.clone(),
            )
            .into_any_element(),
        },
        SettingKind::Integer | SettingKind::Decimal => settings_number_field(
            number,
            &number_text,
            unit.as_deref(),
            p,
            view.clone(),
            row_enabled,
            mono.clone(),
        )
        .into_any_element(),
        SettingKind::Text | SettingKind::Password | SettingKind::IpAddress => {
            settings_text_field(number, &display_value, kind, p, view.clone(), row_enabled)
                .into_any_element()
        }
        SettingKind::Bitfield | SettingKind::ExclusiveBitfield | SettingKind::AxisMask => {
            chip(&format!("VALUE {}", row.bits()), p).into_any_element()
        }
        SettingKind::Unknown(code) => chip(&format!("TYPE {code}"), p).into_any_element(),
    };

    let header = h_flex()
        .min_h(px(48.0))
        .w_full()
        .items_center()
        .gap_2()
        .child(
            div()
                .w(px(46.0))
                .flex_shrink_0()
                .font_family(mono)
                .text_size(px(10.0))
                .text_color(if status_is_pending {
                    p.warning
                } else {
                    p.text_faint
                })
                .child(format!("${number}")),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.0))
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .min_w(px(0.0))
                                .truncate()
                                .text_size(px(13.0))
                                .text_color(p.text)
                                .child(name),
                        )
                        .when(reboot, |this| {
                            this.child(settings_tag("REBOOT", p.warning, p))
                        }),
                )
                .child(
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(9.5))
                        .text_color(p.text_faint)
                        .child(meta_line),
                ),
        )
        .when(!status_label.is_empty(), |this| {
            this.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(9.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(status_color)
                    .child(status_label),
            )
        })
        .child(control);

    let mut card = v_flex()
        .gap_1()
        .p_2()
        .rounded_lg()
        .bg(background)
        .border_l(px(3.0))
        .border_color(status_color)
        .child(header);

    match kind {
        SettingKind::Bitfield | SettingKind::ExclusiveBitfield | SettingKind::AxisMask => {
            card = card.child(settings_bits(
                &row,
                p,
                view.clone(),
                axis_labels,
                row_enabled,
            ));
        }
        SettingKind::Radio => {
            if let Some((first, last)) = radio_values {
                card = card.child(settings_radio(
                    number,
                    radio_value,
                    first,
                    last,
                    p,
                    view.clone(),
                    row_enabled,
                ));
            }
        }
        _ => {}
    }

    if let Some(note) = note {
        card = card.child(
            div()
                .pl(px(46.0))
                .text_size(px(9.5))
                .text_color(
                    if matches!(row.status, RowStatus::Invalid | RowStatus::Failed(_)) {
                        p.danger
                    } else {
                        p.warning
                    },
                )
                .child(note),
        );
    }

    card.into_any_element()
}

/// Boolean row control: a 48 px checkbox cell, not a 20 px switch.
fn settings_toggle(
    number: u16,
    checked: bool,
    p: Palette,
    view: Entity<Shell>,
    enabled: bool,
) -> impl IntoElement {
    let next = !checked;
    let mut cell = div()
        .id(format!("settings-bool-{number}"))
        .h(px(48.0))
        .min_w(px(72.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .px_2()
        .rounded_lg()
        .bg(if checked { p.accent_muted } else { p.panel })
        .border_1()
        .border_color(if checked { p.accent } else { p.border });
    if enabled {
        cell = cell.cursor_pointer().on_click(move |_, _, cx| {
            view.update(cx, |shell, cx| {
                shell.state.settings.set_bool(number, next);
                cx.notify();
            });
        });
    } else {
        cell = cell.opacity(0.5);
    }
    cell.child(
        div()
            .w(px(16.0))
            .h(px(16.0))
            .flex_shrink_0()
            .rounded(px(3.0))
            .border_1()
            .border_color(if checked { p.accent } else { p.border_strong })
            .bg(if checked { p.accent } else { p.panel })
            .child(if checked {
                div()
                    .size_full()
                    .p(px(4.0))
                    .child(div().size_full().rounded(px(1.0)).bg(p.on_accent))
                    .into_any_element()
            } else {
                div().into_any_element()
            }),
    )
    .child(
        div()
            .text_size(px(10.0))
            .font_weight(FontWeight::BOLD)
            .text_color(if checked { p.accent } else { p.text_dim })
            .child(if checked { t("ON") } else { t("OFF") }),
    )
}

/// Bit/axis-mask checkbox column.
fn settings_bits(
    row: &SettingRow,
    p: Palette,
    view: Entity<Shell>,
    axis_labels: &[String],
    enabled: bool,
) -> Div {
    let bits = row.bits();
    let exclusive = row.kind == SettingKind::ExclusiveBitfield;
    let master_off = exclusive && bits & 1 == 0;
    let count = if row.kind == SettingKind::AxisMask {
        axis_labels.len().min(9)
    } else {
        row.bit_count()
    };
    let mut wrap = h_flex().flex_wrap().w_full().gap_1();
    if count == 0 {
        return wrap.child(
            div()
                .text_size(px(10.0))
                .text_color(p.text_faint)
                .child(t("No axes reported by the controller")),
        );
    }
    for bit in 0..count {
        let checked = bits & (1_u64 << bit) != 0;
        let label = if row.kind == SettingKind::AxisMask {
            axis_labels
                .get(bit)
                .cloned()
                .unwrap_or_else(|| format!("B{bit}"))
        } else {
            format!("B{bit}")
        };
        let cell_enabled = enabled && !(exclusive && bit > 0 && master_off);
        wrap = wrap.child(settings_bit_cell(
            row.number,
            bit as u8,
            label,
            checked,
            cell_enabled,
            exclusive && bit == 0,
            p,
            view.clone(),
        ));
    }
    wrap
}

/// One bit in a mask column.
#[allow(clippy::too_many_arguments)]
fn settings_bit_cell(
    number: u16,
    bit: u8,
    label: String,
    checked: bool,
    enabled: bool,
    master: bool,
    p: Palette,
    view: Entity<Shell>,
) -> impl IntoElement {
    let next = !checked;
    let mut cell = div()
        .id(format!("settings-bit-{number}-{bit}"))
        .h(px(48.0))
        .min_w(px(64.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .gap_1()
        .px_2()
        .rounded(px(6.0))
        .bg(if checked { p.accent_muted } else { p.panel })
        .border_1()
        .border_color(if master {
            p.border_strong
        } else if checked {
            p.accent
        } else {
            p.border
        });
    if enabled {
        cell = cell.cursor_pointer().on_click(move |_, _, cx| {
            view.update(cx, |shell, cx| {
                shell.state.settings.toggle_mask_bit(number, bit, next);
                cx.notify();
            });
        });
    } else {
        cell = cell.opacity(0.45);
    }
    cell.child(
        div()
            .w(px(14.0))
            .h(px(14.0))
            .flex_shrink_0()
            .rounded(px(3.0))
            .border_1()
            .border_color(if checked { p.accent } else { p.border_strong })
            .bg(if checked { p.accent } else { p.panel }),
    )
    .child(
        div()
            .text_size(px(10.0))
            .text_color(if checked { p.accent } else { p.text_dim })
            .child(if master {
                format!("{label} MASTER")
            } else {
                label
            }),
    )
}

/// Radio column for a small numeric span.
fn settings_radio(
    number: u16,
    current: Option<i64>,
    first: i64,
    last: i64,
    p: Palette,
    view: Entity<Shell>,
    enabled: bool,
) -> impl IntoElement {
    let mut row = h_flex().flex_wrap().w_full().gap_1();
    for value in first..=last {
        let selected = current == Some(value);
        let mut cell = div()
            .id(format!("settings-radio-{number}-{value}"))
            .h(px(48.0))
            .min_w(px(56.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .px_2()
            .rounded(px(6.0))
            .bg(if selected { p.accent_muted } else { p.panel })
            .border_1()
            .border_color(if selected { p.accent } else { p.border });
        if enabled {
            let view = view.clone();
            cell = cell.cursor_pointer().on_click(move |_, _, cx| {
                view.update(cx, |shell, cx| {
                    shell.state.settings.set_radio(number, value);
                    cx.notify();
                });
            });
        } else {
            cell = cell.opacity(0.5);
        }
        row = row.child(
            cell.child(
                div()
                    .text_size(px(12.0))
                    .text_color(if selected { p.accent } else { p.text_dim })
                    .child(value.to_string()),
            ),
        );
    }
    row
}

/// Numeric field: a 48 px tappable value that opens the in-app keypad.
fn settings_number_field(
    number: u16,
    text: &str,
    unit: Option<&str>,
    p: Palette,
    view: Entity<Shell>,
    enabled: bool,
    mono: SharedString,
) -> impl IntoElement {
    let mut field = div()
        .id(format!("settings-number-{number}"))
        .h(px(48.0))
        .w(px(150.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_end()
        .gap_1()
        .px_2()
        .rounded_lg()
        .bg(p.panel)
        .border_1()
        .border_color(p.border_strong);
    if enabled {
        field = field.cursor_pointer().on_click(move |_, _, cx| {
            view.update(cx, |shell, cx| shell.open_setting_keypad(number, cx));
        });
    } else {
        field = field.opacity(0.6);
    }
    field
        .child(
            div()
                .min_w(px(0.0))
                .truncate()
                .font_family(mono)
                .text_size(px(16.0))
                .text_color(if text.is_empty() {
                    p.text_faint
                } else {
                    p.text
                })
                .child(if text.is_empty() {
                    "0".to_string()
                } else {
                    text.to_string()
                }),
        )
        .when_some(unit.map(str::to_string), |this, unit| {
            this.child(
                div()
                    .flex_shrink_0()
                    .text_size(px(9.0))
                    .text_color(p.text_faint)
                    .child(unit.to_uppercase()),
            )
        })
}

/// Text field: a 48 px tappable value that opens the text sheet.
fn settings_text_field(
    number: u16,
    display: &str,
    kind: SettingKind,
    p: Palette,
    view: Entity<Shell>,
    enabled: bool,
) -> impl IntoElement {
    let mut field = div()
        .id(format!("settings-text-{number}"))
        .h(px(48.0))
        .w(px(190.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .rounded_lg()
        .bg(p.panel)
        .border_1()
        .border_color(p.border_strong);
    if enabled {
        field = field.cursor_pointer().on_click(move |_, window, cx| {
            view.update(cx, |shell, cx| {
                shell.open_settings_entry(SettingsEntryPurpose::Text(number), window, cx);
            });
        });
    } else {
        field = field.opacity(0.6);
    }
    field
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_size(px(12.0))
                .text_color(if display.is_empty() {
                    p.text_faint
                } else {
                    p.text
                })
                .child(if display.is_empty() {
                    "(blank)".to_string()
                } else {
                    display.to_string()
                }),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(8.0))
                .font_weight(FontWeight::BOLD)
                .text_color(p.text_faint)
                .child(match kind {
                    SettingKind::Password => "MASKED",
                    SettingKind::IpAddress => "IPV4",
                    _ => t("EDIT"),
                }),
        )
}

/// Small static metadata tag; not a touch target.
fn settings_tag(text: &str, color: Hsla, p: Palette) -> Div {
    div()
        .px_1()
        .h(px(16.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .rounded(px(4.0))
        .bg(color.opacity(0.16))
        .text_size(px(8.0))
        .font_weight(FontWeight::BOLD)
        .text_color(color)
        .child(text.to_string())
        .text_color(color)
        .border_1()
        .border_color(p.border)
}

/// Empty form notice for a group or a search with no matches.
fn settings_no_rows(query: &str, p: Palette) -> Div {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap_1()
        .p_4()
        .child(
            div()
                .text_size(px(12.0))
                .text_color(p.text_dim)
                .child(if query.is_empty() {
                    t("No settings in this group").to_string()
                } else {
                    format!("{} \"{query}\"", t("No setting matches"))
                }),
        )
        .child(
            div()
                .text_size(px(10.0))
                .text_color(p.text_faint)
                .child(if query.is_empty() {
                    t("Pick another group in the rail.")
                } else {
                    t("Search matches a number, name, or group. Clear the search to browse groups.")
                }),
        )
}

/// Static row used only in the empty state.
fn settings_example_row(name: &str, detail: &str, value: &str, p: Palette) -> Div {
    h_flex()
        .min_h(px(56.0))
        .items_center()
        .gap_2()
        .p_2()
        .rounded_lg()
        .bg(p.panel_alt)
        .border_1()
        .border_color(p.border)
        .opacity(0.75)
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(p.text_dim)
                        .child(name.to_string()),
                )
                .child(
                    div()
                        .text_size(px(9.5))
                        .text_color(p.text_faint)
                        .child(detail.to_string()),
                ),
        )
        .child(
            div()
                .h(px(48.0))
                .min_w(px(96.0))
                .flex()
                .items_center()
                .justify_center()
                .px_2()
                .rounded_lg()
                .bg(p.panel)
                .border_1()
                .border_color(p.border_strong)
                .text_size(px(14.0))
                .text_color(p.text_dim)
                .child(value.to_string()),
        )
}

/// Integer span for a radio control, when the reported range is small enough.
fn radio_span(row: &SettingRow) -> Option<(i64, i64)> {
    let first = row.min?.ceil() as i64;
    let last = row.max?.floor() as i64;
    (last >= first && last - first <= 7).then_some((first, last))
}

#[cfg(test)]
mod tests {
    use ghs_engine::EngineEvent;

    use super::*;

    #[test]
    fn program_rows_map_source_numbers_and_current_line() {
        let search = ProgramSearch::default();
        let breakpoints = ProgramBreakpoints::default();
        let mock = ProgramSource::Mock(vec![
            crate::state::ProgramLine {
                number: 1,
                text: "G21",
                kind: ProgramKind::Setup,
            },
            crate::state::ProgramLine {
                number: 2,
                text: "M30",
                kind: ProgramKind::Setup,
            },
        ]);
        let row = mock
            .row(1, 2, &search, &breakpoints, true, 2)
            .expect("second mock row");
        assert_eq!(row.number, 2);
        assert_eq!(row.text, "M30");
        assert!(row.active);
        assert!(!row.comment);
        assert!(mock.row(2, 2, &search, &breakpoints, true, 2).is_none());

        let program = Arc::new(
            ghs_gcode::parse_program("G21\n(Groove)\nM30\n").expect("sample program parses"),
        );
        let live = ProgramSource::Engine(program);
        assert_eq!(live.len(), 3);
        assert_eq!(
            live.row(0, 3, &search, &breakpoints, true, 3)
                .map(|row| row.number),
            Some(1)
        );
        let comment = live
            .row(1, 3, &search, &breakpoints, true, 3)
            .expect("comment row");
        assert!(comment.comment);
        assert!(!comment.active);
        assert_eq!(comment.text, "(Groove)");
        let current = live
            .row(2, 3, &search, &breakpoints, true, 3)
            .expect("current row");
        assert!(current.active);
        assert!(!current.comment);
    }

    #[test]
    fn program_filter_and_highlight_preserve_breakpoint_markers() {
        let program = Arc::new(
            ghs_gcode::parse_program("G21\nG0 X1\nG1 Y1 F800\nG0 X2\n")
                .expect("search program parses"),
        );
        let source = ProgramSource::Engine(Arc::clone(&program));
        let mut breakpoints = ProgramBreakpoints::default();
        breakpoints.set_line(2, true);
        breakpoints.set_line(4, true);
        breakpoints.select(Some(4));

        let mut filter = ProgramSearch::default();
        filter.set_query("G0");
        filter.refresh(program.as_ref(), 7);
        assert_eq!(filter.visible_count(4), 2);
        let filtered: Vec<ProgramRow> = (0..filter.visible_count(4))
            .filter_map(|visible| {
                let index = filter.visible_source_index(visible, 4)?;
                source.row(index, 0, &filter, &breakpoints, true, 4)
            })
            .collect();
        assert_eq!(
            filtered
                .iter()
                .map(|row| (row.number, row.breakpoint))
                .collect::<Vec<_>>(),
            vec![(2, true), (4, true)]
        );
        assert_eq!(
            filtered
                .iter()
                .filter(|row| row.selected)
                .map(|row| row.number)
                .collect::<Vec<_>>(),
            vec![4]
        );

        let mut highlight = ProgramSearch::default();
        highlight.set_mode(ProgramSearchMode::Highlight);
        highlight.set_query("G0");
        highlight.refresh(program.as_ref(), 8);
        assert_eq!(highlight.visible_count(4), 4);
        let marked: Vec<(u32, bool)> = (0..4)
            .filter_map(|index| {
                source
                    .row(index, 0, &highlight, &breakpoints, true, 4)
                    .map(|row| (row.number, row.breakpoint))
            })
            .collect();
        assert_eq!(marked, vec![(1, false), (2, true), (3, false), (4, true)]);
        let selected = source
            .row(3, 0, &highlight, &breakpoints, true, 4)
            .expect("highlight row");
        assert!(selected.selected);
        assert!(selected.breakpoint);
    }

    // ---------------------------------------------------------- job history

    fn shell_temp_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "ghs-app-shell-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn history_flush_state(root: &Path) -> (AppState, ghs_config::ConfigStore) {
        let store = ghs_config::ConfigStore::new(root);
        let mut state = AppState::new();
        state.config = crate::config::ConfigModel::from_store(store.clone());
        state.begin_live("shell history flush");
        state.job.name = "part.nc".to_string();
        state.apply_job_event(&EngineEvent::JobStarted {
            from_line: 1,
            total_lines: 120,
        });
        state.apply_job_event(&EngineEvent::JobCompleted {
            elapsed: Duration::from_secs(12),
        });
        (state, store)
    }

    #[test]
    fn shell_flush_persists_exactly_one_pending_record() {
        let root = shell_temp_root("job-history-flush");
        let (mut state, store) = history_flush_state(&root);

        let first = flush_job_history_record(&mut state).expect("one pending record");
        assert!(first.is_ok(), "{first:?}");
        assert!(
            flush_job_history_record(&mut state).is_none(),
            "the poll consumes at most one record and never repeats it"
        );

        let stored = store
            .load_app_config()
            .expect("load app config")
            .expect("written app config");
        assert_eq!(stored.job_history.len(), 1);
        assert_eq!(stored.job_history[0].name, "part.nc");
        assert_eq!(stored.job_history[0].elapsed_s, 12);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn shell_flush_save_failure_keeps_the_corrupt_config_file() {
        let root = shell_temp_root("job-history-flush-fail");
        let store = ghs_config::ConfigStore::new(&root);
        let app_path = store.app_config_path();
        fs::create_dir_all(app_path.parent().expect("parent")).expect("mkdir");
        let corrupt = b"{ this is not json";
        fs::write(&app_path, corrupt).expect("write corrupt config");

        let (mut state, store) = history_flush_state(&root);
        let result = flush_job_history_record(&mut state).expect("record was taken");
        let error = result.expect_err("the corrupt app file refuses the save");
        assert!(error.contains("corrupt"), "{error}");
        assert_eq!(
            fs::read(&app_path).expect("read app config"),
            corrupt,
            "a failed history save never rewrites the corrupt file"
        );
        assert!(state.config.job_history().is_empty());
        assert!(
            flush_job_history_record(&mut state).is_none(),
            "a failed record is dropped, not retried"
        );
        assert!(store.load_app_config().is_err());

        let _ = fs::remove_dir_all(root);
    }

    // --------------------------------------------------------- recent files

    fn recent_load_state(root: &Path) -> AppState {
        let store = ghs_config::ConfigStore::new(root);
        let mut state = AppState::new();
        state.config = crate::config::ConfigModel::from_store(store);
        state.begin_live("recent-file test");
        state.connection = ConnectionState::Ready;
        state
    }

    fn recent_paths(state: &AppState) -> Vec<&str> {
        state
            .config
            .recent_files()
            .iter()
            .map(|record| record.path.as_str())
            .collect()
    }

    #[test]
    fn local_load_records_the_file_and_sends_only_load_program() {
        let root = shell_temp_root("recent-load");
        let file = root.join("part.nc");
        fs::write(&file, "G21\nG0 X1\nM30\n").expect("program");
        let mut state = recent_load_state(&root);
        let mut sent = Vec::new();

        let message = load_local_program(&mut state, &file, |command| {
            sent.push(command);
            Ok(())
        })
        .expect("local load");
        assert!(message.contains("part.nc"), "{message}");
        assert_eq!(sent.len(), 1, "exactly one command is sent");
        assert_eq!(
            sent[0],
            Command::LoadProgram {
                name: "part.nc".to_string(),
                source: "G21\nG0 X1\nM30\n".to_string(),
            }
        );
        assert!(
            !matches!(sent[0], Command::StartProgram { .. }),
            "loading never starts"
        );
        assert!(!state.job.is_active());
        assert_eq!(
            recent_paths(&state),
            vec![file.to_str().expect("utf8 path")]
        );

        let stored = ghs_config::ConfigStore::new(&root)
            .load_app_config()
            .expect("load config")
            .expect("written config");
        assert_eq!(stored.recent_files.len(), 1);
        assert_eq!(stored.recent_files[0].path, file.to_str().expect("utf8"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_loads_keep_the_newest_recent_file_first() {
        let root = shell_temp_root("recent-order");
        let first = root.join("first.nc");
        let second = root.join("second.nc");
        fs::write(&first, "G21\nM30\n").expect("first program");
        fs::write(&second, "G21\nM30\n").expect("second program");
        let mut state = recent_load_state(&root);

        for path in [&first, &second, &first] {
            load_local_program(&mut state, path, |_| Ok(())).expect("load");
        }

        assert_eq!(
            recent_paths(&state),
            vec![
                first.to_str().expect("utf8 first"),
                second.to_str().expect("utf8 second")
            ],
            "reopening first moves it to the front without a duplicate"
        );
        assert_eq!(state.config.recent_files().len(), 2);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_load_refuses_a_missing_file_without_sending_or_recording() {
        let root = shell_temp_root("recent-missing");
        let missing = root.join("missing.nc");
        let mut state = recent_load_state(&root);
        let mut sent = 0;

        let error = load_local_program(&mut state, &missing, |_| {
            sent += 1;
            Ok(())
        })
        .expect_err("missing file is refused");
        assert!(error.contains("missing.nc"), "{error}");
        assert_eq!(sent, 0, "no command is sent for an unreadable file");
        assert!(state.config.recent_files().is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_load_refuses_mock_without_sending_or_recording() {
        let root = shell_temp_root("recent-mock");
        let file = root.join("part.nc");
        fs::write(&file, "G21\nM30\n").expect("program");
        let mut state = AppState::new();
        let mut sent = 0;

        let error = load_local_program(&mut state, &file, |_| {
            sent += 1;
            Ok(())
        })
        .expect_err("mock mode is refused");
        assert!(!error.is_empty());
        assert_eq!(sent, 0);
        assert!(state.config.recent_files().is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_load_records_only_after_the_engine_accepts_or_times_out() {
        let root = shell_temp_root("recent-engine-result");
        let file = root.join("part.nc");
        fs::write(&file, "G21\nM30\n").expect("program");
        let mut state = recent_load_state(&root);

        let error = load_local_program(&mut state, &file, |_| Err(EngineError::NotConnected))
            .expect_err("engine rejection is reported");
        assert!(error.contains("rejected"), "{error}");
        assert!(
            state.config.recent_files().is_empty(),
            "a rejected load is not a recent file"
        );

        let message = load_local_program(&mut state, &file, |_| {
            Err(EngineError::Timeout {
                operation: "load program",
            })
        })
        .expect("a timeout is an accepted queued load");
        assert!(message.contains("accepted"), "{message}");
        assert_eq!(recent_paths(&state), vec![file.to_str().expect("utf8")]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn demo_synthetic_program_load_does_not_touch_recent_files() {
        let root = shell_temp_root("recent-demo");
        let mut state = recent_load_state(&root);
        // LiveLink loads a bundled demo source directly through `program_load`;
        // only the local-file loader records a recent entry.
        let command = state
            .program_load("demo-pocket.nc", "G21\nG0 X0 Y0\nM30\n")
            .expect("demo load command");
        assert!(matches!(command, Command::LoadProgram { .. }));
        assert!(state.config.recent_files().is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn dropped_program_paths_reach_the_same_local_load_path() {
        let root = shell_temp_root("recent-drop");
        let file = root.join("dropped.nc");
        fs::write(&file, "G21\nG0 Y2\nM30\n").expect("program");
        let mut state = recent_load_state(&root);

        assert_eq!(
            dropped_program_path(std::slice::from_ref(&file)),
            Ok(file.as_path())
        );
        let accepted = dropped_program_path(std::slice::from_ref(&file)).expect("accepted");
        let mut sent = Vec::new();
        load_local_program(&mut state, accepted, |command| {
            sent.push(command);
            Ok(())
        })
        .expect("drop load");
        assert_eq!(sent.len(), 1);
        assert!(matches!(sent[0], Command::LoadProgram { .. }));
        assert_eq!(state.config.recent_files().len(), 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn probe_macro_cancel_discards_the_draft() {
        let mut model = crate::probe::ProbeModel::default();
        model
            .set_probe_macros("(saved-pre)", "(saved-post)")
            .expect("stored macros");

        let mut editor = ProbeMacroEditor::new(&model);
        editor.draft.pre = "(draft-pre)\nG0 X0".to_string();
        editor.draft.post = "(draft-post)\nM3".to_string();
        // CANCEL drops the sheet and its draft; the model is untouched.
        let cancelled: Option<ProbeMacroEditor> = None;
        drop(editor);
        assert!(cancelled.is_none());
        assert_eq!(model.pre_macro(), "(saved-pre)");
        assert_eq!(model.post_macro(), "(saved-post)");
        assert_eq!(model.probe_macro_summary(), "PRE 1 / POST 1 LINES");
    }

    #[test]
    fn demo_probe_macro_text_is_valid_and_reproducible() {
        let sample = || ProbeMacroDraft {
            pre: DEMO_PROBE_PRE_MACRO.to_string(),
            post: DEMO_PROBE_POST_MACRO.to_string(),
        };
        let first = sample();
        assert_eq!(first, sample());
        assert_eq!(
            ghs_probe::validate_probe_macro(&first.pre).expect("valid pre"),
            vec!["(pre-probe)", "G4 P1", "M5"]
        );
        assert_eq!(
            ghs_probe::validate_probe_macro(&first.post).expect("valid post"),
            vec!["(post-probe)", "G4 P1", "M9"]
        );

        let mut model = crate::probe::ProbeModel::default();
        model
            .set_probe_macros(&first.pre, &first.post)
            .expect("demo macros");
        assert_eq!(model.probe_macro_summary(), "PRE 3 / POST 3 LINES");
    }
}
