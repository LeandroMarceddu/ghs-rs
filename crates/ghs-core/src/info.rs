//! Controller identity and capabilities parsed from the grblHAL `$I`/`$I+`
//! report.

use serde::{Deserialize, Serialize};

use crate::axis::{AXIS_COUNT, Axes, Axis};
use crate::signals::Signals;

/// Maximum length kept for a single text field from the controller.
const MAX_FIELD_LEN: usize = 256;
/// Maximum number of plugins or option tokens kept.
const MAX_LIST_ITEMS: usize = 64;

/// Digital and analog IO port counts from the `[AUX IO:...]` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AuxIoCounts {
    /// Number of digital inputs.
    pub digital_in: u16,
    /// Number of digital outputs.
    pub digital_out: u16,
    /// Number of analog inputs.
    pub analog_in: u16,
    /// Number of analog outputs.
    pub analog_out: u16,
}

impl AuxIoCounts {
    /// Returns `true` when the controller reported no auxiliary IO at all.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.digital_in == 0 && self.digital_out == 0 && self.analog_in == 0 && self.analog_out == 0
    }

    /// Total number of ports.
    #[must_use]
    pub const fn total(self) -> u32 {
        self.digital_in as u32
            + self.digital_out as u32
            + self.analog_in as u32
            + self.analog_out as u32
    }
}

/// Controller identity and capabilities from the `$I`/`$I+` report.
///
/// Every field is optional except the version, which is empty when the
/// controller did not answer with a `[VER:...]` line. Parsing is additive:
/// unknown lines are ignored and no line is required.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ControllerInfo {
    /// Firmware version, for example `"1.1f"`.
    pub version: String,
    /// Build date or identifier, for example `"20240604"`.
    pub build_info: String,
    /// Optional trailing text from the `[VER:<version>.<build>:<info>]` line.
    pub info: String,
    /// Firmware name, for example `"grblHAL"`.
    pub firmware: Option<String>,
    /// MCU or driver name, for example `"STM32F446"`.
    pub driver: Option<String>,
    /// Driver version, for example `"240428"`.
    pub driver_version: Option<String>,
    /// Driver options, for example `"USB"`.
    pub driver_options: Option<String>,
    /// Board name.
    pub board: Option<String>,
    /// Axes reported by `[AXS:...]` or the `OPT` axis count.
    pub axes: Axes,
    /// Raw `OPT` option string, for example `"VNMSL+2"`.
    pub options: String,
    /// Leading option letters from `OPT`, for example `"VNMSL"`.
    pub option_flags: String,
    /// Tokens from the `[NEWOPT:...]` line, in report order.
    pub new_options: Vec<String>,
    /// Input signals the controller can report (capability, not live state).
    pub signals: Signals,
    /// Non-volatile storage description, for example `"*EEPROM"` without the
    /// leading `*`.
    pub nvs: Option<String>,
    /// Free controller memory in kilobytes, as reported.
    pub free_memory_kb: Option<u32>,
    /// Auxiliary IO counts from `[AUX IO:...]`.
    pub aux_io: Option<AuxIoCounts>,
    /// Plugin descriptions, in report order.
    pub plugins: Vec<String>,
    /// Spindle description, for example `"Huanyang v1"`.
    pub spindle: Option<String>,
    /// Serial receive buffer size in bytes, from the second numeric `OPT`
    /// field.
    pub rx_buffer_size: Option<usize>,
    /// Planner/block buffer size, from the first numeric `OPT` field.
    pub planner_blocks: Option<usize>,
    /// Number of controller-side tool table entries, from the `OPT` line.
    pub tool_table_entries: Option<usize>,
}

impl ControllerInfo {
    /// Create an empty capability record.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of axes reported by the controller.
    #[must_use]
    pub fn axis_count(&self) -> usize {
        self.axes.len()
    }

    /// Axes in wire order.
    #[must_use]
    pub fn axis_list(&self) -> Vec<Axis> {
        self.axes.iter().collect()
    }

    /// Serial receive buffer size, falling back to `default` when unreported.
    ///
    /// Always configure the streamer from [`ControllerInfo::rx_buffer_size`]
    /// when it is present. The reference FlexiHAL build reports 1024 bytes;
    /// its `64` is the planner/block buffer, not the serial receive buffer.
    #[must_use]
    pub fn rx_buffer_size_or(&self, default: usize) -> usize {
        self.rx_buffer_size.unwrap_or(default)
    }

    /// Returns `true` when the controller identified itself as grblHAL.
    #[must_use]
    pub fn is_grblhal(&self) -> bool {
        self.firmware
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("grblHAL"))
    }

    /// Returns `true` when a `NEWOPT` token is present (exact match).
    #[must_use]
    pub fn has_new_option(&self, token: &str) -> bool {
        self.new_options.iter().any(|option| option == token)
    }

    /// Returns `true` when a legacy `OPT` option letter is present.
    #[must_use]
    pub fn has_option(&self, letter: char) -> bool {
        self.option_flags.contains(letter)
    }

    /// Settings descriptions are available (`SED`).
    #[must_use]
    pub fn has_sed(&self) -> bool {
        self.has_new_option("SED")
    }

    /// Extended real-time command codes are available (`RT+`).
    #[must_use]
    pub fn rt_plus(&self) -> bool {
        self.has_new_option("RT+")
    }

    /// Homing is enabled (`HOME`).
    #[must_use]
    pub fn homing(&self) -> bool {
        self.has_new_option("HOME")
    }

    /// An E-stop signal is available (`ES`).
    #[must_use]
    pub fn estop(&self) -> bool {
        self.has_new_option("ES")
    }

    /// Expressions are supported (`EXPR`).
    #[must_use]
    pub fn expressions(&self) -> bool {
        self.has_new_option("EXPR")
    }

    /// Manual tool change is available (`TC`).
    #[must_use]
    pub fn manual_tool_change(&self) -> bool {
        self.has_new_option("TC")
    }

    /// Automatic tool change is online (`ATC` or `ATC=1`).
    ///
    /// `ATC=0` means ATC is configured but offline and is not reported as
    /// available.
    #[must_use]
    pub fn has_atc(&self) -> bool {
        self.new_options
            .iter()
            .any(|option| option == "ATC" || option == "ATC=1")
    }

    /// SD card streaming is available (`SD`).
    #[must_use]
    pub fn has_sd(&self) -> bool {
        self.has_new_option("SD")
    }

    /// YModem file transfer is available (`YM`).
    #[must_use]
    pub fn has_ymodem(&self) -> bool {
        self.has_new_option("YM")
    }

    /// MPG/pendant mode is available (`MPG`).
    #[must_use]
    pub fn has_mpg(&self) -> bool {
        self.has_new_option("MPG")
    }

    /// A probe-connected signal is available (`PC`).
    #[must_use]
    pub fn has_probe_connected_signal(&self) -> bool {
        self.has_new_option("PC")
    }

    /// Optional-stop signal handling is available (`OS`).
    #[must_use]
    pub fn has_optional_stop_signal(&self) -> bool {
        self.has_new_option("OS")
    }

    /// Block-delete signal handling is available (`BD`).
    #[must_use]
    pub fn has_block_delete_signal(&self) -> bool {
        self.has_new_option("BD")
    }

    /// Spindle synchronization is available (`SS`).
    #[must_use]
    pub fn has_spindle_sync(&self) -> bool {
        self.has_new_option("SS")
    }

    /// Lathe mode is configured (`LATHE`).
    #[must_use]
    pub fn has_lathe(&self) -> bool {
        self.has_new_option("LATHE")
    }

    /// Odometer data is available (`ODO`).
    #[must_use]
    pub fn has_odometer(&self) -> bool {
        self.has_new_option("ODO")
    }

    /// Auxiliary IO is present on the controller.
    #[must_use]
    pub fn has_aux_io(&self) -> bool {
        self.aux_io.is_some_and(|aux| !aux.is_empty())
    }

    /// The controller has a tool table.
    #[must_use]
    pub fn has_tool_table(&self) -> bool {
        self.tool_table_entries.is_some_and(|entries| entries > 0)
    }

    /// Returns `true` when the controller advertises a signal capability.
    #[must_use]
    pub fn has_signal(&self, signal: Signals) -> bool {
        self.signals.contains(signal)
    }

    /// Apply one `$I+` response line.
    ///
    /// The line may be bracketed (`[TAG:value]`) or bare (`TAG:value`). Returns
    /// `true` when the tag was recognised. Unknown tags and malformed lines are
    /// ignored, never fatal.
    pub fn apply_line(&mut self, line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return false;
        }
        let inner = trimmed
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(trimmed);
        let Some((tag, value)) = inner.split_once(':') else {
            return false;
        };
        let tag = tag.trim().to_ascii_uppercase();
        let value = value.trim();
        match tag.as_str() {
            "VER" => {
                self.apply_version(value);
                true
            }
            "OPT" => {
                self.apply_options(value);
                true
            }
            "AXS" => {
                self.apply_axes(value);
                true
            }
            "NEWOPT" => {
                self.new_options = split_list(value);
                true
            }
            "FIRMWARE" => {
                self.firmware = bounded_option(value, MAX_FIELD_LEN);
                true
            }
            "SIGNALS" => {
                self.signals = Signals::parse(value);
                true
            }
            "NVS STORAGE" => {
                self.nvs = bounded_option(value.trim_start_matches('*'), MAX_FIELD_LEN);
                true
            }
            "FREE MEMORY" => {
                self.free_memory_kb = parse_free_memory(value);
                true
            }
            "DRIVER" => {
                self.driver = bounded_option(value, MAX_FIELD_LEN);
                true
            }
            "DRIVER VERSION" => {
                self.driver_version = bounded_option(value, MAX_FIELD_LEN);
                true
            }
            "DRIVER OPTIONS" => {
                self.driver_options = bounded_option(value, MAX_FIELD_LEN);
                true
            }
            "BOARD" => {
                self.board = bounded_option(value, MAX_FIELD_LEN);
                true
            }
            "AUX IO" => {
                self.aux_io = parse_aux_io(value);
                true
            }
            "PLUGIN" => {
                if self.plugins.len() < MAX_LIST_ITEMS
                    && let Some(plugin) = bounded_option(value, MAX_FIELD_LEN)
                {
                    self.plugins.push(plugin);
                }
                true
            }
            "SPINDLE" => {
                self.spindle = bounded_option(value, MAX_FIELD_LEN);
                true
            }
            _ => false,
        }
    }

    fn apply_version(&mut self, value: &str) {
        let (version_field, info) = match value.split_once(':') {
            Some((version, info)) => (version.trim(), info.trim()),
            None => (value, ""),
        };
        let (version, build_info) = split_build(version_field);
        self.version = version;
        self.build_info = build_info;
        self.info = bounded(info, MAX_FIELD_LEN);
    }

    fn apply_options(&mut self, value: &str) {
        let mut fields = value.split(',');
        if let Some(options) = fields.next() {
            self.options = bounded(options.trim(), MAX_FIELD_LEN);
            self.option_flags = self
                .options
                .chars()
                .take_while(char::is_ascii_alphabetic)
                .collect();
        }
        // grblHAL order: block buffer size, RX buffer size, axes, tool entries.
        // Parse positionally so an empty optional field cannot shift the rest.
        let mut number = || {
            fields
                .next()
                .and_then(|field| field.trim().parse::<usize>().ok())
        };
        self.planner_blocks = number().filter(|blocks| *blocks > 0);
        self.rx_buffer_size = number().filter(|size| *size > 0);
        let opt_axes = number().filter(|count| *count > 0);
        self.tool_table_entries = number();
        if self.axes.is_empty()
            && let Some(count) = opt_axes
        {
            self.axes = Axes::from_iter(Axis::ALL.into_iter().take(count.min(AXIS_COUNT)));
        }
    }

    fn apply_axes(&mut self, value: &str) {
        let mut fields = value.split(':');
        let count = fields
            .next()
            .and_then(|count| count.trim().parse::<usize>().ok());
        let letters = fields.next().unwrap_or("").trim();
        if !letters.is_empty()
            && let Ok(axes) = Axes::parse(letters)
        {
            self.axes = axes;
            return;
        }
        if let Some(count) = count.filter(|count| *count > 0) {
            self.axes = Axes::from_iter(Axis::ALL.into_iter().take(count.min(AXIS_COUNT)));
        }
    }
}

/// Parse a complete `$I`/`$I+` response.
///
/// Lines may be bracketed or bare; unknown lines are ignored. The result is
/// always a valid [`ControllerInfo`], even for an empty or garbage response.
#[must_use]
pub fn parse_controller_info<S: AsRef<str>>(lines: &[S]) -> ControllerInfo {
    let mut info = ControllerInfo::new();
    for line in lines {
        info.apply_line(line.as_ref());
    }
    info
}

/// Split `1.1f.20240604` into `("1.1f", "20240604")`.
///
/// A trailing numeric component of at least six digits is treated as the
/// build identifier; anything else is part of the version.
fn split_build(field: &str) -> (String, String) {
    if let Some((version, suffix)) = field.rsplit_once('.')
        && suffix.len() >= 6
        && suffix.chars().all(|ch| ch.is_ascii_digit())
    {
        return (
            bounded(version, MAX_FIELD_LEN),
            bounded(suffix, MAX_FIELD_LEN),
        );
    }
    (bounded(field, MAX_FIELD_LEN), String::new())
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .take(MAX_LIST_ITEMS)
        .map(|token| bounded(token, MAX_FIELD_LEN))
        .collect()
}

fn parse_free_memory(value: &str) -> Option<u32> {
    let value = value.trim();
    let digits = value.strip_suffix(['K', 'k']).unwrap_or(value);
    digits.trim().parse::<u32>().ok()
}

fn parse_aux_io(value: &str) -> Option<AuxIoCounts> {
    let mut fields = value
        .split(',')
        .map(|field| field.trim().parse::<u16>().ok());
    let mut next = || fields.next().flatten();
    let digital_in = next()?;
    Some(AuxIoCounts {
        digital_in,
        digital_out: next().unwrap_or(0),
        analog_in: next().unwrap_or(0),
        analog_out: next().unwrap_or(0),
    })
}

fn bounded(text: &str, max_len: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= max_len {
        trimmed.to_string()
    } else {
        trimmed.chars().take(max_len).collect()
    }
}

fn bounded_option(text: &str, max_len: usize) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(bounded(trimmed, max_len))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[&str] = &[
        "[VER:1.1f.20240604:]",
        "[OPT:VNMSL+2,64,1024,3,0]",
        "[AXS:3:XYZ]",
        "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]",
        "[FIRMWARE:grblHAL]",
        "[SIGNALS:HSDEP]",
        "[NVS STORAGE:*EEPROM]",
        "[FREE MEMORY:80K]",
        "[DRIVER:STM32F446]",
        "[DRIVER VERSION:240428]",
        "[BOARD:FlexiHAL_20240604_210624]",
        "[AUX IO:4,4,0,0]",
        "[PLUGIN:MODBUS v0.16]",
        "[SPINDLE:Huanyang v1]",
    ];

    #[test]
    fn parses_reference_profile() {
        let info = parse_controller_info(FIXTURE);
        assert_eq!(info.version, "1.1f");
        assert_eq!(info.build_info, "20240604");
        assert_eq!(info.firmware.as_deref(), Some("grblHAL"));
        assert_eq!(info.driver.as_deref(), Some("STM32F446"));
        assert_eq!(info.driver_version.as_deref(), Some("240428"));
        assert_eq!(info.board.as_deref(), Some("FlexiHAL_20240604_210624"));
        assert_eq!(info.axes.to_string(), "XYZ");
        assert_eq!(info.axis_count(), 3);
        assert_eq!(info.options, "VNMSL+2");
        assert_eq!(info.option_flags, "VNMSL");
        assert_eq!(info.rx_buffer_size, Some(1024));
        assert_eq!(info.planner_blocks, Some(64));
        assert_eq!(info.tool_table_entries, Some(0));
        assert_eq!(info.nvs.as_deref(), Some("EEPROM"));
        assert_eq!(info.free_memory_kb, Some(80));
        assert_eq!(
            info.aux_io,
            Some(AuxIoCounts {
                digital_in: 4,
                digital_out: 4,
                analog_in: 0,
                analog_out: 0,
            })
        );
        assert_eq!(info.plugins, vec!["MODBUS v0.16"]);
        assert_eq!(info.spindle.as_deref(), Some("Huanyang v1"));
        assert!(info.is_grblhal());
        assert_eq!(info.signals.to_string(), "HSDEP");
    }

    #[test]
    fn capability_helpers_follow_newopt() {
        let info = parse_controller_info(FIXTURE);
        assert!(info.has_sed());
        assert!(info.rt_plus());
        assert!(info.homing());
        assert!(info.estop());
        assert!(info.expressions());
        assert!(info.manual_tool_change());
        assert!(info.has_aux_io());
        assert!(info.has_signal(Signals::FEED_HOLD));
        assert!(!info.has_sd());
        assert!(!info.has_atc());
        assert!(!info.has_mpg());
        assert!(!info.has_lathe());
        assert!(!info.has_tool_table());
        assert!(!info.has_probe_connected_signal());
        assert_eq!(info.rx_buffer_size_or(128), 1024);
        assert_eq!(info.axis_list(), vec![Axis::X, Axis::Y, Axis::Z]);
    }

    #[test]
    fn atc_availability_honours_offline_flag() {
        let mut info = ControllerInfo::new();
        info.apply_line("[NEWOPT:ATC=0,TC]");
        assert!(!info.has_atc());
        assert!(info.manual_tool_change());
        info.apply_line("[NEWOPT:ATC=1]");
        assert!(info.has_atc());
        info.apply_line("[NEWOPT:ATC]");
        assert!(info.has_atc());
    }

    #[test]
    fn unknown_and_malformed_lines_are_ignored() {
        let mut info = ControllerInfo::new();
        assert!(!info.apply_line(""));
        assert!(!info.apply_line("garbage"));
        assert!(!info.apply_line("[NOTATAG:value]"));
        assert!(info.apply_line("[board:Some Board]"));
        assert_eq!(info.board.as_deref(), Some("Some Board"));
        assert!(info.apply_line("FIRMWARE:grblHAL"));
        assert_eq!(info.firmware.as_deref(), Some("grblHAL"));
    }

    #[test]
    fn missing_axs_uses_opt_axis_count() {
        let info = parse_controller_info(&["[OPT:V,128,512,4,8]", "[VER:1.1f:]"]);
        assert_eq!(info.axes.to_string(), "XYZA");
        assert_eq!(info.rx_buffer_size, Some(512));
        assert_eq!(info.planner_blocks, Some(128));
        assert_eq!(info.tool_table_entries, Some(8));
        assert!(info.has_tool_table());
        assert_eq!(info.build_info, "");
    }

    #[test]
    fn absurd_axis_counts_are_clamped() {
        let info = parse_controller_info(&["[AXS:99:XYZ]".to_string()]);
        assert_eq!(info.axis_count(), 3);
        let info = parse_controller_info(&["[AXS:99:]".to_string()]);
        assert_eq!(info.axis_count(), AXIS_COUNT);
    }

    #[test]
    fn long_fields_are_bounded() {
        let long = "A".repeat(1000);
        let info = parse_controller_info(&[format!("[BOARD:{long}]")]);
        assert_eq!(info.board.as_deref().map(str::len), Some(MAX_FIELD_LEN));
    }

    #[test]
    fn malformed_aux_io_is_not_reported_as_zero_ports() {
        let info = parse_controller_info(&["[AUX IO:not,a,number]".to_string()]);
        assert!(info.aux_io.is_none());
        assert!(!info.has_aux_io());
        let partial = parse_controller_info(&["[AUX IO:4]".to_string()]);
        assert_eq!(
            partial.aux_io,
            Some(AuxIoCounts {
                digital_in: 4,
                digital_out: 0,
                analog_in: 0,
                analog_out: 0,
            })
        );
    }

    #[test]
    fn free_memory_accepts_suffix_and_plain_number() {
        assert_eq!(parse_free_memory("80K"), Some(80));
        assert_eq!(parse_free_memory("80k"), Some(80));
        assert_eq!(parse_free_memory(" 80 "), Some(80));
        assert_eq!(parse_free_memory("lots"), None);
    }
}
