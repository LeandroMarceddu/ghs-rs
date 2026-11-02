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
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ghs_config::{MAX_HEIGHT_MAPS, ToolRecord};
use ghs_engine::{
    Axes, Axis as CoreAxis, Command, EngineError, JobPauseReason, JobState, JogCommand,
    MachineState, Position, Program, RealtimeCommand, SerialConfig, Signals as CoreSignals,
    SystemCommand, Wcs,
};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable as _, Sizable as _, Size, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Anchor, AnyElement, App, AppContext as _, Context, Div, Entity, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Pixels, Render,
    ScrollHandle, ScrollStrategy, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Task, UniformListScrollHandle, Window, div, px, uniform_list,
};

use crate::connection::{BAUD_RATES, ConnectionPanel, TransportChoice};
use crate::height_compensation::{
    HeightApplyGate, HeightApplyPreview, HeightMapApplyStatus, HeightMapSnapshot,
};
use crate::height_map_library::{
    LoadedSavedMap, SavedMapSummary, default_map_name, format_unix_utc,
};
use crate::keypad;
use crate::live::{DemoProgram, LinkMode, LiveLink};
use crate::offsets::{OffsetAction, OffsetGate, OffsetStatus};
use crate::probe::{
    FieldProblem, PreflightRow, ProbeCard, ProbeContext, ProbeField, ProbePhase, ProbePreflight,
    ProbeRun, ProbeTarget, RowState, format_duration, format_estimate, format_mm,
    height_map_summary, outcome_summary,
};
use crate::settings::{GroupRow, RowStatus, SettingKind, SettingRow, SettingsScope};
use crate::startup::{StartupRequest, StartupStatus};
use crate::state::{
    AppState, Axis, ConnectionState, ConsoleLevel, JobView, KeypadAction, KeypadTarget, LeftTab,
    Mode, ProgramKind, SignalState, format_elapsed, pause_reason_label,
};
use crate::theme::Palette;
use crate::tools::{ControllerToolTable, ToolField, ToolTableModel};
use crate::viewer;

const STEP_SIZES: [f64; 5] = [0.01, 0.1, 1.0, 10.0, 100.0];

/// Height of the MDI/DRO and Feed/Jog/Spindle rows.
const ROW_H: f32 = 112.0;
/// Gap between cards and between columns.
const GAP: f32 = 12.0;
/// How often a held continuous jog re-issues its step.
const JOG_REPEAT: Duration = Duration::from_millis(120);

/// One press-and-hold jog gesture.
#[derive(Clone, Copy, Debug)]
struct JogHold {
    sign: f64,
    continuous: bool,
    started: std::time::Instant,
}

/// A release earlier than this is a click, not a hold: keep the one-step jog
/// instead of cancelling it. This also keeps a touch tap working, because GPUI
/// delivers a touch tap's mouse down and up together.
const JOG_CANCEL_AFTER: Duration = Duration::from_millis(250);

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
    program_scroll: UniformListScrollHandle,
    viewer: viewer::ViewerHandle,
    last_job_line: u32,
    demo_program: DemoProgram,
    demo_start: bool,
    demo_start_sent: bool,
    settings_entry: Option<SettingsEntry>,
    tool_editor: Option<ToolEditor>,
    startup_confirm: Option<StartupConfirm>,
    offset_refresh_sent: bool,
    backup_import_armed: bool,
    settings_demo_write: bool,
    settings_demo_write_done: bool,
    settings_demo_group: Option<String>,
    settings_demo_group_done: bool,
    demo_tools: bool,
    demo_tools_done: bool,
    demo_startup_write: bool,
    demo_startup_write_done: bool,
    demo_offsets_write: bool,
    demo_offsets_write_done: bool,
    demo_offsets_goto: bool,
    demo_offsets_goto_done: bool,
    probe_card_demo: Option<String>,
    probe_card_demo_done: bool,
    demo_probe_run: bool,
    demo_probe_run_done: bool,
    demo_probe_apply: bool,
    demo_probe_apply_done: bool,
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
    /// Toolpath captured before a height-map probe replaced the job program.
    height_map_base_program: Option<Arc<ghs_gcode::Program>>,
    _poll_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    pub fn new(cx: &mut Context<Self>) -> Self {
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
        // Demo-only switches used for screenshots and manual reviews. They do
        // not affect normal startup and never touch the controller.
        let forced_light = std::env::args().any(|arg| arg == "--light");
        state.dark = if forced_light {
            false
        } else {
            match state.config.app().theme {
                ghs_config::ThemeMode::Dark => true,
                ghs_config::ThemeMode::Light => false,
                ghs_config::ThemeMode::System => true,
            }
        };
        crate::theme::apply(state.dark, cx);
        let mock = std::env::args().any(|arg| arg == "--mock");
        if mock {
            state.connection = ConnectionState::Mock;
            state.push_console(
                ConsoleLevel::Info,
                "Design-review mock mode; no controller is attached",
            );
        }
        if std::env::args().any(|arg| arg == "--keypad") {
            state.open_keypad(KeypadTarget::Dro(Axis::X));
        }
        // Screenshot switch: open the generated settings workspace directly.
        if std::env::args().any(|arg| arg == "--settings") {
            state.mode = Mode::Settings;
        }

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
        let demo_tools = std::env::args().any(|arg| arg == "--demo-tools");
        let demo_startup_write = std::env::args().any(|arg| arg == "--demo-startup-write");
        let demo_offsets_write = std::env::args().any(|arg| arg == "--demo-offsets-write");
        let demo_offsets_goto = std::env::args().any(|arg| arg == "--demo-offsets-goto");
        let probe_card_demo = std::env::args().find_map(|arg| {
            arg.strip_prefix("--probe-card=")
                .map(std::string::ToString::to_string)
        });
        if probe_card_demo.is_some() || std::env::args().any(|arg| arg == "--probe") {
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
            || demo_height_map_result
            || demo_height_map_save
            || demo_height_map_confirm
            || demo_height_map_apply
            || std::env::args().any(|arg| arg == "--demo-probe-run");

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
            program_scroll: UniformListScrollHandle::new(),
            viewer: viewer::ViewerHandle::new(),
            last_job_line: 0,
            demo_program,
            demo_start,
            demo_start_sent: false,
            settings_entry: None,
            tool_editor: None,
            startup_confirm: None,
            offset_refresh_sent: false,
            backup_import_armed: false,
            settings_demo_write,
            settings_demo_write_done: false,
            settings_demo_group,
            settings_demo_group_done: false,
            demo_tools,
            demo_tools_done: false,
            demo_startup_write,
            demo_startup_write_done: false,
            demo_offsets_write,
            demo_offsets_write_done: false,
            demo_offsets_goto,
            demo_offsets_goto_done: false,
            probe_card_demo,
            probe_card_demo_done: false,
            demo_probe_run,
            demo_probe_run_done: false,
            demo_probe_apply,
            demo_probe_apply_done: false,
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
            height_map_base_program: None,
            _poll_task: None,
            _subscriptions: subscriptions,
        };
        if !mock {
            shell.connect_demo(cx);
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
        if let Some(link) = self.link.as_mut() {
            changed |= link.poll(&mut self.state);
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
        // Follow the current line in the Program tab. Only a changed line
        // scrolls, so manual scrolling is not fought every poll.
        let line = self.state.display_line();
        if self.state.job.has_program() && line != self.last_job_line {
            self.last_job_line = line;
            if line > 0 {
                self.program_scroll
                    .scroll_to_item(line as usize - 1, ScrollStrategy::Center);
            }
        }
        if changed {
            if self.state.console.len() != console_before {
                self.console_scroll.scroll_to_bottom();
            }
            cx.notify();
        }
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
        let tools = self.state.tools.table().clone();
        let offsets = self.state.profile_offset_snapshot();
        let settings = self.state.profile_settings_snapshot();
        match self.state.config.export_backup(&tools, &offsets, settings) {
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

    fn aux_allowed(&self) -> bool {
        self.ready() && (!self.state.live || !self.machine().is_locked())
    }

    fn macro_allowed(&self, action: crate::state::MacroAction) -> bool {
        use crate::state::MacroAction;
        match action {
            MacroAction::Unlock | MacroAction::Home | MacroAction::Park => self.ready(),
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
        let text = self.state.mdi.trim().to_string();
        if text.is_empty() {
            self.state.push_console(ConsoleLevel::Warn, "MDI is empty");
            cx.notify();
            return;
        }
        self.send_mdi_line(text);
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
        let continuous = self.state.continuous;
        self.jog_hold = Some(JogHold {
            sign,
            continuous,
            started: std::time::Instant::now(),
        });
        self.send_jog(sign, false);
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
    fn send_jog(&mut self, sign: f64, quiet_busy: bool) -> bool {
        let axis = self.state.jog_axis;
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
                        shell.send_jog(hold.sign, true);
                        true
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        }));
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
        }
    }

    fn close_probe_card(&mut self, cx: &mut Context<Self>) {
        if self.state.probe.close_card() {
            self.state.clear_saved_map_selection();
            cx.notify();
        } else {
            self.state.push_console(
                ConsoleLevel::Warn,
                "A probe run is active; finish or stop it before leaving the cycle",
            );
            cx.notify();
        }
    }

    /// Load and start a generated probe program after a passing preflight.
    ///
    /// Nothing is sent unless the operator pressed RUN. The engine safety
    /// gateway remains the final authority; a rejected command is shown in the
    /// console and the run is marked failed, never retried automatically.
    fn start_probe_run(&mut self, cx: &mut Context<Self>) {
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

    fn request_spindle_override(&mut self, target: u16) {
        if !self.state.live {
            return;
        }
        self.send_command(Command::SetSpindleOverride(target.clamp(10, 200)));
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
        self.send_command(Command::StartProgram { from_line: None });
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
    /// `reset: true` sends a soft reset after the runner aborts: it discards
    /// the controller's buffered motion, so a queued block cannot execute after
    /// the stop. The loaded program stays in the engine for a restart; the
    /// top-bar STOP remains the gentler feed hold.
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
        self.send_command(Command::StopProgram { reset: true });
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
            MacroAction::Park => {
                self.state.push_console(
                    ConsoleLevel::Warn,
                    "Park needs a machine-specific position; not sent (M7)",
                );
            }
            MacroAction::ProbeZ => {
                self.state.push_console(
                    ConsoleLevel::Warn,
                    "Probe cycles are not wired until M6; not sent",
                );
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
                    // already wrote it into the row draft.
                    KeypadTarget::Setting(_) | KeypadTarget::Tool(_) => {}
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

    fn top_bar(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();

        let modes = Mode::ALL.iter().enumerate().map(|(index, mode)| {
            let mode = *mode;
            let active = self.state.mode == mode;
            let view = view.clone();
            let variant = mode_variant(cx, p, active);
            Button::new(format!("mode-{index}"))
                .label(mode.label())
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

        let theme_view = view.clone();
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
            link_label,
            self.state.profile.capabilities.axes.len(),
            self.state.machine_state
        );
        let link_text = if matches!(
            self.state.connection,
            ConnectionState::Connecting | ConnectionState::Ready
        ) {
            "DISCONNECT"
        } else {
            "CONNECT"
        };

        div()
            .h(px(56.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_3()
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
            .child(
                Switch::new("theme-toggle")
                    .checked(!self.state.dark)
                    .label("Light")
                    .text_size(px(12.0))
                    .on_change(move |checked, _, cx| {
                        let dark = !*checked;
                        theme_view.update(cx, |this, cx| {
                            this.state.dark = dark;
                            Palette::new(dark).apply(cx);
                            this.state.config.set_theme(dark);
                            cx.notify();
                        });
                    }),
            )
            .child(
                Button::new("link")
                    .label(link_text)
                    .secondary()
                    .h(px(48.0))
                    .w(px(96.0))
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
                    .label("RESET")
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
                    .label("STOP")
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
            view,
        );

        let body = match self.state.left_tab {
            LeftTab::Console => self.console(p).into_any_element(),
            LeftTab::Program => self.program(p).into_any_element(),
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
                    .child(program_tab),
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

    fn console(&self, p: Palette) -> impl IntoElement {
        v_flex()
            .gap_1()
            .p_2()
            .h_full()
            .id("console-scroll")
            .track_scroll(&self.console_scroll)
            .overflow_y_scroll()
            .font_family(self.mono.clone())
            .children(self.state.console.iter().map(|line| {
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
            }))
    }

    fn program(&self, p: Palette) -> AnyElement {
        let source = if let Some(program) = self.state.job.program() {
            ProgramSource::Engine(Arc::clone(program))
        } else if self.state.live {
            return empty_program_notice(p).into_any_element();
        } else {
            ProgramSource::Mock(self.state.program.clone())
        };
        let count = source.len();
        let active_line = self.state.display_line();
        uniform_list("program-list", count, move |range, _, _| {
            range
                .filter_map(|index| source.row(index, active_line))
                .map(|row| program_row(row, p))
                .collect::<Vec<_>>()
        })
        .flex_1()
        .min_h(px(0.0))
        .track_scroll(&self.program_scroll)
        .into_any_element()
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
                            .child(self.state.mode.title()),
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
                                .w(px(110.0))
                                .h(px(5.0))
                                .rounded(px(999.0))
                                .bg(p.panel_alt)
                                .child(
                                    div()
                                        .h_full()
                                        .w(px(110.0 * progress))
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
                        ),
                )
                .child(chip(self.state.wcs(), p))
                .child(chip("XY", p))
        } else {
            header
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

    /// Slim toolbar inside the viewer: job state, elapsed time and the four
    /// job controls. Keeping it here preserves the approved three-column grid.
    fn job_toolbar(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let start_view = view.clone();
        let pause_view = view.clone();
        let resume_view = view.clone();
        let stop_view = view;

        let status = self.state.display_job_status();
        let color = job_status_color(&self.state.job, p);
        let detail = self
            .state
            .job
            .pause_reason
            .map(pause_reason_label)
            .map(str::to_string);
        let elapsed = format_elapsed(self.state.job.elapsed);
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
                            .child(status),
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
                                .child("HEIGHT MAP / COMPENSATED"),
                        )
                    })
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(self.mono.clone())
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child(format!("TIME {elapsed}")),
                    ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("job-start")
                            .label("START")
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
                            .label("PAUSE")
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
                            .label("RESUME")
                            .primary()
                            .h(px(48.0))
                            .w(px(88.0))
                            .rounded_lg()
                            .text_size(px(11.0))
                            .disabled(!resume_ok)
                            .on_click(move |_, _, cx| {
                                resume_view.update(cx, |this, cx| this.resume_job(cx));
                            }),
                    )
                    .child(
                        Button::new("job-stop")
                            .label("STOP")
                            .warning()
                            .h(px(48.0))
                            .w(px(68.0))
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
        // Real geometry comes from `JobLoaded`; the synthetic sketch remains
        // only for `--mock` and for a live session with nothing loaded.
        let path_label = match (&program, self.state.live) {
            (Some(_), _) => "TOOLPATH",
            (None, true) => "NO PROGRAM",
            (None, false) => "SYNTHETIC",
        };
        let segments = program
            .as_ref()
            .map(|program| format!("{} SEG", program.toolpath.segment_count()));
        let marker = viewer::marker_mm(self.state.profile.capabilities.units, &self.state.dro);
        let zoom = format!("{}%", self.viewer.zoom_percent());

        let fit_view = cx.entity();
        let fit = chip_button("viewer-fit", "FIT", p, move |_, cx| {
            fit_view.update(cx, |this, cx| {
                this.viewer.fit();
                cx.notify();
            });
        });

        div()
            .relative()
            .size_full()
            .bg(p.bg)
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
                    .child(chip("2D TOP", p))
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
                div()
                    .absolute()
                    .bottom(px(8.0))
                    .left(px(8.0))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_2()
                    .py_1()
                    .rounded_lg()
                    .bg(p.panel.opacity(0.9))
                    .border_1()
                    .border_color(p.border)
                    .child(legend(p.path_cut, "Cut", p))
                    .child(legend(p.path_rapid, "Rapid", p))
                    .child(legend(p.path_plunge, "Plunge", p)),
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
                            "TOOL {} | S{}",
                            self.state
                                .tool
                                .map_or_else(|| "--".to_string(), |tool| tool.to_string()),
                            self.state.rpm
                        )
                    } else {
                        "TOOL 3 | 6 mm FLAT | S12000".to_string()
                    }),
            )
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
                                    .child("PROBE CYCLES"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(p.text_faint)
                                    .child("Pick a card, inspect the preflight, then press RUN"),
                            ),
                    )
                    .child(div().flex_1())
                    .child(chip(self.state.wcs(), p))
                    .child(chip(self.state.unit(), p))
                    .child(chip(if ctx.ready { "LIVE" } else { "OFFLINE" }, p)),
            )
            .child(self.probe_offset_bar(p, cx))
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
                    .child(section_label("PROBE OFFSET X/Y", p))
                    .child(self.probe_offset_button(ProbeField::ProbeOffsetX, "X", p, cx))
                    .child(self.probe_offset_button(ProbeField::ProbeOffsetY, "Y", p, cx))
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_dim)
                            .child("mm | probe tip - controlled point | plan shifts controlled by -offset"),
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
        let partial = matches!(card, ProbeCard::Rotation | ProbeCard::ToolLength);
        let status = if partial {
            "APPLY DEFERRED"
        } else if ctx.ready && ctx.has_probe {
            "READY"
        } else {
            "SETUP"
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
                                    .child(card.title()),
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
                .label("CONFIGURE")
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
            "SAVED MAP"
        } else if let Some(run) = self.state.probe.run() {
            run.phase.label()
        } else if preflight.setup.is_some() {
            "PREFLIGHT OK"
        } else {
            "SETUP"
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
                            .label("CYCLES")
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
                                            .child(card.title()),
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
                    .child(section_label("CYCLE PARAMETERS", p))
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child("millimetres; tap a value to edit"),
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
            field.label().to_string()
        } else {
            format!("{} ({})", field.label(), unit)
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
                        .child(section_label("GRID SUMMARY", p))
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
                        .label("Probe connected")
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
                "POCKET",
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
                .child("TOUCHES"),
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
                    .child(section_label("PREFLIGHT", p))
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
                            .child(if ok { "READY" } else { "REFUSED" }),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(p.text_faint)
                            .child("every gate must pass before RUN"),
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
            "PROBE OFFSET X/Y",
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
                    "SAFE Z",
                    "Safe Z must clear the highest point on the stock; every grid probe descends \
                     from it. SEARCH must cover SAFE Z - SURFACE Z.",
                    p.warning,
                    p,
                ))
                .child(probe_notice(
                    "MAP Z OFFSET",
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
                        .child(format!("REFUSED: {blocker}")),
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
                    .child(section_label("SEARCH BOUND (CONTROLLED-POINT XY)", p))
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
                    .child(section_label("PROBE RUN", p))
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
                            .child(run.phase.label()),
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
                                .label("ABORT")
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
                        summary
                            .details
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
                        .child(section_label("RAW PRB (machine mm)", p))
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
            if run.map.is_none() {
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
                            .label("RETRY PREFLIGHT")
                            .secondary()
                            .h(px(48.0))
                            .w(px(150.0))
                            .rounded_lg()
                            .text_size(px(10.0))
                            .on_click(move |_, _, cx| {
                                retry_view.update(cx, |this, cx| this.start_probe_run(cx));
                            }),
                    )
                    .child(div().text_size(px(10.0)).text_color(p.warning).child(
                        "No automatic retry or resume. Recover the machine, then run again.",
                    )),
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
                    .child(section_label("SAVED MAPS", p))
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
                            .child("per controller profile"),
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
                    .child(format!("SAVE BLOCKED: {blocker}")),
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
                    .child("No saved maps for this controller profile yet."),
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
                                .child(if is_selected { "OPEN" } else { "VIEW" }),
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
        let marker = viewer::marker_mm(self.state.profile.capabilities.units, &self.state.dro);

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
                "WCS OFFSET",
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
                    .child(section_label("SAVED MAP", p))
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
                            "VALID",
                            &format!("{} / {}", summary.valid, summary.total),
                            p,
                        )
                        .w(px(96.0)),
                    )
                    .child(
                        height_map_detail_cell("MISSING", &summary.missing.to_string(), p)
                            .w(px(96.0)),
                    )
                    .child(height_map_detail_cell("MIN Z", &format!("{min_z} mm"), p).w(px(96.0)))
                    .child(height_map_detail_cell("MAX Z", &format!("{max_z} mm"), p).w(px(96.0)))
                    .child(height_map_detail_cell("COMP RANGE", &compensation, p).w(px(96.0)))
                    .child(
                        height_map_detail_cell(
                            "SURFACE",
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
                            .child(section_label("MEASUREMENT FRAME", p))
                            .child(
                                div()
                                    .text_size(px(8.0))
                                    .text_color(p.text_faint)
                                    .child("recorded vs live"),
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
                    .child(div().text_size(px(9.0)).text_color(p.text_faint).child(
                        "Read-only view; opening does not apply compensation or start a job",
                    )),
            )
            .child(
                Button::new("saved-map-close")
                    .label("CLOSE MAP")
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
                            .label("SAVE MAP")
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
                    .child(format!("SAVE BLOCKED: {blocker}")),
            );
        } else if valid_cells == 0 {
            section = section.child(
                div()
                    .text_size(px(8.5))
                    .text_color(p.warning)
                    .child("SAVE BLOCKED: the map has no valid cell"),
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
                (HeightMapApplyStatus::MeasurementOnly.label(), p.warning)
            }
            HeightMapApplyStatus::ReadyToApply => {
                (HeightMapApplyStatus::ReadyToApply.label(), p.accent)
            }
            HeightMapApplyStatus::Applied => (HeightMapApplyStatus::Applied.label(), p.success),
            HeightMapApplyStatus::Reverted => (HeightMapApplyStatus::Reverted.label(), p.text_dim),
        };
        let last_error = self.state.height_map.last_error().map(str::to_string);
        let overlay = self.state.probe.height_overlay();
        let program = self
            .height_map_base_program
            .clone()
            .or_else(|| self.state.job.program().cloned());
        let marker = viewer::marker_mm(self.state.profile.capabilities.units, &self.state.dro);
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
            Err(_) => ("FAILED".to_string(), p.danger),
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
            "COMPLETE"
        } else {
            "PARTIAL"
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
                    .child(section_label("HEIGHT MAP RESULT", p))
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
                        run.phase.label()
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
                            "VALID",
                            &format!("{} / {}", summary.valid, summary.total),
                            p,
                        )
                        .w(px(96.0)),
                    )
                    .child(
                        height_map_detail_cell("MISSING", &summary.missing.to_string(), p)
                            .w(px(96.0)),
                    )
                    .child(height_map_detail_cell("MIN Z", &format!("{min_z} mm"), p).w(px(96.0)))
                    .child(height_map_detail_cell("MAX Z", &format!("{max_z} mm"), p).w(px(96.0)))
                    .child(height_map_detail_cell("COMP RANGE", &compensation, p).w(px(96.0)))
                    .child(
                        height_map_detail_cell(
                            "SURFACE",
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
                                    .label("EXPORT CSV")
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
                .child(section_label("CONFIRM APPLY", p))
                .child(compact_stat_row("PROGRAM", preview.program_name.clone(), p))
                .child(compact_stat_row(
                    "MAP",
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
                                .label("CANCEL")
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
                                .label("CONFIRM APPLY")
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
                                .label("REVERT")
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
                .label("APPLY HEIGHT MAP")
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
                ("FAILED", p.danger)
            } else {
                ("APPLIED", p.success)
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
                ghs_probe::ProbeValue::ToolLength { .. } => {
                    Some("TLO apply is deferred until the tool-change workflow owns it.")
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
                    .child("KEPT: offsets are unchanged. Apply stays available."),
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
                            .child("CONFIRM OFFSET WRITE"),
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
                                    .label("CONFIRM WRITE")
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
                                    .label("CANCEL")
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
                            .label("APPLY OFFSET")
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
                            .label("KEEP")
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
                            .child("Nothing is applied automatically"),
                    ),
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
        let can_run = preflight.setup.is_some() && !running;
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
                "NOT READY".to_string(),
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
                    .label("ABORT")
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
                    .label(if run_exists { "RUN AGAIN" } else { "RUN" })
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
                    .label("CYCLES")
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
                            .child(self.state.mode.title()),
                    )
                    .child(chip(if self.state.live { "LIVE" } else { "MOCK" }, p)),
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
            OffsetStatus::Idle => (p.success, "READY".to_string()),
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
                                "REPORTED"
                            } else {
                                "NOT REPORTED"
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
            .label("ZERO ALL")
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
            .label(action.label())
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
                            .child("PREDEFINED POSITIONS / G92"),
                    )
                    .child(chip("CONFIRM", p)),
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
                .child("READ-ONLY $# PARAMETERS"),
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
                                    .child("WCS OFFSETS"),
                            )
                            .child(chip(self.state.unit(), p)),
                    )
                    .child(
                        Button::new("offset-refresh")
                            .label("REFRESH $#")
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
                            .child("X / Y / Z IN DISPLAY UNITS"),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child("TAP TO EDIT"),
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
                action.label(),
                pending.command()
            )
        } else {
            format!(
                "{} sends {}. No axis moves; the displayed work position changes by the cleared \
                 offset. The old G92 values are shown above and $# is read back after the command.",
                action.label(),
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
                            .child(action.title()),
                    )
                    .child(chip("NOT SENT YET", p)),
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
                            .child("EXACT MDI LINE"),
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
                            .child(action.target_label()),
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
                    .child("CURRENT DRO (DISPLAY UNITS)"),
            )
            .child(dro_row)
            .when(dro_changed, |this| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(p.warning)
                        .child("DRO CHANGED SINCE THIS SHEET OPENED; CANCEL AND RECHECK"),
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
                            .label("CANCEL")
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
                                format!("CONFIRM {}", pending.command())
                            } else {
                                "CLEAR G92".to_string()
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

    fn utility_view(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
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
            let status_label = row.status.label();
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
                                    .label("EDIT")
                                    .secondary()
                                    .h(px(36.0))
                                    .w(px(62.0))
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
                                        .child("NEW"),
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
                                        .label("APPLY")
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
                                .child("WRITE NOT APPLIED - THE PREVIOUS VALUE IS STILL ACTIVE"),
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
                            .child(section_label("STARTUP LINES", p))
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
                            .child(chip("RESET EXECUTION", p)),
                    )
                    .child(
                        Button::new("startup-reload")
                            .label("RELOAD")
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
            .child(div().text_size(px(10.0)).text_color(p.warning).child(
                "Startup lines execute automatically on every controller reset and can command \
                 motion, spindle, or coolant. Writes are allowed only while the controller is Idle; \
                 nothing is written until APPLY and CONFIRM WRITE.",
            ))
            .child(startup_list)
            .when_some(startup_lock, |this, reason| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.warning)
                        .child(format!("STARTUP WRITES LOCKED: {}", reason.to_uppercase())),
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
                    .child(section_label("CONTROLLER PROFILE", p))
                    .child(chip(
                        if corrupt {
                            "CORRUPT"
                        } else if unverified {
                            "UNVERIFIED"
                        } else if has_pending {
                            "PENDING CONFIRMATION"
                        } else if config.key().is_some() {
                            "LOCAL ONLY"
                        } else {
                            "NO CONTROLLER"
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
                            .label("SAVE PROFILE")
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
                            .label("SAVE APP")
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
                                .label("USE LOCAL DATA")
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
                    .child(section_label("BACKUP BUNDLE", p))
                    .child(chip("JSON / V1", p)),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(p.text_faint)
                    .child(format!("PATH   {backup_path}")),
            )
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(
                "Export writes app prefs, identity, settings, offsets, and tools; \
                         import validates before writing and never pushes to the controller.",
            ))
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("backup-export")
                            .label("EXPORT")
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
                            .label("VALIDATE")
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
                                "CONFIRM IMPORT"
                            } else {
                                "IMPORT"
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
            .overflow_y_scrollbar()
            .child(startup_card)
            .child(profile_card)
            .child(backup_card)
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
                    .child(
                        "No tools in the app table. Add one so the M6 sheet can show a \
                         name, diameter, and length instead of only the number.",
                    ),
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
                            .label("EDIT")
                            .secondary()
                            .h(px(40.0))
                            .w(px(62.0))
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
                            .label("DEL")
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
                            .when(dirty, |this| this.child(chip("UNSAVED", p)))
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
                                    .label("NEW TOOL")
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
                                    .label("SAVE")
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
            .child(div().text_size(px(10.0)).text_color(p.text_faint).child(
                "The app owns this table; it persists in the per-controller profile. \
                         Controller [T:] entries stay read-only and are never merged silently.",
            ))
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
                                    .child(section_label("CONTROLLER TOOLS", p))
                                    .child(chip("READ-ONLY", p)),
                            )
                            .child(chip(&format!("{} ENTRIES", controller.len()), p)),
                    )
                    .child(div().text_size(px(10.0)).text_color(p.text_faint).child(
                        "Reported by the controller through [T:...]; this panel never writes, \
                         overwrites, or merges into the app table above.",
                    ));
                if !controller.reported() {
                    panel = panel.child(
                        div()
                            .px_2()
                            .py(px(5.0))
                            .rounded(px(6.0))
                            .bg(p.panel)
                            .text_size(px(11.0))
                            .text_color(p.text_dim)
                            .child("No controller tool table has been reported yet."),
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
                            .child("The controller reports an empty tool table (0 entries)."),
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
                            .child(div().w(px(40.0)).child("TOOL"))
                            .child(div().flex_1().child("NAME"))
                            .child(div().w(px(68.0)).child("X"))
                            .child(div().w(px(68.0)).child("Y"))
                            .child(div().w(px(68.0)).child("Z"))
                            .child(div().w(px(58.0)).child("RADIUS"))
                            .child(div().w(px(46.0)).child("POCKET")),
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
                    .child("SEARCH"),
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
                .child("CLEAR")
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
                    .label("APPLY")
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
                    .label("REVERT")
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
                    "SEARCH"
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
            "IDLE: WRITES ENABLED".to_string()
        } else {
            format!("WRITES LOCKED: {lock_reason}")
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
                    .child("ONE WRITE AT A TIME | READ-BACK VERIFIED"),
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
                "MOCK MODE: NO CONTROLLER SETTINGS",
                "The generated editor is built from the controller's $$ values and $ES/$EG metadata. The design-review mock supplies neither. Start without --mock to connect the in-process FlexiHAL demo controller.",
            )
        } else if !self.ready() {
            (
                "WAITING FOR SETTINGS",
                "The handshake has not reported $$, $ES, or $EG yet. The form appears as soon as the controller answers.",
            )
        } else {
            (
                "THE CONTROLLER REPORTED NO SETTINGS",
                "$$ returned no values and $ES/$EG returned no metadata. The editor stays empty rather than inventing a setting list.",
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
                            .child("EXAMPLE SHAPE (NO VALUES ARE WRITTEN)"),
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
    fn settings_entry_overlay(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(entry) = self.settings_entry.as_ref() else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let title = match entry.purpose {
            SettingsEntryPurpose::Search => "SEARCH SETTINGS".to_string(),
            SettingsEntryPurpose::Text(number) => self.state.settings.row(number).map_or_else(
                || format!("SETTING ${number}"),
                |row| format!("SETTING ${number} / {}", row.name),
            ),
            SettingsEntryPurpose::StartupLine(index) => format!("STARTUP LINE $N{index}"),
            SettingsEntryPurpose::ToolName => "TOOL NAME".to_string(),
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
            SettingsEntryPurpose::HeightMapName => {
                "Short label for this controller profile; the same name replaces that map."
                    .to_string()
            }
        };
        let masked = entry.masked;
        let error = entry.error.clone();
        let input = entry.input.clone();
        let commit_label = match entry.purpose {
            SettingsEntryPurpose::HeightMapName => "SAVE MAP",
            SettingsEntryPurpose::StartupLine(_) => "KEEP DRAFT",
            _ if masked => "SET",
            _ => "APPLY TEXT",
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
                                    .label("CANCEL")
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
                                    .label(commit_label)
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
                            .child(chip("RESET EXECUTION", p)),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(p.warning)
                            .child(
                                "This startup line executes automatically on every controller \
                                 reset. It can command motion, spindle, or coolant.",
                            ),
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
                                    .child("EXACT TEXT TO STORE"),
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
                            .child(format!("Previous controller value   {previous}")),
                    )
                    .child(div().text_size(px(10.0)).text_color(p.text_dim).child(format!(
                        "CONFIRM WRITE sends $N{index}=<text> with exactly this text. The app does \
                         not update the row itself: it reads the line back and shows VERIFIED, \
                         MISMATCH, or FAILED."
                    )))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("startup-confirm-cancel")
                                    .label("CANCEL")
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
                                    .label("CONFIRM WRITE")
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
                    "Feed",
                    format!("{} {}/min", self.state.feed_rate, self.state.unit()),
                ),
            ]
        } else {
            [
                ("Origin X", "-123.450".to_string()),
                ("Origin Y", "-87.200".to_string()),
                ("Origin Z", "-2.000".to_string()),
                (
                    "Feed",
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
                    .child(section_label("COOLANT", p))
                    .child(h_flex().gap_1().children(coolant)),
            )
            .child(
                v_flex()
                    .gap_1()
                    .flex_shrink_0()
                    .child(section_label("MACROS", p))
                    .child(v_flex().gap_1().children(macro_rows)),
            )
            .child(
                v_flex()
                    .gap_1()
                    .flex_shrink_0()
                    .child(section_label("SIGNALS", p))
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
                            .child(section_label("WORK", p))
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

    /// MDI: one 48 px field plus a Send button on a single row, one hint line.
    fn mdi_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let empty = self.state.mdi.trim().is_empty();
        let field_view = view.clone();
        let send_view = view.clone();

        card(&p)
            .h(px(ROW_H))
            .flex_shrink_0()
            .justify_center()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(section_label("MDI", p))
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
                            .label("SEND")
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
                        div()
                            .text_size(px(10.0))
                            .text_color(p.text_faint)
                            .child("TAP FIELD FOR KEYPAD"),
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
                    .child(section_label("DRO", p))
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

    /// Feed: label and 24 px percentage, then a full-width slider flanked by
    /// 48 px -10/+10 buttons.
    fn feed_card(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let down_view = view.clone();
        let up_view = view.clone();
        let down_slider = self.feed_slider.clone();
        let up_slider = self.feed_slider.clone();

        card(&p)
            .h(px(ROW_H))
            .flex_shrink_0()
            .child(
                h_flex()
                    .items_end()
                    .justify_between()
                    .child(section_label("FEED OVERRIDE", p))
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
                h_flex()
                    .items_center()
                    .gap_2()
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
                                    this.state.feed_override =
                                        this.state.feed_override.saturating_sub(10).max(10);
                                    let value = f32::from(this.state.feed_override);
                                    down_slider.update(cx, |slider, cx| {
                                        slider.set_value(value, window, cx);
                                    });
                                    this.request_feed_override(value.round() as u16);
                                    cx.notify();
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
                                    this.state.feed_override =
                                        this.state.feed_override.saturating_add(10).min(200);
                                    let value = f32::from(this.state.feed_override);
                                    up_slider.update(cx, |slider, cx| {
                                        slider.set_value(value, window, cx);
                                    });
                                    this.request_feed_override(value.round() as u16);
                                    cx.notify();
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
                            .child(section_label("JOG", p))
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
                            .label("CONT")
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
                    .child(section_label("SPINDLE", p))
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
                                    .label(if self.state.spindle_on { "ON" } else { "OFF" })
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
            Some(number) => format!("EDIT TOOL T{number}"),
            None => "NEW TOOL".to_string(),
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
                        .child(label),
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
                                .child("NAME"),
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
                                original.map_or("NEW RECORD", |_| "REPLACES BY NUMBER"),
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
                                    .label("CANCEL")
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
                                    .label("SAVE TOOL")
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
                                    .child("TOOL CHANGE REQUIRED"),
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
                            .child("APP TOOL"),
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
                "Jogging is allowed while the job waits; the Jog panel stays active. The job will not send another line until you resume. RESET and STOP above remain available.",
            ))
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("tool-ack")
                            .label(if acknowledged {
                                "ACKNOWLEDGED"
                            } else {
                                "1. ACKNOWLEDGE"
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
                            .label("2. RESUME")
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
                            .label("STOP JOB")
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
                        .child("In-process FlexiHAL simulator over the loopback transport. No serial port is opened."),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(p.text_faint)
                        .child("Same engine, handshake, streamer and safety gateway as a real controller."),
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
                    .label(choice.label())
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
                                    .child("CONNECTION"),
                            )
                            .child(chip(self.state.connection.label(), p)),
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
            .child(section_label("TRANSPORT", p))
            .child(h_flex().gap_2().children(transports))
            .when(connected, |this| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(p.text_faint)
                        .child("Disconnect before switching transport."),
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
                            .child("CURRENT LINK"),
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
                    .label("CONNECT")
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
                    .label("DISCONNECT")
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
                    .label("CLOSE")
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
                            .unwrap_or_else(|| "No serial ports found".to_string()),
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
                    .unwrap_or_else(|| "serial port".to_string());
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
                    .child(section_label("SERIAL PORT", p))
                    .child(
                        Button::new("ports-refresh")
                            .label("REFRESH")
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
                            .child("Tap a port to select it; nothing opens until CONNECT."),
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
                    .child("Line settings are fixed at 8 data bits, no parity, one stop bit. DTR reset pulses are not implemented in M2."),
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
        let p = Palette::new(self.state.dark);
        let width = window.viewport_size().width;
        let compact = width < px(1120.0);

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
            .child(self.top_bar(p, cx))
            .child(content);

        let keypad_state = self.state.keypad.clone();
        if self.tool_editor.is_some() {
            root = root.child(self.tool_editor_overlay(p, cx));
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
        if self.startup_confirm.is_some() {
            root = root.child(self.startup_confirm_overlay(p, cx));
        }
        if self.state.job.tool_change.is_some() {
            root = root.child(self.tool_change_overlay(p, cx));
        }
        if self.panel.open {
            root = root.child(self.connection_overlay(p, cx));
        }
        if self.state.offsets.pending_action().is_some() {
            root = root.child(self.offset_confirm_overlay(p, cx));
        }
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

    fn row(&self, index: usize, active_line: u32) -> Option<ProgramRow> {
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
}

fn program_row(row: ProgramRow, p: Palette) -> Div {
    let active = row.active;
    let text_color = if active {
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
    h_flex()
        .items_center()
        .gap_2()
        .h(px(23.0))
        .px_2()
        .bg(if active { p.accent_muted } else { p.panel })
        .border_l(px(3.0))
        .border_color(if active { p.accent } else { p.panel })
        .child(
            div()
                .w(px(44.0))
                .flex_shrink_0()
                .text_size(px(10.0))
                .text_color(if active { p.accent } else { p.text_faint })
                .child(row.number.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .overflow_hidden()
                .text_size(px(11.5))
                .text_color(text_color)
                .child(row.text),
        )
}

fn empty_program_notice(p: Palette) -> Div {
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
                .child("No program loaded"),
        )
        .child(
            div()
                .text_size(px(10.0))
                .text_color(p.text_faint)
                .child("Demo mode sends a bundled program when the controller is ready."),
        )
}

fn job_status_color(job: &JobView, p: Palette) -> Hsla {
    if job.tool_change.is_some() {
        return p.warning;
    }
    match job.state {
        JobState::Running => p.accent,
        JobState::Paused => p.warning,
        JobState::Completed => p.success,
        JobState::Aborted | JobState::Interrupted => p.danger,
        JobState::Idle => p.text_dim,
    }
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
        "rotation" => Some(ProbeCard::Rotation),
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
        .child(section_label("GENERATED PROGRAM", p))
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
        .label(label.label())
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

fn card(p: &Palette) -> Div {
    v_flex()
        .gap_2()
        .p_2()
        .rounded_lg()
        .border_1()
        .border_color(p.border)
        .bg(p.panel)
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
        .child("STALE")
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
                .child("MEASURED Z"),
        )
        .child(div().text_size(px(9.0)).text_color(p.text_dim).child("LOW"))
        .child(swatches)
        .child(
            div()
                .text_size(px(9.0))
                .text_color(p.text_dim)
                .child("HIGH"),
        )
        .child(
            div()
                .text_size(px(9.0))
                .text_color(p.text_faint)
                .child("dashed = missing"),
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
    let status_label = row.status.label();
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
            .child(if checked { "ON" } else { "OFF" }),
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
                .child("No axes reported by the controller"),
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
                    _ => "EDIT",
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
                    "No settings in this group".to_string()
                } else {
                    format!("No setting matches \"{query}\"")
                }),
        )
        .child(
            div()
                .text_size(px(10.0))
                .text_color(p.text_faint)
                .child(if query.is_empty() {
                    "Pick another group in the rail."
                } else {
                    "Search matches a number, name, or group. Clear the search to browse groups."
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
    use super::*;

    #[test]
    fn program_rows_map_source_numbers_and_current_line() {
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
        let row = mock.row(1, 2).expect("second mock row");
        assert_eq!(row.number, 2);
        assert_eq!(row.text, "M30");
        assert!(row.active);
        assert!(!row.comment);
        assert!(mock.row(2, 2).is_none());

        let program = Arc::new(
            ghs_gcode::parse_program("G21\n(Groove)\nM30\n").expect("sample program parses"),
        );
        let live = ProgramSource::Engine(program);
        assert_eq!(live.len(), 3);
        assert_eq!(live.row(0, 3).map(|row| row.number), Some(1));
        let comment = live.row(1, 3).expect("comment row");
        assert!(comment.comment);
        assert!(!comment.active);
        assert_eq!(comment.text, "(Groove)");
        let current = live.row(2, 3).expect("current row");
        assert!(current.active);
        assert!(!current.comment);
    }
}
