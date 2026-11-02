//! Versioned backup bundles and controller identity checks.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use ghs_core::ControllerInfo;
use serde::{Deserialize, Serialize};

use crate::app::AppConfig;
use crate::controller::ControllerData;
use crate::error::{ConfigError, ValidationError};
use crate::profile::ProfileKey;

/// Schema version written by this build for a backup bundle.
pub const BACKUP_SCHEMA_VERSION: u32 = 1;

/// Controller identity stored in a backup.
///
/// The full [`ControllerInfo`] is kept so a restored bundle can explain which
/// firmware, board, and driver it came from, and so future identity fields do
/// not need a bundle format change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControllerIdentity {
    /// Stable profile key derived from `info`.
    pub profile: ProfileKey,
    /// Controller report the key was derived from.
    pub info: ControllerInfo,
}

/// A complete, portable snapshot of local state.
///
/// A bundle contains app preferences, controller identity, and one controller
/// profile. It is JSON, versioned, and validated before import. It never
/// contains a password setting unless the caller put one in
/// [`ControllerData::settings`] against the documented policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupBundle {
    /// Bundle schema version, always [`BACKUP_SCHEMA_VERSION`] for a bundle
    /// this build writes.
    pub schema_version: u32,
    /// Creation time as Unix seconds. Human-readable JSON keeps this as an
    /// integer so the format needs no time-zone dependency.
    pub created_at_unix_secs: u64,
    /// Application preferences at export time.
    pub app: AppConfig,
    /// Controller the profile belongs to.
    pub controller: ControllerIdentity,
    /// Per-controller settings, offsets, tools, macros, startup lines, and
    /// keymap.
    pub data: ControllerData,
}

impl BackupBundle {
    /// Build a bundle for a controller and its data, stamped with the current
    /// time.
    #[must_use]
    pub fn new(app: AppConfig, info: &ControllerInfo, data: ControllerData) -> Self {
        Self {
            schema_version: BACKUP_SCHEMA_VERSION,
            created_at_unix_secs: now_unix_secs(),
            app,
            controller: ControllerIdentity {
                profile: ProfileKey::from_controller(info),
                info: info.clone(),
            },
            data,
        }
    }

    /// Replace the creation timestamp, for deterministic tests or an imported
    /// timestamp.
    #[must_use]
    pub fn with_created_at(mut self, unix_secs: u64) -> Self {
        self.created_at_unix_secs = unix_secs;
        self
    }

    /// Validate the bundle version, app config, controller data, and the
    /// consistency of the stored profile key.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::UnsupportedVersion`] for an unknown bundle,
    /// app, or profile schema, and any data validation error.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != BACKUP_SCHEMA_VERSION {
            return Err(ValidationError::UnsupportedVersion {
                what: "backup bundle",
                found: self.schema_version,
                supported: BACKUP_SCHEMA_VERSION,
            });
        }
        self.app.validate()?;
        self.data.validate()?;

        let computed = ProfileKey::from_controller(&self.controller.info);
        if computed.id() != self.controller.profile.id() {
            return Err(ValidationError::ProfileIdentityMismatch {
                stored: self.controller.profile.id().to_string(),
                computed: computed.id().to_string(),
            });
        }
        Ok(())
    }

    /// Encode the bundle as pretty JSON with a trailing newline.
    ///
    /// The bundle is validated first, so an unsupported version or invalid
    /// value cannot be written.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Validation`] for an invalid bundle and
    /// [`ConfigError::Encode`] if a value cannot be represented in JSON.
    pub fn to_json_string(&self) -> Result<String, ConfigError> {
        self.validate()?;
        let mut text = serde_json::to_string_pretty(self).map_err(ConfigError::Encode)?;
        text.push('\n');
        Ok(text)
    }

    /// Parse and validate a bundle from JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::JsonText`] for malformed JSON and
    /// [`ConfigError::Validation`] when the version or contents are invalid.
    pub fn from_json_str(text: &str) -> Result<Self, ConfigError> {
        let bundle: Self = serde_json::from_str(text).map_err(ConfigError::JsonText)?;
        bundle.validate()?;
        Ok(bundle)
    }

    /// Compare the bundled identity with a connected controller.
    #[must_use]
    pub fn check_identity(&self, current: &ControllerInfo) -> IdentityCheck {
        let stored = &self.controller.profile;
        let current_key = ProfileKey::from_controller(current);
        if !stored.is_identified() {
            return IdentityCheck::Unverified(stored.clone());
        }
        if !current_key.is_identified() {
            return IdentityCheck::Unverified(current_key);
        }
        if stored.id() == current_key.id() {
            IdentityCheck::Match
        } else {
            IdentityCheck::Mismatch(IdentityMismatch {
                stored: stored.clone(),
                current: current_key,
            })
        }
    }
}

/// Result of comparing a bundle with the connected controller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityCheck {
    /// Keys match and both carry a board or driver identity.
    Match,
    /// The bundle and the controller produced different stable keys.
    Mismatch(IdentityMismatch),
    /// A key lacks a board or driver, so identity cannot be confirmed.
    Unverified(ProfileKey),
}

impl IdentityCheck {
    /// Returns `true` for [`IdentityCheck::Match`].
    #[must_use]
    pub fn is_match(&self) -> bool {
        matches!(self, Self::Match)
    }

    /// Human-readable warning for a non-match, or `None` for a match.
    ///
    /// This is the explicit warning an import UI should show before applying a
    /// bundle with [`IdentityPolicy::Warn`].
    #[must_use]
    pub fn warning(&self) -> Option<String> {
        match self {
            Self::Match => None,
            Self::Mismatch(mismatch) => Some(format!(
                "Backup is for {}, but the connected controller is {}.",
                mismatch.stored, mismatch.current
            )),
            Self::Unverified(profile) => Some(format!(
                "The profile {profile} has no board or driver identity, \
                 so it cannot be matched to a controller."
            )),
        }
    }
}

impl fmt::Display for IdentityCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Match => f.write_str("match"),
            Self::Mismatch(mismatch) => write!(
                f,
                "mismatch (backup: {}, connected: {})",
                mismatch.stored, mismatch.current
            ),
            Self::Unverified(profile) => write!(f, "unverified ({profile})"),
        }
    }
}

/// Two different controller identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityMismatch {
    /// Identity stored in the bundle.
    pub stored: ProfileKey,
    /// Identity of the connected controller.
    pub current: ProfileKey,
}

/// How an import treats a different or unverifiable identity.
///
/// The default is [`IdentityPolicy::Reject`]. A UI should preview with
/// [`crate::ConfigStore::load_backup`] and
/// [`BackupBundle::check_identity`], show [`IdentityCheck::warning`], and use
/// `Warn` only after the user explicitly confirms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IdentityPolicy {
    /// Return an error instead of applying a bundle.
    #[default]
    Reject,
    /// Apply the bundle and return the warning in [`ImportOutcome`].
    Warn,
}

/// A bundle that was read and applied by an import.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportOutcome {
    /// Bundle that was imported.
    pub bundle: BackupBundle,
    /// Identity comparison. `None` when no connected controller was supplied
    /// and the stored profile is identified; an offline import of an
    /// unidentified profile yields `Some(IdentityCheck::Unverified)`.
    pub identity: Option<IdentityCheck>,
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use ghs_core::{Position, Wcs, parse_controller_info};

    use super::*;
    use crate::controller::{
        HeightMapFrame, HeightMapRecord, MAX_PROBE_OFFSET_MM, Macro, ProbeOffsetRecord, ToolRecord,
    };

    const FLEXIHAL: &[&str] = &[
        "[VER:1.1f.20240604:]",
        "[FIRMWARE:grblHAL]",
        "[DRIVER:STM32F446]",
        "[DRIVER VERSION:240428]",
        "[BOARD:FlexiHAL_20240604_210624]",
    ];

    fn info() -> ControllerInfo {
        parse_controller_info(FLEXIHAL)
    }

    fn height_map() -> HeightMapRecord {
        HeightMapRecord {
            name: "Fixture".into(),
            tool_number: Some(1),
            created_at_unix_s: Some(1_700_000_000),
            origin_x_mm: 0.0,
            origin_y_mm: 0.0,
            step_x_mm: 10.0,
            step_y_mm: 10.0,
            columns: 2,
            rows: 2,
            surface_z_mm: -1.0,
            z_offset_mm: Some(0.05),
            samples: vec![Some(0.0), Some(0.02), None, Some(0.04)],
            frame: HeightMapFrame {
                wcs: Wcs::G54,
                wcs_offset: Position::xyz(1.0, 2.0, 3.0),
                g92: Position::xyz(0.1, 0.2, 0.3),
                tool_length_offset_z_mm: -10.0,
            },
        }
    }

    fn data() -> ControllerData {
        let mut data = ControllerData::new();
        data.settings.insert(20, ghs_core::SettingValue::Bool(true));
        data.offsets
            .set_work_offset(Wcs::G54, Position::xyz(1.0, 2.0, 3.0));
        data.tools.insert(ToolRecord {
            number: 1,
            name: "Flat".into(),
            diameter_mm: 6.0,
            length_mm: 30.0,
            pocket: Some(1),
        });
        data.macros = vec![Macro::new("Safe Z", "G0 Z5")];
        data.height_maps = vec![height_map()];
        data.probe_offset = ProbeOffsetRecord {
            x_mm: 3.5,
            y_mm: -1.25,
        };
        data
    }

    fn bundle() -> BackupBundle {
        BackupBundle::new(AppConfig::default(), &info(), data()).with_created_at(1_700_000_000)
    }

    #[test]
    fn bundle_round_trips() {
        let original = bundle();
        let json = original.to_json_string().expect("encode");
        let decoded = BackupBundle::from_json_str(&json).expect("decode");
        assert_eq!(decoded, original);
        assert_eq!(decoded.data.height_maps, original.data.height_maps);
        assert_eq!(decoded.data.probe_offset, original.data.probe_offset);
        assert!(json.contains("\"probe_offset\""));
        decoded.validate().expect("valid");
    }

    #[test]
    fn bundle_round_trips_the_app_locale() {
        let app = AppConfig {
            locale: "pt-BR".into(),
            ..AppConfig::default()
        };
        let original = BackupBundle::new(app, &info(), data()).with_created_at(1_700_000_000);
        let json = original.to_json_string().expect("encode");
        assert!(json.contains("\"locale\": \"pt-BR\""));
        let decoded = BackupBundle::from_json_str(&json).expect("decode");
        assert_eq!(decoded, original);
        assert_eq!(decoded.app.locale, "pt-BR");
    }

    #[test]
    fn old_bundle_without_locale_uses_english() {
        let app = AppConfig {
            locale: "pt-BR".into(),
            ..AppConfig::default()
        };
        let original = BackupBundle::new(app, &info(), data()).with_created_at(1_700_000_000);
        let json = original.to_json_string().expect("encode");
        let mut value: serde_json::Value = serde_json::from_str(&json).expect("json");
        value["app"]
            .as_object_mut()
            .expect("app object")
            .remove("locale");
        let text = serde_json::to_string(&value).expect("json");
        let decoded = BackupBundle::from_json_str(&text).expect("decode");
        assert_eq!(decoded.app.locale, "en");
    }

    #[test]
    fn bundle_rejects_an_invalid_probe_offset() {
        let mut out_of_range = bundle();
        out_of_range.data.probe_offset = ProbeOffsetRecord {
            x_mm: MAX_PROBE_OFFSET_MM + 1.0,
            y_mm: 0.0,
        };
        assert!(matches!(
            out_of_range.validate(),
            Err(ValidationError::ProbeOffsetOutOfRange { axis: "x_mm", .. })
        ));
        assert!(matches!(
            out_of_range.to_json_string(),
            Err(ConfigError::Validation(
                ValidationError::ProbeOffsetOutOfRange { .. }
            ))
        ));

        let mut non_finite = bundle();
        non_finite.data.probe_offset = ProbeOffsetRecord {
            x_mm: 0.0,
            y_mm: f64::NAN,
        };
        assert!(matches!(
            non_finite.validate(),
            Err(ValidationError::NonFiniteProbeOffset { axis: "y_mm", .. })
        ));
        assert!(matches!(
            non_finite.to_json_string(),
            Err(ConfigError::Validation(
                ValidationError::NonFiniteProbeOffset { .. }
            ))
        ));
    }

    #[test]
    fn bundle_preserves_complete_and_partial_height_maps() {
        let mut complete = height_map();
        complete.name = "Complete".into();
        complete.samples = vec![Some(0.0), Some(0.1), Some(0.2), Some(0.3)];
        complete.z_offset_mm = None;
        complete.tool_number = None;
        complete.created_at_unix_s = None;

        let partial = height_map();
        assert!(partial.samples.contains(&None));

        let mut original = bundle();
        original.data.height_maps = vec![complete, partial];
        let json = original.to_json_string().expect("encode");
        let decoded = BackupBundle::from_json_str(&json).expect("decode");
        assert_eq!(decoded, original);
        assert_eq!(decoded.data.height_maps, original.data.height_maps);
        assert_eq!(
            decoded.data.height_maps[0]
                .samples
                .iter()
                .filter(|s| s.is_none())
                .count(),
            0
        );
        assert_eq!(
            decoded.data.height_maps[1]
                .samples
                .iter()
                .filter(|s| s.is_none())
                .count(),
            1
        );
    }

    #[test]
    fn bundle_rejects_a_malformed_height_map() {
        let mut invalid_count = bundle();
        invalid_count.data.height_maps[0].samples.pop();
        assert_eq!(
            invalid_count.validate(),
            Err(ValidationError::HeightMapSampleCount {
                name: "Fixture".into(),
                expected: 4,
                actual: 3,
            })
        );
        assert!(matches!(
            invalid_count.to_json_string(),
            Err(ConfigError::Validation(
                ValidationError::HeightMapSampleCount { .. }
            ))
        ));

        let mut invalid_frame = bundle();
        invalid_frame.data.height_maps[0].frame.wcs_offset = Position::xyz(0.0, 0.0, f64::NAN);
        assert!(matches!(
            invalid_frame.validate(),
            Err(ValidationError::InvalidHeightMap {
                field: "frame.wcs_offset",
                ..
            })
        ));
    }

    #[test]
    fn unknown_bundle_version_is_rejected() {
        let mut value: serde_json::Value =
            serde_json::from_str(&bundle().to_json_string().expect("encode")).expect("json");
        value["schema_version"] = serde_json::json!(99);
        let text = serde_json::to_string(&value).expect("json");
        let error = BackupBundle::from_json_str(&text).expect_err("rejected");
        assert!(matches!(
            error,
            ConfigError::Validation(ValidationError::UnsupportedVersion {
                what: "backup bundle",
                found: 99,
                supported: BACKUP_SCHEMA_VERSION,
            })
        ));
    }

    #[test]
    fn unknown_app_or_profile_version_is_rejected() {
        let mut value: serde_json::Value =
            serde_json::from_str(&bundle().to_json_string().expect("encode")).expect("json");
        value["app"]["schema_version"] = serde_json::json!(42);
        let text = serde_json::to_string(&value).expect("json");
        assert!(matches!(
            BackupBundle::from_json_str(&text),
            Err(ConfigError::Validation(
                ValidationError::UnsupportedVersion {
                    what: "app config",
                    ..
                }
            ))
        ));

        let mut value: serde_json::Value =
            serde_json::from_str(&bundle().to_json_string().expect("encode")).expect("json");
        value["data"]["schema_version"] = serde_json::json!(42);
        let text = serde_json::to_string(&value).expect("json");
        assert!(matches!(
            BackupBundle::from_json_str(&text),
            Err(ConfigError::Validation(
                ValidationError::UnsupportedVersion {
                    what: "controller profile",
                    ..
                }
            ))
        ));
    }

    #[test]
    fn malformed_json_is_rejected() {
        assert!(matches!(
            BackupBundle::from_json_str("{not json"),
            Err(ConfigError::JsonText(_))
        ));
    }

    #[test]
    fn tampered_profile_key_is_rejected() {
        let mut value: serde_json::Value =
            serde_json::from_str(&bundle().to_json_string().expect("encode")).expect("json");
        value["controller"]["profile"]["id"] = serde_json::json!("other-board-0000000000000000");
        let text = serde_json::to_string(&value).expect("json");
        assert!(matches!(
            BackupBundle::from_json_str(&text),
            Err(ConfigError::Validation(
                ValidationError::ProfileIdentityMismatch { .. }
            ))
        ));
    }

    #[test]
    fn identity_check_matches_and_detects_differences() {
        let bundle = bundle();
        assert_eq!(bundle.check_identity(&info()), IdentityCheck::Match);
        assert!(bundle.check_identity(&info()).warning().is_none());

        let mut other = info();
        other.board = Some("OtherBoard".into());
        let check = bundle.check_identity(&other);
        assert!(matches!(check, IdentityCheck::Mismatch(_)));
        assert!(check.warning().is_some());
        assert!(!check.is_match());
    }

    #[test]
    fn identity_check_marks_unidentified_controllers() {
        let mut unidentified = ControllerInfo::new();
        unidentified.version = "1.1f".into();
        let bundle = BackupBundle::new(AppConfig::default(), &unidentified, data());
        let check = bundle.check_identity(&unidentified);
        assert!(matches!(check, IdentityCheck::Unverified(_)));
        assert!(check.warning().is_some());
    }
}
