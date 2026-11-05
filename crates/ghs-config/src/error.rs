//! Typed errors for validation, loading, and saving local data.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

use crate::profile::ProfileKey;

/// A validation problem in data that is about to be persisted or was loaded.
///
/// These variants are intentionally stable enough for a settings or import UI
/// to show a field-specific message without matching on error strings.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ValidationError {
    /// The record was written by a newer or unknown schema version.
    #[error("unsupported {what} schema version {found}; this build supports version {supported}")]
    UnsupportedVersion {
        /// Record that carries the version, for example `"backup bundle"`.
        what: &'static str,
        /// Version found in the data.
        found: u32,
        /// Version this build writes and accepts.
        supported: u32,
    },
    /// Two tool records use the same tool number.
    #[error("duplicate tool number T{0}")]
    DuplicateToolNumber(u32),
    /// A tool dimension is negative or not finite.
    #[error("tool T{tool} {field} must be finite and non-negative (got {value})")]
    InvalidToolDimension {
        /// Tool number.
        tool: u32,
        /// Field name, `"diameter_mm"` or `"length_mm"`.
        field: &'static str,
        /// Rejected value.
        value: f64,
    },
    /// A tool text field is invalid or too long.
    #[error("tool T{tool} {field} is invalid: {reason}")]
    InvalidToolText {
        /// Tool number.
        tool: u32,
        /// Field name, for example `"name"`.
        field: &'static str,
        /// Human-readable reason.
        reason: &'static str,
    },
    /// Two macros share a name.
    #[error("duplicate macro name {0:?}")]
    DuplicateMacro(String),
    /// A macro has an invalid or over-long field.
    #[error("macro {name:?} is invalid: {reason}")]
    InvalidMacro {
        /// Macro name, or an empty string when no name was supplied.
        name: String,
        /// Human-readable reason.
        reason: &'static str,
    },
    /// Two key bindings target the same action.
    #[error("duplicate key binding action {0:?}")]
    DuplicateKeyBinding(String),
    /// A key binding has an invalid or over-long field.
    #[error("key binding {action:?} is invalid: {reason}")]
    InvalidKeyBinding {
        /// Binding action, or an empty string when no action was supplied.
        action: String,
        /// Human-readable reason.
        reason: &'static str,
    },
    /// A startup line is not a single, bounded line.
    #[error("startup line {line} is invalid: {reason}")]
    InvalidStartupLine {
        /// Wire name, `"N0"` or `"N1"`.
        line: &'static str,
        /// Human-readable reason.
        reason: &'static str,
    },
    /// An offset coordinate is NaN or infinite.
    #[error("offset snapshot contains a non-finite {field} coordinate")]
    NonFiniteOffset {
        /// Offset name, for example `"G54"` or `"G28"`.
        field: &'static str,
    },
    /// A probe mount offset component is NaN or infinite.
    #[error("probe offset {axis} must be finite (got {value})")]
    NonFiniteProbeOffset {
        /// Component name, `"x_mm"` or `"y_mm"`.
        axis: &'static str,
        /// Rejected value.
        value: f64,
    },
    /// A probe mount offset component is outside the accepted range.
    #[error("probe offset {axis} must be between {min} and {max} millimetres (got {value})")]
    ProbeOffsetOutOfRange {
        /// Component name, `"x_mm"` or `"y_mm"`.
        axis: &'static str,
        /// Rejected value.
        value: f64,
        /// Smallest accepted value.
        min: f64,
        /// Largest accepted value.
        max: f64,
    },
    /// A persisted probe default is NaN or infinite.
    #[error("probe default {field} must be finite (got {value})")]
    NonFiniteProbeDefault {
        /// Field name, for example `"safe_z_mm"` or `"feed_mm_min"`.
        field: &'static str,
        /// Rejected value.
        value: f64,
    },
    /// A persisted probe default is outside its accepted range.
    #[error("probe default {field} must be between {min} and {max} (got {value})")]
    ProbeDefaultOutOfRange {
        /// Field name, for example `"safe_z_mm"` or `"retract_mm"`.
        field: &'static str,
        /// Rejected value.
        value: f64,
        /// Smallest accepted value.
        min: f64,
        /// Largest accepted value.
        max: f64,
    },
    /// A persisted probe default that must be positive is zero or negative.
    #[error("probe default {field} must be greater than 0 and at most {max} (got {value})")]
    ProbeDefaultNotPositive {
        /// Field name, `"search_distance_mm"` or `"feed_mm_min"`.
        field: &'static str,
        /// Rejected value.
        value: f64,
        /// Largest accepted value.
        max: f64,
    },
    /// A persisted pre- or post-probe macro exceeds the byte limit.
    #[error("probe macro {field} is {bytes} bytes; the limit is {max}")]
    ProbeMacroTooLong {
        /// Field name, `"pre_probe_macro"` or `"post_probe_macro"`.
        field: &'static str,
        /// Length of the rejected text, in UTF-8 bytes.
        bytes: usize,
        /// Largest accepted length, in UTF-8 bytes.
        max: usize,
    },
    /// A persisted probe macro contains a forbidden control character.
    #[error("probe macro {field} must not contain control character {character:?}")]
    ProbeMacroControlCharacter {
        /// Field name, `"pre_probe_macro"` or `"post_probe_macro"`.
        field: &'static str,
        /// First rejected character.
        character: char,
    },
    /// A setting value is NaN, infinite, or structurally invalid.
    #[error("setting ${number} is invalid: {reason}")]
    InvalidSetting {
        /// Setting number.
        number: u16,
        /// Human-readable reason.
        reason: &'static str,
    },
    /// A height-map label, grid count, step, or measurement frame is invalid.
    #[error("height map {name:?} {field} is invalid: {reason}")]
    InvalidHeightMap {
        /// Map label, or an empty string when the label itself is invalid.
        name: String,
        /// Field name, for example `"name"`, `"columns"`, or
        /// `"frame.wcs_offset"`.
        field: &'static str,
        /// Human-readable reason.
        reason: &'static str,
    },
    /// A height-map coordinate, offset, or sample is outside the accepted
    /// range.
    #[error(
        "height map {name:?} {field} must be finite and between {min} and {max} \
         millimetres (got {value})"
    )]
    HeightMapOutOfRange {
        /// Map label.
        name: String,
        /// Field name, for example `"origin_x_mm"` or `"samples"`.
        field: &'static str,
        /// Rejected value.
        value: f64,
        /// Smallest accepted value.
        min: f64,
        /// Largest accepted value.
        max: f64,
    },
    /// A height map's sample vector does not match its declared grid.
    #[error("height map {name:?} has {actual} samples; expected {expected} (columns * rows)")]
    HeightMapSampleCount {
        /// Map label.
        name: String,
        /// Sample count implied by `columns * rows`.
        expected: usize,
        /// Sample count actually stored.
        actual: usize,
    },
    /// A profile stores more height maps than the format allows.
    #[error("profile stores {count} height maps; the limit is {max}")]
    TooManyHeightMaps {
        /// Number of maps in the profile.
        count: usize,
        /// Maximum accepted number of maps.
        max: usize,
    },
    /// A window or layout dimension is outside the accepted range.
    #[error("window {field} must be between {min} and {max} (got {value})")]
    WindowOutOfRange {
        /// Field name, for example `"width"`.
        field: &'static str,
        /// Rejected value.
        value: u32,
        /// Smallest accepted value.
        min: u32,
        /// Largest accepted value.
        max: u32,
    },
    /// A profile key is not safe to use as a file name.
    #[error("profile key {0:?} is not a valid storage key")]
    InvalidProfileKey(String),
    /// A profile display name is not valid.
    #[error("profile display name {0:?} is not valid")]
    InvalidProfileName(String),
    /// A stored UI locale tag is malformed.
    #[error("locale {locale:?} is invalid: {reason}")]
    InvalidLocale {
        /// Rejected locale tag.
        locale: String,
        /// Human-readable reason.
        reason: &'static str,
    },
    /// A job-history record has an invalid field.
    #[error("job history entry {field} is invalid: {reason}")]
    InvalidJobHistoryEntry {
        /// Field name, for example `"name"` or `"elapsed_s"`.
        field: &'static str,
        /// Human-readable reason.
        reason: &'static str,
    },
    /// The app config stores more job-history records than the format allows.
    #[error("app config stores {count} job-history entries; the limit is {max}")]
    TooManyJobHistoryEntries {
        /// Number of entries in the config.
        count: usize,
        /// Maximum accepted number of entries.
        max: usize,
    },
    /// The bundle's stored key does not match its embedded controller info.
    #[error("profile key {stored:?} does not match the embedded controller identity {computed:?}")]
    ProfileIdentityMismatch {
        /// Key stored in the bundle.
        stored: String,
        /// Key computed from the bundle's `ControllerInfo`.
        computed: String,
    },
}

/// An I/O, parse, validation, or identity failure from the config store.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// No standard configuration directory could be found.
    #[error(
        "no platform configuration directory could be determined; \
         set XDG_CONFIG_HOME, HOME, APPDATA, or USERPROFILE"
    )]
    NoConfigDir,
    /// A path has no usable parent directory or file name.
    #[error("configuration path {0} has no usable file name")]
    InvalidPath(PathBuf),
    /// Reading or writing a file failed.
    #[error("failed to {action} {path}: {source}")]
    Io {
        /// Action being attempted, for example `"read"` or `"replace"`.
        action: &'static str,
        /// Path involved.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// A file did not contain valid JSON.
    #[error("{path} is not valid JSON: {source}")]
    Json {
        /// File that failed to parse.
        path: PathBuf,
        /// Underlying parse error with line and column.
        #[source]
        source: serde_json::Error,
    },
    /// In-memory JSON was malformed.
    #[error("configuration JSON is not valid: {0}")]
    JsonText(#[source] serde_json::Error),
    /// A value could not be encoded as JSON.
    #[error("could not encode configuration JSON: {0}")]
    Encode(#[source] serde_json::Error),
    /// The loaded data failed validation.
    #[error(transparent)]
    Validation(#[from] ValidationError),
    /// A backup belongs to a different controller.
    #[error(
        "controller identity mismatch: backup is for {stored}, \
         connected controller is {current}"
    )]
    IdentityMismatch {
        /// Identity stored in the backup.
        stored: ProfileKey,
        /// Identity of the connected controller.
        current: ProfileKey,
    },
    /// The backup's profile has no board or driver identity to check.
    #[error("controller identity could not be verified for {profile}")]
    UnverifiedIdentity {
        /// Profile that lacks identifying fields.
        profile: ProfileKey,
    },
    /// A profile file is stored under the wrong key.
    #[error("profile file {path} contains {found}, expected {expected}")]
    ProfileKeyMismatch {
        /// File that was read.
        path: PathBuf,
        /// Key embedded in the file.
        found: ProfileKey,
        /// Key implied by the file name.
        expected: ProfileKey,
    },
}
