//! Per-controller data: settings, offsets, tools, macros, startup lines, and
//! the keymap.
//!
//! # Secrets
//!
//! Controller settings are stored verbatim in [`ControllerData::settings`].
//! This crate has no `$ES`/`$SED` setting-type metadata, so it cannot tell a
//! password field from ordinary text. The caller must omit every setting whose
//! controller metadata marks it as a password (type 8) before constructing or
//! saving a snapshot. The same applies to any vendor-specific credential the
//! controller reports. Nothing in this module attempts to redact values after
//! the fact; the safe boundary is at collection time.

use std::collections::{BTreeMap, HashSet};

use ghs_core::{Position, SettingValue, Wcs};
use serde::{Deserialize, Serialize};

use crate::error::ValidationError;

/// Schema version written by this build for a controller profile.
pub const CONTROLLER_SCHEMA_VERSION: u32 = 1;

/// Maximum length of a tool name in characters.
const MAX_TOOL_NAME: usize = 128;
/// Maximum length of a macro name in characters.
const MAX_MACRO_NAME: usize = 128;
/// Maximum length of a macro command in bytes.
const MAX_MACRO_COMMAND: usize = 8_192;
/// Maximum length of key-binding text in characters.
const MAX_KEY_TEXT: usize = 128;
/// Maximum length of a startup line in bytes.
const MAX_STARTUP_LINE: usize = 512;
/// Maximum length of a setting string in bytes.
const MAX_SETTING_TEXT: usize = 512;

/// One row of the application-owned tool table.
///
/// Dimensions are millimetres. A zero diameter or length means "not set"; both
/// values must be finite and non-negative.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ToolRecord {
    /// Tool number, the `T` word value.
    pub number: u32,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Tool diameter in millimetres.
    #[serde(default)]
    pub diameter_mm: f64,
    /// Tool length in millimetres. This is the physical/booking length, not the
    /// controller's active tool-length offset.
    #[serde(default)]
    pub length_mm: f64,
    /// Optional pocket number.
    #[serde(default)]
    pub pocket: Option<u32>,
}

impl ToolRecord {
    /// Validate dimensions and text.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::InvalidToolDimension`] for a negative or
    /// non-finite diameter or length, and
    /// [`ValidationError::InvalidToolText`] for an over-long name containing
    /// control characters.
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_dimension(self.number, "diameter_mm", self.diameter_mm)?;
        validate_dimension(self.number, "length_mm", self.length_mm)?;
        if self.name.chars().count() > MAX_TOOL_NAME {
            return Err(ValidationError::InvalidToolText {
                tool: self.number,
                field: "name",
                reason: "exceeds the 128-character limit",
            });
        }
        if self.name.chars().any(char::is_control) {
            return Err(ValidationError::InvalidToolText {
                tool: self.number,
                field: "name",
                reason: "must not contain control characters",
            });
        }
        Ok(())
    }
}

fn validate_dimension(tool: u32, field: &'static str, value: f64) -> Result<(), ValidationError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(ValidationError::InvalidToolDimension { tool, field, value })
    }
}

/// Application-owned tool table.
///
/// The table is ordered by tool number when built through [`ToolTable::insert`];
/// files are validated for duplicate numbers and invalid dimensions.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ToolTable {
    /// Tool records.
    #[serde(default)]
    pub tools: Vec<ToolRecord>,
}

impl ToolTable {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of tools.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Returns `true` when the table has no tools.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Iterate tools in stored order.
    pub fn iter(&self) -> impl Iterator<Item = &ToolRecord> {
        self.tools.iter()
    }

    /// Find a tool by number.
    #[must_use]
    pub fn get(&self, number: u32) -> Option<&ToolRecord> {
        self.tools.iter().find(|tool| tool.number == number)
    }

    /// Insert or replace a tool, keeping the table sorted by tool number.
    ///
    /// This does not validate dimensions; call [`ToolTable::validate`] before
    /// saving.
    pub fn insert(&mut self, tool: ToolRecord) {
        self.tools.retain(|existing| existing.number != tool.number);
        self.tools.push(tool);
        self.tools.sort_by_key(|tool| tool.number);
    }

    /// Remove a tool by number.
    pub fn remove(&mut self, number: u32) -> Option<ToolRecord> {
        let index = self.tools.iter().position(|tool| tool.number == number)?;
        Some(self.tools.remove(index))
    }

    /// Validate every record and reject duplicate tool numbers.
    ///
    /// # Errors
    ///
    /// Returns a [`ValidationError`] for the first invalid record.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut seen = HashSet::with_capacity(self.tools.len());
        for tool in &self.tools {
            if !seen.insert(tool.number) {
                return Err(ValidationError::DuplicateToolNumber(tool.number));
            }
            tool.validate()?;
        }
        Ok(())
    }
}

/// One named macro: an MDI or system-command sequence.
///
/// `command` may contain several newline-separated lines; `confirm` marks a
/// macro that can move the machine or change controller state, so the shell
/// asks before running it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Macro {
    /// Display name, unique within a profile.
    pub name: String,
    /// Command text sent through the engine, without a line terminator.
    pub command: String,
    /// Whether the shell must confirm before running the macro.
    #[serde(default)]
    pub confirm: bool,
}

impl Macro {
    /// Build a macro.
    #[must_use]
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            confirm: false,
        }
    }

    /// Mark this macro as requiring confirmation.
    #[must_use]
    pub fn requiring_confirmation(mut self, confirm: bool) -> Self {
        self.confirm = confirm;
        self
    }

    /// Validate the name and command.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::InvalidMacro`] for an empty, over-long, or
    /// control-character name, or an empty or over-long command.
    pub fn validate(&self) -> Result<(), ValidationError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(ValidationError::InvalidMacro {
                name: self.name.clone(),
                reason: "name must not be empty",
            });
        }
        if name.chars().count() > MAX_MACRO_NAME {
            return Err(ValidationError::InvalidMacro {
                name: self.name.clone(),
                reason: "name exceeds the 128-character limit",
            });
        }
        if name.chars().any(char::is_control) {
            return Err(ValidationError::InvalidMacro {
                name: self.name.clone(),
                reason: "name must not contain control characters",
            });
        }
        if self.command.trim().is_empty() {
            return Err(ValidationError::InvalidMacro {
                name: self.name.clone(),
                reason: "command must not be empty",
            });
        }
        if self.command.len() > MAX_MACRO_COMMAND {
            return Err(ValidationError::InvalidMacro {
                name: self.name.clone(),
                reason: "command exceeds the 8192-byte limit",
            });
        }
        if self.command.contains('\0') {
            return Err(ValidationError::InvalidMacro {
                name: self.name.clone(),
                reason: "command must not contain NUL",
            });
        }
        Ok(())
    }
}

/// One action bound to a key or button gesture.
///
/// `keys` is an opaque, human-readable accelerator string such as `"Ctrl+S"`.
/// It deliberately does not use a GPUI key type, so the keymap is portable and
/// testable without a window.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct KeyBinding {
    /// Stable action id, for example `"jog.x.plus"`.
    pub action: String,
    /// Accelerator text, for example `"Ctrl+Shift+J"`.
    pub keys: String,
}

impl KeyBinding {
    /// Build a binding.
    #[must_use]
    pub fn new(action: impl Into<String>, keys: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            keys: keys.into(),
        }
    }

    /// Validate both fields.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::InvalidKeyBinding`] for an empty, over-long,
    /// or control-character action or key string.
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_key_text(&self.action, "action")?;
        validate_key_text(&self.keys, &self.action)
    }
}

fn validate_key_text(value: &str, action: &str) -> Result<(), ValidationError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::InvalidKeyBinding {
            action: action.to_string(),
            reason: "action and keys must not be empty",
        });
    }
    if trimmed.chars().count() > MAX_KEY_TEXT {
        return Err(ValidationError::InvalidKeyBinding {
            action: action.to_string(),
            reason: "exceeds the 128-character limit",
        });
    }
    if trimmed.chars().any(char::is_control) {
        return Err(ValidationError::InvalidKeyBinding {
            action: action.to_string(),
            reason: "must not contain control characters",
        });
    }
    Ok(())
}

/// Keyboard and gesture bindings for one profile.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Keymap {
    /// Bindings. Actions must be unique.
    #[serde(default)]
    pub bindings: Vec<KeyBinding>,
}

impl Keymap {
    /// An empty keymap.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Find the keys bound to an action.
    #[must_use]
    pub fn get(&self, action: &str) -> Option<&KeyBinding> {
        self.bindings
            .iter()
            .find(|binding| binding.action == action)
    }

    /// Insert or replace a binding by action.
    ///
    /// This does not validate the record; call [`Keymap::validate`] before
    /// saving.
    pub fn insert(&mut self, binding: KeyBinding) {
        if let Some(existing) = self
            .bindings
            .iter_mut()
            .find(|existing| existing.action == binding.action)
        {
            *existing = binding;
        } else {
            self.bindings.push(binding);
        }
    }

    /// Remove the binding for an action.
    pub fn remove(&mut self, action: &str) -> Option<KeyBinding> {
        let index = self
            .bindings
            .iter()
            .position(|binding| binding.action == action)?;
        Some(self.bindings.remove(index))
    }

    /// Validate every binding and reject duplicate actions.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::DuplicateKeyBinding`] or
    /// [`ValidationError::InvalidKeyBinding`].
    pub fn validate(&self) -> Result<(), ValidationError> {
        let mut seen = HashSet::with_capacity(self.bindings.len());
        for binding in &self.bindings {
            binding.validate()?;
            if !seen.insert(binding.action.trim()) {
                return Err(ValidationError::DuplicateKeyBinding(binding.action.clone()));
            }
        }
        Ok(())
    }
}

/// Startup lines written through `$N0` and `$N1`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StartupLines {
    /// `$N0` startup block line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n0: Option<String>,
    /// `$N1` restart block line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n1: Option<String>,
}

impl StartupLines {
    /// Returns `true` when neither line is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.n0.is_none() && self.n1.is_none()
    }

    /// Validate both lines: each must be non-empty, bounded, and single-line.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::InvalidStartupLine`].
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_startup_line("N0", self.n0.as_deref())?;
        validate_startup_line("N1", self.n1.as_deref())
    }
}

fn validate_startup_line(line: &'static str, value: Option<&str>) -> Result<(), ValidationError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.trim().is_empty() {
        return Err(ValidationError::InvalidStartupLine {
            line,
            reason: "must not be empty",
        });
    }
    if value.len() > MAX_STARTUP_LINE {
        return Err(ValidationError::InvalidStartupLine {
            line,
            reason: "exceeds the 512-byte limit",
        });
    }
    if value.contains(['\r', '\n']) {
        return Err(ValidationError::InvalidStartupLine {
            line,
            reason: "must be a single line",
        });
    }
    Ok(())
}

/// Stored offsets for work coordinate systems, home positions, and tool
/// compensation.
///
/// Every position is in millimetres, matching [`ghs_core::Position`]. The map
/// only contains work offsets the controller reported; `G54` need not be
/// present. `G51` scaling factors are dimensionless and stored in axis wire
/// order.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OffsetSnapshot {
    /// Work offsets keyed by coordinate system, serialized as `"G54"` etc.
    #[serde(default)]
    pub work: BTreeMap<Wcs, Position>,
    /// `G28` stored position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub g28: Option<Position>,
    /// `G30` stored position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub g30: Option<Position>,
    /// Active `G92` offset. This is informational only: grblHAL does not
    /// persist `G92` across a reset, and restoring it must be an explicit,
    /// verified operation rather than an automatic profile load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub g92: Option<Position>,
    /// `HOME` position reported by `$#`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<Position>,
    /// Active tool length offset (`TLO`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_length: Option<Position>,
    /// Tool radius offsets (`TLR`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_radius: Option<Position>,
    /// `G51` scaling factors, in axis wire order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scaling: Option<Vec<f64>>,
}

impl OffsetSnapshot {
    /// An empty snapshot.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Stored offset for one work coordinate system.
    #[must_use]
    pub fn work_offset(&self, wcs: Wcs) -> Option<Position> {
        self.work.get(&wcs).copied()
    }

    /// Store an offset for one work coordinate system.
    pub fn set_work_offset(&mut self, wcs: Wcs, offset: Position) {
        self.work.insert(wcs, offset);
    }

    /// Returns `true` when no offset or scaling value is stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.work.is_empty()
            && self.g28.is_none()
            && self.g30.is_none()
            && self.g92.is_none()
            && self.home.is_none()
            && self.tool_length.is_none()
            && self.tool_radius.is_none()
            && self.scaling.is_none()
    }

    /// Validate that every stored coordinate is finite.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::NonFiniteOffset`].
    pub fn validate(&self) -> Result<(), ValidationError> {
        for (wcs, position) in &self.work {
            if !position.is_finite() {
                return Err(ValidationError::NonFiniteOffset {
                    field: wcs.as_str(),
                });
            }
        }
        let named = [
            ("G28", self.g28),
            ("G30", self.g30),
            ("G92", self.g92),
            ("HOME", self.home),
            ("TLO", self.tool_length),
            ("TLR", self.tool_radius),
        ];
        for (field, position) in named {
            if position.is_some_and(|position| !position.is_finite()) {
                return Err(ValidationError::NonFiniteOffset { field });
            }
        }
        if self
            .scaling
            .as_ref()
            .is_some_and(|values| !values.iter().all(|value| value.is_finite()))
        {
            return Err(ValidationError::NonFiniteOffset { field: "G51" });
        }
        Ok(())
    }
}

/// Everything the application persists for one controller.
///
/// This is a snapshot, never a live controller session. Loading a profile does
/// not write offsets, settings, or tools to the controller; the engine's
/// safety gateway and explicit user confirmation own that step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControllerData {
    /// Schema version, always [`CONTROLLER_SCHEMA_VERSION`] for data this
    /// build writes.
    pub schema_version: u32,
    /// Controller settings by number. Password fields must be omitted by the
    /// caller; see the module documentation.
    #[serde(default)]
    pub settings: BTreeMap<u16, SettingValue>,
    /// Work, home, and tool offsets.
    #[serde(default)]
    pub offsets: OffsetSnapshot,
    /// Application-owned tool table.
    #[serde(default)]
    pub tools: ToolTable,
    /// Macros, in user order.
    #[serde(default)]
    pub macros: Vec<Macro>,
    /// `$N0`/`$N1` startup lines.
    #[serde(default)]
    pub startup_lines: StartupLines,
    /// Local keymap.
    #[serde(default)]
    pub keymap: Keymap,
}

impl Default for ControllerData {
    fn default() -> Self {
        Self {
            schema_version: CONTROLLER_SCHEMA_VERSION,
            settings: BTreeMap::new(),
            offsets: OffsetSnapshot::default(),
            tools: ToolTable::default(),
            macros: Vec::new(),
            startup_lines: StartupLines::default(),
            keymap: Keymap::default(),
        }
    }
}

impl ControllerData {
    /// An empty profile for a controller.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate the schema, settings, offsets, tools, macros, startup lines,
    /// and keymap.
    ///
    /// # Errors
    ///
    /// Returns the first [`ValidationError`] found.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != CONTROLLER_SCHEMA_VERSION {
            return Err(ValidationError::UnsupportedVersion {
                what: "controller profile",
                found: self.schema_version,
                supported: CONTROLLER_SCHEMA_VERSION,
            });
        }
        for (number, value) in &self.settings {
            validate_setting(*number, value)?;
        }
        self.offsets.validate()?;
        self.tools.validate()?;

        let mut names = HashSet::with_capacity(self.macros.len());
        for mac in &self.macros {
            mac.validate()?;
            if !names.insert(mac.name.trim()) {
                return Err(ValidationError::DuplicateMacro(mac.name.clone()));
            }
        }
        self.startup_lines.validate()?;
        self.keymap.validate()?;
        Ok(())
    }
}

fn validate_setting(number: u16, value: &SettingValue) -> Result<(), ValidationError> {
    match value {
        SettingValue::Float(value) if !value.is_finite() => Err(ValidationError::InvalidSetting {
            number,
            reason: "float value must be finite",
        }),
        SettingValue::String(text) if text.len() > MAX_SETTING_TEXT => {
            Err(ValidationError::InvalidSetting {
                number,
                reason: "text exceeds the 512-byte limit",
            })
        }
        SettingValue::String(_) => Ok(()),
        SettingValue::AxisArray(values) => {
            if values.values.len() != values.axes.len() {
                return Err(ValidationError::InvalidSetting {
                    number,
                    reason: "axis value count does not match the axis mask",
                });
            }
            if !values.values.iter().all(|value| value.is_finite()) {
                return Err(ValidationError::InvalidSetting {
                    number,
                    reason: "axis array contains a non-finite value",
                });
            }
            Ok(())
        }
        SettingValue::Int(_) | SettingValue::Bool(_) | SettingValue::Float(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use ghs_core::{Axes, AxisValues};

    use super::*;

    fn sample_tools() -> ToolTable {
        let mut table = ToolTable::new();
        table.insert(ToolRecord {
            number: 1,
            name: "Flat 6mm".into(),
            diameter_mm: 6.0,
            length_mm: 35.0,
            pocket: Some(1),
        });
        table.insert(ToolRecord {
            number: 2,
            name: "Ball 3mm".into(),
            diameter_mm: 3.0,
            length_mm: 30.5,
            pocket: Some(2),
        });
        table
    }

    fn sample_data() -> ControllerData {
        let mut data = ControllerData::new();
        data.settings.insert(20, SettingValue::Bool(true));
        data.settings.insert(100, SettingValue::Float(250.0));
        data.settings.insert(110, SettingValue::Int(1200));
        data.settings.insert(
            130,
            SettingValue::AxisArray(
                AxisValues::new(Axes::xyz(), vec![250.0, 250.0, 250.0]).expect("valid"),
            ),
        );
        data.settings
            .insert(14, SettingValue::String("custom".into()));

        let mut offsets = OffsetSnapshot::new();
        offsets.set_work_offset(Wcs::G54, Position::xyz(10.0, 20.0, 30.0));
        offsets.set_work_offset(Wcs::G59_3, Position::xyz(-1.0, -2.0, -3.0));
        offsets.g28 = Some(Position::xyz(0.0, 0.0, 0.0));
        offsets.g30 = Some(Position::xyz(1.0, 1.0, 1.0));
        offsets.g92 = Some(Position::xyz(0.5, 0.0, 0.0));
        offsets.home = Some(Position::xyz(0.0, 0.0, 0.0));
        offsets.tool_length = Some(Position::xyz(0.0, 0.0, -12.0));
        offsets.tool_radius = Some(Position::xyz(3.0, 0.0, 0.0));
        offsets.scaling = Some(vec![1.0, 1.0, 0.5]);
        data.offsets = offsets;

        data.tools = sample_tools();
        data.macros = vec![
            Macro::new("Zero XY", "G10 L20 P1 X0 Y0").requiring_confirmation(true),
            Macro::new("Home", "$H"),
        ];
        data.startup_lines = StartupLines {
            n0: Some("G21 G90".into()),
            n1: Some("G54".into()),
        };
        let mut keymap = Keymap::new();
        keymap.insert(KeyBinding::new("jog.x.plus", "Right"));
        keymap.insert(KeyBinding::new("run.pause", "Space"));
        data.keymap = keymap;
        data
    }

    #[test]
    fn controller_data_round_trips() {
        let data = sample_data();
        data.validate().expect("valid");
        let json = serde_json::to_string_pretty(&data).expect("serialize");
        let decoded: ControllerData = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, data);
        decoded.validate().expect("valid after round trip");
        assert!(json.contains("\"G54\""));
        assert!(json.contains("\"G59.3\""));
    }

    #[test]
    fn tool_table_round_trips_and_sorts() {
        let mut table = ToolTable::new();
        table.insert(ToolRecord {
            number: 7,
            name: "Seven".into(),
            diameter_mm: 7.0,
            length_mm: 40.0,
            pocket: None,
        });
        table.insert(ToolRecord {
            number: 3,
            name: "Three".into(),
            diameter_mm: 3.0,
            length_mm: 20.0,
            pocket: Some(4),
        });
        assert_eq!(
            table.iter().map(|tool| tool.number).collect::<Vec<_>>(),
            vec![3, 7]
        );
        assert_eq!(table.get(3).map(|tool| tool.name.as_str()), Some("Three"));
        table.validate().expect("valid");

        let json = serde_json::to_string(&table).expect("serialize");
        let decoded: ToolTable = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, table);
    }

    #[test]
    fn duplicate_tool_numbers_are_rejected() {
        let mut table = sample_tools();
        table.tools.push(table.tools[0].clone());
        assert_eq!(
            table.validate(),
            Err(ValidationError::DuplicateToolNumber(1))
        );
    }

    #[test]
    fn invalid_tool_dimensions_are_rejected() {
        for value in [-1.0, f64::NAN, f64::INFINITY] {
            let tool = ToolRecord {
                number: 4,
                name: "Bad".into(),
                diameter_mm: value,
                length_mm: 10.0,
                pocket: None,
            };
            let mut table = ToolTable::new();
            table.tools.push(tool);
            assert!(matches!(
                table.validate(),
                Err(ValidationError::InvalidToolDimension { .. })
            ));
        }
    }

    #[test]
    fn duplicate_macros_and_bindings_are_rejected() {
        let mut data = ControllerData::new();
        data.macros = vec![Macro::new("Same", "M5"), Macro::new("Same", "M3")];
        assert!(matches!(
            data.validate(),
            Err(ValidationError::DuplicateMacro(_))
        ));

        let mut data = ControllerData::new();
        data.keymap.bindings = vec![
            KeyBinding::new("run.pause", "Space"),
            KeyBinding::new("run.pause", "P"),
        ];
        assert!(matches!(
            data.validate(),
            Err(ValidationError::DuplicateKeyBinding(_))
        ));
    }

    #[test]
    fn non_finite_offsets_and_settings_are_rejected() {
        let mut offsets = OffsetSnapshot::new();
        offsets.set_work_offset(Wcs::G54, Position::xyz(f64::NAN, 0.0, 0.0));
        assert!(matches!(
            offsets.validate(),
            Err(ValidationError::NonFiniteOffset { field: "G54" })
        ));

        let mut data = ControllerData::new();
        data.settings.insert(100, SettingValue::Float(f64::NAN));
        assert!(matches!(
            data.validate(),
            Err(ValidationError::InvalidSetting { number: 100, .. })
        ));
    }

    #[test]
    fn startup_lines_must_be_single_lines() {
        let lines = StartupLines {
            n0: Some("G21\nG90".into()),
            n1: None,
        };
        assert!(matches!(
            lines.validate(),
            Err(ValidationError::InvalidStartupLine { line: "N0", .. })
        ));
        assert_eq!(StartupLines { n0: None, n1: None }, StartupLines::default());
    }

    #[test]
    fn unknown_schema_version_is_rejected() {
        let mut data = ControllerData::new();
        data.schema_version = 2;
        assert!(matches!(
            data.validate(),
            Err(ValidationError::UnsupportedVersion {
                what: "controller profile",
                found: 2,
                supported: CONTROLLER_SCHEMA_VERSION,
            })
        ));
    }

    #[test]
    fn empty_snapshot_is_valid_and_empty() {
        let data = ControllerData::new();
        data.validate().expect("valid");
        assert!(data.offsets.is_empty());
        assert!(data.tools.is_empty());
        assert!(data.startup_lines.is_empty());
        assert_eq!(data.settings.len(), 0);
    }
}
