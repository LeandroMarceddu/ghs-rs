//! Runtime enumeration of settings, setting groups, alarms and errors.
//!
//! grblHAL controllers that advertise `ENUMS` describe their own settings and
//! code tables. The types here mirror that metadata so the settings UI can be
//! generated instead of hardcoded.

use std::collections::BTreeMap;
use std::fmt;

use ghs_core::{AlarmCode, ErrorCode, SettingValue};

use crate::error::ParseError;
use crate::util::{bounded, parse_f64};

/// Maximum length kept for a setting name.
const MAX_NAME_LEN: usize = 128;
/// Maximum length kept for a unit or format string.
const MAX_SHORT_LEN: usize = 64;
/// Maximum length kept for a description.
const MAX_DESCRIPTION_LEN: usize = 512;

/// Setting data type from the `$ES` `[SETTING:...]` metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingType {
    /// Type 0: boolean checkbox.
    Boolean,
    /// Type 1: bitfield checkbox column.
    Bitfield,
    /// Type 2: exclusive bitfield.
    ExclusiveBitfield,
    /// Type 3: radio buttons.
    Radio,
    /// Type 4: axis-mask checkbox column.
    AxisMask,
    /// Type 5: integer field.
    Integer,
    /// Type 6: decimal field.
    Decimal,
    /// Type 7: string field.
    Text,
    /// Type 8: password field.
    Password,
    /// Type 9: IPv4 address field.
    IpAddress,
    /// An unknown type code, preserved for forward compatibility.
    Unknown(u8),
}

impl SettingType {
    /// Map a wire type code to a setting type.
    #[must_use]
    pub const fn from_code(code: u8) -> Self {
        match code {
            0 => SettingType::Boolean,
            1 => SettingType::Bitfield,
            2 => SettingType::ExclusiveBitfield,
            3 => SettingType::Radio,
            4 => SettingType::AxisMask,
            5 => SettingType::Integer,
            6 => SettingType::Decimal,
            7 => SettingType::Text,
            8 => SettingType::Password,
            9 => SettingType::IpAddress,
            other => SettingType::Unknown(other),
        }
    }

    /// Wire type code.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            SettingType::Boolean => 0,
            SettingType::Bitfield => 1,
            SettingType::ExclusiveBitfield => 2,
            SettingType::Radio => 3,
            SettingType::AxisMask => 4,
            SettingType::Integer => 5,
            SettingType::Decimal => 6,
            SettingType::Text => 7,
            SettingType::Password => 8,
            SettingType::IpAddress => 9,
            SettingType::Unknown(code) => code,
        }
    }

    /// Returns `true` for free-form text types.
    #[must_use]
    pub const fn is_text(self) -> bool {
        matches!(
            self,
            SettingType::Text | SettingType::Password | SettingType::IpAddress
        )
    }

    /// Short human-readable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            SettingType::Boolean => "boolean",
            SettingType::Bitfield => "bitfield",
            SettingType::ExclusiveBitfield => "exclusive bitfield",
            SettingType::Radio => "radio",
            SettingType::AxisMask => "axis mask",
            SettingType::Integer => "integer",
            SettingType::Decimal => "decimal",
            SettingType::Text => "string",
            SettingType::Password => "password",
            SettingType::IpAddress => "IPv4 address",
            SettingType::Unknown(_) => "unknown",
        }
    }
}

impl fmt::Display for SettingType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One `[SETTING:...]` metadata record from `$ES`.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingDescription {
    /// Setting id, the `<n>` in `$<n>`.
    pub id: u16,
    /// Group id this setting belongs to, when reported.
    pub group: Option<u16>,
    /// Setting name.
    pub name: String,
    /// Unit label, absent when the controller reports `N/A`.
    pub unit: Option<String>,
    /// Data type.
    pub setting_type: SettingType,
    /// Display/format hint, for example `#####0.000`.
    pub format: Option<String>,
    /// Minimum value, when numeric.
    pub min: Option<f64>,
    /// Maximum value, when numeric.
    pub max: Option<f64>,
    /// Whether a reboot is required after changing the setting.
    pub reboot_required: bool,
    /// Whether the setting can be cleared / left unset.
    pub null_allowed: bool,
    /// Optional null label when the controller reports text instead of a flag.
    pub null_label: Option<String>,
}

impl SettingDescription {
    /// Returns `true` when the setting takes a free-form string value.
    #[must_use]
    pub const fn is_text(&self) -> bool {
        matches!(
            self.setting_type,
            SettingType::Text | SettingType::Password | SettingType::IpAddress
        )
    }

    /// Clamp and validate a numeric value against the reported range.
    ///
    /// Returns `None` for a non-numeric value or a value outside the range.
    #[must_use]
    pub fn in_range(&self, value: f64) -> bool {
        if !value.is_finite() {
            return false;
        }
        self.min.is_none_or(|min| value >= min) && self.max.is_none_or(|max| value <= max)
    }
}

/// One `[SETTINGGROUP:...]` record from `$EG`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingGroup {
    /// Group id.
    pub id: u16,
    /// Parent group id, when this is a nested group.
    pub parent: Option<u16>,
    /// Group name.
    pub name: String,
}

/// One `[ALARMCODE:...]` or `[ERRORCODE:...]` record from `$EA`/`$EE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeDescription {
    /// Numeric code.
    pub code: u16,
    /// Short message, for example `"Hard limit"`.
    pub message: String,
    /// Longer description.
    pub description: String,
}

/// Parse a `$<n>=<value>` setting line into a number and typed value.
///
/// Accepts the plain `$$` form and the `$<n>=<value> (comment)` form some
/// builds emit; a trailing parenthesised comment is removed only when the text
/// before it is a valid numeric, boolean or axis-array value.
///
/// # Errors
///
/// Returns [`ParseError`] when the line is not a `$<n>=...` setting line.
pub fn parse_setting_line(line: &str) -> Result<(u16, SettingValue), ParseError> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix('$')
        .ok_or(ParseError::Expected("a '$' setting line"))?;
    let (number, value) = rest
        .split_once('=')
        .ok_or(ParseError::Expected("a setting value after '='"))?;
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseError::Expected("a numeric setting id"));
    }
    let number = number
        .parse::<u16>()
        .map_err(|_| ParseError::InvalidNumber(bounded(number, 16)))?;
    Ok((number, SettingValue::parse(value_text(value))))
}

/// Parse a complete `$$` response into setting values, keyed by id.
///
/// Unrecognised lines are ignored.
#[must_use]
pub fn parse_settings<S: AsRef<str>>(lines: &[S]) -> BTreeMap<u16, SettingValue> {
    let mut settings = BTreeMap::new();
    for line in lines {
        if let Ok((number, value)) = parse_setting_line(line.as_ref()) {
            settings.insert(number, value);
        }
    }
    settings
}

/// Parse a `[SETTING:...]` metadata line.
///
/// # Errors
///
/// Returns [`ParseError`] when the line is not a setting description or the id
/// is malformed.
pub fn parse_setting_description(line: &str) -> Result<SettingDescription, ParseError> {
    let inner = strip_tag(line, "SETTING")?;
    let fields: Vec<&str> = inner.split('|').collect();
    if fields.len() < 5 {
        return Err(ParseError::Expected(
            "at least five setting metadata fields",
        ));
    }

    let id = parse_id(fields[0], "setting id")?;
    let group = parse_id_optional(fields[1]);
    let name = bounded(fields[2], MAX_NAME_LEN);
    let unit = optional_field(fields[3], MAX_SHORT_LEN);
    let setting_type = SettingType::from_code(
        fields[4]
            .trim()
            .parse::<u8>()
            .map_err(|_| ParseError::InvalidNumber(bounded(fields[4], 8)))?,
    );
    let format = fields
        .get(5)
        .and_then(|field| optional_field(field, MAX_SHORT_LEN));
    let min = fields.get(6).and_then(|field| parse_f64(field));
    let max = fields.get(7).and_then(|field| parse_f64(field));
    let reboot_required = fields.get(8).is_some_and(|field| truthy(field));
    let (null_allowed, null_label) = fields
        .get(9)
        .map_or((false, None), |field| parse_null(field));

    Ok(SettingDescription {
        id,
        group,
        name,
        unit,
        setting_type,
        format,
        min,
        max,
        reboot_required,
        null_allowed,
        null_label,
    })
}

/// Parse a `[SETTINGGROUP:...]` metadata line.
///
/// # Errors
///
/// Returns [`ParseError`] when the line is not a setting group or the id is
/// malformed.
pub fn parse_setting_group(line: &str) -> Result<SettingGroup, ParseError> {
    let inner = strip_tag(line, "SETTINGGROUP")?;
    let fields: Vec<&str> = inner.split('|').collect();
    if fields.len() < 2 {
        return Err(ParseError::Expected("a setting group id and name"));
    }
    Ok(SettingGroup {
        id: parse_id(fields[0], "group id")?,
        parent: fields.get(1).and_then(|field| parse_id_optional(field)),
        name: bounded(fields.get(2).copied().unwrap_or(""), MAX_NAME_LEN),
    })
}

/// Parse an `[ALARMCODE:...]` or `[ERRORCODE:...]` line.
///
/// # Errors
///
/// Returns [`ParseError`] when the line is not a code description or the code
/// is malformed.
pub fn parse_code_description(line: &str) -> Result<CodeDescription, ParseError> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(trimmed);
    let (tag, rest) = inner
        .split_once(':')
        .ok_or(ParseError::Expected("a code description"))?;
    if !tag.trim().eq_ignore_ascii_case("ALARMCODE")
        && !tag.trim().eq_ignore_ascii_case("ERRORCODE")
    {
        return Err(ParseError::Expected("an ALARMCODE or ERRORCODE line"));
    }
    let fields: Vec<&str> = rest.split('|').collect();
    if fields.len() < 2 {
        return Err(ParseError::Expected("a code and message"));
    }
    Ok(CodeDescription {
        code: parse_id(fields[0], "code")?,
        message: bounded(fields[1], MAX_NAME_LEN),
        description: bounded(fields.get(2).copied().unwrap_or(""), MAX_DESCRIPTION_LEN),
    })
}

/// Descriptions enumerated by `$EA`/`$EE`, keyed by code.
///
/// Controllers that advertise `ENUMS` are the authoritative source; the
/// bundled tables in [`AlarmCode`] and [`ErrorCode`] are the fallback. The
/// label helpers apply that precedence and never panic on an unknown code.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CodeTable {
    codes: BTreeMap<u16, CodeDescription>,
}

impl CodeTable {
    /// Create an empty table.
    #[must_use]
    pub fn new() -> Self {
        CodeTable::default()
    }

    /// Build a table from `[ALARMCODE:...]` or `[ERRORCODE:...]` lines.
    #[must_use]
    pub fn from_lines<S: AsRef<str>>(lines: &[S]) -> Self {
        CodeTable {
            codes: parse_code_descriptions(lines),
        }
    }

    /// Insert or replace one description.
    pub fn insert(&mut self, description: CodeDescription) {
        self.codes.insert(description.code, description);
    }

    /// Number of enumerated codes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.codes.len()
    }

    /// Returns `true` when the controller enumerated no codes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.codes.is_empty()
    }

    /// Description for one code.
    #[must_use]
    pub fn get(&self, code: u16) -> Option<&CodeDescription> {
        self.codes.get(&code)
    }

    /// Human-readable alarm label, preferring the controller's description.
    #[must_use]
    pub fn alarm_label(&self, code: u16) -> String {
        match self.get(code) {
            Some(description) if !description.message.is_empty() => {
                format!("alarm {code}: {}", description.message)
            }
            _ => AlarmCode::new(code).to_string(),
        }
    }

    /// Human-readable error label, preferring the controller's description.
    #[must_use]
    pub fn error_label(&self, code: u16) -> String {
        match self.get(code) {
            Some(description) if !description.message.is_empty() => {
                format!("error {code}: {}", description.message)
            }
            _ => ErrorCode::new(code).to_string(),
        }
    }
}

/// Parse a set of `[ALARMCODE:...]` or `[ERRORCODE:...]` lines.
#[must_use]
pub fn parse_code_descriptions<S: AsRef<str>>(lines: &[S]) -> BTreeMap<u16, CodeDescription> {
    let mut codes = BTreeMap::new();
    for line in lines {
        if let Ok(description) = parse_code_description(line.as_ref()) {
            codes.insert(description.code, description);
        }
    }
    codes
}

fn value_text(raw: &str) -> &str {
    let raw = raw.trim();
    if let Some(index) = raw.find(" (") {
        let head = raw[..index].trim_end();
        if !matches!(SettingValue::parse(head), SettingValue::String(_)) {
            return head;
        }
    }
    raw
}

fn strip_tag<'a>(line: &'a str, tag: &str) -> Result<&'a str, ParseError> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(trimmed);
    let (found, rest) = inner
        .split_once(':')
        .ok_or(ParseError::Expected("a tagged metadata line"))?;
    if !found.trim().eq_ignore_ascii_case(tag) {
        return Err(ParseError::Expected("a matching metadata tag"));
    }
    Ok(rest)
}

fn parse_id(text: &str, what: &str) -> Result<u16, ParseError> {
    text.trim()
        .parse::<u16>()
        .map_err(|_| ParseError::InvalidNumber(format!("{what}: {}", bounded(text, 16))))
}

fn parse_id_optional(text: &str) -> Option<u16> {
    let text = text.trim();
    if unavailable(text) {
        None
    } else {
        text.parse::<u16>().ok()
    }
}

fn optional_field(text: &str, max_len: usize) -> Option<String> {
    let text = text.trim();
    if unavailable(text) {
        None
    } else {
        Some(bounded(text, max_len))
    }
}

fn unavailable(text: &str) -> bool {
    text.is_empty()
        || text.eq_ignore_ascii_case("n/a")
        || text.eq_ignore_ascii_case("na")
        || text == "-"
}

fn truthy(text: &str) -> bool {
    matches!(
        text.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y"
    )
}

fn parse_null(text: &str) -> (bool, Option<String>) {
    let text = text.trim();
    if truthy(text) {
        (true, None)
    } else if text.eq_ignore_ascii_case("false")
        || text.eq_ignore_ascii_case("no")
        || text == "0"
        || unavailable(text)
    {
        (false, None)
    } else {
        (true, Some(bounded(text, MAX_SHORT_LEN)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_setting_lines() {
        let (number, value) = parse_setting_line("$110=8000.000").expect("valid");
        assert_eq!(number, 110);
        assert_eq!(value, SettingValue::Float(8000.0));

        let (number, value) = parse_setting_line("$22=1").expect("valid");
        assert_eq!(number, 22);
        assert_eq!(value, SettingValue::Int(1));

        let (number, value) = parse_setting_line("$100=250.000,250.000,250.000").expect("valid");
        assert_eq!(number, 100);
        assert!(value.as_axis_values().is_some());

        let (_, value) = parse_setting_line("$14=1 (invert control pins)").expect("valid");
        assert_eq!(value, SettingValue::Int(1));
    }

    #[test]
    fn rejects_non_setting_lines() {
        assert!(parse_setting_line("$N0=G54").is_err());
        assert!(parse_setting_line("$J=G91 X1 F100").is_err());
        assert!(parse_setting_line("$100").is_err());
        assert!(parse_setting_line("100=1").is_err());
        assert!(parse_setting_line("").is_err());
        assert!(parse_setting_line("$99999=1").is_err());
    }

    #[test]
    fn bulk_settings_ignore_noise() {
        let settings = parse_settings(&["$0=10", "$1=25", "ok", "$110=8000.000", "[MSG:hello]"]);
        assert_eq!(settings.len(), 3);
        assert_eq!(settings.get(&0), Some(&SettingValue::Int(10)));
        assert_eq!(settings.get(&110), Some(&SettingValue::Float(8000.0)));
    }

    #[test]
    fn parses_setting_description() {
        let line = "[SETTING:110|1|X-axis maximum rate|mm/min|6|#####0.000|1|1000000|0|N/A]";
        let setting = parse_setting_description(line).expect("valid");
        assert_eq!(setting.id, 110);
        assert_eq!(setting.group, Some(1));
        assert_eq!(setting.name, "X-axis maximum rate");
        assert_eq!(setting.unit.as_deref(), Some("mm/min"));
        assert_eq!(setting.setting_type, SettingType::Decimal);
        assert_eq!(setting.setting_type.code(), 6);
        assert_eq!(setting.format.as_deref(), Some("#####0.000"));
        assert_eq!(setting.min, Some(1.0));
        assert_eq!(setting.max, Some(1_000_000.0));
        assert!(!setting.reboot_required);
        assert!(!setting.null_allowed);
        assert!(setting.in_range(500.0));
        assert!(!setting.in_range(0.5));
        assert!(!setting.in_range(f64::NAN));
        assert!(!setting.is_text());
    }

    #[test]
    fn parses_all_setting_types() {
        let cases = [
            (0, SettingType::Boolean),
            (1, SettingType::Bitfield),
            (2, SettingType::ExclusiveBitfield),
            (3, SettingType::Radio),
            (4, SettingType::AxisMask),
            (5, SettingType::Integer),
            (6, SettingType::Decimal),
            (7, SettingType::Text),
            (8, SettingType::Password),
            (9, SettingType::IpAddress),
            (42, SettingType::Unknown(42)),
        ];
        for (code, expected) in cases {
            assert_eq!(SettingType::from_code(code), expected);
            assert_eq!(expected.code(), code);
            assert!(!expected.as_str().is_empty());
        }
        assert!(SettingType::from_code(7).is_text());
        assert!(SettingType::from_code(9).is_text());
    }

    #[test]
    fn parses_setting_group() {
        let group = parse_setting_group("[SETTINGGROUP:2|1|Spindle]").expect("valid");
        assert_eq!(group.id, 2);
        assert_eq!(group.parent, Some(1));
        assert_eq!(group.name, "Spindle");
        let root = parse_setting_group("[SETTINGGROUP:0|N/A|Root]").expect("valid");
        assert_eq!(root.parent, None);
        assert!(parse_setting_group("[SETTINGGROUP:bad]").is_err());
    }

    #[test]
    fn parses_alarm_and_error_code_descriptions() {
        let alarm =
            parse_code_description("[ALARMCODE:1|Hard limit|Hard limit has been triggered.]")
                .expect("valid");
        assert_eq!(alarm.code, 1);
        assert_eq!(alarm.message, "Hard limit");
        assert!(alarm.description.starts_with("Hard limit"));

        let error = parse_code_description(
            "[ERRORCODE:20|Unsupported command|Unsupported or invalid g-code command.]",
        )
        .expect("valid");
        assert_eq!(error.code, 20);

        let codes = parse_code_descriptions(&[
            "[ALARMCODE:1|Hard limit|desc]",
            "[ALARMCODE:2|Soft limit|desc]",
            "garbage",
        ]);
        assert_eq!(codes.len(), 2);
        assert!(parse_code_description("[MSG:hello]").is_err());
    }

    #[test]
    fn enumerated_codes_take_precedence_over_bundled_tables() {
        let table = CodeTable::from_lines(&[
            "[ALARMCODE:1|Hard limit switch|A limit switch was hit.]",
            "[ALARMCODE:99|Vendor alarm|Contact the manufacturer.]",
        ]);
        assert_eq!(table.len(), 2);
        assert!(!table.is_empty());
        assert_eq!(table.alarm_label(1), "alarm 1: Hard limit switch");
        assert_eq!(table.alarm_label(99), "alarm 99: Vendor alarm");
        // Unknown codes fall back to the bundled table, which has no entry for
        // 250, so the safe generic label is used.
        assert_eq!(table.alarm_label(250), "alarm 250");
        assert_eq!(
            table.error_label(20),
            "error 20: unsupported or invalid g-code command"
        );
        assert_eq!(table.error_label(250), "error 250");

        let empty = CodeTable::new();
        assert!(empty.is_empty());
        assert_eq!(empty.alarm_label(1), "alarm 1: hard limit triggered");
    }

    #[test]
    fn null_and_reboot_flags() {
        let line = "[SETTING:10|0|Status report options|N/A|5|####0|0|65535|1|1]";
        let setting = parse_setting_description(line).expect("valid");
        assert!(setting.reboot_required);
        assert!(setting.null_allowed);
        assert_eq!(setting.null_label, None);

        let line = "[SETTING:14|0|Invert control pins|N/A|5|####0|0|65535|0|custom]";
        let setting = parse_setting_description(line).expect("valid");
        assert!(setting.null_allowed);
        assert_eq!(setting.null_label.as_deref(), Some("custom"));
    }
}
