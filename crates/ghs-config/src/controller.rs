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

use ghs_core::{Axis, Position, SettingValue, Wcs};
use serde::{Deserialize, Serialize};

use crate::error::ValidationError;

/// Schema version written by this build for a controller profile.
pub const CONTROLLER_SCHEMA_VERSION: u32 = 1;

/// Maximum number of height maps stored in one controller profile.
///
/// A profile is a per-controller document; maps are per workpiece/setup, so a
/// small bound keeps the profile readable and bounds load and validation time.
/// Callers must surface [`ValidationError::TooManyHeightMaps`] instead of
/// dropping maps silently.
pub const MAX_HEIGHT_MAPS: usize = 16;
/// Maximum number of columns or rows in one persisted height map.
pub const MAX_HEIGHT_MAP_AXIS_POINTS: u16 = 128;
/// Maximum column or row spacing in a persisted height map, in millimetres.
pub const MAX_HEIGHT_MAP_STEP_MM: f64 = 1_000.0;
/// Maximum absolute origin, grid extent, surface, offset, or sample coordinate
/// in a persisted height map, in millimetres.
pub const MAX_HEIGHT_MAP_COORDINATE_MM: f64 = 100_000.0;
/// Maximum length of a height-map label in characters.
const MAX_HEIGHT_MAP_NAME: usize = 128;

/// Maximum absolute X or Y probe mount offset, in millimetres.
///
/// The offset is `probe tip - controlled point`, so each component is bounded
/// to `-MAX_PROBE_OFFSET_MM..=MAX_PROBE_OFFSET_MM`. This matches the bound in
/// the probe crate; `ghs-config` does not depend on that crate, so the value is
/// repeated here and must stay in sync.
pub const MAX_PROBE_OFFSET_MM: f64 = 100.0;

/// Largest absolute safe-Z plane in persisted probe defaults, in millimetres.
///
/// This matches the machine coordinate bound in the probe crate.
pub const MAX_PROBE_SAFE_Z_MM: f64 = 100_000.0;
/// Largest search distance in persisted probe defaults, in millimetres.
pub const MAX_PROBE_SEARCH_DISTANCE_MM: f64 = 100.0;
/// Largest feed in persisted probe defaults, in millimetres per minute.
pub const MAX_PROBE_FEED_MM_MIN: f64 = 2_000.0;
/// Largest retract distance in persisted probe defaults, in millimetres.
pub const MAX_PROBE_RETRACT_MM: f64 = 100.0;
/// Largest probe tip diameter in persisted probe defaults, in millimetres.
pub const MAX_PROBE_DIAMETER_MM: f64 = 50.0;
/// Largest absolute edge offset in persisted probe defaults, in millimetres.
pub const MAX_PROBE_EDGE_OFFSET_MM: f64 = 100.0;
/// Largest touch-plate thickness in persisted probe defaults, in millimetres.
pub const MAX_PROBE_PLATE_THICKNESS_MM: f64 = 100.0;

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

/// Physical XY mount offset of the probe tip for one controller.
///
/// The value is `probe tip - controlled point`, in millimetres, along the
/// machine/work X and Y axes. It is a property of the physical mount, not a
/// work offset: to convert a feature (probe-tip) position to the controlled
/// point, subtract the offset. Loading a profile never applies it to the
/// controller.
///
/// Both components are finite and within `+/-`[`MAX_PROBE_OFFSET_MM`]. A
/// profile that predates this record loads with a zero offset, which is also
/// the default.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ProbeOffsetRecord {
    /// X component of `probe tip - controlled point`, in millimetres.
    #[serde(default)]
    pub x_mm: f64,
    /// Y component of `probe tip - controlled point`, in millimetres.
    #[serde(default)]
    pub y_mm: f64,
}

impl ProbeOffsetRecord {
    /// Validate both components.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::NonFiniteProbeOffset`] when a component is
    /// NaN or infinite, and [`ValidationError::ProbeOffsetOutOfRange`] when it
    /// is outside `+/-`[`MAX_PROBE_OFFSET_MM`].
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_probe_offset_axis("x_mm", self.x_mm)?;
        validate_probe_offset_axis("y_mm", self.y_mm)
    }
}

fn validate_probe_offset_axis(axis: &'static str, value: f64) -> Result<(), ValidationError> {
    if !value.is_finite() {
        return Err(ValidationError::NonFiniteProbeOffset { axis, value });
    }
    if value.abs() > MAX_PROBE_OFFSET_MM {
        return Err(ValidationError::ProbeOffsetOutOfRange {
            axis,
            value,
            min: -MAX_PROBE_OFFSET_MM,
            max: MAX_PROBE_OFFSET_MM,
        });
    }
    Ok(())
}

const DEFAULT_PROBE_SAFE_Z_MM: f64 = 10.0;
const DEFAULT_PROBE_SEARCH_DISTANCE_MM: f64 = 5.0;
const DEFAULT_PROBE_FEED_MM_MIN: f64 = 100.0;
const DEFAULT_PROBE_RETRACT_MM: f64 = 2.0;
const DEFAULT_PROBE_DIAMETER_MM: f64 = 3.0;
const DEFAULT_PROBE_EDGE_OFFSET_MM: f64 = 0.0;
const DEFAULT_PROBE_PLATE_THICKNESS_MM: f64 = 10.0;

const fn default_probe_safe_z_mm() -> f64 {
    DEFAULT_PROBE_SAFE_Z_MM
}

const fn default_probe_search_distance_mm() -> f64 {
    DEFAULT_PROBE_SEARCH_DISTANCE_MM
}

const fn default_probe_feed_mm_min() -> f64 {
    DEFAULT_PROBE_FEED_MM_MIN
}

const fn default_probe_retract_mm() -> f64 {
    DEFAULT_PROBE_RETRACT_MM
}

const fn default_probe_diameter_mm() -> f64 {
    DEFAULT_PROBE_DIAMETER_MM
}

const fn default_probe_edge_offset_mm() -> f64 {
    DEFAULT_PROBE_EDGE_OFFSET_MM
}

const fn default_probe_plate_thickness_mm() -> f64 {
    DEFAULT_PROBE_PLATE_THICKNESS_MM
}

/// Per-profile starting values for the probe cycle parameter sheet.
///
/// The record stores operator-entered defaults only. It carries no retry
/// policy and triggers no motion: the probe engine still validates every cycle
/// and the engine's safety gateway still owns motion. Every length is in
/// millimetres; [`ProbeDefaultsRecord::feed_mm_min`] is in millimetres per
/// minute.
///
/// A profile written before this record existed loads
/// [`ProbeDefaultsRecord::default`]. A record missing individual fields also
/// falls back to their defaults.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProbeDefaultsRecord {
    /// Safe Z plane, in millimetres, within `+/-`[`MAX_PROBE_SAFE_Z_MM`].
    #[serde(default = "default_probe_safe_z_mm")]
    pub safe_z_mm: f64,
    /// Search distance, in millimetres, positive and at most
    /// [`MAX_PROBE_SEARCH_DISTANCE_MM`].
    #[serde(default = "default_probe_search_distance_mm")]
    pub search_distance_mm: f64,
    /// Probe feed, in millimetres per minute, positive and at most
    /// [`MAX_PROBE_FEED_MM_MIN`].
    #[serde(default = "default_probe_feed_mm_min")]
    pub feed_mm_min: f64,
    /// Relative Z lift after a touch, in millimetres, `0..=`[`MAX_PROBE_RETRACT_MM`].
    #[serde(default = "default_probe_retract_mm")]
    pub retract_mm: f64,
    /// Probe tip diameter, in millimetres, `0..=`[`MAX_PROBE_DIAMETER_MM`].
    #[serde(default = "default_probe_diameter_mm")]
    pub probe_diameter_mm: f64,
    /// Distance from the measured edge to the new zero, in millimetres, within
    /// `+/-`[`MAX_PROBE_EDGE_OFFSET_MM`].
    #[serde(default = "default_probe_edge_offset_mm")]
    pub edge_offset_mm: f64,
    /// Touch-plate thickness, in millimetres,
    /// `0..=`[`MAX_PROBE_PLATE_THICKNESS_MM`].
    #[serde(default = "default_probe_plate_thickness_mm")]
    pub plate_thickness_mm: f64,
}

impl Default for ProbeDefaultsRecord {
    fn default() -> Self {
        Self {
            safe_z_mm: DEFAULT_PROBE_SAFE_Z_MM,
            search_distance_mm: DEFAULT_PROBE_SEARCH_DISTANCE_MM,
            feed_mm_min: DEFAULT_PROBE_FEED_MM_MIN,
            retract_mm: DEFAULT_PROBE_RETRACT_MM,
            probe_diameter_mm: DEFAULT_PROBE_DIAMETER_MM,
            edge_offset_mm: DEFAULT_PROBE_EDGE_OFFSET_MM,
            plate_thickness_mm: DEFAULT_PROBE_PLATE_THICKNESS_MM,
        }
    }
}

impl ProbeDefaultsRecord {
    /// Validate every default against the probe engine's bounds.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::NonFiniteProbeDefault`] when a value is NaN or
    /// infinite, [`ValidationError::ProbeDefaultNotPositive`] when search,
    /// feed, or retract is zero or negative, and
    /// [`ValidationError::ProbeDefaultOutOfRange`] when a value is outside its
    /// documented inclusive range.
    /// Only a zero probe diameter and zero plate thickness are accepted as
    /// non-positive values, matching the probe engine.
    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_probe_default_range(
            "safe_z_mm",
            self.safe_z_mm,
            -MAX_PROBE_SAFE_Z_MM,
            MAX_PROBE_SAFE_Z_MM,
        )?;
        validate_probe_default_positive(
            "search_distance_mm",
            self.search_distance_mm,
            MAX_PROBE_SEARCH_DISTANCE_MM,
        )?;
        validate_probe_default_positive("feed_mm_min", self.feed_mm_min, MAX_PROBE_FEED_MM_MIN)?;
        validate_probe_default_positive("retract_mm", self.retract_mm, MAX_PROBE_RETRACT_MM)?;
        validate_probe_default_range(
            "probe_diameter_mm",
            self.probe_diameter_mm,
            0.0,
            MAX_PROBE_DIAMETER_MM,
        )?;
        validate_probe_default_range(
            "edge_offset_mm",
            self.edge_offset_mm,
            0.0,
            MAX_PROBE_EDGE_OFFSET_MM,
        )?;
        validate_probe_default_range(
            "plate_thickness_mm",
            self.plate_thickness_mm,
            0.0,
            MAX_PROBE_PLATE_THICKNESS_MM,
        )
    }
}

fn validate_probe_default_range(
    field: &'static str,
    value: f64,
    min: f64,
    max: f64,
) -> Result<(), ValidationError> {
    if !value.is_finite() {
        return Err(ValidationError::NonFiniteProbeDefault { field, value });
    }
    if value < min || value > max {
        return Err(ValidationError::ProbeDefaultOutOfRange {
            field,
            value,
            min,
            max,
        });
    }
    Ok(())
}

fn validate_probe_default_positive(
    field: &'static str,
    value: f64,
    max: f64,
) -> Result<(), ValidationError> {
    if !value.is_finite() {
        return Err(ValidationError::NonFiniteProbeDefault { field, value });
    }
    if value <= 0.0 {
        return Err(ValidationError::ProbeDefaultNotPositive { field, value, max });
    }
    if value > max {
        return Err(ValidationError::ProbeDefaultOutOfRange {
            field,
            value,
            min: 0.0,
            max,
        });
    }
    Ok(())
}

/// Measurement frame a height map was captured in.
///
/// The frame is stored beside the map so it is not reused after a work-offset
/// or tool change. It is informational: loading a profile never applies an
/// offset, `G92`, or tool-length value to the controller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeightMapFrame {
    /// Work coordinate system selected while probing.
    pub wcs: Wcs,
    /// Stored offset of `wcs` at capture time, in millimetres.
    ///
    /// It must carry X, Y and Z axes; extra axes are preserved.
    pub wcs_offset: Position,
    /// Active `G92` offset at capture time, in millimetres.
    ///
    /// It must carry X, Y and Z axes; extra axes are preserved.
    pub g92: Position,
    /// Active tool length offset (`TLO`) Z component at capture time, in
    /// millimetres.
    pub tool_length_offset_z_mm: f64,
}

/// One persisted height map for a workpiece/setup.
///
/// Samples are row-major: `samples[row * columns + column]`. Each `Some` value
/// is a measured Z coordinate in millimetres and `None` marks a point that was
/// not probed or whose probe failed. The record keeps its probing frame and
/// grid so it can be shown and revalidated, but it contains no probing or
/// motion types, and loading a profile never applies the map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeightMapRecord {
    /// User label. Non-empty, at most 128 characters, and free of control
    /// characters. Labels need not be unique within a profile.
    pub name: String,
    /// Tool number used for the measurement, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_number: Option<u32>,
    /// Creation time as Unix seconds, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at_unix_s: Option<u64>,
    /// X coordinate of the first grid point, in millimetres.
    pub origin_x_mm: f64,
    /// Y coordinate of the first grid point, in millimetres.
    pub origin_y_mm: f64,
    /// Column spacing, in millimetres. Positive and at most
    /// [`MAX_HEIGHT_MAP_STEP_MM`].
    pub step_x_mm: f64,
    /// Row spacing, in millimetres. Positive and at most
    /// [`MAX_HEIGHT_MAP_STEP_MM`].
    pub step_y_mm: f64,
    /// Number of columns, in `2..=`[`MAX_HEIGHT_MAP_AXIS_POINTS`].
    pub columns: u16,
    /// Number of rows, in `2..=`[`MAX_HEIGHT_MAP_AXIS_POINTS`].
    pub rows: u16,
    /// Nominal surface Z used by the probing plan, in millimetres.
    ///
    /// This is a recorded grid parameter, not the measured surface.
    pub surface_z_mm: f64,
    /// Optional constant Z offset recorded with the map, in millimetres.
    ///
    /// `None` means no offset was recorded; zero is distinct from absent.
    pub z_offset_mm: Option<f64>,
    /// Row-major samples. The length must equal `columns * rows`.
    ///
    /// The default is empty so a missing array is reported by
    /// [`HeightMapRecord::validate`] as a count error rather than being
    /// silently filled.
    #[serde(default)]
    pub samples: Vec<Option<f64>>,
    /// Measurement frame at capture time.
    pub frame: HeightMapFrame,
}

impl HeightMapRecord {
    /// Number of grid points, `columns * rows`.
    #[must_use]
    pub fn point_count(&self) -> usize {
        usize::from(self.columns) * usize::from(self.rows)
    }

    /// Row-major sample index for a column and row.
    ///
    /// Returns `None` when the cell is outside the declared grid. The caller
    /// should still use [`HeightMapRecord::sample`] or index [`Self::samples`]
    /// to read a value, because a declared cell may be invalid.
    #[must_use]
    pub fn index(&self, column: u16, row: u16) -> Option<usize> {
        if column >= self.columns || row >= self.rows {
            return None;
        }
        Some(usize::from(row) * usize::from(self.columns) + usize::from(column))
    }

    /// Sample at a column and row.
    ///
    /// Returns `None` when the cell is outside the grid, missing, or invalid.
    /// Use [`HeightMapRecord::index`] to distinguish an out-of-range cell from
    /// an invalid sample.
    #[must_use]
    pub fn sample(&self, column: u16, row: u16) -> Option<f64> {
        self.samples
            .get(self.index(column, row)?)
            .copied()
            .flatten()
    }

    /// Validate the label, grid, samples, and measurement frame.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::InvalidHeightMap`] for an empty, over-long,
    /// or control-character label, an out-of-range column/row count, a
    /// non-positive or over-large step, a frame without X/Y/Z, or a non-finite
    /// frame or tool-length value;
    /// [`ValidationError::HeightMapOutOfRange`] for a finite coordinate or
    /// sample outside ±[`MAX_HEIGHT_MAP_COORDINATE_MM`], including a grid
    /// extent that overflows that bound; and
    /// [`ValidationError::HeightMapSampleCount`] when the sample vector length
    /// is not exactly `columns * rows`.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.name.trim().is_empty() {
            return Err(ValidationError::InvalidHeightMap {
                name: self.name.clone(),
                field: "name",
                reason: "must not be empty",
            });
        }
        if self.name.chars().count() > MAX_HEIGHT_MAP_NAME {
            return Err(ValidationError::InvalidHeightMap {
                name: self.name.clone(),
                field: "name",
                reason: "exceeds the 128-character limit",
            });
        }
        if self.name.chars().any(char::is_control) {
            return Err(ValidationError::InvalidHeightMap {
                name: self.name.clone(),
                field: "name",
                reason: "must not contain control characters",
            });
        }
        for (field, value) in [("columns", self.columns), ("rows", self.rows)] {
            if !(2..=MAX_HEIGHT_MAP_AXIS_POINTS).contains(&value) {
                return Err(ValidationError::InvalidHeightMap {
                    name: self.name.clone(),
                    field,
                    reason: "must be between 2 and 128",
                });
            }
        }
        check_height_map_step(&self.name, "step_x_mm", self.step_x_mm)?;
        check_height_map_step(&self.name, "step_y_mm", self.step_y_mm)?;
        check_height_map_coordinate(&self.name, "origin_x_mm", self.origin_x_mm)?;
        check_height_map_coordinate(&self.name, "origin_y_mm", self.origin_y_mm)?;
        check_height_map_coordinate(&self.name, "surface_z_mm", self.surface_z_mm)?;
        if let Some(value) = self.z_offset_mm {
            check_height_map_coordinate(&self.name, "z_offset_mm", value)?;
        }
        let max_x = self.origin_x_mm + self.step_x_mm * f64::from(self.columns.saturating_sub(1));
        let max_y = self.origin_y_mm + self.step_y_mm * f64::from(self.rows.saturating_sub(1));
        check_height_map_coordinate(&self.name, "grid_max_x_mm", max_x)?;
        check_height_map_coordinate(&self.name, "grid_max_y_mm", max_y)?;

        let expected = self.point_count();
        if self.samples.len() != expected {
            return Err(ValidationError::HeightMapSampleCount {
                name: self.name.clone(),
                expected,
                actual: self.samples.len(),
            });
        }
        for value in self.samples.iter().flatten() {
            check_height_map_coordinate(&self.name, "samples", *value)?;
        }
        validate_height_map_frame(self)?;
        Ok(())
    }
}

fn validate_height_map_frame(map: &HeightMapRecord) -> Result<(), ValidationError> {
    let frame = &map.frame;
    validate_height_map_frame_position(&map.name, "frame.wcs_offset", frame.wcs_offset)?;
    validate_height_map_frame_position(&map.name, "frame.g92", frame.g92)?;
    let tlo = frame.tool_length_offset_z_mm;
    if !tlo.is_finite() {
        return Err(ValidationError::InvalidHeightMap {
            name: map.name.clone(),
            field: "frame.tool_length_offset_z_mm",
            reason: "must be finite",
        });
    }
    if tlo.abs() > MAX_HEIGHT_MAP_COORDINATE_MM {
        return Err(ValidationError::HeightMapOutOfRange {
            name: map.name.clone(),
            field: "frame.tool_length_offset_z_mm",
            value: tlo,
            min: -MAX_HEIGHT_MAP_COORDINATE_MM,
            max: MAX_HEIGHT_MAP_COORDINATE_MM,
        });
    }
    Ok(())
}

fn validate_height_map_frame_position(
    name: &str,
    field: &'static str,
    position: Position,
) -> Result<(), ValidationError> {
    let axes = position.axes();
    if !(axes.contains(Axis::X) && axes.contains(Axis::Y) && axes.contains(Axis::Z)) {
        return Err(ValidationError::InvalidHeightMap {
            name: name.to_string(),
            field,
            reason: "must carry X, Y and Z axes",
        });
    }
    for (_, value) in position.iter() {
        if !value.is_finite() {
            return Err(ValidationError::InvalidHeightMap {
                name: name.to_string(),
                field,
                reason: "coordinates must be finite",
            });
        }
        if value.abs() > MAX_HEIGHT_MAP_COORDINATE_MM {
            return Err(ValidationError::HeightMapOutOfRange {
                name: name.to_string(),
                field,
                value,
                min: -MAX_HEIGHT_MAP_COORDINATE_MM,
                max: MAX_HEIGHT_MAP_COORDINATE_MM,
            });
        }
    }
    Ok(())
}

fn check_height_map_step(
    name: &str,
    field: &'static str,
    value: f64,
) -> Result<(), ValidationError> {
    if value.is_finite() && value > 0.0 && value <= MAX_HEIGHT_MAP_STEP_MM {
        Ok(())
    } else {
        Err(ValidationError::InvalidHeightMap {
            name: name.to_string(),
            field,
            reason: "must be finite, positive, and at most 1000 mm",
        })
    }
}

fn check_height_map_coordinate(
    name: &str,
    field: &'static str,
    value: f64,
) -> Result<(), ValidationError> {
    if !value.is_finite() {
        return Err(ValidationError::InvalidHeightMap {
            name: name.to_string(),
            field,
            reason: "must be finite",
        });
    }
    if value.abs() > MAX_HEIGHT_MAP_COORDINATE_MM {
        return Err(ValidationError::HeightMapOutOfRange {
            name: name.to_string(),
            field,
            value,
            min: -MAX_HEIGHT_MAP_COORDINATE_MM,
            max: MAX_HEIGHT_MAP_COORDINATE_MM,
        });
    }
    Ok(())
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
    /// Physical XY mount offset of the probe tip, `probe tip - controlled
    /// point`, in machine/work axes.
    ///
    /// Profiles written before this field existed load with a zero offset.
    #[serde(default)]
    pub probe_offset: ProbeOffsetRecord,
    /// Starting values for the probe cycle parameter sheet.
    ///
    /// Profiles written before this field existed load
    /// [`ProbeDefaultsRecord::default`]. These are defaults only; loading a
    /// profile never starts a probe cycle.
    #[serde(default)]
    pub probe_defaults: ProbeDefaultsRecord,
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
    /// Persisted height maps, in user order.
    ///
    /// Profiles written before this field existed load with an empty list.
    /// The limit is [`MAX_HEIGHT_MAPS`].
    #[serde(default)]
    pub height_maps: Vec<HeightMapRecord>,
}

impl Default for ControllerData {
    fn default() -> Self {
        Self {
            schema_version: CONTROLLER_SCHEMA_VERSION,
            settings: BTreeMap::new(),
            offsets: OffsetSnapshot::default(),
            probe_offset: ProbeOffsetRecord::default(),
            probe_defaults: ProbeDefaultsRecord::default(),
            tools: ToolTable::default(),
            macros: Vec::new(),
            startup_lines: StartupLines::default(),
            keymap: Keymap::default(),
            height_maps: Vec::new(),
        }
    }
}

impl ControllerData {
    /// An empty profile for a controller.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate the schema, settings, offsets, probe offset, probe defaults,
    /// tools, macros, startup lines, keymap, and height maps.
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
        self.probe_offset.validate()?;
        self.probe_defaults.validate()?;
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
        if self.height_maps.len() > MAX_HEIGHT_MAPS {
            return Err(ValidationError::TooManyHeightMaps {
                count: self.height_maps.len(),
                max: MAX_HEIGHT_MAPS,
            });
        }
        for map in &self.height_maps {
            map.validate()?;
        }
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

    fn sample_height_map() -> HeightMapRecord {
        HeightMapRecord {
            name: "Fixture top".into(),
            tool_number: Some(3),
            created_at_unix_s: Some(1_700_000_000),
            origin_x_mm: 10.0,
            origin_y_mm: 20.0,
            step_x_mm: 5.0,
            step_y_mm: 2.5,
            columns: 3,
            rows: 2,
            surface_z_mm: -1.0,
            z_offset_mm: Some(0.25),
            samples: vec![Some(0.0), Some(0.1), None, Some(0.2), Some(0.3), Some(0.4)],
            frame: HeightMapFrame {
                wcs: Wcs::G55,
                wcs_offset: Position::xyz(1.0, 2.0, 3.0),
                g92: Position::xyz(0.1, 0.2, 0.3),
                tool_length_offset_z_mm: -12.5,
            },
        }
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
        data.probe_offset = ProbeOffsetRecord {
            x_mm: -12.5,
            y_mm: 7.25,
        };
        data.probe_defaults = ProbeDefaultsRecord {
            safe_z_mm: 15.0,
            search_distance_mm: 8.0,
            feed_mm_min: 150.0,
            retract_mm: 2.5,
            probe_diameter_mm: 4.0,
            edge_offset_mm: 1.5,
            plate_thickness_mm: 12.0,
        };

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
        data.height_maps = vec![sample_height_map()];
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
        assert!(json.contains("\"height_maps\""));
        assert!(json.contains("\"probe_offset\""));
        assert_eq!(decoded.height_maps, data.height_maps);
        assert_eq!(decoded.probe_offset, data.probe_offset);
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
        assert!(data.height_maps.is_empty());
    }

    #[test]
    fn height_map_records_round_trip_complete_and_partial() {
        let mut complete = sample_height_map();
        complete.samples = (0..6).map(|index| Some(f64::from(index) * 0.1)).collect();
        complete.z_offset_mm = None;
        complete.tool_number = None;
        complete.created_at_unix_s = None;

        let partial = sample_height_map();
        assert!(partial.samples.contains(&None));

        for map in [complete, partial] {
            let json = serde_json::to_string(&map).expect("serialize");
            let decoded: HeightMapRecord = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(decoded, map);
            decoded.validate().expect("valid");
        }
    }

    #[test]
    fn old_profiles_without_height_maps_load_empty() {
        let data: ControllerData =
            serde_json::from_str(r#"{"schema_version":1}"#).expect("old profile");
        assert!(data.height_maps.is_empty());
        assert_eq!(data.probe_offset, ProbeOffsetRecord::default());
        data.validate().expect("valid");
        assert_eq!(data, ControllerData::default());

        let mut value = serde_json::to_value(sample_data()).expect("json");
        let object = value.as_object_mut().expect("object");
        object.remove("height_maps");
        object.remove("probe_offset");
        let decoded: ControllerData = serde_json::from_value(value).expect("decode");
        assert!(decoded.height_maps.is_empty());
        assert_eq!(decoded.probe_offset, ProbeOffsetRecord::default());
        decoded.validate().expect("valid");
        assert_eq!(decoded, {
            let mut expected = sample_data();
            expected.height_maps.clear();
            expected.probe_offset = ProbeOffsetRecord::default();
            expected
        });
    }

    #[test]
    fn probe_offset_round_trips_and_defaults_to_zero() {
        let default = ControllerData::new();
        assert_eq!(default.probe_offset, ProbeOffsetRecord::default());
        assert_eq!(default.probe_offset.x_mm, 0.0);
        assert_eq!(default.probe_offset.y_mm, 0.0);
        default.validate().expect("default profile is valid");

        let data = sample_data();
        assert_eq!(
            data.probe_offset,
            ProbeOffsetRecord {
                x_mm: -12.5,
                y_mm: 7.25,
            }
        );
        let json = serde_json::to_string_pretty(&data).expect("serialize");
        let decoded: ControllerData = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, data);
        assert_eq!(decoded.probe_offset.x_mm, -12.5);
        assert_eq!(decoded.probe_offset.y_mm, 7.25);
        decoded.validate().expect("valid after round trip");

        let partial: ProbeOffsetRecord =
            serde_json::from_str(r#"{"x_mm":1.5}"#).expect("partial record");
        assert_eq!(
            partial,
            ProbeOffsetRecord {
                x_mm: 1.5,
                y_mm: 0.0
            }
        );
    }

    #[test]
    fn probe_offset_rejects_non_finite_and_out_of_range_values() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for (axis, offset) in [
                (
                    "x_mm",
                    ProbeOffsetRecord {
                        x_mm: value,
                        y_mm: 0.0,
                    },
                ),
                (
                    "y_mm",
                    ProbeOffsetRecord {
                        x_mm: 0.0,
                        y_mm: value,
                    },
                ),
            ] {
                let mut data = ControllerData::new();
                data.probe_offset = offset;
                assert!(matches!(
                    data.validate(),
                    Err(ValidationError::NonFiniteProbeOffset { axis: found, .. })
                        if found == axis
                ));
            }
        }

        for value in [MAX_PROBE_OFFSET_MM + 0.001, -MAX_PROBE_OFFSET_MM - 0.001] {
            for (axis, offset) in [
                (
                    "x_mm",
                    ProbeOffsetRecord {
                        x_mm: value,
                        y_mm: 0.0,
                    },
                ),
                (
                    "y_mm",
                    ProbeOffsetRecord {
                        x_mm: 0.0,
                        y_mm: value,
                    },
                ),
            ] {
                let mut data = ControllerData::new();
                data.probe_offset = offset;
                assert!(matches!(
                    data.validate(),
                    Err(ValidationError::ProbeOffsetOutOfRange { axis: found, .. })
                        if found == axis
                ));
            }
        }

        for value in [MAX_PROBE_OFFSET_MM, -MAX_PROBE_OFFSET_MM, 0.0] {
            let mut data = ControllerData::new();
            data.probe_offset = ProbeOffsetRecord {
                x_mm: value,
                y_mm: value,
            };
            data.validate().expect("bound is inclusive");
        }
    }

    #[test]
    fn height_map_index_and_lookup_are_row_major() {
        let map = sample_height_map();
        assert_eq!(map.point_count(), 6);
        assert_eq!(map.index(0, 0), Some(0));
        assert_eq!(map.index(2, 0), Some(2));
        assert_eq!(map.index(0, 1), Some(3));
        assert_eq!(map.index(2, 1), Some(5));
        assert_eq!(map.index(3, 0), None);
        assert_eq!(map.index(0, 2), None);
        assert_eq!(map.sample(0, 0), Some(0.0));
        assert_eq!(map.sample(2, 0), None);
        assert_eq!(map.sample(3, 0), None);
    }

    #[test]
    fn height_map_sample_count_and_values_are_validated() {
        let mut wrong_count = sample_height_map();
        wrong_count.samples.pop();
        assert_eq!(
            wrong_count.validate(),
            Err(ValidationError::HeightMapSampleCount {
                name: "Fixture top".into(),
                expected: 6,
                actual: 5,
            })
        );

        let mut empty = sample_height_map();
        empty.samples.clear();
        assert!(matches!(
            empty.validate(),
            Err(ValidationError::HeightMapSampleCount {
                expected: 6,
                actual: 0,
                ..
            })
        ));

        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut map = sample_height_map();
            map.samples[0] = Some(value);
            assert!(matches!(
                map.validate(),
                Err(ValidationError::InvalidHeightMap {
                    field: "samples",
                    ..
                })
            ));
        }

        let mut too_large = sample_height_map();
        too_large.samples[0] = Some(MAX_HEIGHT_MAP_COORDINATE_MM + 1.0);
        assert!(matches!(
            too_large.validate(),
            Err(ValidationError::HeightMapOutOfRange {
                field: "samples",
                ..
            })
        ));

        let mut too_small = sample_height_map();
        too_small.samples[0] = Some(-MAX_HEIGHT_MAP_COORDINATE_MM - 1.0);
        assert!(matches!(
            too_small.validate(),
            Err(ValidationError::HeightMapOutOfRange {
                field: "samples",
                ..
            })
        ));
    }

    #[test]
    fn height_map_grid_bounds_are_validated() {
        type Mutation = fn(&mut HeightMapRecord);
        let cases: [(&str, Mutation); 11] = [
            ("columns", |map| map.columns = 1),
            ("rows", |map| map.rows = 1),
            ("columns", |map| {
                map.columns = MAX_HEIGHT_MAP_AXIS_POINTS + 1
            }),
            ("rows", |map| map.rows = MAX_HEIGHT_MAP_AXIS_POINTS + 1),
            ("step_x_mm", |map| map.step_x_mm = 0.0),
            ("step_y_mm", |map| map.step_y_mm = -1.0),
            ("step_x_mm", |map| {
                map.step_x_mm = MAX_HEIGHT_MAP_STEP_MM + 0.5
            }),
            ("step_y_mm", |map| map.step_y_mm = f64::NAN),
            ("origin_x_mm", |map| map.origin_x_mm = f64::NAN),
            ("grid_max_x_mm", |map| {
                map.origin_x_mm = MAX_HEIGHT_MAP_COORDINATE_MM
            }),
            ("grid_max_y_mm", |map| {
                map.origin_y_mm = MAX_HEIGHT_MAP_COORDINATE_MM
            }),
        ];
        for (expected_field, mutate) in cases {
            let mut map = sample_height_map();
            mutate(&mut map);
            match map.validate() {
                Err(ValidationError::InvalidHeightMap { field, .. })
                | Err(ValidationError::HeightMapOutOfRange { field, .. }) => {
                    assert_eq!(field, expected_field);
                }
                other => panic!("expected {expected_field} to fail, got {other:?}"),
            }
        }

        let mut surface = sample_height_map();
        surface.surface_z_mm = f64::INFINITY;
        assert!(matches!(
            surface.validate(),
            Err(ValidationError::InvalidHeightMap {
                field: "surface_z_mm",
                ..
            })
        ));

        let mut offset = sample_height_map();
        offset.z_offset_mm = Some(MAX_HEIGHT_MAP_COORDINATE_MM + 1.0);
        assert!(matches!(
            offset.validate(),
            Err(ValidationError::HeightMapOutOfRange {
                field: "z_offset_mm",
                ..
            })
        ));
    }

    #[test]
    fn height_map_frame_must_match_the_axis_model() {
        let mut missing_z = sample_height_map();
        missing_z
            .frame
            .wcs_offset
            .set_axes(Axes::parse("XY").expect("valid"));
        assert!(matches!(
            missing_z.validate(),
            Err(ValidationError::InvalidHeightMap {
                field: "frame.wcs_offset",
                reason: "must carry X, Y and Z axes",
                ..
            })
        ));

        let mut missing_y = sample_height_map();
        missing_y
            .frame
            .g92
            .set_axes(Axes::parse("XZ").expect("valid"));
        assert!(matches!(
            missing_y.validate(),
            Err(ValidationError::InvalidHeightMap {
                field: "frame.g92",
                ..
            })
        ));

        let mut non_finite = sample_height_map();
        non_finite.frame.g92 = Position::xyz(0.0, 0.0, f64::NAN);
        assert!(matches!(
            non_finite.validate(),
            Err(ValidationError::InvalidHeightMap {
                field: "frame.g92",
                reason: "coordinates must be finite",
                ..
            })
        ));

        let mut out_of_range = sample_height_map();
        out_of_range.frame.wcs_offset = Position::xyz(MAX_HEIGHT_MAP_COORDINATE_MM + 1.0, 0.0, 0.0);
        assert!(matches!(
            out_of_range.validate(),
            Err(ValidationError::HeightMapOutOfRange {
                field: "frame.wcs_offset",
                ..
            })
        ));

        let mut extra_axes = sample_height_map();
        extra_axes.frame.wcs_offset = Position::xyz(1.0, 2.0, 3.0);
        extra_axes
            .frame
            .wcs_offset
            .set_axes(Axes::parse("XYZA").expect("valid"));
        extra_axes.validate().expect("extra axes are preserved");

        let mut non_finite_tlo = sample_height_map();
        non_finite_tlo.frame.tool_length_offset_z_mm = f64::NAN;
        assert!(matches!(
            non_finite_tlo.validate(),
            Err(ValidationError::InvalidHeightMap {
                field: "frame.tool_length_offset_z_mm",
                ..
            })
        ));

        let mut too_large_tlo = sample_height_map();
        too_large_tlo.frame.tool_length_offset_z_mm = MAX_HEIGHT_MAP_COORDINATE_MM + 1.0;
        assert!(matches!(
            too_large_tlo.validate(),
            Err(ValidationError::HeightMapOutOfRange {
                field: "frame.tool_length_offset_z_mm",
                ..
            })
        ));
    }

    #[test]
    fn height_map_labels_are_validated() {
        for (name, reason) in [
            ("", "must not be empty"),
            ("   ", "must not be empty"),
            ("Bad\nName", "must not contain control characters"),
        ] {
            let mut map = sample_height_map();
            map.name = name.into();
            assert!(matches!(
                map.validate(),
                Err(ValidationError::InvalidHeightMap {
                    field: "name",
                    reason: found,
                    ..
                }) if found == reason
            ));
        }

        let mut too_long = sample_height_map();
        too_long.name = "x".repeat(MAX_HEIGHT_MAP_NAME + 1);
        assert!(matches!(
            too_long.validate(),
            Err(ValidationError::InvalidHeightMap {
                field: "name",
                reason: "exceeds the 128-character limit",
                ..
            })
        ));
    }

    #[test]
    fn profile_height_map_limit_is_enforced() {
        let mut data = ControllerData::new();
        data.height_maps = (0..=MAX_HEIGHT_MAPS)
            .map(|index| {
                let mut map = sample_height_map();
                map.name = format!("Map {index}");
                map
            })
            .collect();
        assert_eq!(
            data.validate(),
            Err(ValidationError::TooManyHeightMaps {
                count: MAX_HEIGHT_MAPS + 1,
                max: MAX_HEIGHT_MAPS,
            })
        );

        data.height_maps.pop();
        data.validate().expect("limit is accepted");
    }

    #[test]
    fn probe_defaults_are_stable() {
        let defaults = ProbeDefaultsRecord::default();
        assert_eq!(defaults.safe_z_mm, 10.0);
        assert_eq!(defaults.search_distance_mm, 5.0);
        assert_eq!(defaults.feed_mm_min, 100.0);
        assert_eq!(defaults.retract_mm, 2.0);
        assert_eq!(defaults.probe_diameter_mm, 3.0);
        assert_eq!(defaults.edge_offset_mm, 0.0);
        assert_eq!(defaults.plate_thickness_mm, 10.0);
        assert_eq!(ControllerData::new().probe_defaults, defaults);
        defaults.validate().expect("defaults are valid");
    }

    #[test]
    fn probe_defaults_round_trip_non_default_values() {
        let defaults = ProbeDefaultsRecord {
            safe_z_mm: 25.0,
            search_distance_mm: 12.5,
            feed_mm_min: 250.0,
            retract_mm: 3.5,
            probe_diameter_mm: 6.0,
            edge_offset_mm: 2.5,
            plate_thickness_mm: 12.0,
        };
        defaults.validate().expect("valid");
        let json = serde_json::to_string(&defaults).expect("serialize");
        let decoded: ProbeDefaultsRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, defaults);
        decoded.validate().expect("valid after round trip");

        let data = ControllerData {
            probe_defaults: defaults,
            ..ControllerData::new()
        };
        let json = serde_json::to_string(&data).expect("serialize profile");
        assert!(json.contains("\"probe_defaults\""));
        let decoded: ControllerData = serde_json::from_str(&json).expect("deserialize profile");
        assert_eq!(decoded.probe_defaults, defaults);
        decoded.validate().expect("valid after round trip");
    }

    #[test]
    fn old_profiles_without_probe_defaults_load_the_defaults() {
        let data: ControllerData =
            serde_json::from_str(r#"{"schema_version":1}"#).expect("old profile");
        assert_eq!(data.probe_defaults, ProbeDefaultsRecord::default());
        data.validate().expect("valid");

        let mut value = serde_json::to_value(sample_data()).expect("json");
        value
            .as_object_mut()
            .expect("object")
            .remove("probe_defaults");
        let decoded: ControllerData = serde_json::from_value(value).expect("decode");
        assert_eq!(decoded.probe_defaults, ProbeDefaultsRecord::default());
        decoded.validate().expect("valid");
    }

    #[test]
    fn partial_probe_defaults_use_field_defaults() {
        let partial: ProbeDefaultsRecord =
            serde_json::from_str(r#"{"safe_z_mm":20.0}"#).expect("partial record");
        assert_eq!(partial.safe_z_mm, 20.0);
        assert_eq!(partial.search_distance_mm, 5.0);
        assert_eq!(partial.feed_mm_min, 100.0);
        assert_eq!(partial.retract_mm, 2.0);
        assert_eq!(partial.probe_diameter_mm, 3.0);
        assert_eq!(partial.edge_offset_mm, 0.0);
        assert_eq!(partial.plate_thickness_mm, 10.0);
        partial.validate().expect("partial record is valid");
    }

    #[test]
    fn probe_defaults_accept_every_bound() {
        type SetField = fn(&mut ProbeDefaultsRecord, f64);
        let cases: [(&str, SetField, f64, f64); 7] = [
            (
                "safe_z_mm",
                |record, value| record.safe_z_mm = value,
                -MAX_PROBE_SAFE_Z_MM,
                MAX_PROBE_SAFE_Z_MM,
            ),
            (
                "search_distance_mm",
                |record, value| record.search_distance_mm = value,
                f64::MIN_POSITIVE,
                MAX_PROBE_SEARCH_DISTANCE_MM,
            ),
            (
                "feed_mm_min",
                |record, value| record.feed_mm_min = value,
                f64::MIN_POSITIVE,
                MAX_PROBE_FEED_MM_MIN,
            ),
            (
                "retract_mm",
                |record, value| record.retract_mm = value,
                f64::MIN_POSITIVE,
                MAX_PROBE_RETRACT_MM,
            ),
            (
                "probe_diameter_mm",
                |record, value| record.probe_diameter_mm = value,
                0.0,
                MAX_PROBE_DIAMETER_MM,
            ),
            (
                "edge_offset_mm",
                |record, value| record.edge_offset_mm = value,
                0.0,
                MAX_PROBE_EDGE_OFFSET_MM,
            ),
            (
                "plate_thickness_mm",
                |record, value| record.plate_thickness_mm = value,
                0.0,
                MAX_PROBE_PLATE_THICKNESS_MM,
            ),
        ];
        for (field, set, min, max) in cases {
            for value in [min, max] {
                let mut record = ProbeDefaultsRecord::default();
                set(&mut record, value);
                record
                    .validate()
                    .unwrap_or_else(|error| panic!("{field}={value} rejected: {error}"));
            }
        }
    }

    #[test]
    fn probe_defaults_reject_zero_negative_and_over_max_values() {
        type Mutation = fn(&mut ProbeDefaultsRecord);
        let cases: [(&str, Mutation); 18] = [
            ("safe_z_mm", |record| {
                record.safe_z_mm = MAX_PROBE_SAFE_Z_MM + 1.0
            }),
            ("safe_z_mm", |record| {
                record.safe_z_mm = -MAX_PROBE_SAFE_Z_MM - 1.0
            }),
            ("search_distance_mm", |record| {
                record.search_distance_mm = 0.0
            }),
            ("search_distance_mm", |record| {
                record.search_distance_mm = -1.0
            }),
            ("search_distance_mm", |record| {
                record.search_distance_mm = MAX_PROBE_SEARCH_DISTANCE_MM + 0.5
            }),
            ("feed_mm_min", |record| record.feed_mm_min = 0.0),
            ("feed_mm_min", |record| record.feed_mm_min = -1.0),
            ("feed_mm_min", |record| {
                record.feed_mm_min = MAX_PROBE_FEED_MM_MIN + 1.0
            }),
            ("retract_mm", |record| record.retract_mm = 0.0),
            ("retract_mm", |record| record.retract_mm = -1.0),
            ("retract_mm", |record| {
                record.retract_mm = MAX_PROBE_RETRACT_MM + 0.5
            }),
            ("probe_diameter_mm", |record| {
                record.probe_diameter_mm = -1.0
            }),
            ("probe_diameter_mm", |record| {
                record.probe_diameter_mm = MAX_PROBE_DIAMETER_MM + 0.5
            }),
            ("edge_offset_mm", |record| record.edge_offset_mm = -0.001),
            ("edge_offset_mm", |record| {
                record.edge_offset_mm = -MAX_PROBE_EDGE_OFFSET_MM - 1.0
            }),
            ("edge_offset_mm", |record| {
                record.edge_offset_mm = MAX_PROBE_EDGE_OFFSET_MM + 1.0
            }),
            ("plate_thickness_mm", |record| {
                record.plate_thickness_mm = -1.0
            }),
            ("plate_thickness_mm", |record| {
                record.plate_thickness_mm = MAX_PROBE_PLATE_THICKNESS_MM + 0.5
            }),
        ];
        for (expected_field, mutate) in cases {
            let mut record = ProbeDefaultsRecord::default();
            mutate(&mut record);
            match record.validate() {
                Err(ValidationError::ProbeDefaultOutOfRange {
                    field, min, max, ..
                }) => {
                    assert_eq!(field, expected_field);
                    assert!(min < max);
                }
                Err(ValidationError::ProbeDefaultNotPositive { field, .. }) => {
                    assert_eq!(field, expected_field);
                }
                other => panic!("expected {expected_field} to fail, got {other:?}"),
            }
        }
    }

    #[test]
    fn probe_defaults_reject_non_finite_values() {
        type SetField = fn(&mut ProbeDefaultsRecord, f64);
        let fields: [(&str, SetField); 7] = [
            ("safe_z_mm", |record, value| record.safe_z_mm = value),
            ("search_distance_mm", |record, value| {
                record.search_distance_mm = value
            }),
            ("feed_mm_min", |record, value| record.feed_mm_min = value),
            ("retract_mm", |record, value| record.retract_mm = value),
            ("probe_diameter_mm", |record, value| {
                record.probe_diameter_mm = value
            }),
            ("edge_offset_mm", |record, value| {
                record.edge_offset_mm = value
            }),
            ("plate_thickness_mm", |record, value| {
                record.plate_thickness_mm = value
            }),
        ];
        for (expected_field, set) in fields {
            for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut record = ProbeDefaultsRecord::default();
                set(&mut record, value);
                match record.validate() {
                    Err(ValidationError::NonFiniteProbeDefault {
                        field,
                        value: found,
                    }) => {
                        assert_eq!(field, expected_field);
                        assert!(!found.is_finite());
                    }
                    other => panic!("expected {expected_field}={value} to fail, got {other:?}"),
                }
            }
        }
    }

    #[test]
    fn controller_data_rejects_invalid_probe_defaults() {
        let mut not_positive = ControllerData::new();
        not_positive.probe_defaults.feed_mm_min = 0.0;
        assert!(matches!(
            not_positive.validate(),
            Err(ValidationError::ProbeDefaultNotPositive {
                field: "feed_mm_min",
                ..
            })
        ));

        let mut out_of_range = ControllerData::new();
        out_of_range.probe_defaults.search_distance_mm = MAX_PROBE_SEARCH_DISTANCE_MM + 1.0;
        assert!(matches!(
            out_of_range.validate(),
            Err(ValidationError::ProbeDefaultOutOfRange {
                field: "search_distance_mm",
                ..
            })
        ));

        let mut non_finite = ControllerData::new();
        non_finite.probe_defaults.safe_z_mm = f64::NAN;
        assert!(matches!(
            non_finite.validate(),
            Err(ValidationError::NonFiniteProbeDefault {
                field: "safe_z_mm",
                ..
            })
        ));
    }

    #[test]
    fn probe_defaults_only_add_a_field_to_the_profile() {
        assert_eq!(CONTROLLER_SCHEMA_VERSION, 1);
        let data = sample_data();
        let value = serde_json::to_value(&data).expect("json");
        let object = value.as_object().expect("object");
        assert_eq!(object["schema_version"], 1);
        for field in [
            "schema_version",
            "settings",
            "offsets",
            "probe_offset",
            "probe_defaults",
            "tools",
            "macros",
            "startup_lines",
            "keymap",
            "height_maps",
        ] {
            assert!(object.contains_key(field), "profile is missing {field}");
        }
        assert_eq!(object.len(), 10);
        let decoded: ControllerData = serde_json::from_value(value).expect("decode");
        assert_eq!(decoded, data);
        decoded.validate().expect("valid");
    }
}
