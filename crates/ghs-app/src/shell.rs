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

use std::sync::Arc;
use std::time::Duration;

use ghs_engine::{
    Axes, Axis as CoreAxis, Command, EngineError, JobPauseReason, JobState, JogCommand,
    MachineState, Position, Program, RealtimeCommand, SerialConfig,
};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
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
use crate::keypad;
use crate::live::{DemoProgram, LinkMode, LiveLink};
use crate::settings::{GroupRow, RowStatus, SettingKind, SettingRow, SettingsScope};
use crate::state::{
    AppState, Axis, ConnectionState, ConsoleLevel, JobView, KeypadAction, KeypadTarget, LeftTab,
    Mode, ProgramKind, SignalState, format_elapsed, pause_reason_label,
};
use crate::theme::Palette;
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
}

/// One open settings text sheet.
struct SettingsEntry {
    purpose: SettingsEntryPurpose,
    input: Entity<InputState>,
    masked: bool,
    error: Option<String>,
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
    settings_demo_write: bool,
    settings_demo_write_done: bool,
    settings_demo_group: Option<String>,
    settings_demo_group_done: bool,
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
        state.dark = !std::env::args().any(|arg| arg == "--light");
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
            settings_demo_write,
            settings_demo_write_done: false,
            settings_demo_group,
            settings_demo_group_done: false,
            _poll_task: None,
            _subscriptions: subscriptions,
        };
        if !mock {
            shell.connect_demo(cx);
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

    fn zero_axis(&mut self, axis: Axis, cx: &mut Context<Self>) {
        if !self.state.live {
            self.state.zero_axis(axis);
            cx.notify();
            return;
        }
        let line = format!(
            "G10 L20 P{} {}0",
            self.state.wcs_program_number(),
            axis.label()
        );
        self.send_mdi_line(line);
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
            self.state.wcs_index = index;
            self.send_mdi_line(AppState::wcs_at(index).to_string());
        } else {
            self.state.wcs_index = index;
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
        if self.state.live
            && let Some(keypad) = submitted
        {
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
                        let line = format!(
                            "G10 L20 P{} {}{value:.3}",
                            self.state.wcs_program_number(),
                            axis.label()
                        );
                        self.send_mdi_line(line);
                    }
                }
                // A settings value stays local until Apply; `AppState` already
                // wrote it into the row draft.
                KeypadTarget::Setting(_) => {}
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
            Mode::Settings => format!("{} settings from $ES/$EG", self.state.settings.total()),
            _ => "M2 preview".to_string(),
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
            .child(viewer::view(&self.viewer, p, program, marker, cx.entity()))
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

    fn mode_view(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let caps = &self.state.profile.capabilities;
        let body = match self.state.mode {
            Mode::Job => unreachable!(),
            Mode::Probe => {
                let items: &[(&str, &str)] = if caps.has_probe {
                    &[
                        ("Z touch plate", "M6"),
                        ("XY corner finder", "M6"),
                        ("Circle centre", "M6"),
                        ("Height map", "M6"),
                    ]
                } else {
                    &[
                        ("No probe input reported", "CAP"),
                        ("Probe cycles hidden", "CAP"),
                    ]
                };
                v_flex().gap_2().child(self.list_rows(p, items))
            }
            Mode::Offset => self.offset_rows(p),
            Mode::Utility => self.utility_rows(p),
            Mode::Tools => self.tool_rows(p),
            Mode::Settings => unreachable!(),
        };
        let view = cx.entity();
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
                    .child(chip("M2 PREVIEW", p)),
            )
            .child(body)
            .child(div().flex_1())
            .child(
                Button::new("mode-back")
                    .label("Back to Job")
                    .h(px(48.0))
                    .w(px(140.0))
                    .rounded_lg()
                    .text_size(px(12.0))
                    .secondary()
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.state.mode = Mode::Job;
                            cx.notify();
                        });
                    }),
            )
    }

    fn list_rows(&self, p: Palette, items: &[(&str, &str)]) -> Div {
        v_flex().gap_2().children(
            items
                .iter()
                .map(|(name, milestone)| info_row(name, milestone, p)),
        )
    }

    fn offset_rows(&self, p: Palette) -> Div {
        const ROWS: [(&str, &str, &str, &str); 6] = [
            ("G54", "-123.450", "-87.200", "-2.000"),
            ("G55", "0.000", "0.000", "0.000"),
            ("G56", "-500.000", "-320.000", "0.000"),
            ("G57", "0.000", "0.000", "0.000"),
            ("G58", "0.000", "0.000", "0.000"),
            ("G59", "0.000", "0.000", "0.000"),
        ];
        v_flex()
            .gap_1()
            .p_1()
            .children(ROWS.iter().map(|(wcs, x, y, z)| {
                let active = *wcs == self.state.wcs();
                h_flex()
                    .items_center()
                    .h(px(34.0))
                    .gap_2()
                    .px_2()
                    .rounded(px(6.0))
                    .bg(if active { p.accent_muted } else { p.panel_alt })
                    .child(
                        div()
                            .w(px(48.0))
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(if active { p.accent } else { p.text_dim })
                            .child(*wcs),
                    )
                    .child(num_cell(x, p))
                    .child(num_cell(y, p))
                    .child(num_cell(z, p))
            }))
    }

    fn utility_rows(&self, p: Palette) -> Div {
        const ITEMS: [(&str, &str); 4] = [
            ("Surface stock", "Generate a facing program"),
            ("Square gantry", "Two-edge alignment routine"),
            ("Calibrate steps", "Move, measure, correct"),
            ("Outline check", "Bounds before a run"),
        ];
        v_flex()
            .gap_2()
            .children(ITEMS.iter().map(|(name, detail)| {
                h_flex()
                    .items_center()
                    .justify_between()
                    .p_2()
                    .rounded_lg()
                    .bg(p.panel_alt)
                    .child(
                        v_flex()
                            .child(div().text_size(px(13.0)).text_color(p.text).child(*name))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(p.text_faint)
                                    .child(*detail),
                            ),
                    )
                    .child(chip("M7", p))
            }))
    }

    fn tool_rows(&self, p: Palette) -> Div {
        let caps = &self.state.profile.capabilities;
        let tools: &[(&str, &str, &str)] = if caps.has_tool_table {
            &[
                ("T1", "6 mm flat", "0.000"),
                ("T2", "3 mm flat", "0.000"),
                ("T3", "6 mm ball", "0.000"),
                ("T4", "90 deg V", "0.000"),
            ]
        } else {
            &[]
        };
        v_flex()
            .gap_1()
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(p.text_faint)
                    .child(format!(
                        "APP-SIDE TABLE | controller reports {} entries{}",
                        caps.tool_table_entries,
                        if caps.has_tool_table {
                            ""
                        } else {
                            " | capability not reported"
                        }
                    )),
            )
            .children(tools.iter().map(|(tool, kind, offset)| {
                h_flex()
                    .items_center()
                    .gap_3()
                    .h(px(40.0))
                    .px_2()
                    .rounded(px(6.0))
                    .bg(p.panel_alt)
                    .child(
                        div()
                            .w(px(34.0))
                            .text_size(px(12.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.accent)
                            .child(*tool),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.0))
                            .text_color(p.text)
                            .child(*kind),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(p.text_dim)
                            .child(format!("TLO {offset}")),
                    )
            }))
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
        };
        let masked = entry.masked;
        let error = entry.error.clone();
        let input = entry.input.clone();
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
                                    .label(if masked { "SET" } else { "APPLY TEXT" })
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
        if self.state.job.tool_change.is_some() {
            root = root.child(self.tool_change_overlay(p, cx));
        }
        if self.panel.open {
            root = root.child(self.connection_overlay(p, cx));
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

fn legend(color: Hsla, text: &'static str, p: Palette) -> Div {
    h_flex()
        .items_center()
        .gap_1()
        .child(div().w(px(6.0)).h(px(6.0)).rounded(px(999.0)).bg(color))
        .child(div().text_size(px(10.0)).text_color(p.text_dim).child(text))
}

fn info_row(name: &str, milestone: &str, p: Palette) -> Div {
    h_flex()
        .items_center()
        .justify_between()
        .h(px(40.0))
        .px_2()
        .rounded(px(6.0))
        .bg(p.panel_alt)
        .child(
            div()
                .text_size(px(12.0))
                .text_color(p.text)
                .child(name.to_string()),
        )
        .child(chip(milestone, p))
}

fn num_cell(value: &str, p: Palette) -> Div {
    div()
        .flex_1()
        .text_size(px(11.0))
        .text_color(p.text_dim)
        .child(value.to_string())
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
