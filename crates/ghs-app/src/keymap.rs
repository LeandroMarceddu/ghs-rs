//! Persisted machine hotkeys, their editor model, and their motion guards.
//!
//! The keymap is deliberately a small, fixed action set. Defaults are
//! modifier-heavy so normal typing and MDI entry are never stolen:
//!
//! | Action | Default |
//! |---|---|
//! | JOG X- | `ctrl-shift-left` |
//! | JOG X+ | `ctrl-shift-right` |
//! | JOG Y- | `ctrl-shift-down` |
//! | JOG Y+ | `ctrl-shift-up` |
//! | JOG Z- | `ctrl-shift-pagedown` |
//! | JOG Z+ | `ctrl-shift-pageup` |
//! | START | `f5` |
//! | PAUSE | `f6` |
//! | RESUME | `f7` |
//! | STOP | `f8` |
//!
//! Binding a profile keymap only adds GPUI bindings. It never dispatches an
//! action, starts a job, or jogs an axis by itself. Every key press still goes
//! through the same gates and commands as the on-screen buttons.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::{Duration, Instant};

use ghs_config::{KeyBinding as ConfigKeyBinding, Keymap};
use ghs_core::{JobState, MachineState};
use gpui_kit::{
    Action, App, KeyBinding as GpuiKeyBinding, KeyDownEvent, Keystroke, OperatingSystem,
    SharedString, Unbind,
};

gpui_kit::actions!(
    ghs_app,
    [
        JogXMinus,
        JogXPlus,
        JogYMinus,
        JogYPlus,
        JogZMinus,
        JogZPlus,
        MachineStart,
        MachinePause,
        MachineResume,
        MachineStop,
    ]
);

/// How long one physical key press remains latched for the repeat guard.
///
/// The guard releases on key-up, so this is only a stale-state fallback and
/// not the normal input cadence.
pub const KEY_HOLD_STALE_AFTER: Duration = Duration::from_secs(30);

/// One bounded machine action that can be assigned a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MachineAction {
    /// Jog X in the negative direction.
    JogXMinus,
    /// Jog X in the positive direction.
    JogXPlus,
    /// Jog Y in the negative direction.
    JogYMinus,
    /// Jog Y in the positive direction.
    JogYPlus,
    /// Jog Z in the negative direction.
    JogZMinus,
    /// Jog Z in the positive direction.
    JogZPlus,
    /// Start the loaded job.
    Start,
    /// Feed-hold the running job.
    Pause,
    /// Resume the paused job.
    Resume,
    /// Stop the active job and discard buffered motion.
    Stop,
}

/// Every action in display order.
pub const ALL_ACTIONS: [MachineAction; 10] = [
    MachineAction::JogXMinus,
    MachineAction::JogXPlus,
    MachineAction::JogYMinus,
    MachineAction::JogYPlus,
    MachineAction::JogZMinus,
    MachineAction::JogZPlus,
    MachineAction::Start,
    MachineAction::Pause,
    MachineAction::Resume,
    MachineAction::Stop,
];

impl MachineAction {
    /// Stable profile id.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::JogXMinus => "jog.x.minus",
            Self::JogXPlus => "jog.x.plus",
            Self::JogYMinus => "jog.y.minus",
            Self::JogYPlus => "jog.y.plus",
            Self::JogZMinus => "jog.z.minus",
            Self::JogZPlus => "jog.z.plus",
            Self::Start => "run.start",
            Self::Pause => "run.pause",
            Self::Resume => "run.resume",
            Self::Stop => "run.stop",
        }
    }

    /// Short label for the keymap rows.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::JogXMinus => "JOG X-",
            Self::JogXPlus => "JOG X+",
            Self::JogYMinus => "JOG Y-",
            Self::JogYPlus => "JOG Y+",
            Self::JogZMinus => "JOG Z-",
            Self::JogZPlus => "JOG Z+",
            Self::Start => "START",
            Self::Pause => "PAUSE",
            Self::Resume => "RESUME",
            Self::Stop => "STOP",
        }
    }

    /// Default accelerator in GPUI's stored syntax.
    #[must_use]
    pub const fn default_keys(self) -> &'static str {
        match self {
            Self::JogXMinus => "ctrl-shift-left",
            Self::JogXPlus => "ctrl-shift-right",
            Self::JogYMinus => "ctrl-shift-down",
            Self::JogYPlus => "ctrl-shift-up",
            Self::JogZMinus => "ctrl-shift-pagedown",
            Self::JogZPlus => "ctrl-shift-pageup",
            Self::Start => "f5",
            Self::Pause => "f6",
            Self::Resume => "f7",
            Self::Stop => "f8",
        }
    }

    /// Whether this action stays available while a non-text sheet is open.
    ///
    /// STOP stays available; movement and job start/pause/resume wait until the
    /// sheet is dismissed. Text fields and the keymap capture editor suppress
    /// every action, including STOP.
    #[must_use]
    pub const fn allowed_while_sheet_open(self) -> bool {
        matches!(self, Self::Stop)
    }

    /// The machine intent this action represents.
    #[must_use]
    pub const fn intent(self) -> MachineIntent {
        match self {
            Self::JogXMinus => MachineIntent::Jog {
                axis: JogAxis::X,
                sign: -1.0,
            },
            Self::JogXPlus => MachineIntent::Jog {
                axis: JogAxis::X,
                sign: 1.0,
            },
            Self::JogYMinus => MachineIntent::Jog {
                axis: JogAxis::Y,
                sign: -1.0,
            },
            Self::JogYPlus => MachineIntent::Jog {
                axis: JogAxis::Y,
                sign: 1.0,
            },
            Self::JogZMinus => MachineIntent::Jog {
                axis: JogAxis::Z,
                sign: -1.0,
            },
            Self::JogZPlus => MachineIntent::Jog {
                axis: JogAxis::Z,
                sign: 1.0,
            },
            Self::Start => MachineIntent::Start,
            Self::Pause => MachineIntent::Pause,
            Self::Resume => MachineIntent::Resume,
            Self::Stop => MachineIntent::Stop,
        }
    }
}

/// The three machine axes addressed by jog hotkeys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JogAxis {
    /// X axis.
    X,
    /// Y axis.
    Y,
    /// Z axis.
    Z,
}

/// A typed request produced by a machine hotkey.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MachineIntent {
    /// One relative jog step on one axis.
    Jog {
        /// Axis to jog.
        axis: JogAxis,
        /// `-1.0` or `1.0`.
        sign: f64,
    },
    /// Start the loaded job.
    Start,
    /// Feed-hold the running job.
    Pause,
    /// Resume the paused job.
    Resume,
    /// Stop the active job.
    Stop,
}

/// A snapshot of the gates that decide whether a hotkey may act.
///
/// This mirrors the shell's button gates. Keeping it pure makes the refusal
/// rules testable without a window or controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardGate {
    /// A live engine session is present. The design-review `--mock` shell has
    /// no engine and refuses every machine hotkey.
    pub live: bool,
    /// The link finished its handshake.
    pub ready: bool,
    /// Current machine state.
    pub machine: MachineState,
    /// Current job lifecycle state.
    pub job: JobState,
    /// A program is loaded.
    pub program_loaded: bool,
    /// The loaded program is a generated probe cycle.
    pub probe_owns_program: bool,
    /// A manual tool change awaits acknowledgement.
    pub tool_change_pending: bool,
    /// The pause came from the status watchdog.
    pub watchdog_pause: bool,
}

impl Default for KeyboardGate {
    fn default() -> Self {
        Self {
            live: false,
            ready: false,
            machine: MachineState::Unknown,
            job: JobState::Idle,
            program_loaded: false,
            probe_owns_program: false,
            tool_change_pending: false,
            watchdog_pause: false,
        }
    }
}

impl KeyboardGate {
    fn locked(&self) -> bool {
        matches!(self.machine, MachineState::Alarm | MachineState::Door)
    }
}

/// Explain why a hotkey intent is refused, or return `None` when it may run.
///
/// The caller still invokes the same guarded shell action as a button; this is
/// an early, explicit refusal so a key press never looks ignored.
#[must_use]
pub fn refusal(intent: MachineIntent, gate: &KeyboardGate) -> Option<&'static str> {
    if !gate.live {
        return Some("no live controller");
    }
    if !gate.ready {
        return Some("the controller link is not ready");
    }
    if gate.locked() {
        return Some(match gate.machine {
            MachineState::Alarm => "Alarm is active; motion is locked",
            MachineState::Door => "the safety door is open; motion is locked",
            _ => "motion is locked",
        });
    }
    match intent {
        MachineIntent::Jog { .. } => {
            if !matches!(
                gate.job,
                JobState::Idle | JobState::Completed | JobState::Aborted | JobState::Interrupted
            ) {
                return Some("a job is active; stop it before jogging");
            }
            if !matches!(
                gate.machine,
                MachineState::Idle | MachineState::Hold | MachineState::Jog | MachineState::Tool
            ) {
                return Some("jog is allowed only in Idle, Hold, Jog, or Tool");
            }
            None
        }
        MachineIntent::Start => {
            if gate.probe_owns_program {
                return Some("probe programs can only run from their probe card");
            }
            if !gate.program_loaded {
                return Some("no program is loaded");
            }
            if !matches!(
                gate.job,
                JobState::Idle | JobState::Completed | JobState::Aborted | JobState::Interrupted
            ) {
                return Some("a job is already active");
            }
            if gate.machine != MachineState::Idle {
                return Some("START needs an Idle controller");
            }
            None
        }
        MachineIntent::Pause => {
            if gate.job != JobState::Running {
                return Some("no running job to pause");
            }
            None
        }
        MachineIntent::Resume => {
            if gate.job != JobState::Paused {
                return Some("no paused job to resume");
            }
            if gate.tool_change_pending {
                return Some("acknowledge the tool change before resuming");
            }
            if gate.watchdog_pause {
                return Some("a watchdog pause must be acknowledged, not resumed by hotkey");
            }
            None
        }
        MachineIntent::Stop => {
            if !matches!(gate.job, JobState::Running | JobState::Paused) {
                return Some("no active job to stop");
            }
            None
        }
    }
}

/// One row of the editable keymap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeymapRow {
    /// Action assigned to the key.
    pub action: MachineAction,
    /// Stored accelerator text; canonical after a successful edit.
    pub keys: String,
}

/// The bounded keymap editor model.
///
/// Unknown actions in a stored profile keymap are ignored: this build owns a
/// fixed set of machine actions and does not silently preserve bindings it
/// cannot display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeymapModel {
    rows: Vec<KeymapRow>,
}

impl Default for KeymapModel {
    fn default() -> Self {
        Self {
            rows: ALL_ACTIONS
                .into_iter()
                .map(|action| KeymapRow {
                    action,
                    keys: action.default_keys().to_string(),
                })
                .collect(),
        }
    }
}

impl KeymapModel {
    /// Build a model from a stored profile keymap.
    ///
    /// Missing actions use defaults. Stored text is kept as-is so the editor
    /// can show an invalid value; [`Self::effective_bindings`] chooses what is
    /// safe to bind at runtime.
    #[must_use]
    pub fn from_keymap(keymap: &Keymap) -> Self {
        let rows = ALL_ACTIONS
            .into_iter()
            .map(|action| {
                let keys = keymap
                    .get(action.id())
                    .map(|binding| binding.keys.trim().to_string())
                    .filter(|keys| !keys.is_empty())
                    .unwrap_or_else(|| action.default_keys().to_string());
                KeymapRow { action, keys }
            })
            .collect();
        Self { rows }
    }

    /// Read-only rows in display order.
    #[must_use]
    pub fn rows(&self) -> &[KeymapRow] {
        &self.rows
    }

    /// Stored text for one action.
    #[must_use]
    pub fn keys(&self, action: MachineAction) -> &str {
        self.rows
            .iter()
            .find(|row| row.action == action)
            .map_or("", |row| row.keys.as_str())
    }

    /// Whether every row still holds its documented default.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.rows
            .iter()
            .all(|row| row.keys == row.action.default_keys())
    }

    /// Whether the local draft differs from another model.
    #[must_use]
    pub fn differs_from(&self, other: &Self) -> bool {
        self.rows != other.rows
    }

    /// Replace every row with its documented default. This is local until
    /// [`crate::config::ConfigModel::save_keymap`] is called.
    pub fn reset_defaults(&mut self) {
        for row in &mut self.rows {
            row.keys = row.action.default_keys().to_string();
        }
    }

    /// Validate one proposed accelerator against the other rows.
    ///
    /// On success the row is updated to the canonical spelling.
    ///
    /// # Errors
    ///
    /// Returns a clear message for empty, unparseable, modifier-free,
    /// reserved, or duplicate accelerators.
    pub fn try_set(
        &mut self,
        action: MachineAction,
        text: &str,
        os: OperatingSystem,
    ) -> Result<String, String> {
        let candidate = parse_user_key(text, os)?;
        let canonical = candidate.unparse(os);
        for other in &self.rows {
            if other.action == action {
                continue;
            }
            if let Some(other_keys) = effective_key(other, os)
                && other_keys == canonical
            {
                return Err(format!(
                    "{} is already assigned to {}",
                    pretty_keys(&canonical),
                    other.action.label()
                ));
            }
        }
        let Some(row) = self.rows.iter_mut().find(|row| row.action == action) else {
            return Err("unknown keymap action".to_string());
        };
        row.keys = canonical.clone();
        Ok(canonical)
    }

    /// Validate every row and build the persisted keymap.
    ///
    /// # Errors
    ///
    /// Returns the first invalid or duplicate row with its action label.
    pub fn to_keymap(&self, os: OperatingSystem) -> Result<Keymap, String> {
        let mut bindings = Vec::with_capacity(self.rows.len());
        let mut seen: HashMap<Keystroke, MachineAction> = HashMap::new();
        for row in &self.rows {
            let keystroke = parse_user_key(&row.keys, os)
                .map_err(|message| format!("{}: {message}", row.action.label()))?;
            if let Some(previous) = seen.insert(keystroke.clone(), row.action) {
                return Err(format!(
                    "{} uses {} which is already assigned to {}",
                    row.action.label(),
                    pretty_keys(&keystroke.unparse(os)),
                    previous.label()
                ));
            }
            bindings.push(ConfigKeyBinding::new(
                row.action.id(),
                keystroke.unparse(os),
            ));
        }
        let keymap = Keymap { bindings };
        keymap
            .validate()
            .map_err(|error| format!("keymap is invalid: {error}"))?;
        Ok(keymap)
    }

    /// Safe runtime bindings: invalid or duplicate stored rows fall back to
    /// their default, and an action is omitted only when even its default is
    /// already taken.
    #[must_use]
    pub fn effective_bindings(&self, os: OperatingSystem) -> Vec<(MachineAction, String)> {
        let mut bindings = Vec::with_capacity(self.rows.len());
        let mut seen: HashSet<String> = HashSet::new();
        for row in &self.rows {
            let stored = parse_user_key(&row.keys, os)
                .ok()
                .map(|keys| keys.unparse(os));
            let default = parse_user_key(row.action.default_keys(), os)
                .ok()
                .map(|keys| keys.unparse(os));
            for candidate in [stored, default].into_iter().flatten() {
                if seen.insert(candidate.clone()) {
                    bindings.push((row.action, candidate));
                    break;
                }
            }
        }
        bindings
    }
}

fn effective_key(row: &KeymapRow, os: OperatingSystem) -> Option<String> {
    parse_user_key(&row.keys, os)
        .ok()
        .or_else(|| parse_user_key(row.action.default_keys(), os).ok())
        .map(|keys| keys.unparse(os))
}

/// Parse and validate one user-facing accelerator.
///
/// A binding must include Ctrl, Alt, Fn, or a function key. Bare letters,
/// editing keys, and plain arrows are refused so normal typing stays intact.
///
/// # Errors
///
/// Returns a clear message when the text is empty, malformed, modifier-only,
/// OS-reserved, or too easy to hit while typing.
pub fn parse_user_key(text: &str, os: OperatingSystem) -> Result<Keystroke, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("a key combination is required".to_string());
    }
    // Accept the common `Ctrl+Shift+X` spelling as well as GPUI's stored
    // `ctrl-shift-x` form. Lowercase first so an uppercase letter is not read
    // as an implicit Shift modifier.
    let normalized = text.replace('+', "-").to_ascii_lowercase();
    let keystroke = Keystroke::parse_with_os(&normalized, os)
        .map_err(|_| format!("'{text}' is not a valid key combination"))?;
    let key = keystroke.key.trim().to_ascii_lowercase();
    if key.is_empty() {
        return Err("a key is required after the modifiers".to_string());
    }
    if is_modifier_name(&key) {
        return Err("press a key, not only a modifier".to_string());
    }
    if keystroke.modifiers.platform {
        return Err("Cmd/Super/Win combinations are reserved by the operating system".to_string());
    }
    let is_function_key = function_key_number(&key).is_some();
    let has_modifier =
        keystroke.modifiers.control || keystroke.modifiers.alt || keystroke.modifiers.function;
    if !has_modifier && !is_function_key {
        return Err(
            "use Ctrl, Alt, or Fn, or a function key, so normal typing is not stolen".to_string(),
        );
    }
    let canonical = keystroke.unparse(os);
    if is_reserved(&canonical, &key) {
        return Err(format!(
            "{} is reserved for text editing or the application",
            pretty_keys(&canonical)
        ));
    }
    Ok(keystroke)
}

fn is_modifier_name(key: &str) -> bool {
    matches!(
        key,
        "shift"
            | "control"
            | "ctrl"
            | "alt"
            | "platform"
            | "cmd"
            | "super"
            | "win"
            | "fn"
            | "function"
    )
}

fn function_key_number(key: &str) -> Option<u8> {
    let number = key.strip_prefix('f')?.parse::<u8>().ok()?;
    (1..=24).contains(&number).then_some(number)
}

fn is_reserved(canonical: &str, key: &str) -> bool {
    if canonical == "escape" {
        return true;
    }
    let reserved_edit = matches!(
        key,
        "tab"
            | "enter"
            | "space"
            | "backspace"
            | "delete"
            | "insert"
            | "home"
            | "end"
            | "left"
            | "right"
            | "up"
            | "down"
            | "pageup"
            | "pagedown"
    );
    if reserved_edit && !canonical.contains("ctrl-") && !canonical.contains("alt-") {
        return true;
    }
    matches!(
        canonical,
        "ctrl-a"
            | "ctrl-c"
            | "ctrl-v"
            | "ctrl-x"
            | "ctrl-z"
            | "ctrl-y"
            | "ctrl-shift-z"
            | "ctrl-backspace"
            | "ctrl-delete"
            | "alt-f4"
    )
}

/// Human-readable font for stored accelerator text.
#[must_use]
pub fn pretty_keys(keys: &str) -> String {
    keys.split('-')
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_uppercase)
        .collect::<Vec<_>>()
        .join("+")
}

/// Bindings that must be removed and added to move from one keymap to another.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BindingPlan {
    /// Old keys to disable, in action order.
    pub unbinds: Vec<(MachineAction, String)>,
    /// New keys to add, in action order.
    pub binds: Vec<(MachineAction, String)>,
}

impl BindingPlan {
    /// Compare the active bindings with the desired bindings.
    #[must_use]
    pub fn between(previous: &[(MachineAction, String)], next: &[(MachineAction, String)]) -> Self {
        let mut plan = Self::default();
        for (action, old) in previous {
            let unchanged = next
                .iter()
                .any(|(new_action, new)| new_action == action && new == old);
            if !unchanged {
                plan.unbinds.push((*action, old.clone()));
            }
        }
        for (action, new) in next {
            let unchanged = previous
                .iter()
                .any(|(old_action, old)| old_action == action && old == new);
            if !unchanged {
                plan.binds.push((*action, new.clone()));
            }
        }
        plan
    }

    /// Whether the plan changes anything.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.unbinds.is_empty() && self.binds.is_empty()
    }
}

/// Latch one key press until its key-up, so OS key repeat cannot jog twice.
#[derive(Clone, Debug, Default)]
pub struct KeyHoldGuard {
    held: BTreeSet<MachineAction>,
    began: Option<Instant>,
}

impl KeyHoldGuard {
    /// Mark an action as held after its first dispatch.
    pub fn begin(&mut self, action: MachineAction, now: Instant) {
        self.held.insert(action);
        self.began = Some(now);
    }

    /// Whether a repeat dispatch must be ignored.
    #[must_use]
    pub fn is_held(&self, action: MachineAction, now: Instant) -> bool {
        self.held.contains(&action)
            && self
                .began
                .is_none_or(|began| now.duration_since(began) < KEY_HOLD_STALE_AFTER)
    }

    /// Release every action assigned to the physical key that went up.
    pub fn end_key(&mut self, key: &str, active: &[(MachineAction, String)]) {
        let key = key.trim().to_ascii_lowercase();
        for (action, keys) in active {
            if key_name(keys) == key {
                self.held.remove(action);
            }
        }
        if self.held.is_empty() {
            self.began = None;
        }
    }

    /// Drop every latched key, for example when the window loses focus.
    pub fn clear(&mut self) {
        self.held.clear();
        self.began = None;
    }
}

fn key_name(keys: &str) -> String {
    keys.rsplit_once('-')
        .map_or(keys, |(_, key)| key)
        .trim()
        .to_ascii_lowercase()
}

/// Map a key-down event to one active machine action, if any.
#[must_use]
pub fn action_for_keystroke(
    keystroke: &Keystroke,
    active: &[(MachineAction, String)],
    os: OperatingSystem,
) -> Option<MachineAction> {
    let canonical = keystroke.unparse(os);
    active
        .iter()
        .find(|(_, keys)| keys == &canonical)
        .map(|(action, _)| *action)
}

/// A GPUI binding for one typed machine action.
///
/// # Errors
///
/// Returns a message when GPUI rejects the accelerator text.
pub fn binding_for(action: MachineAction, keys: &str, cx: &App) -> Result<GpuiKeyBinding, String> {
    GpuiKeyBinding::load(keys, boxed_action(action), None, false, None, cx)
        .map_err(|error| format!("{}: {error}", action.label()))
}

/// A GPUI unbind marker for one old typed action binding.
///
/// # Errors
///
/// Returns a message when GPUI rejects the accelerator text.
pub fn unbind_for(action: MachineAction, keys: &str, cx: &App) -> Result<GpuiKeyBinding, String> {
    GpuiKeyBinding::load(
        keys,
        Box::new(Unbind(SharedString::from(action_name(action)))),
        None,
        false,
        None,
        cx,
    )
    .map_err(|error| format!("{}: {error}", action.label()))
}

fn boxed_action(action: MachineAction) -> Box<dyn Action> {
    match action {
        MachineAction::JogXMinus => Box::new(JogXMinus),
        MachineAction::JogXPlus => Box::new(JogXPlus),
        MachineAction::JogYMinus => Box::new(JogYMinus),
        MachineAction::JogYPlus => Box::new(JogYPlus),
        MachineAction::JogZMinus => Box::new(JogZMinus),
        MachineAction::JogZPlus => Box::new(JogZPlus),
        MachineAction::Start => Box::new(MachineStart),
        MachineAction::Pause => Box::new(MachinePause),
        MachineAction::Resume => Box::new(MachineResume),
        MachineAction::Stop => Box::new(MachineStop),
    }
}

fn action_name(action: MachineAction) -> &'static str {
    match action {
        MachineAction::JogXMinus => <JogXMinus as Action>::name_for_type(),
        MachineAction::JogXPlus => <JogXPlus as Action>::name_for_type(),
        MachineAction::JogYMinus => <JogYMinus as Action>::name_for_type(),
        MachineAction::JogYPlus => <JogYPlus as Action>::name_for_type(),
        MachineAction::JogZMinus => <JogZMinus as Action>::name_for_type(),
        MachineAction::JogZPlus => <JogZPlus as Action>::name_for_type(),
        MachineAction::Start => <MachineStart as Action>::name_for_type(),
        MachineAction::Pause => <MachinePause as Action>::name_for_type(),
        MachineAction::Resume => <MachineResume as Action>::name_for_type(),
        MachineAction::Stop => <MachineStop as Action>::name_for_type(),
    }
}

/// Feed a key-down event to the repeat guard.
pub fn note_key_down(
    guard: &mut KeyHoldGuard,
    event: &KeyDownEvent,
    active: &[(MachineAction, String)],
    os: OperatingSystem,
    now: Instant,
) {
    if let Some(action) = action_for_keystroke(&event.keystroke, active, os) {
        guard.begin(action, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINUX: OperatingSystem = OperatingSystem::Linux;

    fn gate() -> KeyboardGate {
        KeyboardGate {
            live: true,
            ready: true,
            machine: MachineState::Idle,
            job: JobState::Idle,
            program_loaded: true,
            probe_owns_program: false,
            tool_change_pending: false,
            watchdog_pause: false,
        }
    }

    #[test]
    fn defaults_are_complete_and_modifier_heavy() {
        let model = KeymapModel::default();
        assert_eq!(model.rows().len(), ALL_ACTIONS.len());
        assert!(model.is_default());
        for row in model.rows() {
            let parsed = parse_user_key(row.keys.as_str(), LINUX).expect("default parses");
            assert!(!parsed.key.is_empty());
        }
        assert_eq!(model.keys(MachineAction::JogXMinus), "ctrl-shift-left");
        assert_eq!(model.keys(MachineAction::JogYPlus), "ctrl-shift-up");
        assert_eq!(model.keys(MachineAction::JogZMinus), "ctrl-shift-pagedown");
        assert_eq!(model.keys(MachineAction::Start), "f5");
        assert_eq!(model.keys(MachineAction::Stop), "f8");
    }

    #[test]
    fn valid_binding_parses_and_canonicalizes() {
        let parsed = parse_user_key("Ctrl+Shift+PageUp", LINUX).expect("valid");
        assert_eq!(parsed.unparse(LINUX), "ctrl-shift-pageup");
        let plain = parse_user_key("Ctrl+S", LINUX).expect("valid");
        assert_eq!(plain.unparse(LINUX), "ctrl-s");
        let mut model = KeymapModel::default();
        let canonical = model
            .try_set(MachineAction::JogXMinus, "Ctrl+Shift+Home", LINUX)
            .expect("set");
        assert_eq!(canonical, "ctrl-shift-home");
        assert_eq!(model.keys(MachineAction::JogXMinus), "ctrl-shift-home");
        assert!(!model.is_default());
        model.reset_defaults();
        assert!(model.is_default());
    }

    #[test]
    fn empty_invalid_and_reserved_bindings_are_rejected() {
        let cases = [
            ("", "required"),
            ("not-a-key", "not a valid"),
            ("ctrl-shift", "not only a modifier"),
            ("a", "Ctrl, Alt, or Fn"),
            ("left", "Ctrl, Alt, or Fn"),
            ("ctrl-c", "reserved"),
            ("alt-f4", "reserved"),
            ("super-k", "reserved"),
        ];
        for (text, expected) in cases {
            let error = parse_user_key(text, LINUX).expect_err(text);
            assert!(
                error.contains(expected),
                "{text}: expected '{expected}', got '{error}'"
            );
        }
    }

    #[test]
    fn duplicate_keys_are_rejected_with_the_other_action() {
        let mut model = KeymapModel::default();
        let error = model
            .try_set(MachineAction::Start, "f6", LINUX)
            .expect_err("F6 is PAUSE");
        assert!(error.contains("PAUSE"), "{error}");
        assert_eq!(model.keys(MachineAction::Start), "f5");
        assert_eq!(model.keys(MachineAction::Pause), "f6");
    }

    #[test]
    fn binding_plan_unbinds_old_keys_and_binds_changed_keys() {
        let previous = vec![
            (MachineAction::Start, "f5".to_string()),
            (MachineAction::Pause, "f6".to_string()),
        ];
        let next = vec![
            (MachineAction::Start, "f9".to_string()),
            (MachineAction::Pause, "f6".to_string()),
        ];
        let plan = BindingPlan::between(&previous, &next);
        assert_eq!(plan.unbinds, vec![(MachineAction::Start, "f5".to_string())]);
        assert_eq!(plan.binds, vec![(MachineAction::Start, "f9".to_string())]);
        assert!(!plan.is_empty());
        assert!(BindingPlan::between(&next, &next).is_empty());
    }

    #[test]
    fn key_repeat_is_suppressed_until_key_up() {
        let active = KeymapModel::default().effective_bindings(LINUX);
        let mut guard = KeyHoldGuard::default();
        let now = Instant::now();
        guard.begin(MachineAction::JogXPlus, now);
        assert!(guard.is_held(MachineAction::JogXPlus, now + Duration::from_millis(40)));
        assert!(!guard.is_held(MachineAction::JogYPlus, now));
        guard.end_key("right", &active);
        assert!(!guard.is_held(MachineAction::JogXPlus, now));
        guard.begin(MachineAction::JogXPlus, now);
        guard.begin(MachineAction::JogYPlus, now);
        guard.end_key("right", &active);
        assert!(
            guard.is_held(MachineAction::JogYPlus, now),
            "another held axis is not released by the X key"
        );
    }

    #[test]
    fn actions_map_to_the_expected_intents() {
        let cases = [
            (
                MachineAction::JogXMinus,
                MachineIntent::Jog {
                    axis: JogAxis::X,
                    sign: -1.0,
                },
            ),
            (
                MachineAction::JogXPlus,
                MachineIntent::Jog {
                    axis: JogAxis::X,
                    sign: 1.0,
                },
            ),
            (
                MachineAction::JogYMinus,
                MachineIntent::Jog {
                    axis: JogAxis::Y,
                    sign: -1.0,
                },
            ),
            (
                MachineAction::JogZPlus,
                MachineIntent::Jog {
                    axis: JogAxis::Z,
                    sign: 1.0,
                },
            ),
            (MachineAction::Start, MachineIntent::Start),
            (MachineAction::Pause, MachineIntent::Pause),
            (MachineAction::Resume, MachineIntent::Resume),
            (MachineAction::Stop, MachineIntent::Stop),
        ];
        for (action, intent) in cases {
            assert_eq!(action.intent(), intent, "{}", action.label());
        }
        for action in ALL_ACTIONS {
            assert_eq!(
                action.allowed_while_sheet_open(),
                action == MachineAction::Stop,
                "{}",
                action.label()
            );
        }
    }

    #[test]
    fn gates_refuse_mock_alarm_door_and_job_active() {
        let mut mock = gate();
        mock.live = false;
        assert_eq!(
            refusal(MachineIntent::Start, &mock),
            Some("no live controller")
        );

        let mut alarm = gate();
        alarm.machine = MachineState::Alarm;
        assert!(
            refusal(MachineIntent::Stop, &alarm)
                .unwrap_or_default()
                .contains("Alarm")
        );

        let mut door = gate();
        door.machine = MachineState::Door;
        assert!(
            refusal(MachineIntent::Pause, &door)
                .unwrap_or_default()
                .contains("door")
        );

        let mut running = gate();
        running.job = JobState::Running;
        assert!(
            refusal(
                MachineIntent::Jog {
                    axis: JogAxis::X,
                    sign: 1.0,
                },
                &running
            )
            .unwrap_or_default()
            .contains("job is active")
        );
        assert_eq!(
            refusal(MachineIntent::Start, &running),
            Some("a job is already active")
        );
        assert!(refusal(MachineIntent::Pause, &running).is_none());

        let mut paused = gate();
        paused.job = JobState::Paused;
        paused.machine = MachineState::Hold;
        assert!(refusal(MachineIntent::Resume, &paused).is_none());
        assert!(refusal(MachineIntent::Stop, &paused).is_none());
        paused.tool_change_pending = true;
        assert!(
            refusal(MachineIntent::Resume, &paused)
                .unwrap_or_default()
                .contains("tool change")
        );
        paused.tool_change_pending = false;
        paused.watchdog_pause = true;
        assert!(
            refusal(MachineIntent::Resume, &paused)
                .unwrap_or_default()
                .contains("watchdog")
        );
    }

    #[test]
    fn loading_a_profile_builds_bindings_but_no_intent() {
        let mut source = Keymap::default();
        source
            .bindings
            .push(ConfigKeyBinding::new(MachineAction::Start.id(), "f9"));
        let model = KeymapModel::from_keymap(&source);
        let bindings = model.effective_bindings(LINUX);
        assert_eq!(bindings.len(), ALL_ACTIONS.len());
        assert_eq!(
            bindings
                .iter()
                .find(|(action, _)| *action == MachineAction::Start)
                .map(|(_, keys)| keys.as_str()),
            Some("f9")
        );
        // Loading only produces bindings. There is no action dispatch, queued
        // jog, or job start in this model at all.
        let plan = BindingPlan::between(&[], &bindings);
        assert_eq!(plan.binds.len(), ALL_ACTIONS.len());
        assert!(plan.unbinds.is_empty());
    }

    #[test]
    fn invalid_profile_keys_fall_back_to_defaults() {
        let mut source = Keymap::default();
        source.bindings.push(ConfigKeyBinding::new(
            MachineAction::Start.id(),
            "just-a-key",
        ));
        source.bindings.push(ConfigKeyBinding::new(
            MachineAction::Pause.id(),
            "not a keystroke",
        ));
        let model = KeymapModel::from_keymap(&source);
        let bindings = model.effective_bindings(LINUX);
        assert_eq!(
            bindings
                .iter()
                .find(|(action, _)| *action == MachineAction::Start)
                .map(|(_, keys)| keys.as_str()),
            Some("f5")
        );
        assert_eq!(
            bindings
                .iter()
                .find(|(action, _)| *action == MachineAction::Pause)
                .map(|(_, keys)| keys.as_str()),
            Some("f6")
        );
    }
}
