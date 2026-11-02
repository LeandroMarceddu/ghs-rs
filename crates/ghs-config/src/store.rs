//! Platform config directory, atomic JSON saves, backups, and profile files.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ghs_core::ControllerInfo;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::app::AppConfig;
use crate::backup::{BackupBundle, IdentityCheck, IdentityMismatch, IdentityPolicy, ImportOutcome};
use crate::controller::ControllerData;
use crate::error::ConfigError;
use crate::profile::ProfileKey;

/// Application directory name under the platform configuration root.
pub const APP_DIR_NAME: &str = "ghs-rs";
/// File name of the app preferences document.
pub const APP_CONFIG_FILE: &str = "app.json";
/// Directory holding per-controller profile documents.
pub const PROFILES_DIR: &str = "profiles";
/// Directory holding rotated copies of store-owned documents.
pub const BACKUPS_DIR: &str = "backups";
/// Number of previous versions kept by default.
pub const DEFAULT_BACKUP_LIMIT: usize = 5;

/// Operating-system family used for config path resolution.
///
/// The value is explicit so path rules for every supported platform can be
/// tested on any host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostPlatform {
    /// Linux, BSD, and other XDG-style systems.
    Unix,
    /// macOS.
    MacOs,
    /// Windows.
    Windows,
}

/// Persistent store rooted at an explicit or platform configuration directory.
///
/// Every write goes to a temporary file in the destination directory, is
/// flushed and fsynced, then renamed over the destination. A failed write
/// leaves the previous file intact and removes its temporary file. Before a
/// store-owned document is replaced, its previous bytes are copied into
/// [`ConfigStore::backups_dir`] and old backups are pruned to the configured
/// limit.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    root: PathBuf,
    backup_limit: usize,
}

impl ConfigStore {
    /// Build a store at an explicit root.
    ///
    /// This is the test and portable-mode constructor. It performs no I/O and
    /// never reads environment variables. Use [`ConfigStore::platform`] for
    /// the normal per-user location.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            backup_limit: DEFAULT_BACKUP_LIMIT,
        }
    }

    /// Build a store at the platform configuration directory:
    ///
    /// - Unix: `$XDG_CONFIG_HOME/ghs-rs`, or `$HOME/.config/ghs-rs` when the
    ///   variable is missing or relative.
    /// - macOS: `$HOME/Library/Application Support/ghs-rs`.
    /// - Windows: `%APPDATA%\ghs-rs`, falling back to
    ///   `%USERPROFILE%\AppData\Roaming\ghs-rs`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::NoConfigDir`] when no required variable is set.
    pub fn platform() -> Result<Self, ConfigError> {
        let root = config_root_with(host_platform(), |key| std::env::var_os(key))?;
        Ok(Self::new(root))
    }

    /// Replace the number of previous versions kept per document.
    ///
    /// `0` disables rotated backups. The default is
    /// [`DEFAULT_BACKUP_LIMIT`].
    #[must_use]
    pub fn with_backup_limit(mut self, limit: usize) -> Self {
        self.backup_limit = limit;
        self
    }

    /// Number of previous versions kept per document.
    #[must_use]
    pub const fn backup_limit(&self) -> usize {
        self.backup_limit
    }

    /// Root directory of this store.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path of the app preferences document.
    #[must_use]
    pub fn app_config_path(&self) -> PathBuf {
        self.root.join(APP_CONFIG_FILE)
    }

    /// Directory holding per-controller profiles.
    #[must_use]
    pub fn profiles_dir(&self) -> PathBuf {
        self.root.join(PROFILES_DIR)
    }

    /// Path of the document for one controller key.
    #[must_use]
    pub fn profile_path(&self, key: &ProfileKey) -> PathBuf {
        self.profiles_dir().join(key.file_name())
    }

    /// Directory holding rotated backups.
    #[must_use]
    pub fn backups_dir(&self) -> PathBuf {
        self.root.join(BACKUPS_DIR)
    }

    /// Load the app preferences.
    ///
    /// Returns `Ok(None)` only when the file does not exist. A corrupt or
    /// unsupported file is an error and is never overwritten silently.
    ///
    /// # Errors
    ///
    /// Returns an I/O, JSON, or validation error. The version is checked
    /// before the value is returned.
    pub fn load_app_config(&self) -> Result<Option<AppConfig>, ConfigError> {
        let path = self.app_config_path();
        let Some(bytes) = read_optional(&path)? else {
            return Ok(None);
        };
        let config: AppConfig = decode(&path, &bytes)?;
        config.validate()?;
        Ok(Some(config))
    }

    /// Load the app preferences, falling back to [`AppConfig::default`] only
    /// when the file is missing.
    ///
    /// # Errors
    ///
    /// Returns an error for an unreadable, corrupt, or unsupported file. It
    /// does not replace the file.
    pub fn load_or_default(&self) -> Result<AppConfig, ConfigError> {
        Ok(self.load_app_config()?.unwrap_or_default())
    }

    /// Atomically save app preferences and rotate the previous version.
    ///
    /// # Errors
    ///
    /// Returns a validation error before touching the file, or an I/O error if
    /// the temporary file, rename, or backup step fails.
    pub fn save_app_config(&self, config: &AppConfig) -> Result<(), ConfigError> {
        config.validate()?;
        self.write_json(&self.app_config_path(), config, true)
    }

    /// Load one controller profile.
    ///
    /// Returns `Ok(None)` when no file exists for the key. The embedded key
    /// must match the file name and the data must validate.
    ///
    /// # Errors
    ///
    /// Returns a JSON, validation, or key-mismatch error for a corrupt file.
    pub fn load_controller_data(
        &self,
        key: &ProfileKey,
    ) -> Result<Option<ControllerData>, ConfigError> {
        let path = self.profile_path(key);
        let Some(bytes) = read_optional(&path)? else {
            return Ok(None);
        };
        let file: ProfileFile = decode(&path, &bytes)?;
        if file.profile.id() != key.id() {
            return Err(ConfigError::ProfileKeyMismatch {
                path,
                found: file.profile,
                expected: key.clone(),
            });
        }
        file.data.validate()?;
        Ok(Some(file.data))
    }

    /// Atomically save one controller profile and rotate the previous version.
    ///
    /// # Errors
    ///
    /// Returns a validation error before touching the file, or an I/O error.
    pub fn save_controller_data(
        &self,
        key: &ProfileKey,
        data: &ControllerData,
    ) -> Result<(), ConfigError> {
        data.validate()?;
        let path = self.profile_path(key);
        let file = ProfileFile {
            profile: key.clone(),
            data: data.clone(),
        };
        self.write_json(&path, &file, true)
    }

    /// List saved profiles by reading and validating every profile document.
    ///
    /// Files that are not `.json` or not regular files are ignored. A malformed
    /// profile is an error rather than being silently hidden.
    ///
    /// # Errors
    ///
    /// Returns an I/O, JSON, validation, or key-mismatch error.
    pub fn list_profile_keys(&self) -> Result<Vec<ProfileKey>, ConfigError> {
        let dir = self.profiles_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(io_error("read directory", dir, source)),
        };

        let mut keys = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| io_error("read directory", dir.clone(), source))?;
            let path = entry.path();
            if !entry
                .file_type()
                .map_err(|source| io_error("inspect", path.clone(), source))?
                .is_file()
            {
                continue;
            }
            if path.extension() != Some(OsStr::new("json")) {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(OsStr::to_str) else {
                return Err(ConfigError::InvalidPath(path));
            };
            let expected = ProfileKey::from_id(stem)?;
            let bytes = read_required(&path)?;
            let file: ProfileFile = decode(&path, &bytes)?;
            if file.profile.id() != expected.id() {
                return Err(ConfigError::ProfileKeyMismatch {
                    path,
                    found: file.profile,
                    expected,
                });
            }
            file.data.validate()?;
            keys.push(file.profile);
        }
        keys.sort_by(|left, right| left.id().cmp(right.id()));
        Ok(keys)
    }

    /// Read and validate a backup bundle from a file.
    ///
    /// # Errors
    ///
    /// Returns an I/O, JSON, or validation error. Validation happens before
    /// the bundle is returned, so no caller can apply an unknown version.
    pub fn load_backup(&self, path: &Path) -> Result<BackupBundle, ConfigError> {
        let bytes = read_required(path)?;
        let bundle: BackupBundle = decode(path, &bytes)?;
        bundle.validate()?;
        Ok(bundle)
    }

    /// Atomically export a backup bundle to a user-chosen path.
    ///
    /// Validation runs before any write. Export does not rotate files in the
    /// store's backup directory; the destination is the user's copy.
    ///
    /// # Errors
    ///
    /// Returns a validation error or an I/O error.
    pub fn export_backup(&self, path: &Path, bundle: &BackupBundle) -> Result<(), ConfigError> {
        bundle.validate()?;
        self.write_json(path, bundle, false)
    }

    /// Validate, identity-check, and apply a backup bundle.
    ///
    /// With [`IdentityPolicy::Reject`], a different or unverifiable controller
    /// identity returns an error before anything is written. With
    /// [`IdentityPolicy::Warn`], the bundle is applied and the same check is
    /// returned in [`ImportOutcome::identity`] for the UI to display.
    ///
    /// When `current` is `None` (an offline restore), no comparison with a
    /// live controller is possible. `identity` is `None` for an identified
    /// stored profile, and [`IdentityCheck::Unverified`] when the stored
    /// profile has no board or driver identity.
    ///
    /// # Errors
    ///
    /// Returns a JSON or validation error for a bad bundle, an identity error
    /// when rejecting a mismatch, or an I/O error while saving.
    pub fn import_backup(
        &self,
        path: &Path,
        current: Option<&ControllerInfo>,
        policy: IdentityPolicy,
    ) -> Result<ImportOutcome, ConfigError> {
        let bundle = self.load_backup(path)?;
        let identity = match current {
            Some(info) => Some(bundle.check_identity(info)),
            None if !bundle.controller.profile.is_identified() => {
                Some(IdentityCheck::Unverified(bundle.controller.profile.clone()))
            }
            None => None,
        };
        if policy == IdentityPolicy::Reject {
            match &identity {
                Some(IdentityCheck::Mismatch(IdentityMismatch { stored, current })) => {
                    return Err(ConfigError::IdentityMismatch {
                        stored: stored.clone(),
                        current: current.clone(),
                    });
                }
                Some(IdentityCheck::Unverified(profile)) => {
                    return Err(ConfigError::UnverifiedIdentity {
                        profile: profile.clone(),
                    });
                }
                Some(IdentityCheck::Match) | None => {}
            }
        }
        self.apply_backup(&bundle)?;
        Ok(ImportOutcome { bundle, identity })
    }

    /// Write a validated bundle into the store.
    ///
    /// This does not compare controller identities; call
    /// [`BackupBundle::check_identity`] first or use
    /// [`ConfigStore::import_backup`], which enforces
    /// [`IdentityPolicy`].
    ///
    /// The controller profile is written first, then the app config with
    /// `last_profile` set to the bundle's profile. Each file is replaced
    /// atomically, but the pair is not a single transaction: a failure between
    /// the two leaves the profile saved and the app config unchanged.
    ///
    /// # Errors
    ///
    /// Returns a validation error before writing, or an I/O error.
    pub fn apply_backup(&self, bundle: &BackupBundle) -> Result<(), ConfigError> {
        bundle.validate()?;
        self.save_controller_data(&bundle.controller.profile, &bundle.data)?;
        let mut app = bundle.app.clone();
        app.last_profile = Some(bundle.controller.profile.clone());
        app.validate()?;
        self.save_app_config(&app)
    }

    fn write_json<T: Serialize>(
        &self,
        path: &Path,
        value: &T,
        rotate: bool,
    ) -> Result<(), ConfigError> {
        let mut bytes = serde_json::to_vec_pretty(value).map_err(ConfigError::Encode)?;
        bytes.push(b'\n');

        if rotate && self.backup_limit > 0 {
            match fs::read(path) {
                Ok(previous) => self.write_rotated_backup(path, &previous)?,
                Err(source) if source.kind() == io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(io_error(
                        "read previous version",
                        path.to_path_buf(),
                        source,
                    ));
                }
            }
        }
        atomic_write(path, &bytes)
    }

    fn write_rotated_backup(&self, path: &Path, previous: &[u8]) -> Result<(), ConfigError> {
        let dir = self.backups_dir();
        create_dir(&dir)?;
        let stem = path
            .file_stem()
            .and_then(OsStr::to_str)
            .ok_or_else(|| ConfigError::InvalidPath(path.to_path_buf()))?;
        let name = unique_backup_name(&dir, stem);
        atomic_write(&dir.join(name), previous)?;
        self.prune_backups(&dir, stem)
    }

    fn prune_backups(&self, dir: &Path, stem: &str) -> Result<(), ConfigError> {
        let prefix = format!("{stem}.");
        let entries = fs::read_dir(dir)
            .map_err(|source| io_error("read directory", dir.to_path_buf(), source))?;
        let mut names = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|source| io_error("read directory", dir.to_path_buf(), source))?;
            let is_file = entry
                .file_type()
                .map_err(|source| io_error("inspect", entry.path(), source))?
                .is_file();
            if !is_file {
                continue;
            }
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            if name.starts_with(&prefix) && name.ends_with(".json") {
                names.push(entry.path());
            }
        }
        names.sort();
        let excess = names.len().saturating_sub(self.backup_limit);
        for path in names.into_iter().take(excess) {
            fs::remove_file(&path).map_err(|source| io_error("remove old backup", path, source))?;
        }
        Ok(())
    }
}

/// Resolve a configuration root using an environment-variable lookup.
///
/// This is platform-rule logic without global environment mutation, so every
/// branch is testable on any host. `get` should behave like
/// [`std::env::var_os`].
///
/// # Errors
///
/// Returns [`ConfigError::NoConfigDir`] when the required home directory
/// variable is missing.
pub fn config_root_with<F>(platform: HostPlatform, get: F) -> Result<PathBuf, ConfigError>
where
    F: Fn(&str) -> Option<OsString>,
{
    match platform {
        HostPlatform::Windows => {
            if let Some(appdata) = non_empty_path(get("APPDATA")) {
                return Ok(appdata.join(APP_DIR_NAME));
            }
            if let Some(profile) = non_empty_path(get("USERPROFILE")) {
                return Ok(profile.join("AppData").join("Roaming").join(APP_DIR_NAME));
            }
            Err(ConfigError::NoConfigDir)
        }
        HostPlatform::MacOs => {
            let home = non_empty_path(get("HOME")).ok_or(ConfigError::NoConfigDir)?;
            Ok(home
                .join("Library")
                .join("Application Support")
                .join(APP_DIR_NAME))
        }
        HostPlatform::Unix => {
            if let Some(xdg) = non_empty_path(get("XDG_CONFIG_HOME"))
                && xdg.is_absolute()
            {
                return Ok(xdg.join(APP_DIR_NAME));
            }
            let home = non_empty_path(get("HOME")).ok_or(ConfigError::NoConfigDir)?;
            Ok(home.join(".config").join(APP_DIR_NAME))
        }
    }
}

fn host_platform() -> HostPlatform {
    if cfg!(target_os = "windows") {
        HostPlatform::Windows
    } else if cfg!(target_os = "macos") {
        HostPlatform::MacOs
    } else {
        HostPlatform::Unix
    }
}

fn non_empty_path(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
}

#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
struct ProfileFile {
    profile: ProfileKey,
    data: ControllerData,
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, ConfigError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(io_error("read", path.to_path_buf(), source)),
    }
}

fn read_required(path: &Path) -> Result<Vec<u8>, ConfigError> {
    read_optional(path)?.ok_or_else(|| {
        io_error(
            "read",
            path.to_path_buf(),
            io::Error::new(io::ErrorKind::NotFound, "file does not exist"),
        )
    })
}

fn decode<T: DeserializeOwned>(path: &Path, bytes: &[u8]) -> Result<T, ConfigError> {
    serde_json::from_slice(bytes).map_err(|source| ConfigError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn create_dir(path: &Path) -> Result<(), ConfigError> {
    fs::create_dir_all(path)
        .map_err(|source| io_error("create directory", path.to_path_buf(), source))
}

fn io_error(action: &'static str, path: PathBuf, source: io::Error) -> ConfigError {
    ConfigError::Io {
        action,
        path,
        source,
    }
}

/// Write `bytes` to `path` through a temporary file in the same directory.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ConfigError> {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    create_dir(parent)?;
    let Some(file_name) = path.file_name() else {
        return Err(ConfigError::InvalidPath(path.to_path_buf()));
    };

    let (tmp, mut file) = create_temp_file(parent, file_name)?;
    let result = (|| {
        file.write_all(bytes)
            .map_err(|source| io_error("write temporary file", tmp.clone(), source))?;
        file.flush()
            .map_err(|source| io_error("flush temporary file", tmp.clone(), source))?;
        file.sync_all()
            .map_err(|source| io_error("fsync temporary file", tmp.clone(), source))?;
        drop(file);
        fs::rename(&tmp, path).map_err(|source| io_error("replace", path.to_path_buf(), source))
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result?;

    // Persist the directory entry too. Some filesystems do not support fsync
    // on a directory; failing that step must not fail an otherwise-complete
    // save.
    #[cfg(unix)]
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn create_temp_file(parent: &Path, file_name: &OsStr) -> Result<(PathBuf, File), ConfigError> {
    let mut last_source = None;
    for _ in 0..32 {
        let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut name = OsString::from(".");
        name.push(file_name);
        name.push(format!(".tmp-{}-{sequence}", std::process::id()));
        let path = parent.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                last_source = Some(source);
            }
            Err(source) => return Err(io_error("create temporary file", path, source)),
        }
    }
    Err(io_error(
        "create temporary file",
        parent.to_path_buf(),
        last_source.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "temporary file name collision",
            )
        }),
    ))
}

fn unique_backup_name(dir: &Path, stem: &str) -> OsString {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let base = format!("{stem}.{millis:013}");
    for attempt in 0..10_000 {
        let name = format!("{base}.{attempt:04}.json");
        if !dir.join(&name).exists() {
            return OsString::from(name);
        }
    }
    OsString::from(format!("{base}.{}.json", std::process::id()))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use ghs_core::{Position, Wcs, parse_controller_info};

    use super::*;
    use crate::backup::IdentityPolicy;
    use crate::controller::{
        ControllerData, HeightMapFrame, HeightMapRecord, MAX_PROBE_OFFSET_MM, ProbeOffsetRecord,
        ToolRecord,
    };
    use crate::error::ValidationError;

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

    fn other_info() -> ControllerInfo {
        let mut info = info();
        info.board = Some("OtherBoard".into());
        info
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
                "ghs-config-{label}-{}-{nanos}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn store(label: &str) -> (TempDir, ConfigStore) {
        let dir = TempDir::new(label);
        let store = ConfigStore::new(dir.path());
        (dir, store)
    }

    fn sample_data() -> ControllerData {
        let mut data = ControllerData::new();
        data.settings.insert(20, ghs_core::SettingValue::Bool(true));
        data.settings
            .insert(100, ghs_core::SettingValue::Float(250.0));
        data.offsets
            .set_work_offset(Wcs::G54, Position::xyz(10.0, 20.0, 30.0));
        data.offsets
            .set_work_offset(Wcs::G59_3, Position::xyz(-1.0, -2.0, -3.0));
        data.tools.insert(ToolRecord {
            number: 1,
            name: "Flat 6mm".into(),
            diameter_mm: 6.0,
            length_mm: 35.0,
            pocket: Some(1),
        });
        data.macros = vec![crate::controller::Macro::new("Zero XY", "G10 L20 P1 X0 Y0")];
        data.startup_lines.n0 = Some("G21 G90".into());
        data.keymap
            .insert(crate::controller::KeyBinding::new("run.pause", "Space"));
        data.height_maps = vec![sample_height_map()];
        data.probe_offset = ProbeOffsetRecord {
            x_mm: 4.0,
            y_mm: -2.5,
        };
        data
    }

    fn sample_height_map() -> HeightMapRecord {
        HeightMapRecord {
            name: "Fixture top".into(),
            tool_number: Some(1),
            created_at_unix_s: Some(1_700_000_000),
            origin_x_mm: 5.0,
            origin_y_mm: 6.0,
            step_x_mm: 10.0,
            step_y_mm: 10.0,
            columns: 2,
            rows: 2,
            surface_z_mm: -1.5,
            z_offset_mm: Some(0.1),
            samples: vec![Some(0.0), Some(0.02), None, Some(0.04)],
            frame: HeightMapFrame {
                wcs: Wcs::G54,
                wcs_offset: Position::xyz(10.0, 20.0, 30.0),
                g92: Position::xyz(0.5, 0.0, 0.0),
                tool_length_offset_z_mm: -12.0,
            },
        }
    }

    fn count_backups(store: &ConfigStore, stem: &str) -> usize {
        let prefix = format!("{stem}.");
        fs::read_dir(store.backups_dir())
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry.file_name().to_str().is_some_and(|name| {
                            name.starts_with(&prefix) && name.ends_with(".json")
                        })
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    fn assert_no_temp_files(root: &Path) {
        let mut stack = vec![root.to_path_buf()];
        while let Some(path) = stack.pop() {
            for entry in fs::read_dir(&path).expect("read dir") {
                let entry = entry.expect("entry");
                let name = entry.file_name();
                let name = name.to_string_lossy();
                assert!(!name.contains(".tmp-"), "left temporary file {name}");
                if entry.file_type().expect("file type").is_dir() {
                    stack.push(entry.path());
                }
            }
        }
    }

    #[test]
    fn missing_app_config_returns_defaults() {
        let (dir, store) = store("missing-app");
        assert_eq!(store.root(), dir.path());
        assert_eq!(
            store.load_or_default().expect("default"),
            AppConfig::default()
        );
        assert!(store.load_app_config().expect("none").is_none());
        assert!(!store.app_config_path().exists());
    }

    #[test]
    fn app_config_round_trips() {
        let (_dir, store) = store("app-round-trip");
        let config = AppConfig {
            theme: crate::app::ThemeMode::Light,
            units: ghs_core::Units::Inch,
            locale: "pt-BR".into(),
            last_profile: Some(ProfileKey::from_controller(&info())),
            window: crate::app::WindowLayout {
                width: 1920,
                height: 1080,
                ..crate::app::WindowLayout::default()
            },
            ..AppConfig::default()
        };
        store.save_app_config(&config).expect("save");
        assert_eq!(store.load_app_config().expect("load"), Some(config.clone()));
        assert_eq!(store.load_or_default().expect("load"), config);
    }

    #[test]
    fn corrupt_app_config_is_an_error_and_is_not_overwritten() {
        let (_dir, store) = store("corrupt-app");
        let path = store.app_config_path();
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let corrupt = b"{ this is not json";
        fs::write(&path, corrupt).expect("write");
        let error = store.load_or_default().expect_err("corrupt");
        assert!(matches!(error, ConfigError::Json { .. }));
        assert_eq!(fs::read(&path).expect("read"), corrupt);
    }

    #[test]
    fn unsupported_app_config_version_is_rejected() {
        let (_dir, store) = store("app-version");
        let path = store.app_config_path();
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let value = serde_json::json!({
            "schema_version": 99,
            "theme": "dark",
            "units": "Mm",
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
        fs::write(&path, serde_json::to_vec(&value).expect("json")).expect("write");
        assert!(matches!(
            store.load_or_default(),
            Err(ConfigError::Validation(
                crate::error::ValidationError::UnsupportedVersion {
                    what: "app config",
                    found: 99,
                    ..
                }
            ))
        ));
    }

    #[test]
    fn controller_data_round_trips() {
        let (_dir, store) = store("controller-round-trip");
        let key = ProfileKey::from_controller(&info());
        assert!(store.load_controller_data(&key).expect("missing").is_none());

        let data = sample_data();
        store.save_controller_data(&key, &data).expect("save");
        assert_eq!(
            store.load_controller_data(&key).expect("load"),
            Some(data.clone())
        );
        assert_eq!(store.list_profile_keys().expect("list"), vec![key]);
    }

    #[test]
    fn height_maps_are_stored_per_controller_profile() {
        let (_dir, store) = store("height-map-profile");
        let first = ProfileKey::from_controller(&info());
        let second = ProfileKey::from_controller(&other_info());

        let with_map = sample_data();
        assert_eq!(with_map.height_maps, vec![sample_height_map()]);
        store
            .save_controller_data(&first, &with_map)
            .expect("save first");
        store
            .save_controller_data(&second, &ControllerData::new())
            .expect("save second");

        let loaded_first = store
            .load_controller_data(&first)
            .expect("load first")
            .expect("present");
        let loaded_second = store
            .load_controller_data(&second)
            .expect("load second")
            .expect("present");
        assert_eq!(loaded_first, with_map);
        assert_eq!(loaded_first.height_maps, vec![sample_height_map()]);
        assert!(loaded_second.height_maps.is_empty());
    }

    #[test]
    fn saving_an_invalid_height_map_writes_nothing() {
        let (_dir, store) = store("invalid-height-map");
        let key = ProfileKey::from_controller(&info());
        let mut data = sample_data();
        data.height_maps[0].samples.pop();
        assert!(matches!(
            store.save_controller_data(&key, &data),
            Err(ConfigError::Validation(
                ValidationError::HeightMapSampleCount { .. }
            ))
        ));
        assert!(!store.profile_path(&key).exists());
    }

    #[test]
    fn backup_round_trip_preserves_height_maps() {
        let (dir, store) = store("backup-height-map");
        let info = info();
        let key = ProfileKey::from_controller(&info);
        let data = sample_data();
        let bundle = BackupBundle::new(AppConfig::default(), &info, data.clone())
            .with_created_at(1_700_000_000);
        let path = dir.path().join("height-map.json");
        store.export_backup(&path, &bundle).expect("export");

        let loaded = store.load_backup(&path).expect("load");
        assert_eq!(loaded.data.height_maps, data.height_maps);

        let outcome = store
            .import_backup(&path, Some(&info), IdentityPolicy::Reject)
            .expect("import");
        assert_eq!(outcome.bundle.data.height_maps, data.height_maps);
        assert_eq!(
            store.load_controller_data(&key).expect("load profile"),
            Some(data)
        );
    }

    #[test]
    fn malformed_height_map_in_a_backup_is_rejected_before_writing() {
        let (dir, store) = store("backup-invalid-height-map");
        let info = info();
        let key = ProfileKey::from_controller(&info);
        let bundle = BackupBundle::new(AppConfig::default(), &info, sample_data());
        let path = dir.path().join("height-map.json");
        store.export_backup(&path, &bundle).expect("export");

        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        value["data"]["height_maps"][0]["columns"] = serde_json::json!(1);
        let text = serde_json::to_string(&value).expect("json");
        fs::write(&path, text).expect("write");

        assert!(matches!(
            store.load_backup(&path),
            Err(ConfigError::Validation(ValidationError::InvalidHeightMap {
                field: "columns",
                ..
            }))
        ));
        assert!(matches!(
            store.import_backup(&path, Some(&info), IdentityPolicy::Reject),
            Err(ConfigError::Validation(ValidationError::InvalidHeightMap {
                field: "columns",
                ..
            }))
        ));
        assert!(!store.profile_path(&key).exists());
        assert_no_temp_files(dir.path());
    }

    #[test]
    fn invalid_probe_offset_in_a_backup_is_rejected_before_writing() {
        let (dir, store) = store("backup-invalid-probe-offset");
        let info = info();
        let key = ProfileKey::from_controller(&info);
        let bundle = BackupBundle::new(AppConfig::default(), &info, sample_data());
        let path = dir.path().join("probe-offset.json");
        store.export_backup(&path, &bundle).expect("export");

        let mut value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        value["data"]["probe_offset"]["x_mm"] = serde_json::json!(MAX_PROBE_OFFSET_MM + 1.0);
        let text = serde_json::to_string(&value).expect("json");
        fs::write(&path, text).expect("write");

        assert!(matches!(
            store.load_backup(&path),
            Err(ConfigError::Validation(
                ValidationError::ProbeOffsetOutOfRange { axis: "x_mm", .. }
            ))
        ));
        assert!(matches!(
            store.import_backup(&path, Some(&info), IdentityPolicy::Reject),
            Err(ConfigError::Validation(
                ValidationError::ProbeOffsetOutOfRange { .. }
            ))
        ));
        assert!(!store.profile_path(&key).exists());
        assert_no_temp_files(dir.path());
    }

    #[test]
    fn saving_invalid_controller_data_writes_nothing() {
        let (_dir, store) = store("invalid-controller");
        let key = ProfileKey::from_controller(&info());
        let mut data = ControllerData::new();
        data.tools.tools.push(ToolRecord {
            number: 1,
            name: "A".into(),
            diameter_mm: -1.0,
            length_mm: 0.0,
            pocket: None,
        });
        assert!(matches!(
            store.save_controller_data(&key, &data),
            Err(ConfigError::Validation(_))
        ));
        assert!(!store.profile_path(&key).exists());
    }

    #[test]
    fn profile_file_must_match_its_key() {
        let (_dir, store) = store("profile-key");
        let key = ProfileKey::from_controller(&info());
        let mut other = info();
        other.board = Some("Different".into());
        let other_key = ProfileKey::from_controller(&other);
        store
            .save_controller_data(&key, &sample_data())
            .expect("save");
        fs::rename(store.profile_path(&key), store.profile_path(&other_key)).expect("rename");
        assert!(matches!(
            store.load_controller_data(&other_key),
            Err(ConfigError::ProfileKeyMismatch { .. })
        ));
    }

    #[test]
    fn listing_profiles_keeps_display_names_and_order() {
        let (_dir, store) = store("profile-list");
        let first = ProfileKey::from_controller(&info());
        let second = ProfileKey::from_controller(&other_info());
        store
            .save_controller_data(&second, &sample_data())
            .expect("save second");
        store
            .save_controller_data(&first, &sample_data())
            .expect("save first");

        let keys = store.list_profile_keys().expect("list");
        assert_eq!(keys.len(), 2);
        assert!(keys.windows(2).all(|pair| pair[0].id() < pair[1].id()));
        let stored_first = keys
            .iter()
            .find(|key| key.id() == first.id())
            .expect("first");
        assert_eq!(stored_first.display_name(), first.display_name());
    }

    #[test]
    fn atomic_save_leaves_no_temporary_files() {
        let (dir, store) = store("atomic");
        let mut config = AppConfig::default();
        for _ in 0..3 {
            store.save_app_config(&config).expect("save");
            config.window.width += 1;
        }
        assert!(store.app_config_path().exists());
        assert_no_temp_files(dir.path());
    }

    #[test]
    fn failed_replace_removes_the_temporary_file() {
        let (dir, _store) = store("atomic-failure");
        let target = dir.path().join("target");
        fs::create_dir(&target).expect("make directory at target");
        assert!(atomic_write(&target, b"partial").is_err());
        assert!(target.is_dir());
        assert_no_temp_files(dir.path());
    }

    #[test]
    fn backup_rotation_keeps_the_default_limit() {
        let (dir, store) = store("rotation-default");
        assert_eq!(store.backup_limit(), DEFAULT_BACKUP_LIMIT);
        let mut config = AppConfig::default();
        for _ in 0..8 {
            store.save_app_config(&config).expect("save");
            config.window.width = config.window.width.saturating_add(1);
        }
        assert_eq!(count_backups(&store, "app"), DEFAULT_BACKUP_LIMIT);
        assert_no_temp_files(dir.path());
    }

    #[test]
    fn backup_rotation_honours_a_custom_limit() {
        let (dir, store) = store("rotation-custom");
        let store = store.with_backup_limit(2);
        let mut config = AppConfig::default();
        for _ in 0..6 {
            store.save_app_config(&config).expect("save");
            config.window.width = config.window.width.saturating_add(1);
        }
        assert_eq!(count_backups(&store, "app"), 2);
        assert_no_temp_files(dir.path());
    }

    #[test]
    fn zero_backup_limit_disables_rotation() {
        let (dir, store) = store("rotation-off");
        let store = store.with_backup_limit(0);
        store.save_app_config(&AppConfig::default()).expect("save");
        store.save_app_config(&AppConfig::default()).expect("save");
        assert_eq!(count_backups(&store, "app"), 0);
        assert_no_temp_files(dir.path());
    }

    #[test]
    fn export_rejects_an_invalid_bundle_before_writing() {
        let (dir, store) = store("export-invalid");
        let mut bundle = BackupBundle::new(AppConfig::default(), &info(), sample_data());
        bundle.data.schema_version = 99;
        let path = dir.path().join("invalid.json");
        assert!(matches!(
            store.export_backup(&path, &bundle),
            Err(ConfigError::Validation(_))
        ));
        assert!(!path.exists());
        assert_no_temp_files(dir.path());
    }

    #[test]
    fn backup_round_trips_and_imports() {
        let (dir, store) = store("backup-import");
        let info = info();
        let key = ProfileKey::from_controller(&info);
        let data = sample_data();
        let bundle = BackupBundle::new(AppConfig::default(), &info, data.clone())
            .with_created_at(1_700_000_000);
        let path = dir.path().join("export.json");
        store.export_backup(&path, &bundle).expect("export");
        assert_eq!(store.load_backup(&path).expect("load"), bundle);

        let outcome = store
            .import_backup(&path, Some(&info), IdentityPolicy::Reject)
            .expect("import");
        assert_eq!(outcome.identity, Some(IdentityCheck::Match));
        assert_eq!(store.load_controller_data(&key).expect("load"), Some(data));
        assert_eq!(
            store.load_or_default().expect("app").last_profile,
            Some(key)
        );
        assert_no_temp_files(dir.path());
    }

    #[test]
    fn import_rejects_a_different_controller_before_writing() {
        let (dir, store) = store("backup-mismatch");
        let info = info();
        let key = ProfileKey::from_controller(&info);
        let bundle = BackupBundle::new(AppConfig::default(), &info, sample_data());
        let path = dir.path().join("export.json");
        store.export_backup(&path, &bundle).expect("export");

        let error = store
            .import_backup(&path, Some(&other_info()), IdentityPolicy::Reject)
            .expect_err("reject");
        assert!(matches!(error, ConfigError::IdentityMismatch { .. }));
        assert!(!store.profile_path(&key).exists());

        let outcome = store
            .import_backup(&path, Some(&other_info()), IdentityPolicy::Warn)
            .expect("warn import");
        assert!(matches!(outcome.identity, Some(IdentityCheck::Mismatch(_))));
        assert!(
            outcome
                .identity
                .as_ref()
                .and_then(IdentityCheck::warning)
                .is_some()
        );
        assert!(store.profile_path(&key).exists());
    }

    #[test]
    fn import_can_run_without_a_connected_controller() {
        let (dir, store) = store("backup-offline");
        let bundle = BackupBundle::new(AppConfig::default(), &info(), sample_data());
        let path = dir.path().join("export.json");
        store.export_backup(&path, &bundle).expect("export");
        let outcome = store
            .import_backup(&path, None, IdentityPolicy::Reject)
            .expect("import");
        assert!(outcome.identity.is_none());
    }

    #[test]
    fn offline_import_of_an_unidentified_profile_is_explicit() {
        let (dir, store) = store("backup-unidentified");
        let mut unidentified = ControllerInfo::new();
        unidentified.version = "1.1f".into();
        let bundle = BackupBundle::new(AppConfig::default(), &unidentified, sample_data());
        let path = dir.path().join("export.json");
        store.export_backup(&path, &bundle).expect("export");

        assert!(matches!(
            store.import_backup(&path, None, IdentityPolicy::Reject),
            Err(ConfigError::UnverifiedIdentity { .. })
        ));
        let outcome = store
            .import_backup(&path, None, IdentityPolicy::Warn)
            .expect("warn import");
        assert!(matches!(
            outcome.identity,
            Some(IdentityCheck::Unverified(_))
        ));
        assert!(
            outcome
                .identity
                .as_ref()
                .and_then(IdentityCheck::warning)
                .is_some()
        );
    }

    #[test]
    fn config_roots_follow_platform_rules() {
        let root = config_root_with(HostPlatform::Unix, |key| match key {
            "XDG_CONFIG_HOME" => Some(OsString::from("/tmp/xdg")),
            _ => None,
        })
        .expect("xdg");
        assert_eq!(root, PathBuf::from("/tmp/xdg").join(APP_DIR_NAME));

        let root = config_root_with(HostPlatform::Unix, |key| match key {
            "XDG_CONFIG_HOME" => Some(OsString::from("relative/xdg")),
            "HOME" => Some(OsString::from("/home/user")),
            _ => None,
        })
        .expect("relative xdg ignored");
        assert_eq!(
            root,
            PathBuf::from("/home/user")
                .join(".config")
                .join(APP_DIR_NAME)
        );

        let root = config_root_with(HostPlatform::MacOs, |key| match key {
            "HOME" => Some(OsString::from("/Users/user")),
            _ => None,
        })
        .expect("mac");
        assert_eq!(
            root,
            PathBuf::from("/Users/user")
                .join("Library")
                .join("Application Support")
                .join(APP_DIR_NAME)
        );

        let appdata = PathBuf::from(r"C:\Users\user\AppData\Roaming");
        let root = config_root_with(HostPlatform::Windows, |key| match key {
            "APPDATA" => Some(appdata.clone().into_os_string()),
            _ => None,
        })
        .expect("windows");
        assert_eq!(root, appdata.join(APP_DIR_NAME));

        assert!(matches!(
            config_root_with(HostPlatform::Unix, |_| None),
            Err(ConfigError::NoConfigDir)
        ));
        assert!(matches!(
            config_root_with(HostPlatform::Windows, |_| None),
            Err(ConfigError::NoConfigDir)
        ));
    }
}
