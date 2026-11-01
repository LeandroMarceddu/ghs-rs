//! Mock machine state for the M0 shell.
//!
//! No serial, protocol, or engine code is involved. `Profile` is a plain data
//! value: panels and axis rows are derived from `Capabilities`, never from a
//! board name or a compile-time feature. The reference machine is one profile,
//! not a code path; a generic or extended controller can supply a different
//! value without touching this module's consumers.

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
    /// Reserved for G20 display at M2; the M0 mock is metric only.
    #[allow(dead_code)]
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

    /// Post-v1.0 panels exposed by this profile. The M0 shell only reports
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
    pub name: &'static str,
    pub capabilities: Capabilities,
}

impl Profile {
    pub fn mock_reference() -> Self {
        Self {
            name: "Mock 3-axis mill",
            capabilities: Capabilities::mock_reference(),
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
}

#[derive(Clone, Debug)]
pub struct ConsoleLine {
    pub time: &'static str,
    pub level: ConsoleLevel,
    pub text: String,
}

impl ConsoleLine {
    fn new(level: ConsoleLevel, text: impl Into<String>) -> Self {
        Self {
            time: "----",
            level,
            text: text.into(),
        }
    }
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

const WCS: [&str; 6] = ["G54", "G55", "G56", "G57", "G58", "G59"];
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

/// Everything the shell draws. Plain data, no GPUI types.
pub struct AppState {
    pub profile: Profile,
    pub mode: Mode,
    pub left_tab: LeftTab,
    pub dark: bool,
    pub machine_state: &'static str,
    pub job_name: &'static str,
    pub dro: Vec<f64>,
    pub wcs_index: usize,
    pub jog_axis: Axis,
    pub jog_step: f64,
    pub continuous: bool,
    pub feed_override: u16,
    pub spindle_override: u16,
    pub spindle_on: bool,
    pub spindle_cw: bool,
    pub rpm: u32,
    pub coolant: [bool; 3],
    pub console: Vec<ConsoleLine>,
    pub program: Vec<ProgramLine>,
    pub active_line: usize,
    pub progress: f32,
    pub keypad: Option<KeypadState>,
    pub mdi: String,
    pub macros: Vec<Macro>,
    pub signals: Vec<Signal>,
    pub feed_rate: u32,
}

impl AppState {
    pub fn new() -> Self {
        let profile = Profile::mock_reference();
        let dro = initial_dro(&profile.capabilities.axes);
        let signals = build_signals(&profile.capabilities, "Idle");
        Self {
            profile,
            mode: Mode::Job,
            left_tab: LeftTab::Console,
            dark: true,
            machine_state: "Idle",
            job_name: "shop-sign-pocket.nc",
            dro,
            wcs_index: 0,
            jog_axis: Axis::X,
            jog_step: 1.0,
            continuous: false,
            feed_override: 100,
            spindle_override: 100,
            spindle_on: true,
            spindle_cw: true,
            rpm: 12_000,
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
            feed_rate: 1200,
        }
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
            format!("G10 L20 P1 {}0.000 (mock)", axis.label()),
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
                self.push_console(ConsoleLevel::Info, "MDI text updated with keypad");
            }
            KeypadTarget::Dro(axis) => match keypad.buffer.trim().parse::<f64>() {
                Ok(value) => {
                    self.set_value(axis, value);
                    self.push_console(
                        ConsoleLevel::Command,
                        format!("{} = {:.3} mm (mock keypad)", axis.label(), value),
                    );
                }
                Err(_) => self.push_console(
                    ConsoleLevel::Warn,
                    format!("'{}' is not a number", keypad.buffer),
                ),
            },
            KeypadTarget::Rpm => match keypad.buffer.trim().parse::<f64>() {
                Ok(value) => {
                    self.rpm = value.clamp(0.0, 24_000.0) as u32;
                    self.push_console(
                        ConsoleLevel::Command,
                        format!("S{} (mock keypad)", self.rpm),
                    );
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

fn build_signals(capabilities: &Capabilities, machine_state: &str) -> Vec<Signal> {
    let mut out = Vec::new();
    if capabilities.limit_pins {
        for axis in capabilities.axes.iter().take(3) {
            out.push(Signal {
                label: format!("{} LIMIT", axis.label()),
                state: SignalState::Ok,
            });
        }
    }
    if capabilities.has_probe {
        out.push(Signal {
            label: "PROBE".into(),
            state: SignalState::Warn,
        });
    }
    if capabilities.has_estop {
        out.push(Signal {
            label: "E-STOP".into(),
            state: if machine_state == "Alarm" {
                SignalState::Alert
            } else {
                SignalState::Ok
            },
        });
    }
    if capabilities.has_door {
        out.push(Signal {
            label: "DOOR".into(),
            state: SignalState::Off,
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
            line.time = "10:42";
            line
        })
        .collect()
}
