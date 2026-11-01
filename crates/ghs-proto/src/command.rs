//! Command encoding: realtime bytes, system commands and jog commands.
//!
//! Every encoder returns a single line **without** a line terminator. The
//! streamer adds `\n` when it sends the line, and that byte counts against the
//! controller's receive buffer.
//!
//! All encoders reject bytes that could inject a command or a realtime byte
//! into the controller stream. A line containing `CR`, `LF`, `NUL`, any other
//! control character, `DEL`, or a non-ASCII byte is rejected.

use ghs_core::{Axes, Position, SettingValue};

use crate::error::EncodeError;

/// Maximum length of a command line, in bytes.
pub const MAX_COMMAND_LINE: usize = 4096;

/// Largest jog feed accepted by the encoder, in millimetres per minute.
///
/// This is a sanity bound against obvious unit mistakes, not a machine limit;
/// the controller enforces its own maximum feed rate.
pub const MAX_JOG_FEED_MM_MIN: f64 = 1_000_000.0;

/// Maximum length of a setting value written with `$<n>=<value>`.
pub const MAX_SETTING_VALUE: usize = 512;

/// Maximum length of a startup line written with `$N<n>=<line>`.
pub const MAX_STARTUP_LINE: usize = 256;

/// Highest setting number accepted by the encoders.
pub const MAX_SETTING_NUMBER: u16 = 9999;

/// Highest startup-line index accepted by the encoders.
///
/// grbl and grblHAL provide two startup lines, `$N0` and `$N1`.
pub const MAX_STARTUP_LINE_INDEX: u8 = 1;

/// Validate a command line before it is handed to the transport.
///
/// # Errors
///
/// Returns [`EncodeError::InvalidCharacter`] for a control or non-ASCII byte and
/// [`EncodeError::LineTooLong`] for a line above [`MAX_COMMAND_LINE`].
pub fn validate_line(line: &str) -> Result<(), EncodeError> {
    if line.len() > MAX_COMMAND_LINE {
        return Err(EncodeError::LineTooLong {
            len: line.len(),
            limit: MAX_COMMAND_LINE,
        });
    }
    for (index, byte) in line.bytes().enumerate() {
        if byte != b'\t' && (byte < 0x20 || byte == 0x7F || byte >= 0x80) {
            return Err(EncodeError::InvalidCharacter { index, byte });
        }
    }
    Ok(())
}

/// Remove control and non-ASCII characters from a line.
///
/// This is a last-resort normaliser for display or logging. Command builders
/// use [`validate_line`] and reject bad input instead of silently changing it.
#[must_use]
pub fn sanitize_line(line: &str) -> String {
    line.chars()
        .filter(|ch| ch.is_ascii() && !ch.is_control())
        .collect()
}

fn validated_line(text: &str, max_len: usize) -> Result<String, EncodeError> {
    validate_line(text)?;
    if text.len() > max_len {
        return Err(EncodeError::LineTooLong {
            len: text.len(),
            limit: max_len,
        });
    }
    Ok(text.to_string())
}

fn validated_text(text: &str, max_len: usize) -> Result<String, EncodeError> {
    if text.trim().is_empty() {
        return Err(EncodeError::InvalidValue("empty value".into()));
    }
    validated_line(text, max_len)
}

fn validated_setting(number: u16) -> Result<u16, EncodeError> {
    if number > MAX_SETTING_NUMBER {
        Err(EncodeError::InvalidSettingNumber(number))
    } else {
        Ok(number)
    }
}

/// A grblHAL realtime command byte.
///
/// The three printable commands (`?`, `~`, `!`) have fixed extended
/// counterparts `0x80`, `0x81` and `0x82` when the controller advertises
/// `RT+`. These are **not** a bitwise OR of the printable byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RealtimeCommand {
    /// Status report request: `?`, or `0x80` when `RT+` is available.
    StatusReport,
    /// Cycle start / resume: `~`, or `0x81` when `RT+` is available.
    CycleStart,
    /// Feed hold: `!`, or `0x82` when `RT+` is available.
    FeedHold,
    /// Soft reset (`0x18`, Ctrl-X).
    SoftReset,
    /// Stop (`0x19`, Ctrl-Y); grblHAL extension.
    Stop,
    /// Exit (`0x03`, Ctrl-C); grblHAL extension.
    Exit,
    /// Reboot (`0x14`, Ctrl-T), valid only after an ESC (`0x1B`).
    Reboot,
    /// Program demarcation (`0x25`, `%`).
    ProgramDemarcation,
    /// Parser state report (`0x83`); shortcut for `$G`.
    ParserStateReport,
    /// Safety door (`0x84`).
    SafetyDoor,
    /// Jog cancel (`0x85`).
    JogCancel,
    /// Full status report (`0x87`).
    FullStatusReport,
    /// Optional-stop toggle (`0x88`).
    OptionalStopToggle,
    /// Single-block toggle (`0x89`).
    SingleBlockToggle,
    /// Fan 0 toggle (`0x8A`).
    Fan0Toggle,
    /// MPG mode toggle (`0x8B`).
    MpgModeToggle,
    /// Auto-reporting toggle (`0x8C`).
    AutoReportToggle,
    /// Feed override reset to 100 % (`0x90`).
    FeedOverrideReset,
    /// Feed override +10 % (`0x91`).
    FeedOverrideCoarsePlus,
    /// Feed override -10 % (`0x92`).
    FeedOverrideCoarseMinus,
    /// Feed override +1 % (`0x93`).
    FeedOverrideFinePlus,
    /// Feed override -1 % (`0x94`).
    FeedOverrideFineMinus,
    /// Rapid override reset to 100 % (`0x95`).
    RapidOverrideReset,
    /// Rapid override 50 % (`0x96`).
    RapidOverrideMedium,
    /// Rapid override 25 % (`0x97`).
    RapidOverrideLow,
    /// Rapid override 5 % (`0x98`).
    RapidOverrideExtraLow,
    /// Spindle override reset to 100 % (`0x99`).
    SpindleOverrideReset,
    /// Spindle override +10 % (`0x9A`).
    SpindleOverrideCoarsePlus,
    /// Spindle override -10 % (`0x9B`).
    SpindleOverrideCoarseMinus,
    /// Spindle override +1 % (`0x9C`).
    SpindleOverrideFinePlus,
    /// Spindle override -1 % (`0x9D`).
    SpindleOverrideFineMinus,
    /// Spindle stop override toggle (`0x9E`).
    SpindleStopToggle,
    /// Soft E-stop (`0x9F`): soft reset plus an E-stop alarm.
    SoftEStop,
    /// Flood coolant toggle (`0xA0`).
    FloodToggle,
    /// Mist coolant toggle (`0xA1`).
    MistToggle,
    /// PID report (`0xA2`).
    PidReport,
    /// Manual tool change acknowledge (`0xA3`).
    ToolChangeAck,
    /// Probe-connected toggle (`0xA4`).
    ProbeConnectedToggle,
    /// Plugin macro 0-7 (`0xB0`-`0xB7`).
    Macro(u8),
}

const fn macro_index(index: u8) -> u8 {
    if index > 7 { 7 } else { index }
}

impl RealtimeCommand {
    /// Single byte for this command.
    ///
    /// `rt_plus` selects the extended `0x80`/`0x81`/`0x82` forms of the
    /// printable commands. Every other command has one fixed byte.
    ///
    /// [`RealtimeCommand::Reboot`] returns `0x14`; use
    /// [`RealtimeCommand::bytes`] to get the required `ESC` prefix.
    #[must_use]
    pub const fn encode(self, rt_plus: bool) -> u8 {
        match self {
            RealtimeCommand::StatusReport => {
                if rt_plus {
                    0x80
                } else {
                    b'?'
                }
            }
            RealtimeCommand::CycleStart => {
                if rt_plus {
                    0x81
                } else {
                    b'~'
                }
            }
            RealtimeCommand::FeedHold => {
                if rt_plus {
                    0x82
                } else {
                    b'!'
                }
            }
            RealtimeCommand::SoftReset => 0x18,
            RealtimeCommand::Stop => 0x19,
            RealtimeCommand::Exit => 0x03,
            RealtimeCommand::Reboot => 0x14,
            RealtimeCommand::ProgramDemarcation => 0x25,
            RealtimeCommand::ParserStateReport => 0x83,
            RealtimeCommand::SafetyDoor => 0x84,
            RealtimeCommand::JogCancel => 0x85,
            RealtimeCommand::FullStatusReport => 0x87,
            RealtimeCommand::OptionalStopToggle => 0x88,
            RealtimeCommand::SingleBlockToggle => 0x89,
            RealtimeCommand::Fan0Toggle => 0x8A,
            RealtimeCommand::MpgModeToggle => 0x8B,
            RealtimeCommand::AutoReportToggle => 0x8C,
            RealtimeCommand::FeedOverrideReset => 0x90,
            RealtimeCommand::FeedOverrideCoarsePlus => 0x91,
            RealtimeCommand::FeedOverrideCoarseMinus => 0x92,
            RealtimeCommand::FeedOverrideFinePlus => 0x93,
            RealtimeCommand::FeedOverrideFineMinus => 0x94,
            RealtimeCommand::RapidOverrideReset => 0x95,
            RealtimeCommand::RapidOverrideMedium => 0x96,
            RealtimeCommand::RapidOverrideLow => 0x97,
            RealtimeCommand::RapidOverrideExtraLow => 0x98,
            RealtimeCommand::SpindleOverrideReset => 0x99,
            RealtimeCommand::SpindleOverrideCoarsePlus => 0x9A,
            RealtimeCommand::SpindleOverrideCoarseMinus => 0x9B,
            RealtimeCommand::SpindleOverrideFinePlus => 0x9C,
            RealtimeCommand::SpindleOverrideFineMinus => 0x9D,
            RealtimeCommand::SpindleStopToggle => 0x9E,
            RealtimeCommand::SoftEStop => 0x9F,
            RealtimeCommand::FloodToggle => 0xA0,
            RealtimeCommand::MistToggle => 0xA1,
            RealtimeCommand::PidReport => 0xA2,
            RealtimeCommand::ToolChangeAck => 0xA3,
            RealtimeCommand::ProbeConnectedToggle => 0xA4,
            RealtimeCommand::Macro(index) => 0xB0 + macro_index(index),
        }
    }

    /// Complete byte sequence for this command.
    ///
    /// Handles the `ESC` prefix required by [`RealtimeCommand::Reboot`].
    #[must_use]
    pub fn bytes(self, rt_plus: bool) -> Vec<u8> {
        match self {
            RealtimeCommand::Reboot => vec![0x1B, self.encode(rt_plus)],
            _ => vec![self.encode(rt_plus)],
        }
    }

    /// Resolve a received realtime byte.
    ///
    /// Both the printable and the extended forms map to the same command.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            b'?' | 0x80 => Some(RealtimeCommand::StatusReport),
            b'~' | 0x81 => Some(RealtimeCommand::CycleStart),
            b'!' | 0x82 => Some(RealtimeCommand::FeedHold),
            0x18 => Some(RealtimeCommand::SoftReset),
            0x19 => Some(RealtimeCommand::Stop),
            0x03 => Some(RealtimeCommand::Exit),
            0x14 => Some(RealtimeCommand::Reboot),
            0x25 => Some(RealtimeCommand::ProgramDemarcation),
            0x83 => Some(RealtimeCommand::ParserStateReport),
            0x84 => Some(RealtimeCommand::SafetyDoor),
            0x85 => Some(RealtimeCommand::JogCancel),
            0x87 => Some(RealtimeCommand::FullStatusReport),
            0x88 => Some(RealtimeCommand::OptionalStopToggle),
            0x89 => Some(RealtimeCommand::SingleBlockToggle),
            0x8A => Some(RealtimeCommand::Fan0Toggle),
            0x8B => Some(RealtimeCommand::MpgModeToggle),
            0x8C => Some(RealtimeCommand::AutoReportToggle),
            0x90 => Some(RealtimeCommand::FeedOverrideReset),
            0x91 => Some(RealtimeCommand::FeedOverrideCoarsePlus),
            0x92 => Some(RealtimeCommand::FeedOverrideCoarseMinus),
            0x93 => Some(RealtimeCommand::FeedOverrideFinePlus),
            0x94 => Some(RealtimeCommand::FeedOverrideFineMinus),
            0x95 => Some(RealtimeCommand::RapidOverrideReset),
            0x96 => Some(RealtimeCommand::RapidOverrideMedium),
            0x97 => Some(RealtimeCommand::RapidOverrideLow),
            0x98 => Some(RealtimeCommand::RapidOverrideExtraLow),
            0x99 => Some(RealtimeCommand::SpindleOverrideReset),
            0x9A => Some(RealtimeCommand::SpindleOverrideCoarsePlus),
            0x9B => Some(RealtimeCommand::SpindleOverrideCoarseMinus),
            0x9C => Some(RealtimeCommand::SpindleOverrideFinePlus),
            0x9D => Some(RealtimeCommand::SpindleOverrideFineMinus),
            0x9E => Some(RealtimeCommand::SpindleStopToggle),
            0x9F => Some(RealtimeCommand::SoftEStop),
            0xA0 => Some(RealtimeCommand::FloodToggle),
            0xA1 => Some(RealtimeCommand::MistToggle),
            0xA2 => Some(RealtimeCommand::PidReport),
            0xA3 => Some(RealtimeCommand::ToolChangeAck),
            0xA4 => Some(RealtimeCommand::ProbeConnectedToggle),
            0xB0..=0xB7 => Some(RealtimeCommand::Macro(byte - 0xB0)),
            _ => None,
        }
    }

    /// Stable name for logging.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            RealtimeCommand::StatusReport => "status report",
            RealtimeCommand::CycleStart => "cycle start",
            RealtimeCommand::FeedHold => "feed hold",
            RealtimeCommand::SoftReset => "soft reset",
            RealtimeCommand::Stop => "stop",
            RealtimeCommand::Exit => "exit",
            RealtimeCommand::Reboot => "reboot",
            RealtimeCommand::ProgramDemarcation => "program demarcation",
            RealtimeCommand::ParserStateReport => "parser state report",
            RealtimeCommand::SafetyDoor => "safety door",
            RealtimeCommand::JogCancel => "jog cancel",
            RealtimeCommand::FullStatusReport => "full status report",
            RealtimeCommand::OptionalStopToggle => "optional stop toggle",
            RealtimeCommand::SingleBlockToggle => "single block toggle",
            RealtimeCommand::Fan0Toggle => "fan 0 toggle",
            RealtimeCommand::MpgModeToggle => "MPG mode toggle",
            RealtimeCommand::AutoReportToggle => "auto report toggle",
            RealtimeCommand::FeedOverrideReset => "feed override reset",
            RealtimeCommand::FeedOverrideCoarsePlus => "feed override +10%",
            RealtimeCommand::FeedOverrideCoarseMinus => "feed override -10%",
            RealtimeCommand::FeedOverrideFinePlus => "feed override +1%",
            RealtimeCommand::FeedOverrideFineMinus => "feed override -1%",
            RealtimeCommand::RapidOverrideReset => "rapid override reset",
            RealtimeCommand::RapidOverrideMedium => "rapid override 50%",
            RealtimeCommand::RapidOverrideLow => "rapid override 25%",
            RealtimeCommand::RapidOverrideExtraLow => "rapid override 5%",
            RealtimeCommand::SpindleOverrideReset => "spindle override reset",
            RealtimeCommand::SpindleOverrideCoarsePlus => "spindle override +10%",
            RealtimeCommand::SpindleOverrideCoarseMinus => "spindle override -10%",
            RealtimeCommand::SpindleOverrideFinePlus => "spindle override +1%",
            RealtimeCommand::SpindleOverrideFineMinus => "spindle override -1%",
            RealtimeCommand::SpindleStopToggle => "spindle stop toggle",
            RealtimeCommand::SoftEStop => "soft E-stop",
            RealtimeCommand::FloodToggle => "flood coolant toggle",
            RealtimeCommand::MistToggle => "mist coolant toggle",
            RealtimeCommand::PidReport => "PID report",
            RealtimeCommand::ToolChangeAck => "tool change acknowledge",
            RealtimeCommand::ProbeConnectedToggle => "probe connected toggle",
            RealtimeCommand::Macro(_) => "plugin macro",
        }
    }
}

/// A `$` system command.
#[derive(Debug, Clone, PartialEq)]
pub enum SystemCommand {
    /// `$I` or the extended `$I+` identity report.
    Info {
        /// Request the extended `$I+` report.
        extended: bool,
    },
    /// `$I=<text>`: write the build-info string, when enabled.
    SetBuildInfo(String),
    /// `$$`: read all settings.
    Settings,
    /// `$+`: read all settings including extended ones.
    SettingsExtended,
    /// `$$=<n>`: details for one setting.
    SettingDetails(u16),
    /// `$<n>`: read one setting.
    ReadSetting(u16),
    /// `$<n>=<value>`: write one setting.
    WriteSetting {
        /// Setting number.
        number: u16,
        /// Value to write.
        value: SettingValue,
    },
    /// `$SED=<n>`: setting description.
    SettingsDescription(u16),
    /// `$G`: parser state.
    ParserState,
    /// `$#`: all `$#` parameters.
    Offsets,
    /// `$#=<n>`: one `$#` parameter.
    OffsetDetail(u16),
    /// `$N`: read the startup lines.
    StartupLines,
    /// `$N<n>`: read one startup line.
    ReadStartupLine(u8),
    /// `$N<n>=<line>`: write one startup line.
    WriteStartupLine {
        /// Startup line index, 0 or 1 on current builds.
        index: u8,
        /// Stored line, without a terminator.
        line: String,
    },
    /// `$H`: home all axes.
    HomeAll,
    /// `$H<axis>`: home one axis, for example `$HX`.
    HomeAxis(ghs_core::Axis),
    /// `$HSS`: homing switch state.
    HomingSwitchState,
    /// `$X`: unlock the alarm state.
    Unlock,
    /// `$SLP`: enter sleep.
    Sleep,
    /// `$C`: enable check mode; a reset exits it.
    CheckMode,
    /// `$B`: toggle block delete.
    ToggleBlockDelete,
    /// `$S`: toggle single block.
    ToggleSingleBlock,
    /// `$O`: toggle optional stop.
    ToggleOptionalStop,
    /// `$TLR`: set the tool length reference.
    ToolLengthReference,
    /// `$TPW`: probe the tool plate.
    ProbeToolPlate,
    /// `$RST=<target>`: reset stored data.
    Reset(ResetTarget),
    /// `$HELP` or `$HELP <topic>`.
    Help(Option<String>),
    /// A runtime-enumeration or extended command.
    Extended(ExtendedCommand),
}

/// Target of `$RST=...`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResetTarget {
    /// `$RST=*`: settings, parameters, startup lines and build info.
    All,
    /// `$RST=$`: core setting defaults.
    Settings,
    /// `$RST=#`: offsets and tool data.
    Offsets,
    /// `$RST=&`: driver and plugin defaults.
    DriverParameters,
}

impl ResetTarget {
    /// Wire target character.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ResetTarget::All => "*",
            ResetTarget::Settings => "$",
            ResetTarget::Offsets => "#",
            ResetTarget::DriverParameters => "&",
        }
    }
}

/// Extended `$` commands that are not part of the classic command set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExtendedCommand {
    /// `$EA`: enumerate alarms.
    Alarms,
    /// `$EE`: enumerate errors.
    Errors,
    /// `$ES`: enumerate settings.
    Settings,
    /// `$EG`: enumerate setting groups.
    SettingGroups,
    /// `$EAG`: enumerate alarms in CSV form.
    AlarmsCsv,
    /// `$EEG`: enumerate errors in CSV form.
    ErrorsCsv,
    /// `$ESG`: enumerate settings in CSV form.
    SettingsCsv,
    /// `$ESH`: enumerate settings in grblHAL form.
    SettingsHal,
    /// `$E*`: enumerate everything.
    Everything,
    /// `$PINS`: pin mapping.
    Pins,
    /// `$PINSTATE`: auxiliary pin states.
    PinState,
    /// `$PORTS`: UART ports.
    Ports,
    /// `$LIM`: current limit switch state.
    Limits,
    /// `$LEV`: last control and limit events.
    Events,
    /// `$SPINDLES`: enumerate spindles.
    Spindles,
    /// `$SPINDLESH`: enumerate spindles in grblHAL form.
    SpindlesHal,
    /// `$SD`: spindle encoder data.
    SpindleData,
    /// `$SR`: reset spindle encoder data.
    SpindleReset,
    /// `$SDS`: stepper driver status.
    StepperStatus,
    /// `$REBOOT`: reboot the controller, when supported.
    Reboot,
    /// `$RTC`: read or set the real-time clock.
    Rtc,
    /// `$DWNGRD`: toggle downgrade flags.
    Downgrade,
}

impl ExtendedCommand {
    /// The literal command line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ExtendedCommand::Alarms => "$EA",
            ExtendedCommand::Errors => "$EE",
            ExtendedCommand::Settings => "$ES",
            ExtendedCommand::SettingGroups => "$EG",
            ExtendedCommand::AlarmsCsv => "$EAG",
            ExtendedCommand::ErrorsCsv => "$EEG",
            ExtendedCommand::SettingsCsv => "$ESG",
            ExtendedCommand::SettingsHal => "$ESH",
            ExtendedCommand::Everything => "$E*",
            ExtendedCommand::Pins => "$PINS",
            ExtendedCommand::PinState => "$PINSTATE",
            ExtendedCommand::Ports => "$PORTS",
            ExtendedCommand::Limits => "$LIM",
            ExtendedCommand::Events => "$LEV",
            ExtendedCommand::Spindles => "$SPINDLES",
            ExtendedCommand::SpindlesHal => "$SPINDLESH",
            ExtendedCommand::SpindleData => "$SD",
            ExtendedCommand::SpindleReset => "$SR",
            ExtendedCommand::StepperStatus => "$SDS",
            ExtendedCommand::Reboot => "$REBOOT",
            ExtendedCommand::Rtc => "$RTC",
            ExtendedCommand::Downgrade => "$DWNGRD",
        }
    }
}

impl SystemCommand {
    /// Build `$I` or `$I+`.
    #[must_use]
    pub const fn info(extended: bool) -> Self {
        SystemCommand::Info { extended }
    }

    /// Build `$I+`.
    #[must_use]
    pub const fn info_extended() -> Self {
        SystemCommand::Info { extended: true }
    }

    /// Encode this command as a single line without a terminator.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] when a caller-supplied value contains an
    /// invalid character, is too long, or a number is out of range.
    pub fn encode(&self) -> Result<String, EncodeError> {
        let line = match self {
            SystemCommand::Info { extended } => {
                if *extended {
                    "$I+".to_string()
                } else {
                    "$I".to_string()
                }
            }
            SystemCommand::SetBuildInfo(text) => {
                format!("$I={}", validated_line(text, 128)?)
            }
            SystemCommand::Settings => "$$".to_string(),
            SystemCommand::SettingsExtended => "$+".to_string(),
            SystemCommand::SettingDetails(number) => {
                format!("$$={}", validated_setting(*number)?)
            }
            SystemCommand::ReadSetting(number) => {
                format!("${}", validated_setting(*number)?)
            }
            SystemCommand::WriteSetting { number, value } => {
                let number = validated_setting(*number)?;
                let value = value.to_string();
                validate_line(&value)?;
                if value.len() > MAX_SETTING_VALUE {
                    return Err(EncodeError::LineTooLong {
                        len: value.len(),
                        limit: MAX_SETTING_VALUE,
                    });
                }
                format!("${number}={value}")
            }
            SystemCommand::SettingsDescription(number) => {
                format!("$SED={}", validated_setting(*number)?)
            }
            SystemCommand::ParserState => "$G".to_string(),
            SystemCommand::Offsets => "$#".to_string(),
            SystemCommand::OffsetDetail(number) => {
                format!("$#={}", validated_setting(*number)?)
            }
            SystemCommand::StartupLines => "$N".to_string(),
            SystemCommand::ReadStartupLine(index) => {
                format!("$N{}", validated_startup_index(*index)?)
            }
            SystemCommand::WriteStartupLine { index, line } => {
                let index = validated_startup_index(*index)?;
                format!("$N{index}={}", validated_line(line, MAX_STARTUP_LINE)?)
            }
            SystemCommand::HomeAll => "$H".to_string(),
            SystemCommand::HomeAxis(axis) => format!("$H{}", axis.letter()),
            SystemCommand::HomingSwitchState => "$HSS".to_string(),
            SystemCommand::Unlock => "$X".to_string(),
            SystemCommand::Sleep => "$SLP".to_string(),
            SystemCommand::CheckMode => "$C".to_string(),
            SystemCommand::ToggleBlockDelete => "$B".to_string(),
            SystemCommand::ToggleSingleBlock => "$S".to_string(),
            SystemCommand::ToggleOptionalStop => "$O".to_string(),
            SystemCommand::ToolLengthReference => "$TLR".to_string(),
            SystemCommand::ProbeToolPlate => "$TPW".to_string(),
            SystemCommand::Reset(target) => format!("$RST={}", target.as_str()),
            SystemCommand::Help(topic) => match topic {
                Some(topic) => format!("$HELP {}", validated_text(topic, 64)?),
                None => "$HELP".to_string(),
            },
            SystemCommand::Extended(command) => command.as_str().to_string(),
        };
        validate_line(&line)?;
        Ok(line)
    }
}

fn validated_startup_index(index: u8) -> Result<u8, EncodeError> {
    if index > MAX_STARTUP_LINE_INDEX {
        Err(EncodeError::InvalidStartupLine(index))
    } else {
        Ok(index)
    }
}

/// A jog motion, encoded as `$J=G21 G90|G91 <axes> F<feed>`.
///
/// Jog distances are millimetres because the encoder always emits `G21`.
/// Absolute jogs target the active work coordinate system; machine-coordinate
/// jogs (`G53`) are not modelled here.
#[derive(Debug, Clone, PartialEq)]
pub struct JogCommand {
    /// Axes to move; must match [`JogCommand::distance`]'s axis mask.
    pub axes: Axes,
    /// Target or incremental distance in millimetres.
    pub distance: Position,
    /// Feed rate in millimetres per minute.
    pub feed: f64,
    /// Absolute or incremental mode.
    pub mode: JogMode,
}

/// Jog distance mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JogMode {
    /// `G91`, move by the given distance.
    Incremental,
    /// `G90`, move to the given position in the active work coordinate system.
    Absolute,
}

impl JogMode {
    /// Selecting G-code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            JogMode::Incremental => "G91",
            JogMode::Absolute => "G90",
        }
    }
}

impl JogCommand {
    /// Build a jog command, validating axes, coordinates and feed.
    ///
    /// # Errors
    ///
    /// Returns [`EncodeError`] for an empty or mismatched axis mask, a
    /// non-finite coordinate, or a feed that is not in `(0, MAX_JOG_FEED]`.
    pub fn new(
        axes: Axes,
        distance: Position,
        feed: f64,
        mode: JogMode,
    ) -> Result<Self, EncodeError> {
        if axes.is_empty() {
            return Err(EncodeError::EmptyJog);
        }
        if axes != distance.axes() {
            return Err(EncodeError::JogAxisMismatch);
        }
        if !distance.is_finite() {
            return Err(EncodeError::InvalidAxisValue);
        }
        if !feed.is_finite() || feed <= 0.0 || feed > MAX_JOG_FEED_MM_MIN {
            return Err(EncodeError::InvalidFeed(MAX_JOG_FEED_MM_MIN));
        }
        Ok(JogCommand {
            axes,
            distance,
            feed,
            mode,
        })
    }

    /// Build an incremental jog from a position whose axis mask selects the
    /// moving axes.
    ///
    /// # Errors
    ///
    /// See [`JogCommand::new`].
    pub fn incremental(distance: Position, feed: f64) -> Result<Self, EncodeError> {
        JogCommand::new(distance.axes(), distance, feed, JogMode::Incremental)
    }

    /// Build an absolute jog from a position whose axis mask selects the
    /// moving axes.
    ///
    /// # Errors
    ///
    /// See [`JogCommand::new`].
    pub fn absolute(distance: Position, feed: f64) -> Result<Self, EncodeError> {
        JogCommand::new(distance.axes(), distance, feed, JogMode::Absolute)
    }

    /// Encode the jog line without a terminator.
    ///
    /// # Errors
    ///
    /// Re-validates the public fields and rejects inconsistent state.
    pub fn encode(&self) -> Result<String, EncodeError> {
        Self::new(self.axes, self.distance, self.feed, self.mode)?;
        let mut line = String::with_capacity(80);
        line.push_str("$J=G21 ");
        line.push_str(self.mode.as_str());
        for (axis, value) in self.distance.iter() {
            let formatted = format!(" {axis}{value:.3}");
            line.push_str(&formatted);
        }
        let formatted = format!(" F{:.3}", self.feed);
        line.push_str(&formatted);
        validate_line(&line)?;
        Ok(line)
    }
}

impl std::fmt::Display for JogCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.encode() {
            Ok(line) => f.write_str(&line),
            Err(_) => f.write_str("<invalid jog>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghs_core::Axis;

    #[test]
    fn printable_realtime_commands_use_fixed_extended_codes() {
        assert_eq!(RealtimeCommand::StatusReport.encode(false), b'?');
        assert_eq!(RealtimeCommand::StatusReport.encode(true), 0x80);
        assert_ne!(RealtimeCommand::StatusReport.encode(true), b'?' | 0x80);

        assert_eq!(RealtimeCommand::CycleStart.encode(false), b'~');
        assert_eq!(RealtimeCommand::CycleStart.encode(true), 0x81);
        assert_eq!(RealtimeCommand::FeedHold.encode(false), b'!');
        assert_eq!(RealtimeCommand::FeedHold.encode(true), 0x82);
    }

    #[test]
    fn every_realtime_command_has_its_documented_byte() {
        let cases = [
            (RealtimeCommand::SoftReset, 0x18),
            (RealtimeCommand::Stop, 0x19),
            (RealtimeCommand::Exit, 0x03),
            (RealtimeCommand::Reboot, 0x14),
            (RealtimeCommand::ProgramDemarcation, 0x25),
            (RealtimeCommand::ParserStateReport, 0x83),
            (RealtimeCommand::SafetyDoor, 0x84),
            (RealtimeCommand::JogCancel, 0x85),
            (RealtimeCommand::FullStatusReport, 0x87),
            (RealtimeCommand::OptionalStopToggle, 0x88),
            (RealtimeCommand::SingleBlockToggle, 0x89),
            (RealtimeCommand::Fan0Toggle, 0x8A),
            (RealtimeCommand::MpgModeToggle, 0x8B),
            (RealtimeCommand::AutoReportToggle, 0x8C),
            (RealtimeCommand::FeedOverrideReset, 0x90),
            (RealtimeCommand::FeedOverrideCoarsePlus, 0x91),
            (RealtimeCommand::FeedOverrideCoarseMinus, 0x92),
            (RealtimeCommand::FeedOverrideFinePlus, 0x93),
            (RealtimeCommand::FeedOverrideFineMinus, 0x94),
            (RealtimeCommand::RapidOverrideReset, 0x95),
            (RealtimeCommand::RapidOverrideMedium, 0x96),
            (RealtimeCommand::RapidOverrideLow, 0x97),
            (RealtimeCommand::RapidOverrideExtraLow, 0x98),
            (RealtimeCommand::SpindleOverrideReset, 0x99),
            (RealtimeCommand::SpindleOverrideCoarsePlus, 0x9A),
            (RealtimeCommand::SpindleOverrideCoarseMinus, 0x9B),
            (RealtimeCommand::SpindleOverrideFinePlus, 0x9C),
            (RealtimeCommand::SpindleOverrideFineMinus, 0x9D),
            (RealtimeCommand::SpindleStopToggle, 0x9E),
            (RealtimeCommand::SoftEStop, 0x9F),
            (RealtimeCommand::FloodToggle, 0xA0),
            (RealtimeCommand::MistToggle, 0xA1),
            (RealtimeCommand::PidReport, 0xA2),
            (RealtimeCommand::ToolChangeAck, 0xA3),
            (RealtimeCommand::ProbeConnectedToggle, 0xA4),
        ];
        for (command, expected) in cases {
            assert_eq!(command.encode(false), expected, "{command:?}");
            assert_eq!(command.encode(true), expected, "{command:?}");
            assert_eq!(RealtimeCommand::from_byte(expected), Some(command));
            assert!(!command.name().is_empty());
        }
        for index in 0..=7u8 {
            let command = RealtimeCommand::Macro(index);
            assert_eq!(command.encode(false), 0xB0 + index);
            assert_eq!(RealtimeCommand::from_byte(0xB0 + index), Some(command));
        }
        assert_eq!(RealtimeCommand::Macro(99).encode(false), 0xB7);
        assert_eq!(RealtimeCommand::from_byte(0xB8), None);
        assert_eq!(RealtimeCommand::from_byte(0x00), None);
    }

    #[test]
    fn reboot_requires_an_escape_prefix() {
        assert_eq!(RealtimeCommand::Reboot.bytes(false), vec![0x1B, 0x14]);
        assert_eq!(RealtimeCommand::StatusReport.bytes(false), vec![b'?']);
        assert_eq!(RealtimeCommand::StatusReport.bytes(true), vec![0x80]);
    }

    #[test]
    fn system_command_golden_strings() {
        let cases: Vec<(SystemCommand, &str)> = vec![
            (SystemCommand::info(false), "$I"),
            (SystemCommand::info(true), "$I+"),
            (SystemCommand::Settings, "$$"),
            (SystemCommand::SettingsExtended, "$+"),
            (SystemCommand::Offsets, "$#"),
            (SystemCommand::ParserState, "$G"),
            (SystemCommand::StartupLines, "$N"),
            (SystemCommand::HomeAll, "$H"),
            (SystemCommand::Unlock, "$X"),
            (SystemCommand::Sleep, "$SLP"),
            (SystemCommand::CheckMode, "$C"),
            (SystemCommand::HomingSwitchState, "$HSS"),
            (SystemCommand::ToolLengthReference, "$TLR"),
            (SystemCommand::ProbeToolPlate, "$TPW"),
            (SystemCommand::ToggleBlockDelete, "$B"),
            (SystemCommand::ToggleSingleBlock, "$S"),
            (SystemCommand::ToggleOptionalStop, "$O"),
            (SystemCommand::Reset(ResetTarget::All), "$RST=*"),
            (SystemCommand::Reset(ResetTarget::Settings), "$RST=$"),
            (SystemCommand::Reset(ResetTarget::Offsets), "$RST=#"),
            (
                SystemCommand::Reset(ResetTarget::DriverParameters),
                "$RST=&",
            ),
            (SystemCommand::SettingsDescription(110), "$SED=110"),
            (SystemCommand::SettingDetails(110), "$$=110"),
            (SystemCommand::OffsetDetail(3), "$#=3"),
            (SystemCommand::ReadSetting(100), "$100"),
            (SystemCommand::ReadStartupLine(0), "$N0"),
            (SystemCommand::Help(None), "$HELP"),
            (
                SystemCommand::Help(Some("commands".into())),
                "$HELP commands",
            ),
            (SystemCommand::Extended(ExtendedCommand::Alarms), "$EA"),
            (SystemCommand::Extended(ExtendedCommand::Everything), "$E*"),
            (SystemCommand::Extended(ExtendedCommand::Pins), "$PINS"),
            (
                SystemCommand::Extended(ExtendedCommand::PinState),
                "$PINSTATE",
            ),
            (SystemCommand::Extended(ExtendedCommand::Ports), "$PORTS"),
            (SystemCommand::Extended(ExtendedCommand::Limits), "$LIM"),
            (SystemCommand::Extended(ExtendedCommand::Events), "$LEV"),
            (
                SystemCommand::Extended(ExtendedCommand::Spindles),
                "$SPINDLES",
            ),
            (
                SystemCommand::Extended(ExtendedCommand::SpindlesHal),
                "$SPINDLESH",
            ),
            (SystemCommand::Extended(ExtendedCommand::SpindleData), "$SD"),
            (
                SystemCommand::Extended(ExtendedCommand::SpindleReset),
                "$SR",
            ),
            (
                SystemCommand::Extended(ExtendedCommand::StepperStatus),
                "$SDS",
            ),
            (SystemCommand::Extended(ExtendedCommand::Reboot), "$REBOOT"),
            (SystemCommand::Extended(ExtendedCommand::Rtc), "$RTC"),
            (
                SystemCommand::Extended(ExtendedCommand::Downgrade),
                "$DWNGRD",
            ),
        ];
        for (command, expected) in cases {
            assert_eq!(command.encode().as_deref(), Ok(expected), "{command:?}");
        }
    }

    #[test]
    fn single_axis_homing_uses_one_axis_per_command() {
        assert_eq!(
            SystemCommand::HomeAxis(ghs_core::Axis::X)
                .encode()
                .as_deref(),
            Ok("$HX")
        );
        assert_eq!(
            SystemCommand::HomeAxis(ghs_core::Axis::Z)
                .encode()
                .as_deref(),
            Ok("$HZ")
        );
        assert_eq!(SystemCommand::HomeAll.encode().as_deref(), Ok("$H"));
    }

    #[test]
    fn setting_and_startup_writes() {
        let write = SystemCommand::WriteSetting {
            number: 100,
            value: SettingValue::Float(250.0),
        };
        assert_eq!(write.encode().as_deref(), Ok("$100=250"));

        let startup = SystemCommand::WriteStartupLine {
            index: 1,
            line: "G54".into(),
        };
        assert_eq!(startup.encode().as_deref(), Ok("$N1=G54"));

        let build = SystemCommand::SetBuildInfo("shop mill".into());
        assert_eq!(build.encode().as_deref(), Ok("$I=shop mill"));

        let clear = SystemCommand::WriteStartupLine {
            index: 0,
            line: String::new(),
        };
        assert_eq!(clear.encode().as_deref(), Ok("$N0="));
        assert_eq!(
            SystemCommand::SetBuildInfo(String::new())
                .encode()
                .as_deref(),
            Ok("$I=")
        );
    }

    #[test]
    fn system_commands_reject_injection() {
        for payload in ["G0\nM3", "G0\rM3", "G0\0M3", "G0\u{7f}", "caf\u{e9}"] {
            let command = SystemCommand::WriteStartupLine {
                index: 0,
                line: payload.into(),
            };
            assert!(
                matches!(command.encode(), Err(EncodeError::InvalidCharacter { .. })),
                "{payload:?}"
            );
            let help = SystemCommand::Help(Some(payload.into()));
            assert!(help.encode().is_err());
            let build = SystemCommand::SetBuildInfo(payload.into());
            assert!(build.encode().is_err());
        }
        assert!(
            SystemCommand::WriteSetting {
                number: 100,
                value: SettingValue::String("1\nM3".into()),
            }
            .encode()
            .is_err()
        );
        assert!(SystemCommand::ReadSetting(10_000).encode().is_err());
        assert!(SystemCommand::ReadStartupLine(2).encode().is_err());
        assert!(SystemCommand::Help(Some("   ".into())).encode().is_err());
    }

    #[test]
    fn jog_encoding_is_golden() {
        let incremental =
            JogCommand::incremental(Position::xyz(1.0, -2.5, 0.0), 1000.0).expect("valid jog");
        assert_eq!(
            incremental.encode().as_deref(),
            Ok("$J=G21 G91 X1.000 Y-2.500 Z0.000 F1000.000")
        );
        let absolute =
            JogCommand::absolute(Position::xyz(0.0, 0.0, 5.0), 500.0).expect("valid jog");
        assert_eq!(
            absolute.encode().as_deref(),
            Ok("$J=G21 G90 X0.000 Y0.000 Z5.000 F500.000")
        );
        let mut z_only = Position::new(Axes::from(Axis::Z));
        z_only.set(Axis::Z, 5.0);
        assert_eq!(
            JogCommand::absolute(z_only, 500.0)
                .expect("valid")
                .encode()
                .as_deref(),
            Ok("$J=G21 G90 Z5.000 F500.000")
        );
    }

    #[test]
    fn jog_rejects_bad_values() {
        assert_eq!(
            JogCommand::incremental(Position::new(Axes::empty()), 100.0),
            Err(EncodeError::EmptyJog)
        );
        assert_eq!(
            JogCommand::incremental(Position::xyz(f64::NAN, 0.0, 0.0), 100.0),
            Err(EncodeError::InvalidAxisValue)
        );
        assert_eq!(
            JogCommand::incremental(Position::xyz(1.0, 0.0, 0.0), 0.0),
            Err(EncodeError::InvalidFeed(MAX_JOG_FEED_MM_MIN))
        );
        assert!(JogCommand::incremental(Position::xyz(1.0, 0.0, 0.0), -5.0).is_err());
        assert!(JogCommand::incremental(Position::xyz(1.0, 0.0, 0.0), f64::INFINITY).is_err());
        assert!(
            JogCommand::incremental(Position::xyz(1.0, 0.0, 0.0), MAX_JOG_FEED_MM_MIN + 1.0)
                .is_err()
        );

        let mut x_only = Position::new(Axes::from(Axis::X));
        x_only.set(Axis::X, 1.0);
        let mut mismatch = JogCommand::incremental(x_only, 100.0).expect("valid");
        mismatch.axes = Axes::xyz();
        assert_eq!(mismatch.encode(), Err(EncodeError::JogAxisMismatch));
    }

    #[test]
    fn jog_handles_extra_axes() {
        let mut distance = Position::new(Axes::parse("XYZAB").expect("valid"));
        distance.set(Axis::A, 90.0);
        distance.set(Axis::B, -45.0);
        let jog = JogCommand::incremental(distance, 250.0).expect("valid");
        assert_eq!(
            jog.encode().as_deref(),
            Ok("$J=G21 G91 X0.000 Y0.000 Z0.000 A90.000 B-45.000 F250.000")
        );
    }

    #[test]
    fn sanitize_and_validate() {
        assert_eq!(sanitize_line("G0 X1\r\nM3"), "G0 X1M3");
        assert_eq!(sanitize_line("caf\u{e9}"), "caf");
        assert!(validate_line("G0 X1 Y2").is_ok());
        assert!(validate_line("G0 X1\tY2").is_ok());
        assert!(matches!(
            validate_line("G0\r\nX1"),
            Err(EncodeError::InvalidCharacter { .. })
        ));
        assert!(validate_line("").is_ok());
        assert!(validate_line(&"G0".repeat(MAX_COMMAND_LINE)).is_err());
    }
}
