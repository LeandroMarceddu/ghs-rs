//! Locale string catalogs.
//!
//! Each locale is one immutable table of `(english_key, translation)` pairs.
//! The tables are small enough that a linear scan is cheaper than building a
//! runtime map, and this module has no mutable global state.
//!
//! Catalog sources are UTF-8 because Italian, German, and Dutch translations
//! need non-ASCII letters. Keys remain exact, case-sensitive UI literals.

pub(crate) mod de;
pub(crate) mod en;
pub(crate) mod it;
pub(crate) mod nl;

use crate::Locale;

/// Immutable catalog for `locale`.
pub(crate) fn entries(locale: Locale) -> &'static [(&'static str, &'static str)] {
    match locale {
        Locale::En => en::CATALOG,
        Locale::It => it::CATALOG,
        Locale::De => de::CATALOG,
        Locale::Nl => nl::CATALOG,
    }
}

/// Look up `key` in `locale`, case-sensitively.
///
/// A missing key is `None`; the caller decides which English literal to use.
pub(crate) fn lookup(locale: Locale, key: &str) -> Option<&'static str> {
    entries(locale)
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, translation)| *translation)
}
