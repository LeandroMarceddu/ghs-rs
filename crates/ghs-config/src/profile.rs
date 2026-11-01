//! Stable, filesystem-safe controller profile keys.
//!
//! A key is derived only from controller identity, never from the serial port
//! or connection order, so reconnecting the same board selects the same stored
//! profile. The id is a lower-case ASCII slug plus a 64-bit FNV-1a digest of
//! the identity fields; the display name is a human-readable label. The digest
//! keeps ids short and distinct, but it is not a security boundary.
//!
//! # Identity policy
//!
//! Every identity field is length-prefixed before hashing, so moving a
//! character between `board` and `driver` cannot produce the same key by
//! accident. A missing or blank field is hashed differently from a present
//! value. A board or driver name is required for
//! [`ProfileKey::is_identified`] to return `true`: firmware and version alone
//! cannot distinguish two boards. The app must ask the user before reusing a
//! profile whose key is not verified.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use ghs_core::ControllerInfo;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ValidationError;

/// Maximum length of a storage id, including the hash suffix.
const MAX_ID_LEN: usize = 96;
/// Maximum length of a human-readable profile name.
const MAX_NAME_LEN: usize = 128;
/// Maximum length of the readable slug before the hash.
const MAX_SLUG_LEN: usize = 40;
/// Prefix that marks a key derived without a board or driver identity.
const UNIDENTIFIED_PREFIX: &str = "unidentified-";
/// File-name stems reserved by Windows, rejected even though generated ids
/// always include a hash suffix.
const WINDOWS_RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];
/// FNV-1a 64-bit offset basis.
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// A stable profile identifier plus a human-readable name.
///
/// The id is safe as a single file name component on Linux, macOS, and
/// Windows: lower-case ASCII letters, digits, and `-` only. The display name
/// is cosmetic and never participates in identity comparisons.
#[derive(Debug, Clone)]
pub struct ProfileKey {
    id: String,
    name: String,
}

impl PartialEq for ProfileKey {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ProfileKey {}

impl Hash for ProfileKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl ProfileKey {
    /// Derive a key from a controller's `$I`/`$I+` identity.
    ///
    /// The firmware, board, driver, driver version, firmware version, and
    /// build are hashed. Missing fields are distinguished from present ones.
    #[must_use]
    pub fn from_controller(info: &ControllerInfo) -> Self {
        let identified = has_board_or_driver(info);
        let slug = readable_slug(info);
        let digest = identity_hash(info);
        let id = if identified {
            format!("{slug}-{digest:016x}")
        } else {
            format!("{UNIDENTIFIED_PREFIX}{slug}-{digest:016x}")
        };
        let name = human_name(info);
        Self { id, name }
    }

    /// Build a key from an existing storage id.
    ///
    /// The display name is set to the id. This is used when enumerating saved
    /// profile files; callers that know the original controller should prefer
    /// [`ProfileKey::from_controller`], which also restores the readable name.
    ///
    /// # Errors
    ///
    /// Returns [`ValidationError::InvalidProfileKey`] when the id is empty,
    /// too long, or contains anything other than lower-case ASCII letters,
    /// digits, and `-`.
    pub fn from_id(id: impl Into<String>) -> Result<Self, ValidationError> {
        let id = id.into();
        validate_id(&id)?;
        Ok(Self {
            name: id.clone(),
            id,
        })
    }

    /// Build a key from an id and an optional stored name.
    fn from_parts(id: String, name: Option<String>) -> Result<Self, ValidationError> {
        validate_id(&id)?;
        let name = name
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .map(|name| truncate_chars(&name, MAX_NAME_LEN))
            .unwrap_or_else(|| id.clone());
        if name.chars().any(char::is_control) {
            return Err(ValidationError::InvalidProfileName(name));
        }
        Ok(Self { id, name })
    }

    /// Storage id, safe as a single file name component.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Human-readable profile name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.name
    }

    /// File name for this profile, for example `flexihal-...-1a2b3c4d.json`.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{}.json", self.id)
    }

    /// Returns `true` when a board or driver name contributed to the key.
    ///
    /// A key that is not identified is still stable for the fields that were
    /// reported, but it cannot prove that it belongs to a particular board.
    /// The app must not silently apply such a profile to a newly connected
    /// controller.
    #[must_use]
    pub fn is_identified(&self) -> bool {
        !self.id.starts_with(UNIDENTIFIED_PREFIX)
    }

    /// Human label including the id when it adds information.
    #[must_use]
    pub fn label(&self) -> String {
        if self.name == self.id {
            self.id.clone()
        } else {
            format!("{} [{}]", self.name, self.id)
        }
    }
}

impl fmt::Display for ProfileKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.name == self.id {
            f.write_str(&self.id)
        } else {
            write!(f, "{} [{}]", self.name, self.id)
        }
    }
}

impl FromStr for ProfileKey {
    type Err = ValidationError;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        Self::from_id(id)
    }
}

impl Serialize for ProfileKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ProfileKeyRef {
            id: &self.id,
            name: &self.name,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProfileKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match ProfileKeyWire::deserialize(deserializer)? {
            ProfileKeyWire::Full { id, name } => {
                ProfileKey::from_parts(id, name).map_err(serde::de::Error::custom)
            }
            ProfileKeyWire::Id(id) => {
                ProfileKey::from_parts(id, None).map_err(serde::de::Error::custom)
            }
        }
    }
}

#[derive(Serialize)]
struct ProfileKeyRef<'a> {
    id: &'a str,
    name: &'a str,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ProfileKeyWire {
    Full {
        id: String,
        #[serde(default)]
        name: Option<String>,
    },
    Id(String),
}

fn validate_id(id: &str) -> Result<(), ValidationError> {
    if id.is_empty() || id.len() > MAX_ID_LEN {
        return Err(ValidationError::InvalidProfileKey(id.to_string()));
    }
    if id.starts_with('-') || id.ends_with('-') {
        return Err(ValidationError::InvalidProfileKey(id.to_string()));
    }
    if !id
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(ValidationError::InvalidProfileKey(id.to_string()));
    }
    if WINDOWS_RESERVED.contains(&id) {
        return Err(ValidationError::InvalidProfileKey(id.to_string()));
    }
    Ok(())
}

fn has_board_or_driver(info: &ControllerInfo) -> bool {
    non_empty(info.board.as_deref()).is_some() || non_empty(info.driver.as_deref()).is_some()
}

fn readable_slug(info: &ControllerInfo) -> String {
    non_empty(info.board.as_deref())
        .and_then(slug)
        .or_else(|| non_empty(info.firmware.as_deref()).and_then(slug))
        .or_else(|| non_empty(info.driver.as_deref()).and_then(slug))
        .unwrap_or_else(|| "controller".to_string())
}

fn human_name(info: &ControllerInfo) -> String {
    let base = non_empty(info.board.as_deref())
        .or_else(|| non_empty(info.firmware.as_deref()))
        .or_else(|| non_empty(info.driver.as_deref()))
        .unwrap_or("Unknown controller");
    let mut name = base.to_string();
    if let Some(version) =
        non_empty(Some(info.version.as_str())).or_else(|| non_empty(Some(info.build_info.as_str())))
    {
        name.push(' ');
        name.push_str(version);
    }
    if let Some(driver) = non_empty(info.driver.as_deref())
        && driver != base
    {
        name.push_str(" (");
        name.push_str(driver);
        name.push(')');
    }
    truncate_chars(&name, MAX_NAME_LEN)
}

fn slug(text: &str) -> Option<String> {
    let mut slug = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        return None;
    }
    slug.truncate(MAX_SLUG_LEN);
    while slug.ends_with('-') {
        slug.pop();
    }
    Some(slug)
}

fn identity_hash(info: &ControllerInfo) -> u64 {
    let mut hash = FNV_OFFSET;
    hash_bytes(&mut hash, b"ghs-profile-v1");
    let fields: [(&str, Option<&str>); 6] = [
        ("firmware", info.firmware.as_deref()),
        ("board", info.board.as_deref()),
        ("driver", info.driver.as_deref()),
        ("driver_version", info.driver_version.as_deref()),
        ("version", Some(info.version.as_str())),
        ("build", Some(info.build_info.as_str())),
    ];
    for (label, value) in fields {
        hash_bytes(&mut hash, label.as_bytes());
        hash_bytes(&mut hash, &[0]);
        match non_empty(value) {
            Some(value) => {
                hash_bytes(&mut hash, &[1]);
                hash_bytes(&mut hash, &(value.len() as u64).to_le_bytes());
                hash_bytes(&mut hash, value.as_bytes());
            }
            None => hash_bytes(&mut hash, &[0]),
        }
    }
    hash
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for &byte in bytes {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        text.chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use ghs_core::parse_controller_info;

    use super::*;

    const FLEXIHAL: &[&str] = &[
        "[VER:1.1f.20240604:]",
        "[OPT:VNMSL+2,64,1024,3,0]",
        "[AXS:3:XYZ]",
        "[NEWOPT:ENUMS,RT+,HOME,ES,EXPR,TC,SED]",
        "[FIRMWARE:grblHAL]",
        "[DRIVER:STM32F446]",
        "[DRIVER VERSION:240428]",
        "[BOARD:FlexiHAL_20240604_210624]",
    ];

    fn flexihal() -> ControllerInfo {
        parse_controller_info(FLEXIHAL)
    }

    #[test]
    fn same_controller_produces_a_stable_key() {
        let first = ProfileKey::from_controller(&flexihal());
        let second = ProfileKey::from_controller(&flexihal());
        assert_eq!(first, second);
        assert_eq!(first.id(), second.id());
        assert!(first.is_identified());
        assert!(first.display_name().contains("FlexiHAL"));
    }

    #[test]
    fn different_board_or_version_changes_the_key() {
        let base = flexihal();
        let mut board = base.clone();
        board.board = Some("OtherBoard".into());
        let mut version = base.clone();
        version.version = "1.1g".into();
        let mut driver = base.clone();
        driver.driver = Some("RP2040".into());

        let base_key = ProfileKey::from_controller(&base);
        assert_ne!(base_key.id(), ProfileKey::from_controller(&board).id());
        assert_ne!(base_key.id(), ProfileKey::from_controller(&version).id());
        assert_ne!(base_key.id(), ProfileKey::from_controller(&driver).id());
    }

    #[test]
    fn capability_changes_do_not_change_identity() {
        let base = flexihal();
        let mut capabilities = base.clone();
        capabilities.new_options.push("SD".into());
        capabilities.rx_buffer_size = Some(2048);
        capabilities.tool_table_entries = Some(16);
        assert_eq!(
            ProfileKey::from_controller(&base).id(),
            ProfileKey::from_controller(&capabilities).id()
        );
    }

    #[test]
    fn missing_board_is_not_collapsed_with_a_present_board() {
        let base = flexihal();
        let mut missing = base.clone();
        missing.board = None;
        missing.driver = None;

        let base_key = ProfileKey::from_controller(&base);
        let missing_key = ProfileKey::from_controller(&missing);
        assert_ne!(base_key.id(), missing_key.id());
        assert!(!missing_key.is_identified());
        assert!(missing_key.id().starts_with(UNIDENTIFIED_PREFIX));
    }

    #[test]
    fn field_boundaries_cannot_collide() {
        let mut first = ControllerInfo::new();
        first.board = Some("AB".into());
        first.driver = Some("C".into());
        let mut second = ControllerInfo::new();
        second.board = Some("A".into());
        second.driver = Some("BC".into());
        assert_ne!(
            ProfileKey::from_controller(&first).id(),
            ProfileKey::from_controller(&second).id()
        );
    }

    #[test]
    fn missing_and_empty_fields_are_equivalent_but_still_distinct_from_values() {
        let mut empty = ControllerInfo::new();
        empty.board = Some("   ".into());
        let mut missing = ControllerInfo::new();
        missing.board = None;
        let mut present = ControllerInfo::new();
        present.board = Some("Board".into());

        assert_eq!(
            ProfileKey::from_controller(&empty).id(),
            ProfileKey::from_controller(&missing).id()
        );
        assert_ne!(
            ProfileKey::from_controller(&empty).id(),
            ProfileKey::from_controller(&present).id()
        );
    }

    #[test]
    fn key_is_filesystem_safe() {
        let key = ProfileKey::from_controller(&flexihal());
        assert!(
            key.id()
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        );
        assert!(key.file_name().ends_with(".json"));
        assert!(!key.id().contains('/'));
        assert!(!key.id().contains('\\'));
        assert!(!key.id().contains('.'));
    }

    #[test]
    fn key_serializes_with_its_display_name() {
        let key = ProfileKey::from_controller(&flexihal());
        let json = serde_json::to_string(&key).expect("serialize");
        assert!(json.contains("\"id\""));
        assert!(json.contains("\"name\""));
        let decoded: ProfileKey = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded, key);
    }

    #[test]
    fn display_name_does_not_affect_identity() {
        let key = ProfileKey::from_controller(&flexihal());
        let renamed = ProfileKey::from_parts(key.id().to_string(), Some("Renamed".into()))
            .expect("valid renamed key");
        assert_eq!(key, renamed);
        assert_ne!(key.display_name(), renamed.display_name());
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let hash = |value: &ProfileKey| {
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            hasher.finish()
        };
        assert_eq!(hash(&key), hash(&renamed));
    }

    #[test]
    fn key_accepts_a_bare_id_string() {
        let key: ProfileKey = serde_json::from_str("\"flexihal-abc\"").expect("deserialize");
        assert_eq!(key.id(), "flexihal-abc");
        assert_eq!(key.display_name(), "flexihal-abc");
        let key: ProfileKey =
            serde_json::from_str(r#"{"id":"flexihal-abc"}"#).expect("deserialize");
        assert_eq!(key.id(), "flexihal-abc");
        assert_eq!(key.display_name(), "flexihal-abc");
    }

    #[test]
    fn invalid_ids_are_rejected() {
        for id in [
            "",
            "../escape",
            "UPPER",
            "a/b",
            "a\\b",
            "-lead",
            "trail-",
            "a.b",
            "con",
        ] {
            assert!(
                ProfileKey::from_id(id).is_err(),
                "expected {:?} to be rejected",
                id
            );
        }
    }
}
