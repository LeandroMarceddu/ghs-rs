//! Startup and selector wiring for the UI locale.
//!
//! [`ghs_i18n`] owns the catalogs and the process-wide locale. This module is
//! the app-side adapter: it normalizes a stored [`ghs_config::AppConfig`] tag,
//! applies the startup locale before the first frame, and persists a selector
//! choice.
//!
//! Persisted tags may be full BCP-47-style values such as `pt-BR`; only the
//! primary subtag is considered here and unknown locales display English
//! without rewriting the stored tag.

use std::path::PathBuf;

use ghs_config::AppConfig;
use ghs_i18n::Locale;

use crate::config::ConfigModel;

/// Resolve a persisted locale tag to a supported catalog.
///
/// Matching is case-insensitive and only the primary subtag is considered, so
/// `en`, `EN`, and `en-US` all select English. A tag without a supported
/// catalog falls back to English; the caller still keeps the original string
/// in [`AppConfig::locale`].
#[must_use]
pub fn resolve_locale(tag: &str) -> Locale {
    let trimmed = tag.trim();
    let primary = trimmed.split(['-', '_']).next().unwrap_or(trimmed);
    let lower = primary.to_ascii_lowercase();
    Locale::from_code(&lower).unwrap_or(Locale::En)
}

/// Resolve the locale stored in app preferences without touching the global
/// catalog. Used by startup and by tests that need a deterministic result.
#[must_use]
pub fn startup_locale(config: &AppConfig) -> Locale {
    resolve_locale(&config.locale)
}

/// Apply the persisted startup locale to the process-wide catalog.
///
/// `Shell::new` calls this before building the first frame. The returned value
/// is the actually active locale, which is English when the stored tag names a
/// language without a catalog.
pub fn apply_startup_locale(config: &AppConfig) -> Locale {
    let locale = startup_locale(config);
    ghs_i18n::set_locale(locale);
    locale
}

/// Apply a language-selector choice and persist it.
///
/// The global locale changes even when persistence fails, so the current
/// window stays consistent; the returned error is reported to the operator.
///
/// # Errors
///
/// Returns the [`ConfigModel::set_locale`] error when the app config cannot be
/// written.
pub fn choose_locale(config: &mut ConfigModel, locale: Locale) -> Result<PathBuf, String> {
    ghs_i18n::set_locale(locale);
    config.set_locale(locale)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, MutexGuard};
    use std::time::{SystemTime, UNIX_EPOCH};

    use ghs_config::{AppConfig, ConfigStore};

    use super::*;
    use crate::setup::{SafetyItem, SetupStep};

    /// Tests that change the process-wide locale must not interleave.
    static LOCALE_LOCK: Mutex<()> = Mutex::new(());

    fn locale_lock() -> MutexGuard<'static, ()> {
        LOCALE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Restore English when a test finishes, including on panic.
    struct ResetLocale;

    impl Drop for ResetLocale {
        fn drop(&mut self) {
            ghs_i18n::set_locale(Locale::En);
        }
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!(
                "ghs-app-i18n-{label}-{}-{nanos}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn resolve_locale_accepts_supported_codes_in_any_case() {
        assert_eq!(resolve_locale("en"), Locale::En);
        assert_eq!(resolve_locale("EN"), Locale::En);
        assert_eq!(resolve_locale("It"), Locale::It);
        assert_eq!(resolve_locale("DE"), Locale::De);
        assert_eq!(resolve_locale("nl"), Locale::Nl);
        assert_eq!(resolve_locale("  it  "), Locale::It);
    }

    #[test]
    fn resolve_locale_uses_the_primary_subtag() {
        assert_eq!(resolve_locale("en-US"), Locale::En);
        assert_eq!(resolve_locale("de_DE"), Locale::De);
        assert_eq!(resolve_locale("it-IT"), Locale::It);
        assert_eq!(resolve_locale("nl-BE"), Locale::Nl);
    }

    #[test]
    fn resolve_locale_falls_back_to_english() {
        assert_eq!(resolve_locale(""), Locale::En);
        assert_eq!(resolve_locale("fr"), Locale::En);
        assert_eq!(resolve_locale("zz-ZZ"), Locale::En);
        assert_eq!(resolve_locale("english"), Locale::En);
    }

    #[test]
    fn startup_locale_reads_the_persisted_tag() {
        let mut config = AppConfig::default();
        assert_eq!(startup_locale(&config), Locale::En);

        config.locale = "it".into();
        assert_eq!(startup_locale(&config), Locale::It);

        config.locale = "NL".into();
        assert_eq!(startup_locale(&config), Locale::Nl);

        config.locale = "pt-BR".into();
        assert_eq!(startup_locale(&config), Locale::En);
    }

    #[test]
    fn apply_startup_locale_sets_the_global_catalog() {
        let _guard = locale_lock();
        let _reset = ResetLocale;

        let config = AppConfig {
            locale: "it".into(),
            ..AppConfig::default()
        };
        assert_eq!(apply_startup_locale(&config), Locale::It);
        assert_eq!(ghs_i18n::locale(), Locale::It);
        assert_eq!(ghs_i18n::t("Job"), "Lavoro");
        assert_eq!(ghs_i18n::t("SETTINGS"), "IMPOSTAZIONI");
    }

    #[test]
    fn unknown_locale_displays_english_but_keeps_the_stored_tag() {
        let _guard = locale_lock();
        let _reset = ResetLocale;

        let config = AppConfig {
            locale: "fr".into(),
            ..AppConfig::default()
        };
        assert_eq!(apply_startup_locale(&config), Locale::En);
        assert_eq!(ghs_i18n::locale(), Locale::En);
        assert_eq!(ghs_i18n::t("Job"), "Job");
        assert_eq!(config.locale, "fr");
    }

    #[test]
    fn config_model_set_locale_persists_the_tag() {
        let dir = TempDir::new("persist");
        let mut model = ConfigModel::from_store(ConfigStore::new(&dir.path));
        let path = model.set_locale(Locale::De).expect("locale save succeeds");
        assert!(path.exists());
        assert_eq!(model.app().locale, "de");

        let reopened = ConfigModel::from_store(ConfigStore::new(&dir.path));
        assert_eq!(reopened.app().locale, "de");
        assert_eq!(startup_locale(reopened.app()), Locale::De);
    }

    #[test]
    fn language_selector_changes_global_locale_and_saves() {
        let _guard = locale_lock();
        let _reset = ResetLocale;

        let dir = TempDir::new("select");
        let mut model = ConfigModel::from_store(ConfigStore::new(&dir.path));
        choose_locale(&mut model, Locale::Nl).expect("selector saves");
        assert_eq!(ghs_i18n::locale(), Locale::Nl);
        assert_eq!(ghs_i18n::t("Job"), "Taak");

        let reopened = ConfigModel::from_store(ConfigStore::new(&dir.path));
        assert_eq!(reopened.app().locale, "nl");
    }

    #[test]
    fn selector_labels_are_native_and_catalog_labels_match() {
        assert_eq!(
            Locale::ALL.map(Locale::label),
            ["English", "Italiano", "Deutsch", "Nederlands"]
        );
        assert_eq!(ghs_i18n::t_in(Locale::It, "Job"), "Lavoro");
        assert_eq!(ghs_i18n::t_in(Locale::De, "SETTINGS"), "EINSTELLUNGEN");
        assert_eq!(ghs_i18n::t_in(Locale::Nl, "START"), "START");
        assert_eq!(
            ghs_i18n::t_in(Locale::It, "CONNECT"),
            "CONNETTI",
            "primary controls use the active catalog"
        );
    }

    #[test]
    fn check_mode_labels_translate_in_every_catalog() {
        assert_eq!(
            ghs_i18n::t_in(Locale::It, "CHECK COMPLETE"),
            "CONTROLLO COMPLETATO"
        );
        assert_eq!(
            ghs_i18n::t_in(Locale::De, "CHECK COMPLETE"),
            "PRÜFLAUF ABGESCHLOSSEN"
        );
        assert_eq!(
            ghs_i18n::t_in(Locale::Nl, "CHECK COMPLETE"),
            "CONTROLE VOLTOOID"
        );
        assert_eq!(ghs_i18n::t_in(Locale::It, "CHECK"), "CONTROLLO");
        assert_eq!(ghs_i18n::t_in(Locale::De, "CHECK"), "PRÜFEN");
        assert_eq!(ghs_i18n::t_in(Locale::Nl, "CHECK"), "CONTROLE");
        assert_eq!(ghs_i18n::t_in(Locale::It, "START CHECK"), "AVVIA CONTROLLO");
        assert_eq!(
            ghs_i18n::t_in(Locale::De, "START CHECK"),
            "PRÜFLAUF STARTEN"
        );
        assert_eq!(
            ghs_i18n::t_in(Locale::Nl, "DRY RUN - NO MOTION"),
            "DROOGLOOP - GEEN BEWEGING"
        );
        assert_eq!(
            ghs_i18n::t_in(Locale::It, "CHECK STOPPED"),
            "CONTROLLO INTERROTTO"
        );
        assert_eq!(
            ghs_i18n::t_in(Locale::De, "CHECK PROGRAM (DRY RUN)"),
            "PROGRAMM PRÜFEN (TROCKENLAUF)"
        );
        assert_eq!(
            ghs_i18n::t_in(Locale::Nl, "WHAT THE DRY RUN DOES"),
            "WAT DE DROOGLOOP DOET"
        );
    }

    #[test]
    fn language_card_labels_are_translated_but_selector_stays_native() {
        assert_eq!(ghs_i18n::t_in(Locale::It, "LANGUAGE"), "LINGUA");
        assert_eq!(ghs_i18n::t_in(Locale::De, "LANGUAGE"), "SPRACHE");
        assert_eq!(ghs_i18n::t_in(Locale::Nl, "LANGUAGE"), "TAAL");
        // A language is always offered in its own language, never translated
        // by whichever catalog happens to be active.
        for active in Locale::ALL {
            assert_eq!(
                Locale::ALL.map(|locale| ghs_i18n::t_in(active, locale.label())),
                ["English", "Italiano", "Deutsch", "Nederlands"]
            );
        }
    }

    #[test]
    fn file_browser_and_run_options_labels_use_the_catalog() {
        assert_eq!(ghs_i18n::t_in(Locale::It, "FOLDERS"), "CARTELLE");
        assert_eq!(ghs_i18n::t_in(Locale::De, "FOLDERS"), "ORDNER");
        assert_eq!(ghs_i18n::t_in(Locale::Nl, "FOLDERS"), "MAPPEN");
        assert_eq!(
            ghs_i18n::t_in(Locale::Nl, "READY TO LOAD"),
            "KLAAR OM TE LADEN"
        );
        assert_eq!(
            ghs_i18n::t_in(Locale::It, "LIVE / READY"),
            "ATTIVO / PRONTO"
        );
        assert_eq!(
            ghs_i18n::t_in(Locale::Nl, "OPTIONAL STOP"),
            "OPTIONELE STOP"
        );
        assert_eq!(ghs_i18n::t_in(Locale::It, "BLOCK DELETE"), "ELIMINA BLOCCO");
        assert_eq!(ghs_i18n::t_in(Locale::De, "SINGLE BLOCK"), "EINZELSATZ");
        assert_eq!(
            ghs_i18n::t_in(
                Locale::De,
                "BLOCK DELETE IS LOCKED WHILE A JOB IS RUNNING OR PAUSED"
            ),
            "SATZ LÖSCHEN IST GESPERRT, WÄHREND EIN AUFTRAG LÄUFT ODER PAUSIERT"
        );
    }

    #[test]
    fn setup_checklist_titles_and_details_translate() {
        let _guard = locale_lock();
        let _reset = ResetLocale;

        ghs_i18n::set_locale(Locale::It);
        assert_eq!(ghs_i18n::t(SetupStep::Review.title()), "RIEPILOGO");
        assert_eq!(
            ghs_i18n::t(SafetyItem::EStop.title()),
            "E-STOP FISICO INDIVIDUATO E COLLAUDATO"
        );
        assert_eq!(
            ghs_i18n::t(SafetyItem::Workholding.detail()),
            "Fissa il pezzo, libera l'utensile e tieni le mani lontane da tutti gli assi."
        );

        ghs_i18n::set_locale(Locale::De);
        assert_eq!(ghs_i18n::t(SetupStep::Safety.title()), "SICHERHEIT");
        assert_eq!(
            ghs_i18n::t(SafetyItem::TravelLimits.title()),
            "MASCHINENVERFAHRWEG UND SOFT-LIMITS GEPRÜFT"
        );
        assert_eq!(
            ghs_i18n::t(SafetyItem::SimulatorFirst.detail()),
            "Lerne den Sender im Simulator, verbinde dann mit ausgeschalteter Spindel und führe nur Status, DRO und kleine Jogs aus."
        );

        ghs_i18n::set_locale(Locale::Nl);
        assert_eq!(ghs_i18n::t(SetupStep::Units.title()), "EENHEDEN");
        assert_eq!(
            ghs_i18n::t(SafetyItem::SpindleStop.title()),
            "SPINDEL KAN ONAFHANKELIJK WORDEN GESTOPT"
        );
    }

    #[test]
    fn new_feature_labels_translate_in_every_catalog() {
        let _guard = locale_lock();
        let _reset = ResetLocale;

        ghs_i18n::set_locale(Locale::It);
        assert_eq!(ghs_i18n::t("JOB HISTORY"), "CRONOLOGIA LAVORI");
        assert_eq!(ghs_i18n::t("CONTROLLER RESET"), "RESET CONTROLLER");
        assert_eq!(ghs_i18n::t("PROBE MACROS"), "MACRO SONDA");
        assert_eq!(ghs_i18n::t("HIGH CONTRAST"), "ALTO CONTRASTO");
        assert_eq!(ghs_i18n::t("GANTRY SQUARING"), "SQUADRATURA PORTALE");

        ghs_i18n::set_locale(Locale::De);
        assert_eq!(ghs_i18n::t("JOB HISTORY"), "AUFTRAGSVERLAUF");
        assert_eq!(ghs_i18n::t("CONTROLLER RESET"), "CONTROLLER-RESET");
        assert_eq!(ghs_i18n::t("PROBE MACROS"), "TASTER-MAKROS");
        assert_eq!(ghs_i18n::t("HIGH CONTRAST"), "HOHER KONTRAST");
        assert_eq!(ghs_i18n::t("GANTRY SQUARING"), "PORTALAUSRICHTUNG");

        ghs_i18n::set_locale(Locale::Nl);
        assert_eq!(ghs_i18n::t("JOB HISTORY"), "TAAKGESCHIEDENIS");
        assert_eq!(ghs_i18n::t("CONTROLLER RESET"), "CONTROLLER RESETTEN");
        assert_eq!(ghs_i18n::t("PROBE MACROS"), "TASTERMACRO'S");
        assert_eq!(ghs_i18n::t("HIGH CONTRAST"), "HOOG CONTRAST");
        assert_eq!(ghs_i18n::t("GANTRY SQUARING"), "PORTAAL UITLIJNEN");
    }

    #[test]
    fn catalog_keys_are_non_empty_unique_and_bounded() {
        let english = ghs_i18n::catalog_entries(Locale::En);
        let english_keys: HashSet<&str> = english.iter().map(|(key, _)| *key).collect();
        assert!(english.len() > 500, "the second pass adds the UI labels");

        for locale in Locale::ALL {
            let entries = ghs_i18n::catalog_entries(locale);
            let mut seen = HashSet::new();
            for (key, translation) in entries {
                assert!(!key.is_empty(), "{locale:?} has an empty key");
                assert!(
                    !translation.is_empty(),
                    "{locale:?} has an empty translation for {key:?}"
                );
                assert!(
                    seen.insert(*key),
                    "{locale:?} contains duplicate key {key:?}"
                );
                assert!(
                    key.len() <= ghs_i18n::MAX_KEY_BYTES,
                    "{locale:?} key {key:?} exceeds the byte bound"
                );
                assert!(
                    translation.len() <= ghs_i18n::MAX_TRANSLATION_BYTES,
                    "{locale:?} translation for {key:?} exceeds the byte bound"
                );
            }
            let keys: HashSet<&str> = entries.iter().map(|(key, _)| *key).collect();
            if locale != Locale::En {
                assert_eq!(
                    keys, english_keys,
                    "{locale:?} must cover every English key exactly once"
                );
            }
        }
    }
}
