//! UI localization for ghs-rs.
//!
//! This crate owns locale identifiers, the string catalogs, and the
//! process-wide active locale. It has no GPUI, engine, or I/O dependencies.
//!
//! # Catalog model
//!
//! The key passed to [`t`] is the exact English UI literal, for example
//! `"LOAD PROGRAM"`. Lookup is case-sensitive, so `"LOAD PROGRAM"` and
//! `"load program"` are different keys. A missing key is returned unchanged:
//! the UI keeps its English literal instead of showing an empty label or
//! panicking.
//!
//! Catalog sources are UTF-8. Translations need accented characters (Italian
//! `à`, German `ü`, Dutch `ë`), so the usual ASCII-only source rule has an
//! explicit exception for the catalog modules.
//!
//! # Active locale
//!
//! [`set_locale`] and [`locale`] use a process-wide [`AtomicU8`]. Relaxed
//! ordering is sufficient: the locale is a display hint and carries no data
//! that must be published alongside it. Changing it is cheap and lock-free.

#![forbid(unsafe_code)]

mod catalog;

use std::sync::atomic::{AtomicU8, Ordering};

/// Maximum accepted length of an English catalog key, in UTF-8 bytes.
///
/// Catalog maintenance tests reject longer keys so a full label cannot turn
/// into an unbounded layout surprise. The bound is generous enough for the
/// short explanatory paragraphs used by confirmation sheets and card help
/// text, while still rejecting accidentally pasted documents.
pub const MAX_KEY_BYTES: usize = 320;

/// Maximum accepted length of one translation, in UTF-8 bytes.
///
/// Translated paragraphs are usually longer than the English source, so this
/// bound is wider than [`MAX_KEY_BYTES`].
pub const MAX_TRANSLATION_BYTES: usize = 400;

/// Supported UI locale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Locale {
    /// English.
    En = 0,
    /// Italian.
    It = 1,
    /// German.
    De = 2,
    /// Dutch.
    Nl = 3,
}

impl Locale {
    /// Every supported locale, in display order.
    pub const ALL: [Self; 4] = [Self::En, Self::It, Self::De, Self::Nl];

    /// Stable lowercase language code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::It => "it",
            Self::De => "de",
            Self::Nl => "nl",
        }
    }

    /// Locale name in its own language.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::It => "Italiano",
            Self::De => "Deutsch",
            Self::Nl => "Nederlands",
        }
    }

    /// Parse an exact lowercase language code.
    ///
    /// Only `"en"`, `"it"`, `"de"`, and `"nl"` are accepted. Registry-style
    /// values such as `"en-US"` are not parsed here; a caller that stores a
    /// full tag must take its primary language subtag first.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "en" => Some(Self::En),
            "it" => Some(Self::It),
            "de" => Some(Self::De),
            "nl" => Some(Self::Nl),
            _ => None,
        }
    }

    /// Decode a value stored in [`ACTIVE_LOCALE`].
    ///
    /// An unknown byte is treated as English; the atomic can only hold values
    /// written by [`set_locale`], so this is a defensive fallback.
    const fn from_stored(value: u8) -> Self {
        match value {
            1 => Self::It,
            2 => Self::De,
            3 => Self::Nl,
            _ => Self::En,
        }
    }
}

/// Process-wide active locale. Defaults to English.
static ACTIVE_LOCALE: AtomicU8 = AtomicU8::new(Locale::En as u8);

/// Select the process-wide UI locale.
pub fn set_locale(locale: Locale) {
    ACTIVE_LOCALE.store(locale as u8, Ordering::Relaxed);
}

/// Return the process-wide UI locale.
#[must_use]
pub fn locale() -> Locale {
    Locale::from_stored(ACTIVE_LOCALE.load(Ordering::Relaxed))
}

/// Translate `key` using the process-wide active locale.
///
/// `key` is the exact English UI literal. When the active catalog has no
/// entry, the key itself is returned, so the UI falls back to English.
#[must_use]
pub fn t(key: &'static str) -> &'static str {
    t_in(locale(), key)
}

/// Translate `key` using `locale` without touching the active locale.
///
/// The lookup is case-sensitive. English returns `key`; a missing key in any
/// locale also returns `key`.
#[must_use]
pub fn t_in(locale: Locale, key: &'static str) -> &'static str {
    catalog::lookup(locale, key).unwrap_or(key)
}

/// Read-only catalog slice for `locale`.
///
/// The slice is the crate's immutable table of `(english_key, translation)`
/// pairs. It exists for diagnostics and for app-level catalog tests; callers
/// must not assume any ordering.
#[must_use]
pub fn catalog_entries(locale: Locale) -> &'static [(&'static str, &'static str)] {
    catalog::entries(locale)
}

/// Translate `key` using the active locale, falling back to `fallback`.
///
/// This is the explicit-fallback variant of [`t`]. It is useful for a label
/// whose English fallback is not the key itself. English keys are always
/// considered present, so the English catalog returns `key` rather than
/// `fallback`.
#[must_use]
pub fn t_or(key: &'static str, fallback: &'static str) -> &'static str {
    catalog::lookup(locale(), key).unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::{Mutex, MutexGuard};

    use super::*;

    /// Catalog tests do not touch the active locale, but the two tests that do
    /// must not interleave with each other.
    static LOCALE_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn locale_test_lock() -> MutexGuard<'static, ()> {
        LOCALE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Reset the process-wide locale when a test finishes, including on panic.
    struct ResetLocale;

    impl Drop for ResetLocale {
        fn drop(&mut self) {
            set_locale(Locale::En);
        }
    }

    #[test]
    fn locale_metadata_round_trips() {
        assert_eq!(Locale::ALL.len(), 4);
        for locale in Locale::ALL {
            assert_eq!(Locale::from_code(locale.code()), Some(locale));
            assert!(!locale.label().is_empty());
        }

        assert_eq!(Locale::En.code(), "en");
        assert_eq!(Locale::It.code(), "it");
        assert_eq!(Locale::De.code(), "de");
        assert_eq!(Locale::Nl.code(), "nl");
        assert_eq!(Locale::En.label(), "English");
        assert_eq!(Locale::It.label(), "Italiano");
        assert_eq!(Locale::De.label(), "Deutsch");
        assert_eq!(Locale::Nl.label(), "Nederlands");

        assert_eq!(Locale::from_code("fr"), None);
        assert_eq!(Locale::from_code(""), None);
        assert_eq!(Locale::from_code("EN"), None);
        assert_eq!(Locale::from_code("en-US"), None);
    }

    #[test]
    fn english_catalog_returns_the_key() {
        for (key, translation) in catalog::entries(Locale::En) {
            assert_eq!(key, translation);
            assert_eq!(t_in(Locale::En, key), *key);
        }
    }

    #[test]
    fn translated_catalogs_have_the_same_keys_as_english() {
        let english: HashSet<&str> = catalog::entries(Locale::En)
            .iter()
            .map(|(key, _)| *key)
            .collect();
        for locale in [Locale::It, Locale::De, Locale::Nl] {
            let translated: HashSet<&str> = catalog::entries(locale)
                .iter()
                .map(|(key, _)| *key)
                .collect();
            assert_eq!(
                translated, english,
                "{locale:?} must cover every English key exactly once"
            );
        }
    }

    #[test]
    fn catalogs_have_unique_bounded_and_trimmed_entries() {
        for locale in Locale::ALL {
            let entries = catalog::entries(locale);
            assert!(!entries.is_empty(), "{locale:?} catalog is empty");

            let mut seen = HashSet::new();
            for (key, translation) in entries {
                assert!(!key.is_empty(), "{locale:?} has an empty key");
                assert!(
                    !translation.is_empty(),
                    "{locale:?} has an empty translation for {key:?}"
                );
                assert_eq!(*key, key.trim(), "{locale:?} key has outer whitespace");
                assert_eq!(
                    *translation,
                    translation.trim(),
                    "{locale:?} translation for {key:?} has outer whitespace"
                );
                assert!(
                    key.len() <= MAX_KEY_BYTES,
                    "{locale:?} key {key:?} exceeds {MAX_KEY_BYTES} bytes"
                );
                assert!(
                    translation.len() <= MAX_TRANSLATION_BYTES,
                    "{locale:?} translation for {key:?} exceeds {MAX_TRANSLATION_BYTES} bytes"
                );
                assert!(
                    !key.chars().any(char::is_control),
                    "{locale:?} key {key:?} contains a control character"
                );
                assert!(
                    !translation.chars().any(char::is_control),
                    "{locale:?} translation for {key:?} contains a control character"
                );
                assert!(
                    seen.insert(*key),
                    "{locale:?} contains duplicate key {key:?}"
                );
            }
        }
    }

    #[test]
    fn italian_german_and_dutch_sample_translations() {
        let samples = [
            ("Job", "Lavoro", "Auftrag", "Taak"),
            ("Probe", "Sonda", "Antasten", "Taster"),
            (
                "LOAD PROGRAM",
                "CARICA PROGRAMMA",
                "PROGRAMM LADEN",
                "PROGRAMMA LADEN",
            ),
            ("START", "AVVIA", "START", "START"),
            ("PAUSE", "PAUSA", "PAUSE", "PAUZE"),
            ("RESUME", "RIPRENDI", "FORTSETZEN", "HERVATTEN"),
            ("STOP", "ARRESTO", "STOPP", "STOP"),
            ("CANCEL", "ANNULLA", "ABBRECHEN", "ANNULEREN"),
            ("SAVE", "SALVA", "SPEICHERN", "OPSLAAN"),
            (
                "WORK OFFSETS",
                "ORIGINI PEZZO",
                "WERKSTÜCKNULLPUNKTE",
                "WERKSTUKNULPUNTEN",
            ),
            ("SETTINGS", "IMPOSTAZIONI", "EINSTELLUNGEN", "INSTELLINGEN"),
            (
                "Z touch plate",
                "Piastra di contatto Z",
                "Z-Tastplatte",
                "Z-tastplaat",
            ),
            ("Corner", "Angolo", "Ecke", "Hoek"),
            ("Delete", "Elimina", "Löschen", "Verwijderen"),
        ];

        for (key, it, de, nl) in samples {
            assert_eq!(t_in(Locale::It, key), it, "Italian {key:?}");
            assert_eq!(t_in(Locale::De, key), de, "German {key:?}");
            assert_eq!(t_in(Locale::Nl, key), nl, "Dutch {key:?}");
        }
    }

    #[test]
    fn missing_keys_fall_back_to_the_english_literal() {
        let missing = "this key is not in any catalog";
        for locale in Locale::ALL {
            assert_eq!(t_in(locale, missing), missing);
        }
    }

    #[test]
    fn catalog_lookup_is_case_sensitive() {
        assert_eq!(t_in(Locale::It, "Apply"), "Applica");
        assert_eq!(t_in(Locale::It, "apply"), "apply");
        assert_eq!(t_in(Locale::Nl, "Job"), "Taak");
        assert_eq!(t_in(Locale::Nl, "job"), "job");
        assert_eq!(t_in(Locale::De, "START"), "START");
        assert_eq!(t_in(Locale::De, "Start"), "Start");
    }

    #[test]
    fn active_locale_is_process_wide() {
        let _guard = locale_test_lock();
        let _reset = ResetLocale;

        set_locale(Locale::En);
        assert_eq!(locale(), Locale::En);
        assert_eq!(t("Job"), "Job");

        set_locale(Locale::It);
        assert_eq!(locale(), Locale::It);
        assert_eq!(t("Job"), "Lavoro");

        set_locale(Locale::De);
        assert_eq!(locale(), Locale::De);
        assert_eq!(t("Job"), "Auftrag");

        set_locale(Locale::Nl);
        assert_eq!(locale(), Locale::Nl);
        assert_eq!(t("Job"), "Taak");
    }

    #[test]
    fn t_or_uses_the_explicit_fallback_only_when_missing() {
        let _guard = locale_test_lock();
        let _reset = ResetLocale;

        set_locale(Locale::Nl);
        assert_eq!(t_or("Job", "Fallback"), "Taak");
        assert_eq!(t_or("missing key", "Fallback"), "Fallback");

        set_locale(Locale::En);
        assert_eq!(t_or("Job", "Fallback"), "Job");
    }
}
