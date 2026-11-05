//! Application-wide preferences: theme, units, locale, last profile, layout,
//! and a bounded local job history.

use ghs_core::Units;
use serde::{Deserialize, Serialize};

use crate::error::ValidationError;
use crate::profile::ProfileKey;

/// Schema version written by this build.
pub const APP_SCHEMA_VERSION: u32 = 1;

/// Maximum number of finished-job records kept in [`AppConfig::job_history`].
pub const MAX_JOB_HISTORY: usize = 100;
/// Maximum accepted length of a job-history name, in UTF-8 bytes.
pub const MAX_JOB_HISTORY_NAME_BYTES: usize = 256;
/// Maximum accepted job runtime in a history record: 30 days in seconds.
pub const MAX_JOB_HISTORY_ELAPSED_S: u64 = 30 * 24 * 60 * 60;

/// Maximum number of recently opened program files kept in
/// [`AppConfig::recent_files`].
pub const MAX_RECENT_FILES: usize = 10;
/// Maximum accepted length of a recent-file path, in UTF-8 bytes.
pub const MAX_RECENT_FILE_PATH_BYTES: usize = 4096;

/// Default window width in logical pixels.
const DEFAULT_WIDTH: u32 = 1280;
/// Default window height in logical pixels.
const DEFAULT_HEIGHT: u32 = 800;
/// Default left rail width in logical pixels.
const DEFAULT_LEFT_PANEL: u32 = 360;
/// Default bottom panel height in logical pixels.
const DEFAULT_BOTTOM_PANEL: u32 = 240;
/// Smallest accepted main-window dimension.
const MIN_WINDOW_WIDTH: u32 = 320;
/// Smallest accepted main-window height.
const MIN_WINDOW_HEIGHT: u32 = 240;
/// Largest accepted main-window dimension, to bound corrupt files.
const MAX_WINDOW: u32 = 16_384;
/// Largest accepted side-panel dimension.
const MAX_PANEL: u32 = 8_192;
/// Largest accepted locale tag length in bytes.
const MAX_LOCALE_LEN: usize = 16;
/// Locale used when a config predates the field or no catalog matches.
const DEFAULT_LOCALE: &str = "en";

/// Colour theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    /// Follow the operating system.
    #[default]
    System,
    /// Always use the dark theme.
    Dark,
    /// Always use the light theme.
    Light,
}

/// How a finished job ended, as stored in [`JobHistoryRecord`].
///
/// The variants are persisted with `snake_case` names. The application maps
/// its live job state and abort reason to one of them when it appends a
/// record; this crate does not infer the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobHistoryResult {
    /// The job reached the end of the streamed program.
    Completed,
    /// The job was aborted before completion, for example by an alarm, a
    /// safety door, an E-stop, or a reset.
    Aborted,
    /// The operator stopped the job.
    Stopped,
    /// The link was lost or the controller interrupted the job.
    Interrupted,
    /// The job could not be streamed or failed with a program error.
    Failed,
}

/// One finished job in the bounded local history.
///
/// Records are appended with [`AppConfig::push_job_history`], which validates
/// the fields, trims the name, and keeps the newest [`MAX_JOB_HISTORY`]
/// entries. [`started_unix_s`](Self::started_unix_s) is supplied by the
/// caller; this crate never reads the clock for a history record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobHistoryRecord {
    /// Loaded program name or path.
    ///
    /// The stored value is trimmed, non-empty, and at most
    /// [`MAX_JOB_HISTORY_NAME_BYTES`] UTF-8 bytes.
    pub name: String,
    /// Job start time as Unix seconds.
    pub started_unix_s: u64,
    /// Active job runtime in seconds, paused intervals excluded.
    ///
    /// At most [`MAX_JOB_HISTORY_ELAPSED_S`] (30 days).
    pub elapsed_s: u64,
    /// Number of streamed or loaded source lines.
    pub lines: u32,
    /// How the job ended.
    pub result: JobHistoryResult,
    /// Whether the run was a `$C` dry run.
    pub check_run: bool,
}

impl JobHistoryRecord {
    /// Validate the name and runtime bounds.
    ///
    /// The name is trimmed before it is checked, so surrounding whitespace is
    /// not significant; the byte limit applies to the trimmed value.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::InvalidJobHistoryEntry`] when the trimmed
    /// name is empty or longer than [`MAX_JOB_HISTORY_NAME_BYTES`], or when
    /// `elapsed_s` exceeds [`MAX_JOB_HISTORY_ELAPSED_S`].
    pub fn validate(&self) -> Result<(), ValidationError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(ValidationError::InvalidJobHistoryEntry {
                field: "name",
                reason: "must not be empty",
            });
        }
        if name.len() > MAX_JOB_HISTORY_NAME_BYTES {
            return Err(ValidationError::InvalidJobHistoryEntry {
                field: "name",
                reason: "exceeds the 256-byte limit",
            });
        }
        if self.elapsed_s > MAX_JOB_HISTORY_ELAPSED_S {
            return Err(ValidationError::InvalidJobHistoryEntry {
                field: "elapsed_s",
                reason: "must not exceed 30 days",
            });
        }
        Ok(())
    }
}

/// One local program file that was opened recently.
///
/// Records are added with [`AppConfig::push_recent_file`], which trims the
/// path, removes an exact duplicate, and keeps the newest
/// [`MAX_RECENT_FILES`] entries. The timestamp is supplied by the caller; this
/// crate never reads the clock for a recent-file record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentFileRecord {
    /// File path as recorded by the app.
    ///
    /// The stored value is trimmed, non-empty, and at most
    /// [`MAX_RECENT_FILE_PATH_BYTES`] UTF-8 bytes.
    pub path: String,
    /// Last time the file was opened, as Unix seconds.
    pub opened_unix_s: u64,
}

impl RecentFileRecord {
    /// Validate the path bounds.
    ///
    /// The path is trimmed before it is checked, so surrounding whitespace is
    /// not significant; the byte limit applies to the trimmed value.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::InvalidRecentFileEntry`] when the trimmed
    /// path is empty or longer than [`MAX_RECENT_FILE_PATH_BYTES`].
    pub fn validate(&self) -> Result<(), ValidationError> {
        let path = self.path.trim();
        if path.is_empty() {
            return Err(ValidationError::InvalidRecentFileEntry {
                field: "path",
                reason: "must not be empty",
            });
        }
        if path.len() > MAX_RECENT_FILE_PATH_BYTES {
            return Err(ValidationError::InvalidRecentFileEntry {
                field: "path",
                reason: "exceeds the 4096-byte limit",
            });
        }
        Ok(())
    }
}

/// Last known main-window geometry and panel sizes.
///
/// These values are suggestions for the shell; a window manager may clamp or
/// ignore them. Sizes are in logical pixels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowLayout {
    /// Main-window width in logical pixels.
    pub width: u32,
    /// Main-window height in logical pixels.
    pub height: u32,
    /// Saved X position, when the window manager reports one.
    pub x: Option<i32>,
    /// Saved Y position, when the window manager reports one.
    pub y: Option<i32>,
    /// Whether the window was maximized.
    pub maximized: bool,
    /// Width of the left rail in logical pixels.
    pub left_panel_width: u32,
    /// Height of the bottom panel in logical pixels.
    pub bottom_panel_height: u32,
}

impl Default for WindowLayout {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            x: None,
            y: None,
            maximized: false,
            left_panel_width: DEFAULT_LEFT_PANEL,
            bottom_panel_height: DEFAULT_BOTTOM_PANEL,
        }
    }
}

impl WindowLayout {
    /// Validate bounds that matter when restoring a window.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::WindowOutOfRange`] for a zero or absurdly
    /// large dimension.
    pub fn validate(&self) -> Result<(), ValidationError> {
        check_range("width", self.width, MIN_WINDOW_WIDTH, MAX_WINDOW)?;
        check_range("height", self.height, MIN_WINDOW_HEIGHT, MAX_WINDOW)?;
        check_range("left_panel_width", self.left_panel_width, 0, MAX_PANEL)?;
        check_range(
            "bottom_panel_height",
            self.bottom_panel_height,
            0,
            MAX_PANEL,
        )
    }
}

/// Persisted application preferences.
///
/// The file is human-readable JSON. [`crate::ConfigStore::load_or_default`]
/// returns [`AppConfig::default`] when the file is missing and an error when
/// it is corrupt; a corrupt file is never silently replaced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    /// Schema version, always [`APP_SCHEMA_VERSION`] for a config this build
    /// writes.
    pub schema_version: u32,
    /// Colour theme preference.
    #[serde(default)]
    pub theme: ThemeMode,
    /// Display unit preference. Domain coordinates stay in millimetres.
    #[serde(default)]
    pub units: Units,
    /// UI locale tag, for example `"en"` or `"pt-BR"`.
    ///
    /// Unknown locales are accepted at this layer so adding catalogs does not
    /// require a schema change; the shell falls back to English when no
    /// catalog matches. A stored tag must be non-empty, at most 16 bytes, and
    /// contain only ASCII letters, hyphens, and underscores.
    #[serde(default = "default_locale")]
    pub locale: String,
    /// Profile that was active at the last successful connection or import.
    #[serde(default)]
    pub last_profile: Option<ProfileKey>,
    /// Whether the first-run setup wizard has been completed.
    ///
    /// Configs written before this field existed load as `false`, so a new or
    /// upgraded profile sees onboarding once. The shell saves the app config
    /// after the wizard finishes.
    #[serde(default)]
    pub setup_complete: bool,
    /// Window geometry and panel sizes.
    #[serde(default)]
    pub window: WindowLayout,
    /// Bounded local history of finished jobs, oldest first.
    ///
    /// New records are appended with [`AppConfig::push_job_history`]. Configs
    /// written before this field existed load an empty list; unknown or
    /// corrupt entries are rejected by [`AppConfig::validate`].
    #[serde(default)]
    pub job_history: Vec<JobHistoryRecord>,
    /// Recently opened local program files, newest first.
    ///
    /// New records are added with [`AppConfig::push_recent_file`]. Configs
    /// written before this field existed load an empty list; invalid entries
    /// and an over-limit list are rejected by [`AppConfig::validate`].
    #[serde(default)]
    pub recent_files: Vec<RecentFileRecord>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: APP_SCHEMA_VERSION,
            theme: ThemeMode::default(),
            units: Units::default(),
            locale: default_locale(),
            last_profile: None,
            setup_complete: false,
            window: WindowLayout::default(),
            job_history: Vec::new(),
            recent_files: Vec::new(),
        }
    }
}

impl AppConfig {
    /// Validate the schema version, locale tag, layout bounds, job history,
    /// and recent files.
    ///
    /// # Errors
    ///
    /// Returns a [`ValidationError`] when the record was written by another
    /// schema version, the locale tag is malformed, the window size is
    /// impossible, the job history or recent-file list exceeds its cap, or an
    /// entry is invalid.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != APP_SCHEMA_VERSION {
            return Err(ValidationError::UnsupportedVersion {
                what: "app config",
                found: self.schema_version,
                supported: APP_SCHEMA_VERSION,
            });
        }
        validate_locale(&self.locale)?;
        self.window.validate()?;
        validate_job_history(&self.job_history)?;
        validate_recent_files(&self.recent_files)
    }

    /// Append one finished job to the local history.
    ///
    /// The record is validated and its name is trimmed before it is stored.
    /// When the history is already at [`MAX_JOB_HISTORY`] entries, the oldest
    /// entries are dropped so the newest entries are kept. This helper only
    /// updates the in-memory config; persistence is owned by
    /// [`crate::ConfigStore::save_app_config`].
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::InvalidJobHistoryEntry`] when the record is
    /// invalid. The config is left unchanged in that case.
    pub fn push_job_history(
        &mut self,
        mut record: JobHistoryRecord,
    ) -> Result<(), ValidationError> {
        record.name = record.name.trim().to_owned();
        record.validate()?;
        self.job_history.push(record);
        let excess = self.job_history.len().saturating_sub(MAX_JOB_HISTORY);
        if excess > 0 {
            self.job_history.drain(..excess);
        }
        Ok(())
    }

    /// Add or move one local program file to the front of the recent list.
    ///
    /// The path is trimmed before it is stored. A record with the same exact
    /// path (after trimming) replaces the previous one, so reopening a file
    /// moves it to the front with a new timestamp. The newest
    /// [`MAX_RECENT_FILES`] entries are kept and older entries are dropped.
    /// This helper only updates the in-memory config; persistence is owned by
    /// [`crate::ConfigStore::save_app_config`].
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::InvalidRecentFileEntry`] when the trimmed
    /// path is empty or exceeds [`MAX_RECENT_FILE_PATH_BYTES`]. The config is
    /// left unchanged in that case.
    pub fn push_recent_file(
        &mut self,
        path: &str,
        opened_unix_s: u64,
    ) -> Result<(), ValidationError> {
        let record = RecentFileRecord {
            path: path.trim().to_owned(),
            opened_unix_s,
        };
        record.validate()?;
        self.recent_files
            .retain(|existing| existing.path.trim() != record.path);
        self.recent_files.insert(0, record);
        self.recent_files.truncate(MAX_RECENT_FILES);
        Ok(())
    }
}

fn validate_job_history(history: &[JobHistoryRecord]) -> Result<(), ValidationError> {
    if history.len() > MAX_JOB_HISTORY {
        return Err(ValidationError::TooManyJobHistoryEntries {
            count: history.len(),
            max: MAX_JOB_HISTORY,
        });
    }
    for record in history {
        record.validate()?;
    }
    Ok(())
}

fn validate_recent_files(files: &[RecentFileRecord]) -> Result<(), ValidationError> {
    if files.len() > MAX_RECENT_FILES {
        return Err(ValidationError::TooManyRecentFiles {
            count: files.len(),
            max: MAX_RECENT_FILES,
        });
    }
    for record in files {
        record.validate()?;
    }
    Ok(())
}

fn default_locale() -> String {
    DEFAULT_LOCALE.to_owned()
}

/// Validate the locale tag without requiring a known catalog.
fn validate_locale(locale: &str) -> Result<(), ValidationError> {
    let reason = if locale.is_empty() {
        "must not be empty"
    } else if locale.len() > MAX_LOCALE_LEN {
        "exceeds the 16-byte limit"
    } else if !locale
        .bytes()
        .all(|byte| byte.is_ascii_alphabetic() || byte == b'-' || byte == b'_')
    {
        "must contain only ASCII letters, hyphens, and underscores"
    } else {
        return Ok(());
    };
    Err(ValidationError::InvalidLocale {
        locale: locale.to_owned(),
        reason,
    })
}

fn check_range(field: &'static str, value: u32, min: u32, max: u32) -> Result<(), ValidationError> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(ValidationError::WindowOutOfRange {
            field,
            value,
            min,
            max,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RESULTS: [JobHistoryResult; 5] = [
        JobHistoryResult::Completed,
        JobHistoryResult::Aborted,
        JobHistoryResult::Stopped,
        JobHistoryResult::Interrupted,
        JobHistoryResult::Failed,
    ];

    fn history_record(name: &str) -> JobHistoryRecord {
        JobHistoryRecord {
            name: name.to_owned(),
            started_unix_s: 1_700_000_000,
            elapsed_s: 42,
            lines: 1_200,
            result: JobHistoryResult::Completed,
            check_run: false,
        }
    }

    #[test]
    fn defaults_are_stable() {
        let config = AppConfig::default();
        assert_eq!(config.schema_version, APP_SCHEMA_VERSION);
        assert_eq!(config.theme, ThemeMode::System);
        assert_eq!(config.units, Units::Mm);
        assert_eq!(config.locale, "en");
        assert_eq!(config.last_profile, None);
        assert!(!config.setup_complete);
        assert_eq!(config.window.width, 1280);
        assert_eq!(config.window.height, 800);
        assert_eq!(config.window.left_panel_width, 360);
        assert_eq!(config.window.bottom_panel_height, 240);
        assert!(config.job_history.is_empty());
        config.validate().expect("defaults are valid");
    }

    #[test]
    fn config_round_trips_through_json() {
        let config = AppConfig {
            theme: ThemeMode::Light,
            units: Units::Inch,
            locale: "pt-BR".into(),
            last_profile: Some(
                ProfileKey::from_id("flexihal-0123456789abcdef").expect("valid key"),
            ),
            window: WindowLayout {
                width: 1920,
                height: 1080,
                maximized: true,
                left_panel_width: 420,
                ..WindowLayout::default()
            },
            ..AppConfig::default()
        };

        let json = serde_json::to_string_pretty(&config).expect("serialize");
        assert!(json.contains("\"locale\": \"pt-BR\""));
        let decoded: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, config);
        assert_eq!(decoded.locale, "pt-BR");
        decoded.validate().expect("valid");
    }

    #[test]
    fn setup_complete_defaults_to_false() {
        assert!(!AppConfig::default().setup_complete);
        let decoded: AppConfig = serde_json::from_str(r#"{"schema_version":1}"#).expect("parse");
        assert!(!decoded.setup_complete);
    }

    #[test]
    fn setup_complete_round_trips_both_values() {
        for setup_complete in [false, true] {
            let config = AppConfig {
                setup_complete,
                ..AppConfig::default()
            };
            let json = serde_json::to_string(&config).expect("serialize");
            let decoded: AppConfig = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(decoded, config);
            assert_eq!(decoded.setup_complete, setup_complete);
            decoded.validate().expect("valid");
        }
    }

    #[test]
    fn old_json_without_setup_complete_loads_false() {
        let value = serde_json::json!({
            "schema_version": 1,
            "theme": "dark",
            "units": "Mm",
            "locale": "en",
            "last_profile": null,
            "window": {
                "width": 1280,
                "height": 800,
                "x": null,
                "y": null,
                "maximized": false,
                "left_panel_width": 360,
                "bottom_panel_height": 240
            }
        });
        let decoded: AppConfig = serde_json::from_value(value).expect("parse");
        assert!(!decoded.setup_complete);
        decoded.validate().expect("valid");
    }

    #[test]
    fn old_json_without_locale_uses_english() {
        let value = serde_json::json!({
            "schema_version": 1,
            "theme": "dark",
            "units": "Inch",
            "last_profile": null,
            "window": {
                "width": 1280,
                "height": 800,
                "x": null,
                "y": null,
                "maximized": false,
                "left_panel_width": 360,
                "bottom_panel_height": 240
            }
        });
        let decoded: AppConfig = serde_json::from_value(value).expect("parse");
        assert_eq!(decoded.locale, "en");
        assert_eq!(decoded.theme, ThemeMode::Dark);
        assert_eq!(decoded.units, Units::Inch);
        decoded.validate().expect("valid");
    }

    #[test]
    fn invalid_locales_are_rejected() {
        for locale in [
            "",
            "en us",
            "en/us",
            "en2",
            "pt-BR!",
            "p\u{00fc}",
            "abcdefghijklmnopq",
        ] {
            let config = AppConfig {
                locale: locale.to_owned(),
                ..AppConfig::default()
            };
            let error = config.validate().expect_err(locale);
            assert!(
                matches!(error, ValidationError::InvalidLocale { .. }),
                "{locale:?} was accepted or produced {error}"
            );
        }
    }

    #[test]
    fn well_formed_locales_are_accepted() {
        for locale in ["en", "pt-BR", "pt_BR", "zz-ZZ", "abcdefghijklmnop"] {
            let config = AppConfig {
                locale: locale.to_owned(),
                ..AppConfig::default()
            };
            config
                .validate()
                .unwrap_or_else(|error| panic!("{locale:?} was rejected: {error}"));
        }
    }

    #[test]
    fn partial_json_uses_defaults() {
        let decoded: AppConfig = serde_json::from_str(r#"{"schema_version":1}"#).expect("parse");
        assert_eq!(decoded, AppConfig::default());
        let decoded: AppConfig =
            serde_json::from_str(r#"{"schema_version":1,"theme":"dark"}"#).expect("parse");
        assert_eq!(decoded.theme, ThemeMode::Dark);
    }

    #[test]
    fn unknown_version_is_rejected() {
        let config = AppConfig {
            schema_version: 99,
            ..AppConfig::default()
        };
        let error = config.validate().expect_err("rejected");
        assert!(matches!(
            error,
            ValidationError::UnsupportedVersion {
                what: "app config",
                found: 99,
                supported: APP_SCHEMA_VERSION,
            }
        ));
    }

    #[test]
    fn impossible_window_sizes_are_rejected() {
        let mut config = AppConfig::default();
        config.window.width = 0;
        assert!(config.validate().is_err());
        config.window.width = 1280;
        config.window.height = u32::MAX;
        assert!(config.validate().is_err());
        config.window.height = 800;
        config.window.left_panel_width = MAX_PANEL + 1;
        assert!(config.validate().is_err());
    }

    #[test]
    fn job_history_defaults_to_empty() {
        assert!(AppConfig::default().job_history.is_empty());
        let decoded: AppConfig = serde_json::from_str(r#"{"schema_version":1}"#).expect("parse");
        assert!(decoded.job_history.is_empty());
        decoded.validate().expect("valid");
    }

    #[test]
    fn old_json_without_job_history_loads_empty() {
        let value = serde_json::json!({
            "schema_version": 1,
            "theme": "dark",
            "units": "Mm",
            "locale": "en",
            "last_profile": null,
            "setup_complete": true,
            "window": {
                "width": 1280,
                "height": 800,
                "x": null,
                "y": null,
                "maximized": false,
                "left_panel_width": 360,
                "bottom_panel_height": 240
            }
        });
        let decoded: AppConfig = serde_json::from_value(value).expect("parse");
        assert!(decoded.job_history.is_empty());
        decoded.validate().expect("valid");
    }

    #[test]
    fn job_history_round_trips_every_result_and_check_flag() {
        for result in RESULTS {
            for check_run in [false, true] {
                let record = JobHistoryRecord {
                    result,
                    check_run,
                    ..history_record("part.nc")
                };
                record.validate().expect("valid record");
                let json = serde_json::to_string(&record).expect("serialize");
                let decoded: JobHistoryRecord = serde_json::from_str(&json).expect("deserialize");
                assert_eq!(decoded, record);
                assert_eq!(decoded.result, result);
                assert_eq!(decoded.check_run, check_run);
            }
        }
    }

    #[test]
    fn job_history_result_serializes_as_snake_case() {
        for (result, expected) in [
            (JobHistoryResult::Completed, "completed"),
            (JobHistoryResult::Aborted, "aborted"),
            (JobHistoryResult::Stopped, "stopped"),
            (JobHistoryResult::Interrupted, "interrupted"),
            (JobHistoryResult::Failed, "failed"),
        ] {
            let record = JobHistoryRecord {
                result,
                ..history_record("part.nc")
            };
            let json = serde_json::to_string(&record).expect("serialize");
            let needle = format!("\"result\":\"{expected}\"");
            assert!(json.contains(&needle), "{json} does not contain {needle}");
        }
    }

    #[test]
    fn job_history_round_trips_inside_app_config_json() {
        let mut config = AppConfig::default();
        config
            .push_job_history(history_record(" part.nc "))
            .expect("append");
        let mut check_run = history_record("check.nc");
        check_run.result = JobHistoryResult::Interrupted;
        check_run.check_run = true;
        config.push_job_history(check_run).expect("append");

        let json = serde_json::to_string_pretty(&config).expect("serialize");
        assert!(json.contains("\"job_history\""));
        let decoded: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, config);
        assert_eq!(decoded.job_history.len(), 2);
        assert_eq!(decoded.job_history[0].name, "part.nc");
        assert!(decoded.job_history[1].check_run);
        decoded.validate().expect("valid");
    }

    #[test]
    fn push_job_history_keeps_order_and_drops_the_oldest() {
        let extra = 5;
        let mut config = AppConfig::default();
        for index in 0..(MAX_JOB_HISTORY + extra) {
            let mut record = history_record("part.nc");
            record.started_unix_s = 1_700_000_000 + index as u64;
            record.lines = index as u32;
            config.push_job_history(record).expect("append");
        }

        assert_eq!(config.job_history.len(), MAX_JOB_HISTORY);
        assert_eq!(
            config
                .job_history
                .first()
                .expect("oldest kept")
                .started_unix_s,
            1_700_000_000 + extra as u64
        );
        assert_eq!(
            config.job_history.last().expect("newest").started_unix_s,
            1_700_000_000 + (MAX_JOB_HISTORY + extra - 1) as u64
        );
        assert!(
            config
                .job_history
                .windows(2)
                .all(|pair| pair[0].started_unix_s < pair[1].started_unix_s)
        );
        config.validate().expect("valid");
    }

    #[test]
    fn push_job_history_rejects_empty_and_whitespace_names() {
        for name in ["", " ", " \t\n "] {
            let mut config = AppConfig::default();
            let error = config
                .push_job_history(history_record(name))
                .expect_err("rejected");
            assert!(
                matches!(
                    error,
                    ValidationError::InvalidJobHistoryEntry { field: "name", .. }
                ),
                "{name:?} produced {error}"
            );
            assert!(
                config.job_history.is_empty(),
                "rejected record must not be stored"
            );
        }
    }

    #[test]
    fn push_job_history_bounds_names_in_utf8_bytes() {
        let mut config = AppConfig::default();
        let at_limit = "\u{00e9}".repeat(MAX_JOB_HISTORY_NAME_BYTES / 2);
        assert_eq!(at_limit.len(), MAX_JOB_HISTORY_NAME_BYTES);
        config
            .push_job_history(history_record(&at_limit))
            .expect("256 bytes accepted");
        assert_eq!(config.job_history[0].name, at_limit);

        let over_limit = "\u{00e9}".repeat(MAX_JOB_HISTORY_NAME_BYTES / 2 + 1);
        assert!(over_limit.len() > MAX_JOB_HISTORY_NAME_BYTES);
        let error = config
            .push_job_history(history_record(&over_limit))
            .expect_err("rejected");
        assert!(
            matches!(
                error,
                ValidationError::InvalidJobHistoryEntry { field: "name", .. }
            ),
            "over-limit name produced {error}"
        );
        assert_eq!(config.job_history.len(), 1, "rejected record was stored");

        let ascii_over_limit = "a".repeat(MAX_JOB_HISTORY_NAME_BYTES + 1);
        assert!(
            config
                .push_job_history(history_record(&ascii_over_limit))
                .is_err()
        );
    }

    #[test]
    fn push_job_history_trims_the_name() {
        let mut config = AppConfig::default();
        config
            .push_job_history(history_record("  /jobs/part.nc \n"))
            .expect("append");
        assert_eq!(config.job_history[0].name, "/jobs/part.nc");
    }

    #[test]
    fn job_history_rejects_runtimes_over_thirty_days() {
        let mut record = history_record("part.nc");
        record.elapsed_s = MAX_JOB_HISTORY_ELAPSED_S;
        record.validate().expect("30 days accepted");
        record.elapsed_s = MAX_JOB_HISTORY_ELAPSED_S + 1;
        let error = record.validate().expect_err("rejected");
        assert!(
            matches!(
                error,
                ValidationError::InvalidJobHistoryEntry {
                    field: "elapsed_s",
                    ..
                }
            ),
            "over-limit runtime produced {error}"
        );

        let mut config = AppConfig::default();
        assert!(config.push_job_history(record).is_err());
        assert!(config.job_history.is_empty());
    }

    #[test]
    fn config_validate_rejects_an_invalid_history_entry() {
        let mut config = AppConfig::default();
        config.job_history.push(history_record(" "));
        let error = config.validate().expect_err("invalid name");
        assert!(matches!(
            error,
            ValidationError::InvalidJobHistoryEntry { field: "name", .. }
        ));

        config.job_history[0] = history_record("part.nc");
        config.job_history[0].elapsed_s = MAX_JOB_HISTORY_ELAPSED_S + 1;
        let error = config.validate().expect_err("invalid runtime");
        assert!(matches!(
            error,
            ValidationError::InvalidJobHistoryEntry {
                field: "elapsed_s",
                ..
            }
        ));
    }

    #[test]
    fn config_validate_rejects_too_many_history_entries() {
        let mut config = AppConfig::default();
        for _ in 0..=MAX_JOB_HISTORY {
            config.job_history.push(history_record("part.nc"));
        }
        assert_eq!(
            config.validate(),
            Err(ValidationError::TooManyJobHistoryEntries {
                count: MAX_JOB_HISTORY + 1,
                max: MAX_JOB_HISTORY,
            })
        );

        config.job_history.pop();
        config.validate().expect("exactly the limit is accepted");
    }

    fn recent(path: &str, opened_unix_s: u64) -> RecentFileRecord {
        RecentFileRecord {
            path: path.to_owned(),
            opened_unix_s,
        }
    }

    #[test]
    fn recent_files_default_to_empty() {
        let config = AppConfig::default();
        assert!(config.recent_files.is_empty());
        config.validate().expect("defaults are valid");

        let decoded: AppConfig = serde_json::from_str(r#"{"schema_version":1}"#).expect("parse");
        assert!(decoded.recent_files.is_empty());
        decoded.validate().expect("valid");
    }

    #[test]
    fn old_json_without_recent_files_loads_empty() {
        let value = serde_json::json!({
            "schema_version": 1,
            "theme": "dark",
            "units": "Mm",
            "locale": "en",
            "last_profile": null,
            "setup_complete": true,
            "window": {
                "width": 1280,
                "height": 800,
                "x": null,
                "y": null,
                "maximized": false,
                "left_panel_width": 360,
                "bottom_panel_height": 240
            },
            "job_history": []
        });
        let decoded: AppConfig = serde_json::from_value(value).expect("parse");
        assert!(decoded.recent_files.is_empty());
        decoded.validate().expect("valid");
    }

    #[test]
    fn recent_files_round_trip_inside_app_config_json() {
        let mut config = AppConfig::default();
        config
            .push_recent_file(" /jobs/first.nc ", 1_700_000_000)
            .expect("first");
        config
            .push_recent_file("/jobs/second.nc", 1_700_000_100)
            .expect("second");

        let json = serde_json::to_string_pretty(&config).expect("serialize");
        assert!(json.contains("\"recent_files\""));
        let decoded: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, config);
        assert_eq!(decoded.recent_files.len(), 2);
        assert_eq!(decoded.recent_files[0].path, "/jobs/second.nc");
        assert_eq!(decoded.recent_files[1].path, "/jobs/first.nc");
        assert_eq!(decoded.recent_files[1].opened_unix_s, 1_700_000_000);
        decoded.validate().expect("valid");
    }

    #[test]
    fn push_recent_file_moves_a_duplicate_to_the_front() {
        let mut config = AppConfig::default();
        config
            .push_recent_file("/jobs/a.nc", 1_700_000_000)
            .expect("a");
        config
            .push_recent_file("/jobs/b.nc", 1_700_000_100)
            .expect("b");
        config
            .push_recent_file("/jobs/a.nc", 1_700_000_200)
            .expect("a again");

        assert_eq!(
            config
                .recent_files
                .iter()
                .map(|record| (record.path.as_str(), record.opened_unix_s))
                .collect::<Vec<_>>(),
            vec![("/jobs/a.nc", 1_700_000_200), ("/jobs/b.nc", 1_700_000_100)],
            "a duplicate is moved to the front with its new timestamp"
        );
        config.validate().expect("valid");
    }

    #[test]
    fn push_recent_file_trims_but_dedupes_exact_paths() {
        let mut config = AppConfig::default();
        config
            .push_recent_file("  /jobs/part.nc  ", 1_700_000_000)
            .expect("trimmed");
        assert_eq!(config.recent_files[0].path, "/jobs/part.nc");

        config
            .push_recent_file("/jobs/part.nc\n", 1_700_000_100)
            .expect("same path");
        assert_eq!(config.recent_files.len(), 1);
        assert_eq!(config.recent_files[0].opened_unix_s, 1_700_000_100);
    }

    #[test]
    fn push_recent_file_keeps_the_newest_ten() {
        let extra = 4;
        let mut config = AppConfig::default();
        for index in 0..(MAX_RECENT_FILES + extra) {
            config
                .push_recent_file(
                    &format!("/jobs/part-{index}.nc"),
                    1_700_000_000 + index as u64,
                )
                .expect("append");
        }

        assert_eq!(config.recent_files.len(), MAX_RECENT_FILES);
        assert_eq!(config.recent_files[0].path, "/jobs/part-13.nc");
        assert_eq!(
            config
                .recent_files
                .last()
                .map(|record| record.path.as_str()),
            Some("/jobs/part-4.nc")
        );
        config.validate().expect("valid");
    }

    #[test]
    fn push_recent_file_rejects_empty_and_oversized_paths() {
        for path in ["", " ", " \t\n "] {
            let mut config = AppConfig::default();
            let error = config
                .push_recent_file(path, 1_700_000_000)
                .expect_err("empty path rejected");
            assert!(
                matches!(
                    error,
                    ValidationError::InvalidRecentFileEntry { field: "path", .. }
                ),
                "{path:?} produced {error}"
            );
            assert!(config.recent_files.is_empty());
        }

        let mut config = AppConfig::default();
        let at_limit = "/".to_string() + &"a".repeat(MAX_RECENT_FILE_PATH_BYTES - 1);
        assert_eq!(at_limit.len(), MAX_RECENT_FILE_PATH_BYTES);
        config
            .push_recent_file(&at_limit, 1_700_000_000)
            .expect("4096 bytes accepted");

        let over_limit = "/".to_string() + &"a".repeat(MAX_RECENT_FILE_PATH_BYTES);
        let error = config
            .push_recent_file(&over_limit, 1_700_000_000)
            .expect_err("oversized path rejected");
        assert!(
            matches!(
                error,
                ValidationError::InvalidRecentFileEntry { field: "path", .. }
            ),
            "over-limit path produced {error}"
        );
        assert_eq!(config.recent_files.len(), 1, "rejected record was stored");
    }

    #[test]
    fn config_validate_rejects_invalid_recent_files() {
        let mut config = AppConfig::default();
        config.recent_files.push(recent(" ", 1_700_000_000));
        let error = config.validate().expect_err("empty path");
        assert!(matches!(
            error,
            ValidationError::InvalidRecentFileEntry { field: "path", .. }
        ));

        config.recent_files[0] = recent(
            &("/".to_string() + &"a".repeat(MAX_RECENT_FILE_PATH_BYTES)),
            1_700_000_000,
        );
        let error = config.validate().expect_err("oversized path");
        assert!(matches!(
            error,
            ValidationError::InvalidRecentFileEntry { field: "path", .. }
        ));

        config.recent_files[0] = recent("/jobs/part.nc", 1_700_000_000);
        config.validate().expect("valid record");
    }

    #[test]
    fn config_validate_rejects_too_many_recent_files() {
        let mut config = AppConfig::default();
        for index in 0..=MAX_RECENT_FILES {
            config
                .recent_files
                .push(recent(&format!("/jobs/part-{index}.nc"), 1_700_000_000));
        }
        assert_eq!(
            config.validate(),
            Err(ValidationError::TooManyRecentFiles {
                count: MAX_RECENT_FILES + 1,
                max: MAX_RECENT_FILES,
            })
        );

        config.recent_files.pop();
        config.validate().expect("exactly the limit is accepted");
    }
}
