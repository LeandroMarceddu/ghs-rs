//! Local configuration, profiles, tool tables, and backups.
//!
//! This crate owns data at rest, not controller I/O. The engine writes settings
//! and offsets; this crate stores the user's profiles, app preferences, tool
//! table, and versioned backups with atomic writes and rolling backup files.
//!
//! Everything is human-readable JSON with stable field and key ordering. There
//! is no binary format, no telemetry, and no network access.
//!
//! # Data at rest
//!
//! - [`AppConfig`] holds theme, display units, the last profile key,
//!   window/layout fields, a bounded job history, and the recent-file list in
//!   `app.json`.
//! - [`ControllerData`] is one profile: settings snapshot, offsets, probe
//!   mount offset, probe defaults and macros, the application-owned tool
//!   table, macros, startup lines, keymap, and persisted height maps.
//! - [`BackupBundle`] is a versioned portable snapshot with app config,
//!   controller identity, controller data, and a creation timestamp.
//!
//! On disk, `app.json` is an [`AppConfig`] and `profiles/<id>.json` is an
//! object with a `profile` key (the full [`ProfileKey`]) and a `data` key (the
//! [`ControllerData`]). `backups/` holds rotated previous versions of those
//! store-owned documents; [`ConfigStore::export_backup`] writes a user-chosen
//! bundle path instead.
//!
//! # Safety boundaries
//!
//! - Loading or importing a profile never writes it to a controller. The
//!   engine's safety gateway and explicit user confirmation own that step.
//! - A corrupt or unknown-version file is a typed error, not a silent reset.
//! - A backup for a different or unverifiable controller is an explicit
//!   warning or error; see [`IdentityCheck`] and [`IdentityPolicy`].
//! - Password settings must be filtered by the caller before they reach
//!   [`ControllerData::settings`]. This crate has no setting-type metadata and
//!   cannot recognise a secret after the fact.
//!
//! # Example
//!
//! ```
//! use ghs_config::{AppConfig, ConfigStore};
//! use std::time::{SystemTime, UNIX_EPOCH};
//!
//! let stamp = SystemTime::now()
//!     .duration_since(UNIX_EPOCH)
//!     .map(|duration| duration.as_nanos())
//!     .unwrap_or(0);
//! let root = std::env::temp_dir().join(format!("ghs-config-doc-{stamp}"));
//! let store = ConfigStore::new(&root);
//! let config = store.load_or_default().expect("missing file uses defaults");
//! assert_eq!(config, AppConfig::default());
//! # let _ = std::fs::remove_dir_all(&root);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod app;
mod backup;
mod controller;
mod error;
mod profile;
mod store;

pub use app::{
    APP_SCHEMA_VERSION, AppConfig, JobHistoryRecord, JobHistoryResult, MAX_JOB_HISTORY,
    MAX_JOB_HISTORY_ELAPSED_S, MAX_JOB_HISTORY_NAME_BYTES, MAX_RECENT_FILE_PATH_BYTES,
    MAX_RECENT_FILES, RecentFileRecord, ThemeMode, WindowLayout,
};
pub use backup::{
    BACKUP_SCHEMA_VERSION, BackupBundle, ControllerIdentity, IdentityCheck, IdentityMismatch,
    IdentityPolicy, ImportOutcome,
};
pub use controller::{
    CONTROLLER_SCHEMA_VERSION, ControllerData, HeightMapFrame, HeightMapRecord, KeyBinding, Keymap,
    MAX_HEIGHT_MAP_AXIS_POINTS, MAX_HEIGHT_MAP_COORDINATE_MM, MAX_HEIGHT_MAP_STEP_MM,
    MAX_HEIGHT_MAPS, MAX_PROBE_DIAMETER_MM, MAX_PROBE_EDGE_OFFSET_MM, MAX_PROBE_FEED_MM_MIN,
    MAX_PROBE_MACRO_BYTES, MAX_PROBE_OFFSET_MM, MAX_PROBE_PLATE_THICKNESS_MM, MAX_PROBE_RETRACT_MM,
    MAX_PROBE_SAFE_Z_MM, MAX_PROBE_SEARCH_DISTANCE_MM, Macro, OffsetSnapshot, ProbeDefaultsRecord,
    ProbeOffsetRecord, StartupLines, ToolRecord, ToolTable,
};
pub use error::{ConfigError, ValidationError};
pub use profile::ProfileKey;
pub use store::{
    APP_CONFIG_FILE, APP_DIR_NAME, BACKUPS_DIR, ConfigStore, DEFAULT_BACKUP_LIMIT, HostPlatform,
    PROFILES_DIR, config_root_with,
};
