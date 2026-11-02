//! Application-wide preferences: theme, units, locale, last profile, and layout.

use ghs_core::Units;
use serde::{Deserialize, Serialize};

use crate::error::ValidationError;
use crate::profile::ProfileKey;

/// Schema version written by this build.
pub const APP_SCHEMA_VERSION: u32 = 1;

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
    /// Window geometry and panel sizes.
    #[serde(default)]
    pub window: WindowLayout,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: APP_SCHEMA_VERSION,
            theme: ThemeMode::default(),
            units: Units::default(),
            locale: default_locale(),
            last_profile: None,
            window: WindowLayout::default(),
        }
    }
}

impl AppConfig {
    /// Validate the schema version, locale tag, and layout bounds.
    ///
    /// # Errors
    ///
    /// Returns a [`ValidationError`] when the record was written by another
    /// schema version, the locale tag is malformed, or the window size is
    /// impossible.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.schema_version != APP_SCHEMA_VERSION {
            return Err(ValidationError::UnsupportedVersion {
                what: "app config",
                found: self.schema_version,
                supported: APP_SCHEMA_VERSION,
            });
        }
        validate_locale(&self.locale)?;
        self.window.validate()
    }
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

    #[test]
    fn defaults_are_stable() {
        let config = AppConfig::default();
        assert_eq!(config.schema_version, APP_SCHEMA_VERSION);
        assert_eq!(config.theme, ThemeMode::System);
        assert_eq!(config.units, Units::Mm);
        assert_eq!(config.locale, "en");
        assert_eq!(config.last_profile, None);
        assert_eq!(config.window.width, 1280);
        assert_eq!(config.window.height, 800);
        assert_eq!(config.window.left_panel_width, 360);
        assert_eq!(config.window.bottom_panel_height, 240);
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
}
