//! Local configuration, controller profiles, and backups.
//!
//! The store is intentionally small and explicit. Connecting a controller
//! derives a [`ProfileKey`](ghs_config::ProfileKey) from `$I+`; the matching
//! local profile is loaded, but nothing in it is sent back to the controller.
//! Offsets and tools stay app-side until the operator saves or exports them.
//! A corrupt file is reported and is never silently replaced.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ghs_config::{
    AppConfig, BackupBundle, ConfigStore, ControllerData, HeightMapRecord, IdentityCheck,
    IdentityPolicy, ImportOutcome, Keymap, MAX_HEIGHT_MAPS, Macro, OffsetSnapshot,
    ProbeDefaultsRecord, ProbeOffsetRecord, ProfileKey, ThemeMode, ToolTable,
};
use ghs_core::Units as CoreUnits;
use ghs_engine::{ControllerInfo, SettingValue};

/// Result of noticing a controller during the handshake.
#[derive(Clone, Debug)]
pub struct ProfileLoad {
    /// Tool table to adopt, when the profile was safely matched.
    pub tools: Option<ToolTable>,
    /// Profile file path.
    pub path: Option<PathBuf>,
    /// Console notice for the operator.
    pub notice: String,
    /// True when the identity could not be verified and the data is waiting
    /// for an explicit operator action.
    pub unverified: bool,
}

/// A loaded backup bundle and its identity comparison.
#[derive(Clone, Debug)]
pub struct BackupPreview {
    /// File that was read.
    pub path: PathBuf,
    /// Profile stored in the bundle.
    pub key: ProfileKey,
    /// Comparison with the connected controller, when one is connected.
    pub identity: Option<IdentityCheck>,
    /// Warning to show before an import.
    pub warning: Option<String>,
    /// Number of tools in the bundle.
    pub tools: usize,
    /// Number of work coordinate systems in the bundle.
    pub work_offsets: usize,
}

/// Result of applying a backup bundle.
#[derive(Clone, Debug)]
pub struct BackupImport {
    /// File that was imported.
    pub path: PathBuf,
    /// Profile that was written into the store.
    pub key: ProfileKey,
    /// Identity warning that was accepted, when any.
    pub warning: Option<String>,
    /// Tools to adopt when the bundle belongs to the connected profile.
    pub tools: Option<ToolTable>,
}

/// Short success/error line for the Utility tab and console.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigStatus {
    /// Whether the operation succeeded.
    pub ok: bool,
    /// Human-readable message.
    pub text: String,
}

/// App preferences plus the current controller profile and backup helpers.
#[derive(Debug)]
pub struct ConfigModel {
    store: Option<ConfigStore>,
    app: AppConfig,
    data: ControllerData,
    key: Option<ProfileKey>,
    info: Option<ControllerInfo>,
    app_error: Option<String>,
    profile_error: Option<String>,
    corrupt_app: bool,
    corrupt_profile: bool,
    pending_data: Option<ControllerData>,
    unverified_identity: bool,
    last_result: Option<ConfigStatus>,
    preview: Option<BackupPreview>,
}

impl Default for ConfigModel {
    fn default() -> Self {
        Self::platform()
    }
}

impl ConfigModel {
    /// Open the platform configuration directory and load `app.json`.
    ///
    /// A missing file uses defaults. A corrupt file is reported and blocks
    /// saves until it is repaired, so it is never silently overwritten.
    #[must_use]
    pub fn platform() -> Self {
        // Unit tests use `from_store` with a temporary root. Never touch the
        // developer's real config from a test binary.
        if cfg!(test) {
            return Self::empty();
        }
        match ConfigStore::platform() {
            Ok(store) => Self::from_store(store),
            Err(error) => Self {
                store: None,
                app_error: Some(error.to_string()),
                last_result: Some(ConfigStatus {
                    ok: false,
                    text: error.to_string(),
                }),
                ..Self::empty()
            },
        }
    }

    /// Build a model at an explicit root. Used by tests and portable runs.
    #[must_use]
    pub fn from_store(store: ConfigStore) -> Self {
        let mut model = Self {
            store: Some(store),
            ..Self::empty()
        };
        model.load_app_config();
        model
    }

    fn empty() -> Self {
        Self {
            store: None,
            app: AppConfig::default(),
            data: ControllerData::new(),
            key: None,
            info: None,
            app_error: None,
            profile_error: None,
            corrupt_app: false,
            corrupt_profile: false,
            pending_data: None,
            unverified_identity: false,
            last_result: None,
            preview: None,
        }
    }

    fn load_app_config(&mut self) {
        let Some(store) = &self.store else {
            return;
        };
        match store.load_app_config() {
            Ok(Some(app)) => {
                self.app = app;
                self.corrupt_app = false;
                self.app_error = None;
            }
            Ok(None) => {
                self.app = AppConfig::default();
                self.corrupt_app = false;
                self.app_error = None;
            }
            Err(error) => {
                self.app = AppConfig::default();
                self.corrupt_app = true;
                self.app_error = Some(error.to_string());
            }
        }
    }

    /// App preferences currently in memory.
    #[must_use]
    pub fn app(&self) -> &AppConfig {
        &self.app
    }

    /// Whether the first-run safety/setup wizard has been completed.
    ///
    /// This is an app preference, not a controller setting. A config written
    /// before the field existed loads as `false`.
    #[must_use]
    pub fn setup_complete(&self) -> bool {
        self.app.setup_complete
    }

    /// Persist the first-run setup completion flag through [`Self::save_app`].
    ///
    /// This is the only path that flips [`AppConfig::setup_complete`]. The flag
    /// is changed in memory only after the write succeeds; a refusal (no store,
    /// corrupt `app.json`, or an I/O error) leaves the previous value and never
    /// overwrites a corrupt file. Completing an already-complete setup still
    /// writes the file, so the operator can review and update the choices.
    ///
    /// # Errors
    ///
    /// Returns the [`Self::save_app`] message when the store is unavailable or
    /// the write is refused.
    pub fn set_setup_complete(&mut self, complete: bool) -> Result<PathBuf, String> {
        let previous = self.app.setup_complete;
        self.app.setup_complete = complete;
        match self.save_app() {
            Ok(path) => Ok(path),
            Err(error) => {
                self.app.setup_complete = previous;
                self.last_result = Some(ConfigStatus {
                    ok: false,
                    text: format!("Setup completion was not saved: {error}"),
                });
                Err(error)
            }
        }
    }

    /// Current controller profile data.
    #[must_use]
    pub fn data(&self) -> &ControllerData {
        &self.data
    }

    /// Stable key for the connected controller, once the handshake finished.
    #[must_use]
    pub fn key(&self) -> Option<&ProfileKey> {
        self.key.as_ref()
    }

    /// Configuration root, when a platform directory was available.
    #[must_use]
    pub fn store_root(&self) -> Option<&Path> {
        self.store.as_ref().map(ConfigStore::root)
    }

    /// Path of the current controller profile file.
    #[must_use]
    pub fn profile_path(&self) -> Option<PathBuf> {
        let store = self.store.as_ref()?;
        let key = self.key.as_ref()?;
        Some(store.profile_path(key))
    }

    /// Fixed path used by the explicit Backups buttons.
    ///
    /// This is under the configuration directory, so a file picker is not
    /// required for M5.
    #[must_use]
    pub fn backup_path(&self) -> Option<PathBuf> {
        let root = self.store_root()?;
        Some(root.join("exports").join("ghs-backup.json"))
    }

    /// Last status line.
    #[must_use]
    pub fn last_result(&self) -> Option<&ConfigStatus> {
        self.last_result.as_ref()
    }

    /// App config error, if the file could not be loaded.
    #[must_use]
    pub fn app_error(&self) -> Option<&str> {
        self.app_error.as_deref()
    }

    /// Profile error, if the file could not be loaded or identity is unverified.
    #[must_use]
    pub fn profile_error(&self) -> Option<&str> {
        self.profile_error.as_deref()
    }

    /// Whether the current profile file is corrupt and saving is blocked.
    #[must_use]
    pub fn corrupt_profile(&self) -> bool {
        self.corrupt_profile
    }

    /// Whether an unverified profile is waiting for an explicit action.
    #[must_use]
    pub fn has_pending_profile(&self) -> bool {
        self.pending_data.is_some()
    }

    /// Whether the current profile's identity could not be verified.
    #[must_use]
    pub fn unverified_identity(&self) -> bool {
        self.unverified_identity
    }

    /// Persisted height maps for the current controller profile.
    ///
    /// The slice is empty before a profile loads or when the profile has none;
    /// maps never cross a controller boundary.
    #[must_use]
    pub fn height_maps(&self) -> &[HeightMapRecord] {
        &self.data.height_maps
    }

    /// User-defined macros for the active controller profile.
    ///
    /// The slice is empty before a profile loads or when the profile has none;
    /// macros never cross a controller boundary and loading them never sends a
    /// command.
    #[must_use]
    pub fn macros(&self) -> &[Macro] {
        &self.data.macros
    }

    /// Why a macro cannot be written to the current profile, if any.
    ///
    /// This mirrors the gate used by [`ConfigModel::save_macros`] and by the
    /// tools/maps profile writes so the UI can explain a refusal.
    #[must_use]
    pub fn macro_save_blocker(&self) -> Option<String> {
        if self.store.is_none() {
            return Some("no platform configuration directory".to_string());
        }
        if self.corrupt_profile {
            return Some(
                "the stored profile is corrupt; it was not overwritten. Repair or remove it first"
                    .to_string(),
            );
        }
        if self.pending_data.is_some() {
            return Some(
                "the controller profile is unverified; confirm or discard it in Utility first"
                    .to_string(),
            );
        }
        if self.unverified_identity {
            return Some(
                "the controller identity is not verified; macros are saved only for a known \
                 controller"
                    .to_string(),
            );
        }
        match self.key.as_ref() {
            None => {
                Some("no controller profile is active; connect and finish the handshake".into())
            }
            Some(key) if !key.is_identified() => Some(
                "the controller identity is not verified; macros are saved only for a known \
                 controller"
                    .to_string(),
            ),
            Some(_) => None,
        }
    }

    /// Why the keymap cannot be written to the current profile, if any.
    ///
    /// This mirrors the gate used by [`ConfigModel::save_keymap`] so the
    /// editor can explain a refusal before the operator presses SAVE.
    #[must_use]
    pub fn keymap_save_blocker(&self) -> Option<String> {
        if self.store.is_none() {
            return Some("no platform configuration directory".to_string());
        }
        if self.corrupt_profile {
            return Some(
                "the stored profile is corrupt; it was not overwritten. Repair or remove it first"
                    .to_string(),
            );
        }
        if self.pending_data.is_some() {
            return Some(
                "the controller profile is unverified; confirm or discard it in Utility first"
                    .to_string(),
            );
        }
        if self.unverified_identity {
            return Some(
                "the controller identity is not verified; the keymap is saved only for a known \
                 controller"
                    .to_string(),
            );
        }
        match self.key.as_ref() {
            None => {
                Some("no controller profile is active; connect and finish the handshake".into())
            }
            Some(key) if !key.is_identified() => Some(
                "the controller identity is not verified; the keymap is saved only for a known \
                 controller"
                    .to_string(),
            ),
            Some(_) => None,
        }
    }

    /// Why a height map cannot be written to the current profile, if any.
    ///
    /// This mirrors the gate used by [`ConfigModel::save_height_map`] so the UI
    /// can explain a refusal before the operator types a name.
    #[must_use]
    pub fn height_map_save_blocker(&self) -> Option<String> {
        if self.store.is_none() {
            return Some("no platform configuration directory".to_string());
        }
        if self.corrupt_profile {
            return Some(
                "the stored profile is corrupt; it was not overwritten. Repair or remove it first"
                    .to_string(),
            );
        }
        if self.pending_data.is_some() {
            return Some(
                "the controller profile is unverified; confirm or discard it in Utility first"
                    .to_string(),
            );
        }
        if self.unverified_identity {
            return Some(
                "the controller identity is not verified; height maps are saved only for a \
                 known controller"
                    .to_string(),
            );
        }
        match self.key.as_ref() {
            None => {
                Some("no controller profile is active; connect and finish the handshake".into())
            }
            Some(key) if !key.is_identified() => Some(
                "the controller identity is not verified; height maps are saved only for a \
                 known controller"
                    .to_string(),
            ),
            Some(_) => None,
        }
    }

    /// Clear the per-session controller identity before a new connection.
    ///
    /// The app preferences and the store stay open; only the previously
    /// connected controller's data is dropped.
    pub fn begin_connection(&mut self) {
        self.key = None;
        self.info = None;
        self.data = ControllerData::new();
        self.pending_data = None;
        self.unverified_identity = false;
        self.corrupt_profile = false;
        self.profile_error = None;
        self.preview = None;
    }

    /// Notice a controller from the handshake and load its local profile.
    ///
    /// Repeating the same controller returns `None`, so a status refresh does
    /// not reload or overwrite the table.
    pub fn on_controller(&mut self, info: &ControllerInfo) -> Option<ProfileLoad> {
        let key = ProfileKey::from_controller(info);
        if self.key.as_ref().is_some_and(|current| *current == key) {
            return None;
        }
        if let Some(previous) = &self.key
            && *previous != key
        {
            self.pending_data = None;
            self.unverified_identity = false;
        }
        self.key = Some(key.clone());
        self.info = Some(info.clone());
        self.preview = None;
        let switched_note = self
            .app
            .last_profile
            .as_ref()
            .filter(|last| **last != key)
            .map(|last| {
                format!(
                    "; the previous last profile {} was not applied",
                    last.display_name()
                )
            });

        let store = self.store.clone()?;
        let path = store.profile_path(&key);
        match store.load_controller_data(&key) {
            Ok(Some(data)) => {
                self.corrupt_profile = false;
                self.profile_error = None;
                if key.is_identified() {
                    self.data = data;
                    self.pending_data = None;
                    self.unverified_identity = false;
                    self.remember_last_profile(&key);
                    let mut notice = format!(
                        "Local profile loaded: {} ({} tools)",
                        key.display_name(),
                        self.data.tools.len()
                    );
                    if let Some(note) = &switched_note {
                        notice.push_str(note);
                    }
                    Some(ProfileLoad {
                        tools: Some(self.data.tools.clone()),
                        path: Some(path),
                        notice,
                        unverified: false,
                    })
                } else {
                    // A key without a board or driver cannot prove it belongs
                    // to this exact machine. Keep the data aside and require
                    // an explicit "use local data" action.
                    self.pending_data = Some(data);
                    self.data = ControllerData::new();
                    self.unverified_identity = true;
                    self.profile_error = Some(
                        "controller identity has no board or driver; \
                         local profile data is not applied"
                            .to_string(),
                    );
                    self.last_result = Some(ConfigStatus {
                        ok: false,
                        text: "unverified controller identity".to_string(),
                    });
                    Some(ProfileLoad {
                        tools: None,
                        path: Some(path),
                        notice: format!(
                            "Unverified controller identity; local profile data is waiting \
                             for confirmation{}",
                            switched_note.as_deref().unwrap_or("")
                        ),
                        unverified: true,
                    })
                }
            }
            Ok(None) => {
                self.data = ControllerData::new();
                self.corrupt_profile = false;
                self.profile_error = None;
                self.pending_data = None;
                self.unverified_identity = false;
                self.remember_last_profile(&key);
                Some(ProfileLoad {
                    tools: Some(ToolTable::new()),
                    path: Some(path),
                    notice: format!(
                        "New controller profile; no local tool table yet{}",
                        switched_note.as_deref().unwrap_or("")
                    ),
                    unverified: false,
                })
            }
            Err(error) => {
                self.data = ControllerData::new();
                self.corrupt_profile = true;
                self.pending_data = None;
                self.unverified_identity = false;
                self.profile_error = Some(error.to_string());
                self.last_result = Some(ConfigStatus {
                    ok: false,
                    text: format!("Profile not loaded: {error}"),
                });
                Some(ProfileLoad {
                    tools: Some(ToolTable::new()),
                    path: Some(path),
                    notice: format!(
                        "Local profile is corrupt and was not loaded: {error}{}",
                        switched_note.as_deref().unwrap_or("")
                    ),
                    unverified: false,
                })
            }
        }
    }

    /// Adopt the unverified profile data after an explicit operator action.
    ///
    /// Returns `None` when nothing was waiting.
    pub fn use_pending_profile(&mut self) -> Option<ToolTable> {
        let data = self.pending_data.take()?;
        let tools = data.tools.clone();
        self.data = data;
        self.profile_error = Some(
            "unverified profile data was applied by the operator; saving keeps the same key"
                .to_string(),
        );
        self.last_result = Some(ConfigStatus {
            ok: true,
            text: "Local profile data applied after confirmation".to_string(),
        });
        Some(tools)
    }

    fn remember_last_profile(&mut self, key: &ProfileKey) {
        if self.app.last_profile.as_ref() == Some(key) {
            return;
        }
        self.app.last_profile = Some(key.clone());
        if let Err(error) = self.save_app() {
            self.last_result = Some(ConfigStatus {
                ok: false,
                text: error,
            });
        }
    }

    /// Save the app preferences (theme, units, last profile) atomically.
    ///
    /// # Errors
    ///
    /// Returns a message when no store is available or the existing app file
    /// is corrupt. A corrupt file is never overwritten.
    pub fn save_app(&mut self) -> Result<PathBuf, String> {
        if self.corrupt_app {
            return Err("app.json is corrupt; repair or remove it before saving".to_string());
        }
        let Some(store) = self.store.clone() else {
            return Err("no platform configuration directory".to_string());
        };
        store
            .save_app_config(&self.app)
            .map_err(|error| error.to_string())?;
        let path = store.app_config_path();
        self.last_result = Some(ConfigStatus {
            ok: true,
            text: format!("App settings saved to {}", path.display()),
        });
        Ok(path)
    }

    /// Persist the theme toggle immediately.
    pub fn set_theme(&mut self, dark: bool) {
        self.app.theme = if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        let _ = self.save_app();
    }

    /// Persist the UI locale for the next session.
    ///
    /// The global catalog is applied by the caller (see
    /// [`crate::i18n::choose_locale`]); this method only writes
    /// [`AppConfig::locale`] through [`Self::save_app`]. The selector applies
    /// the new locale even when this write fails.
    ///
    /// # Errors
    ///
    /// Returns the [`Self::save_app`] error when no store is available or the
    /// existing app file is corrupt.
    pub fn set_locale(&mut self, locale: ghs_i18n::Locale) -> Result<PathBuf, String> {
        self.app.locale = locale.code().to_string();
        self.save_app()
    }

    /// Persist the parser's unit mode for the next session.
    pub fn set_units(&mut self, units: CoreUnits) {
        if self.app.units == units {
            return;
        }
        self.app.units = units;
        let _ = self.save_app();
    }

    /// Persisted physical probe mount offset for the active profile.
    #[must_use]
    pub fn probe_offset(&self) -> ProbeOffsetRecord {
        self.data.probe_offset
    }

    /// Persisted shared probe defaults for the active profile.
    ///
    /// A profile written before the record existed returns the factory
    /// defaults. This is local setup data only.
    #[must_use]
    pub fn probe_defaults(&self) -> ProbeDefaultsRecord {
        self.data.probe_defaults
    }

    /// Save the probe mount offset into the current controller profile.
    ///
    /// The value is `probe tip - controlled point`, in millimetres. It is a
    /// local setup value: this writes only the profile JSON and never sends a
    /// command, offset, or setting to the controller.
    ///
    /// # Errors
    ///
    /// Returns a message when no controller profile is active, the stored
    /// profile is corrupt, the record is non-finite/out of range, or writing
    /// fails. A refused record leaves the previous value in place.
    pub fn save_probe_offset(&mut self, offset: ProbeOffsetRecord) -> Result<PathBuf, String> {
        if self.corrupt_profile {
            return Err(
                "the stored profile is corrupt; it was not overwritten. Repair or remove it first"
                    .to_string(),
            );
        }
        if self.store.is_none() {
            return Err("no platform configuration directory".to_string());
        }
        let Some(key) = self.key.clone() else {
            return Err("connect a controller before saving the probe offset".to_string());
        };
        if !key.is_identified() {
            return Err(
                "the controller identity is not verified; the probe offset is saved only for a \
                 known controller"
                    .to_string(),
            );
        }
        offset
            .validate()
            .map_err(|error| format!("probe offset is invalid: {error}"))?;
        let mut data = self.data.clone();
        data.probe_offset = offset;
        let path = self
            .store
            .as_ref()
            .map(|store| store.profile_path(&key))
            .ok_or_else(|| "no platform configuration directory".to_string())?;
        if let Err(error) = self
            .store
            .as_ref()
            .ok_or_else(|| "no platform configuration directory".to_string())
            .and_then(|store| {
                store
                    .save_controller_data(&key, &data)
                    .map_err(|error| error.to_string())
            })
        {
            self.last_result = Some(ConfigStatus {
                ok: false,
                text: format!("Probe offset save failed: {error}"),
            });
            return Err(error);
        }
        self.data = data;
        self.profile_error = None;
        self.last_result = Some(ConfigStatus {
            ok: true,
            text: format!("Probe offset saved to {}", path.display()),
        });
        Ok(path)
    }

    /// Save the shared probe defaults into the current controller profile.
    ///
    /// Only `probe_defaults` changes in a cloned profile document. The write
    /// goes through the same atomic `ConfigStore` path as tools and maps and
    /// never sends a command, setting, offset, or profile replacement to the
    /// controller. In-memory data changes only after the write succeeds, so a
    /// failure keeps the previous record.
    ///
    /// # Errors
    ///
    /// Returns a message when no identified controller profile is active, the
    /// stored profile is corrupt, the record is invalid, or writing fails.
    pub fn save_probe_defaults(
        &mut self,
        defaults: ProbeDefaultsRecord,
    ) -> Result<PathBuf, String> {
        if self.corrupt_profile {
            return Err(
                "the stored profile is corrupt; it was not overwritten. Repair or remove it first"
                    .to_string(),
            );
        }
        if self.store.is_none() {
            return Err("no platform configuration directory".to_string());
        }
        let Some(key) = self.key.clone() else {
            return Err("connect a controller before saving probe defaults".to_string());
        };
        if !key.is_identified() {
            return Err(
                "the controller identity is not verified; probe defaults are saved only for a \
                 known controller"
                    .to_string(),
            );
        }
        defaults
            .validate()
            .map_err(|error| format!("probe defaults are invalid: {error}"))?;
        let mut data = self.data.clone();
        data.probe_defaults = defaults;
        let path = self
            .store
            .as_ref()
            .map(|store| store.profile_path(&key))
            .ok_or_else(|| "no platform configuration directory".to_string())?;
        if let Err(error) = self
            .store
            .as_ref()
            .ok_or_else(|| "no platform configuration directory".to_string())
            .and_then(|store| {
                store
                    .save_controller_data(&key, &data)
                    .map_err(|error| error.to_string())
            })
        {
            self.last_result = Some(ConfigStatus {
                ok: false,
                text: format!("Probe defaults save failed: {error}"),
            });
            return Err(error);
        }
        self.data = data;
        self.profile_error = None;
        self.last_result = Some(ConfigStatus {
            ok: true,
            text: format!("Probe defaults saved to {}", path.display()),
        });
        Ok(path)
    }

    /// Save the current controller profile: tools, offsets, and a settings
    /// snapshot.
    ///
    /// This writes only local JSON. It never sends settings, offsets, or tools
    /// to the controller.
    ///
    /// # Errors
    ///
    /// Returns a message when there is no connected profile, the file is
    /// corrupt, or validation/writing fails.
    pub fn save_profile(
        &mut self,
        tools: &ToolTable,
        offsets: &OffsetSnapshot,
        settings: BTreeMap<u16, SettingValue>,
    ) -> Result<PathBuf, String> {
        if self.corrupt_profile {
            return Err(
                "the stored profile is corrupt; it was not overwritten. Repair or remove it first"
                    .to_string(),
            );
        }
        if self.store.is_none() {
            return Err("no platform configuration directory".to_string());
        }
        let Some(key) = self.key.clone() else {
            return Err("connect a controller before saving a profile".to_string());
        };
        let path = self
            .store
            .as_ref()
            .map(|store| store.profile_path(&key))
            .ok_or_else(|| "no platform configuration directory".to_string())?;
        self.data.tools = tools.clone();
        self.data.offsets = offsets.clone();
        self.data.settings = settings;
        let saved = self
            .store
            .as_ref()
            .ok_or_else(|| "no platform configuration directory".to_string())
            .and_then(|store| {
                store
                    .save_controller_data(&key, &self.data)
                    .map_err(|error| error.to_string())
            });
        if let Err(text) = saved {
            self.last_result = Some(ConfigStatus {
                ok: false,
                text: format!("Profile save failed: {text}"),
            });
            return Err(text);
        }
        self.profile_error = None;
        self.last_result = Some(ConfigStatus {
            ok: true,
            text: format!("Profile saved to {}", path.display()),
        });
        Ok(path)
    }

    /// Save one measured height map into the current controller profile.
    ///
    /// A record with the same name replaces the stored one in place. A new name
    /// is refused at [`MAX_HEIGHT_MAPS`] instead of dropping an older map. The
    /// write goes through the same atomic `ConfigStore` profile path as
    /// [`ConfigModel::save_profile`] and never sends anything to the controller.
    ///
    /// # Errors
    ///
    /// Returns a message when no verified profile is active, the profile file is
    /// corrupt, the record fails validation, the profile is full, or writing
    /// fails.
    pub fn save_height_map(&mut self, record: HeightMapRecord) -> Result<PathBuf, String> {
        if let Some(blocker) = self.height_map_save_blocker() {
            self.last_result = Some(ConfigStatus {
                ok: false,
                text: format!("Height map not saved: {blocker}"),
            });
            return Err(blocker);
        }
        record
            .validate()
            .map_err(|error| format!("height map is invalid: {error}"))?;
        let Some(key) = self.key.clone() else {
            return Err("no controller profile is active".to_string());
        };
        let Some(store) = self.store.clone() else {
            return Err("no platform configuration directory".to_string());
        };

        let mut data = self.data.clone();
        if let Some(index) = data
            .height_maps
            .iter()
            .position(|map| map.name == record.name)
        {
            data.height_maps[index] = record;
        } else {
            if data.height_maps.len() >= MAX_HEIGHT_MAPS {
                let message = format!(
                    "this controller profile already has {MAX_HEIGHT_MAPS} saved maps; \
                     reuse a name to replace one"
                );
                self.last_result = Some(ConfigStatus {
                    ok: false,
                    text: format!("Height map not saved: {message}"),
                });
                return Err(message);
            }
            data.height_maps.push(record);
        }
        let path = store.profile_path(&key);
        if let Err(error) = store.save_controller_data(&key, &data) {
            let message = error.to_string();
            self.last_result = Some(ConfigStatus {
                ok: false,
                text: format!("Height map save failed: {message}"),
            });
            return Err(message);
        }
        self.data = data;
        self.profile_error = None;
        self.last_result = Some(ConfigStatus {
            ok: true,
            text: format!(
                "Height map saved to {} ({} stored)",
                path.display(),
                self.data.height_maps.len()
            ),
        });
        Ok(path)
    }

    /// Save the full ordered macro list into the current controller profile.
    ///
    /// Every record is validated for the single-line/no-system-command policy
    /// and names are unique within the profile. The write goes through the same
    /// atomic `ConfigStore` profile path as tools and maps and never sends
    /// anything to the controller.
    ///
    /// # Errors
    ///
    /// Returns a message when no verified profile is active, the profile file is
    /// corrupt, a record fails validation, a name is duplicated, or writing
    /// fails. A refusal leaves the previous list in place.
    pub fn save_macros(&mut self, macros: Vec<Macro>) -> Result<PathBuf, String> {
        if let Some(blocker) = self.macro_save_blocker() {
            self.last_result = Some(ConfigStatus {
                ok: false,
                text: format!("Macro not saved: {blocker}"),
            });
            return Err(blocker);
        }
        for (index, mac) in macros.iter().enumerate() {
            if let Err(error) = crate::macros::validate_for_run(mac) {
                return Err(format!("macro is invalid: {error}"));
            }
            if macros[..index]
                .iter()
                .any(|previous| previous.name.trim() == mac.name.trim())
            {
                return Err(format!(
                    "a macro named '{}' already exists in this profile",
                    mac.name.trim()
                ));
            }
        }
        let Some(key) = self.key.clone() else {
            return Err("no controller profile is active".to_string());
        };
        let Some(store) = self.store.clone() else {
            return Err("no platform configuration directory".to_string());
        };
        let mut data = self.data.clone();
        data.macros = macros;
        let path = store.profile_path(&key);
        if let Err(error) = store.save_controller_data(&key, &data) {
            let message = error.to_string();
            self.last_result = Some(ConfigStatus {
                ok: false,
                text: format!("Macro save failed: {message}"),
            });
            return Err(message);
        }
        self.data = data;
        self.profile_error = None;
        self.last_result = Some(ConfigStatus {
            ok: true,
            text: format!(
                "Macros saved to {} ({} stored)",
                path.display(),
                self.data.macros.len()
            ),
        });
        Ok(path)
    }

    /// Save the bounded machine keymap into the current controller profile.
    ///
    /// The write goes through the same atomic `ConfigStore` profile path as
    /// tools, macros, and maps. It never sends a command to the controller and
    /// never starts a job or jogs an axis.
    ///
    /// # Errors
    ///
    /// Returns a message when no verified profile is active, the profile file
    /// is corrupt, the keymap fails record validation, or writing fails. A
    /// refusal leaves the previous keymap in place.
    pub fn save_keymap(&mut self, keymap: Keymap) -> Result<PathBuf, String> {
        if let Some(blocker) = self.keymap_save_blocker() {
            self.last_result = Some(ConfigStatus {
                ok: false,
                text: format!("Keymap not saved: {blocker}"),
            });
            return Err(blocker);
        }
        keymap
            .validate()
            .map_err(|error| format!("keymap is invalid: {error}"))?;
        let Some(key) = self.key.clone() else {
            return Err("no controller profile is active".to_string());
        };
        let Some(store) = self.store.clone() else {
            return Err("no platform configuration directory".to_string());
        };
        let mut data = self.data.clone();
        data.keymap = keymap;
        let path = store.profile_path(&key);
        if let Err(error) = store.save_controller_data(&key, &data) {
            let message = error.to_string();
            self.last_result = Some(ConfigStatus {
                ok: false,
                text: format!("Keymap save failed: {message}"),
            });
            return Err(message);
        }
        self.data = data;
        self.profile_error = None;
        self.last_result = Some(ConfigStatus {
            ok: true,
            text: format!(
                "Keymap saved to {} ({} bindings)",
                path.display(),
                self.data.keymap.bindings.len()
            ),
        });
        Ok(path)
    }

    /// Export the current profile as a versioned backup bundle.
    ///
    /// The bundle is written under the configuration directory at
    /// [`ConfigModel::backup_path`].
    ///
    /// # Errors
    ///
    /// Returns a message when no controller is connected or validation or
    /// writing fails.
    pub fn export_backup(
        &mut self,
        tools: &ToolTable,
        offsets: &OffsetSnapshot,
        settings: BTreeMap<u16, SettingValue>,
    ) -> Result<PathBuf, String> {
        if self.store.is_none() {
            return Err("no platform configuration directory".to_string());
        }
        let Some(info) = self.info.clone() else {
            return Err("connect a controller before exporting a backup".to_string());
        };
        let Some(path) = self.backup_path() else {
            return Err("no backup path is available".to_string());
        };
        let mut data = self.data.clone();
        data.tools = tools.clone();
        data.offsets = offsets.clone();
        data.settings = settings;
        let bundle = BackupBundle::new(self.app.clone(), &info, data);
        let exported = self
            .store
            .as_ref()
            .ok_or_else(|| "no platform configuration directory".to_string())
            .and_then(|store| {
                store
                    .export_backup(&path, &bundle)
                    .map_err(|error| error.to_string())
            });
        if let Err(text) = exported {
            self.last_result = Some(ConfigStatus {
                ok: false,
                text: format!("Backup export failed: {text}"),
            });
            return Err(text);
        }
        self.last_result = Some(ConfigStatus {
            ok: true,
            text: format!("Backup exported to {}", path.display()),
        });
        Ok(path)
    }

    /// Validate the backup at the fixed path and remember the preview.
    ///
    /// # Errors
    ///
    /// Returns a JSON, validation, or I/O message.
    pub fn validate_backup(&mut self) -> Result<BackupPreview, String> {
        let store = self
            .store
            .clone()
            .ok_or_else(|| "no platform configuration directory".to_string())?;
        let Some(path) = self.backup_path() else {
            return Err("no backup path is available".to_string());
        };
        let bundle = store
            .load_backup(&path)
            .map_err(|error| error.to_string())?;
        let identity = self.info.as_ref().map(|info| bundle.check_identity(info));
        let warning = identity.as_ref().and_then(IdentityCheck::warning);
        let ok = warning.is_none();
        let preview = BackupPreview {
            path,
            key: bundle.controller.profile.clone(),
            identity,
            warning,
            tools: bundle.data.tools.len(),
            work_offsets: bundle.data.offsets.work.len(),
        };
        self.preview = Some(preview.clone());
        self.last_result = Some(ConfigStatus {
            ok,
            text: match &preview.warning {
                Some(warning) => format!("Backup needs confirmation: {warning}"),
                None => format!(
                    "Backup validated: {} ({} tools)",
                    preview.key.display_name(),
                    preview.tools
                ),
            },
        });
        Ok(preview)
    }

    /// Import the validated backup at the fixed path.
    ///
    /// A mismatch or unverified identity is only accepted with
    /// `allow_warning = true`, and never becomes the current profile's data.
    ///
    /// # Errors
    ///
    /// Returns validation, identity, or I/O errors.
    pub fn import_backup(&mut self, allow_warning: bool) -> Result<BackupImport, String> {
        let preview = self.validate_backup()?;
        if preview.warning.is_some() && !allow_warning {
            return Err(preview
                .warning
                .unwrap_or_else(|| "backup identity needs confirmation".to_string()));
        }
        if self.store.is_none() {
            return Err("no platform configuration directory".to_string());
        }
        let policy = if preview.warning.is_some() {
            IdentityPolicy::Warn
        } else {
            IdentityPolicy::Reject
        };
        let info = self.info.clone();
        let outcome = self
            .store
            .as_ref()
            .ok_or_else(|| "no platform configuration directory".to_string())
            .and_then(|store| {
                store
                    .import_backup(&preview.path, info.as_ref(), policy)
                    .map_err(|error| error.to_string())
            })?;
        let warning = outcome.identity.as_ref().and_then(IdentityCheck::warning);
        let tools = self.adopt_import(&outcome);
        self.last_result = Some(ConfigStatus {
            ok: warning.is_none(),
            text: match &warning {
                Some(warning) => format!(
                    "Imported {} with a warning: {warning}",
                    preview.key.display_name()
                ),
                None => format!(
                    "Imported {} from {}",
                    preview.key.display_name(),
                    preview.path.display()
                ),
            },
        });
        Ok(BackupImport {
            path: preview.path,
            key: outcome.bundle.controller.profile.clone(),
            warning,
            tools,
        })
    }

    fn adopt_import(&mut self, outcome: &ImportOutcome) -> Option<ToolTable> {
        let imported = &outcome.bundle;
        let same = self
            .key
            .as_ref()
            .is_some_and(|key| *key == imported.controller.profile);
        if same {
            self.data = imported.data.clone();
            self.corrupt_profile = false;
            self.pending_data = None;
            Some(self.data.tools.clone())
        } else {
            None
        }
    }

    /// Human-readable identity line for the Utility tab.
    #[must_use]
    pub fn identity_line(&self) -> String {
        let Some(key) = &self.key else {
            return "No controller profile".to_string();
        };
        let state = if self.corrupt_profile {
            "CORRUPT FILE"
        } else if self.unverified_identity {
            "UNVERIFIED IDENTITY"
        } else if key.is_identified() {
            "IDENTIFIED"
        } else {
            "UNIDENTIFIED"
        };
        format!("{state} | {}", key.display_name())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use ghs_config::{KeyBinding, ToolRecord};
    use ghs_core::{Position, Wcs, parse_controller_info};

    use super::*;

    const FLEXIHAL: &[&str] = &[
        "[VER:1.1f.20240604:]",
        "[FIRMWARE:grblHAL]",
        "[DRIVER:STM32F446]",
        "[DRIVER VERSION:240428]",
        "[BOARD:FlexiHAL_20240604_210624]",
    ];

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
                "ghs-app-config-{label}-{}-{nanos}-{}",
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

    fn info() -> ControllerInfo {
        parse_controller_info(FLEXIHAL)
    }

    fn tool(number: u32, name: &str) -> ToolRecord {
        ToolRecord {
            number,
            name: name.to_string(),
            diameter_mm: 6.0,
            length_mm: 35.0,
            pocket: Some(number),
        }
    }

    fn offsets() -> OffsetSnapshot {
        let mut snapshot = OffsetSnapshot::new();
        snapshot.set_work_offset(Wcs::G54, Position::xyz(1.0, 2.0, 3.0));
        snapshot
    }

    #[test]
    fn app_preferences_round_trip() {
        let dir = TempDir::new("app");
        let mut model = ConfigModel::from_store(ConfigStore::new(&dir.path));
        model.app.units = CoreUnits::Inch;
        model.set_theme(false);
        assert_eq!(model.app().units, CoreUnits::Inch);
        assert_eq!(model.app().theme, ThemeMode::Light);

        let reopened = ConfigModel::from_store(ConfigStore::new(&dir.path));
        assert_eq!(reopened.app().units, CoreUnits::Inch);
        assert_eq!(reopened.app().theme, ThemeMode::Light);
    }

    #[test]
    fn setup_complete_round_trips_through_the_store() {
        let dir = TempDir::new("setup-flag");
        let store = ConfigStore::new(&dir.path);
        let mut model = ConfigModel::from_store(store.clone());
        assert!(!model.setup_complete(), "a new config is not complete");

        let path = model.set_setup_complete(true).expect("save setup flag");
        assert_eq!(path, store.app_config_path());
        assert!(model.setup_complete());
        assert!(ConfigModel::from_store(store.clone()).setup_complete());

        model.set_setup_complete(false).expect("clear setup flag");
        assert!(!model.setup_complete());
        assert!(!ConfigModel::from_store(store).setup_complete());
    }

    #[test]
    fn an_old_app_config_without_setup_complete_loads_false() {
        let dir = TempDir::new("setup-old");
        let store = ConfigStore::new(&dir.path);
        let path = store.app_config_path();
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(
            &path,
            r#"{
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
            }"#,
        )
        .expect("write old config");

        let model = ConfigModel::from_store(store);
        assert!(model.app_error().is_none());
        assert!(!model.setup_complete());
    }

    #[test]
    fn corrupt_app_config_blocks_setup_complete_and_is_never_overwritten() {
        let dir = TempDir::new("setup-corrupt");
        let store = ConfigStore::new(&dir.path);
        let path = store.app_config_path();
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let corrupt = b"{ this is not json";
        fs::write(&path, corrupt).expect("write corrupt config");

        let mut model = ConfigModel::from_store(store);
        let error = model
            .set_setup_complete(true)
            .expect_err("a corrupt app file blocks the write");
        assert!(error.contains("corrupt"), "{error}");
        assert!(
            !model.setup_complete(),
            "a failed write leaves the flag false"
        );
        assert_eq!(fs::read(&path).expect("read"), corrupt);
    }

    #[test]
    fn setup_complete_is_saved_again_when_the_wizard_is_revisited() {
        let dir = TempDir::new("setup-again");
        let store = ConfigStore::new(&dir.path);
        let mut model = ConfigModel::from_store(store.clone());
        model.set_setup_complete(true).expect("first completion");

        // A changed app preference is written by the same save on the next
        // completion, proving the method does not skip a completed flag.
        model.app.units = CoreUnits::Inch;
        model.set_setup_complete(true).expect("second completion");
        assert!(model.setup_complete());

        let reopened = ConfigModel::from_store(store);
        assert!(reopened.setup_complete());
        assert_eq!(reopened.app().units, CoreUnits::Inch);
    }

    #[test]
    fn tool_table_persists_per_profile() {
        let dir = TempDir::new("tools");
        let store = ConfigStore::new(&dir.path);
        let info = info();
        let mut model = ConfigModel::from_store(store.clone());
        let load = model.on_controller(&info).expect("profile load");
        assert!(load.tools.is_some());
        assert!(!model.unverified_identity());

        let mut table = ToolTable::new();
        table.insert(tool(3, "Flat 6mm"));
        let path = model
            .save_profile(&table, &offsets(), BTreeMap::new())
            .expect("save profile");
        assert!(path.exists());
        assert_eq!(
            store
                .load_controller_data(&ProfileKey::from_controller(&info))
                .expect("load")
                .expect("data")
                .tools,
            table
        );

        let mut reopened = ConfigModel::from_store(store);
        let load = reopened.on_controller(&info).expect("reload");
        assert_eq!(load.tools, Some(table));
        assert_eq!(reopened.data().offsets, offsets());
    }

    #[test]
    fn probe_offset_round_trips_through_the_profile_and_rejects_bad_values() {
        let dir = TempDir::new("probe-offset");
        let store = ConfigStore::new(&dir.path);
        let info = info();
        let key = ProfileKey::from_controller(&info);
        let mut model = ConfigModel::from_store(store.clone());
        model.on_controller(&info).expect("profile");
        assert_eq!(model.probe_offset(), ProbeOffsetRecord::default());

        let offset = ProbeOffsetRecord {
            x_mm: 12.5,
            y_mm: -7.25,
        };
        let path = model.save_probe_offset(offset).expect("save offset");
        assert!(path.exists());
        assert_eq!(model.probe_offset(), offset);
        assert_eq!(
            store
                .load_controller_data(&key)
                .expect("load")
                .expect("data")
                .probe_offset,
            offset
        );

        let mut reopened = ConfigModel::from_store(store);
        let load = reopened.on_controller(&info).expect("reload");
        assert!(!load.unverified);
        assert_eq!(reopened.probe_offset(), offset);

        let error = reopened
            .save_probe_offset(ProbeOffsetRecord {
                x_mm: f64::NAN,
                y_mm: 0.0,
            })
            .expect_err("non-finite offset is refused");
        assert!(
            error.contains("invalid") || error.contains("finite"),
            "{error}"
        );
        assert_eq!(
            reopened.probe_offset(),
            offset,
            "a refusal keeps the old value"
        );
    }

    #[test]
    fn probe_offset_save_needs_an_identified_profile() {
        let dir = TempDir::new("probe-offset-gate");
        let store = ConfigStore::new(&dir.path);
        let mut model = ConfigModel::from_store(store);
        let error = model
            .save_probe_offset(ProbeOffsetRecord {
                x_mm: 1.0,
                y_mm: 2.0,
            })
            .expect_err("no controller profile");
        assert!(error.contains("connect a controller"), "{error}");
        assert_eq!(model.probe_offset(), ProbeOffsetRecord::default());
    }

    #[test]
    fn unverified_profile_waits_for_an_explicit_action() {
        let dir = TempDir::new("unverified");
        let store = ConfigStore::new(&dir.path);
        let mut anonymous = ControllerInfo::new();
        anonymous.version = "1.1f".into();
        let key = ProfileKey::from_controller(&anonymous);
        assert!(!key.is_identified());
        let mut data = ControllerData::new();
        data.tools.insert(tool(1, "One"));
        store.save_controller_data(&key, &data).expect("save");

        let mut model = ConfigModel::from_store(store);
        let load = model.on_controller(&anonymous).expect("load notice");
        assert!(load.unverified);
        assert!(load.tools.is_none());
        assert!(model.has_pending_profile());
        assert!(model.unverified_identity());
        assert!(model.data().tools.is_empty());
        assert_eq!(
            model.use_pending_profile().map(|table| table.len()),
            Some(1)
        );
        assert!(!model.has_pending_profile());
        assert_eq!(model.data().tools.len(), 1);
    }

    #[test]
    fn corrupt_profile_is_reported_and_never_overwritten() {
        let dir = TempDir::new("corrupt");
        let store = ConfigStore::new(&dir.path);
        let info = info();
        let key = ProfileKey::from_controller(&info);
        let path = store.profile_path(&key);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let corrupt = b"{ not json";
        fs::write(&path, corrupt).expect("write");

        let mut model = ConfigModel::from_store(store);
        let load = model.on_controller(&info).expect("load notice");
        assert!(load.notice.contains("corrupt"));
        assert!(model.corrupt_profile());
        assert!(model.data().tools.is_empty());

        let error = model
            .save_profile(&ToolTable::new(), &offsets(), BTreeMap::new())
            .expect_err("save is blocked");
        assert!(error.contains("corrupt"));
        assert_eq!(fs::read(&path).expect("read"), corrupt);
    }

    #[test]
    fn backup_round_trip_uses_the_fixed_path_and_validates_identity() {
        let dir = TempDir::new("backup");
        let store = ConfigStore::new(&dir.path);
        let info = info();
        let mut model = ConfigModel::from_store(store.clone());
        model.on_controller(&info).expect("load");
        let mut table = ToolTable::new();
        table.insert(tool(2, "Ball 3mm"));
        let path = model
            .export_backup(&table, &offsets(), BTreeMap::new())
            .expect("export");
        assert_eq!(path, model.backup_path().expect("path"));
        assert!(path.exists());
        assert!(path.starts_with(&dir.path));

        let preview = model.validate_backup().expect("validate");
        assert_eq!(preview.tools, 1);
        assert_eq!(preview.work_offsets, 1);
        assert!(preview.warning.is_none());
        assert!(
            preview
                .identity
                .as_ref()
                .is_some_and(IdentityCheck::is_match)
        );

        let imported = model.import_backup(false).expect("import");
        assert_eq!(imported.tools.map(|table| table.len()), Some(1));
        assert_eq!(model.data().offsets, offsets());
    }

    #[test]
    fn an_identity_mismatch_requires_confirmation_and_is_not_applied() {
        let dir = TempDir::new("mismatch");
        let store = ConfigStore::new(&dir.path);
        let source = info();
        let mut other = info();
        other.board = Some("OtherBoard".into());

        let mut writer = ConfigModel::from_store(store.clone());
        writer.on_controller(&source).expect("source");
        let mut table = ToolTable::new();
        table.insert(tool(4, "Four"));
        writer
            .export_backup(&table, &offsets(), BTreeMap::new())
            .expect("export");

        let mut reader = ConfigModel::from_store(store);
        reader.on_controller(&other).expect("other");
        let error = reader.import_backup(false).expect_err("needs confirmation");
        assert!(
            error.contains("Backup is for") || error.contains("mismatch"),
            "{error}"
        );
        assert!(reader.data().tools.is_empty());

        let imported = reader.import_backup(true).expect("confirmed import");
        assert!(imported.warning.is_some());
        assert!(imported.tools.is_none(), "other profile is not applied");
        assert!(reader.data().tools.is_empty());
    }

    fn saved_map(name: &str, seed: f64) -> HeightMapRecord {
        HeightMapRecord {
            name: name.to_string(),
            tool_number: Some(3),
            created_at_unix_s: Some(1_700_000_000),
            origin_x_mm: 10.0,
            origin_y_mm: 20.0,
            step_x_mm: 5.0,
            step_y_mm: 2.5,
            columns: 3,
            rows: 2,
            surface_z_mm: -1.0,
            z_offset_mm: None,
            samples: (0..6).map(|index| Some(seed + f64::from(index))).collect(),
            frame: ghs_config::HeightMapFrame {
                wcs: Wcs::G55,
                wcs_offset: Position::xyz(1.0, 2.0, 3.0),
                g92: Position::xyz(0.1, 0.2, 0.3),
                tool_length_offset_z_mm: -12.5,
            },
        }
    }

    #[test]
    fn height_map_save_replaces_same_name_and_enforces_the_cap() {
        let dir = TempDir::new("height-maps");
        let store = ConfigStore::new(&dir.path);
        let info = info();
        let key = ProfileKey::from_controller(&info);
        let mut model = ConfigModel::from_store(store.clone());
        model.on_controller(&info).expect("profile");
        assert!(model.height_map_save_blocker().is_none());

        for index in 0..MAX_HEIGHT_MAPS {
            let path = model
                .save_height_map(saved_map(&format!("map-{index}"), index as f64))
                .expect("save map");
            assert!(path.exists());
        }
        assert_eq!(model.height_maps().len(), MAX_HEIGHT_MAPS);

        let replacement = saved_map("map-5", 100.0);
        model
            .save_height_map(replacement.clone())
            .expect("replace same name");
        assert_eq!(model.height_maps().len(), MAX_HEIGHT_MAPS);
        assert_eq!(
            model.height_maps().get(5).map(|map| &map.samples),
            Some(&replacement.samples),
            "same name replaces in place"
        );

        let error = model
            .save_height_map(saved_map("map-overflow", 0.0))
            .expect_err("cap is enforced");
        assert!(error.contains("16"), "{error}");
        assert_eq!(model.height_maps().len(), MAX_HEIGHT_MAPS);

        let stored = store
            .load_controller_data(&key)
            .expect("load")
            .expect("profile");
        assert_eq!(stored.height_maps.len(), MAX_HEIGHT_MAPS);
        assert_eq!(stored.height_maps[5], replacement);
    }

    #[test]
    fn height_map_save_is_refused_without_a_verified_profile_or_after_corruption() {
        let dir = TempDir::new("height-map-gates");
        let store = ConfigStore::new(&dir.path);

        let mut no_profile = ConfigModel::from_store(store.clone());
        let error = no_profile
            .save_height_map(saved_map("first", 0.0))
            .expect_err("no controller profile");
        assert!(error.contains("no controller profile"), "{error}");

        // Corrupt file: reported, never overwritten.
        let info = info();
        let key = ProfileKey::from_controller(&info);
        let path = store.profile_path(&key);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let corrupt = b"{ not json";
        fs::write(&path, corrupt).expect("write");
        let mut corrupt_model = ConfigModel::from_store(store.clone());
        let load = corrupt_model.on_controller(&info).expect("load notice");
        assert!(load.notice.contains("corrupt"));
        let error = corrupt_model
            .save_height_map(saved_map("first", 0.0))
            .expect_err("corrupt profile refused");
        assert!(error.contains("corrupt"), "{error}");
        assert_eq!(fs::read(&path).expect("read"), corrupt);

        // Unverified identity: stored data waits for an explicit action.
        let dir = TempDir::new("height-map-unverified");
        let store = ConfigStore::new(&dir.path);
        let mut anonymous = ControllerInfo::new();
        anonymous.version = "1.1f".into();
        let key = ProfileKey::from_controller(&anonymous);
        assert!(!key.is_identified());
        store
            .save_controller_data(&key, &ControllerData::new())
            .expect("save");
        let mut unverified = ConfigModel::from_store(store);
        assert!(
            unverified
                .on_controller(&anonymous)
                .expect("notice")
                .unverified
        );
        let error = unverified
            .save_height_map(saved_map("first", 0.0))
            .expect_err("unverified profile refused");
        assert!(
            error.contains("unverified") || error.contains("not verified"),
            "{error}"
        );
    }

    #[test]
    fn saving_a_height_map_keeps_the_existing_profile_fields() {
        let dir = TempDir::new("height-map-preserve");
        let store = ConfigStore::new(&dir.path);
        let info = info();
        let key = ProfileKey::from_controller(&info);
        let mut model = ConfigModel::from_store(store.clone());
        model.on_controller(&info).expect("profile");
        let mut table = ToolTable::new();
        table.insert(tool(6, "Six"));
        model
            .save_profile(&table, &offsets(), BTreeMap::new())
            .expect("profile save");
        model
            .save_height_map(saved_map("top", 1.0))
            .expect("map save");

        let stored = store
            .load_controller_data(&key)
            .expect("load")
            .expect("data");
        assert_eq!(stored.tools, table);
        assert_eq!(stored.offsets, offsets());
        assert_eq!(stored.height_maps.len(), 1);
    }

    #[test]
    fn height_maps_never_cross_a_controller_profile() {
        let dir = TempDir::new("height-map-profiles");
        let store = ConfigStore::new(&dir.path);
        let first = info();
        let mut second = info();
        second.board = Some("OtherBoard".into());

        let mut writer = ConfigModel::from_store(store.clone());
        writer.on_controller(&first).expect("first");
        writer
            .save_height_map(saved_map("first-only", 1.0))
            .expect("save first");

        let mut reader = ConfigModel::from_store(store.clone());
        reader.on_controller(&second).expect("second");
        assert!(
            reader.height_maps().is_empty(),
            "a different controller must not inherit maps"
        );
        reader
            .save_height_map(saved_map("second-only", 2.0))
            .expect("save second");

        let first_key = ProfileKey::from_controller(&first);
        let second_key = ProfileKey::from_controller(&second);
        let first_data = store
            .load_controller_data(&first_key)
            .expect("first load")
            .expect("first data");
        let second_data = store
            .load_controller_data(&second_key)
            .expect("second load")
            .expect("second data");
        assert_eq!(first_data.height_maps.len(), 1);
        assert_eq!(first_data.height_maps[0].name, "first-only");
        assert_eq!(second_data.height_maps.len(), 1);
        assert_eq!(second_data.height_maps[0].name, "second-only");
    }

    fn test_keymap(stop: &str) -> Keymap {
        let mut keymap = Keymap::new();
        keymap.insert(KeyBinding::new("jog.x.minus", "ctrl-shift-left"));
        keymap.insert(KeyBinding::new("run.stop", stop));
        keymap
    }

    #[test]
    fn keymap_round_trips_through_the_profile() {
        let dir = TempDir::new("keymap");
        let store = ConfigStore::new(&dir.path);
        let info = info();
        let key = ProfileKey::from_controller(&info);
        let mut model = ConfigModel::from_store(store.clone());
        model.on_controller(&info).expect("profile");
        let keymap = test_keymap("f8");
        let path = model.save_keymap(keymap.clone()).expect("save keymap");
        assert!(path.exists());
        assert_eq!(model.data().keymap, keymap);
        assert_eq!(
            store
                .load_controller_data(&key)
                .expect("load")
                .expect("data")
                .keymap,
            keymap
        );

        let mut reopened = ConfigModel::from_store(store);
        reopened.on_controller(&info).expect("reload");
        assert_eq!(reopened.data().keymap, keymap);
    }

    #[test]
    fn keymaps_are_isolated_per_profile() {
        let dir = TempDir::new("keymap-profiles");
        let store = ConfigStore::new(&dir.path);
        let first = info();
        let mut second = info();
        second.board = Some("OtherBoard".into());

        let mut first_model = ConfigModel::from_store(store.clone());
        first_model.on_controller(&first).expect("first");
        let first_map = test_keymap("f8");
        first_model
            .save_keymap(first_map.clone())
            .expect("first keymap");

        let mut second_model = ConfigModel::from_store(store.clone());
        second_model.on_controller(&second).expect("second");
        assert!(
            second_model.data().keymap.bindings.is_empty(),
            "a new controller profile starts with no stored keymap"
        );
        let second_map = test_keymap("f12");
        second_model
            .save_keymap(second_map.clone())
            .expect("second keymap");

        let mut first_reopened = ConfigModel::from_store(store.clone());
        first_reopened.on_controller(&first).expect("first reload");
        assert_eq!(first_reopened.data().keymap, first_map);
        let mut second_reopened = ConfigModel::from_store(store);
        second_reopened
            .on_controller(&second)
            .expect("second reload");
        assert_eq!(second_reopened.data().keymap, second_map);
    }

    #[test]
    fn keymap_save_requires_a_verified_profile_and_never_overwrites_corruption() {
        let dir = TempDir::new("keymap-gates");
        let store = ConfigStore::new(&dir.path);

        let mut no_profile = ConfigModel::from_store(store.clone());
        let error = no_profile
            .save_keymap(test_keymap("f8"))
            .expect_err("no profile");
        assert!(error.contains("no controller profile"), "{error}");
        assert!(no_profile.keymap_save_blocker().is_some());

        let info = info();
        let key = ProfileKey::from_controller(&info);
        let path = store.profile_path(&key);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let corrupt = b"{ not json";
        fs::write(&path, corrupt).expect("write");
        let mut corrupt_model = ConfigModel::from_store(store.clone());
        corrupt_model.on_controller(&info).expect("notice");
        let error = corrupt_model
            .save_keymap(test_keymap("f8"))
            .expect_err("corrupt profile refused");
        assert!(error.contains("corrupt"), "{error}");
        assert_eq!(fs::read(&path).expect("read"), corrupt);

        let dir = TempDir::new("keymap-unverified");
        let store = ConfigStore::new(&dir.path);
        let mut anonymous = ControllerInfo::new();
        anonymous.version = "1.1f".into();
        let key = ProfileKey::from_controller(&anonymous);
        assert!(!key.is_identified());
        store
            .save_controller_data(&key, &ControllerData::new())
            .expect("save");
        let mut unverified = ConfigModel::from_store(store);
        assert!(
            unverified
                .on_controller(&anonymous)
                .expect("notice")
                .unverified
        );
        let error = unverified
            .save_keymap(test_keymap("f8"))
            .expect_err("unverified profile refused");
        assert!(
            error.contains("unverified") || error.contains("not verified"),
            "{error}"
        );
    }

    #[test]
    fn saving_a_keymap_preserves_tools_and_offsets() {
        let dir = TempDir::new("keymap-preserve");
        let store = ConfigStore::new(&dir.path);
        let info = info();
        let key = ProfileKey::from_controller(&info);
        let mut model = ConfigModel::from_store(store.clone());
        model.on_controller(&info).expect("profile");
        let mut table = ToolTable::new();
        table.insert(tool(7, "Seven"));
        model
            .save_profile(&table, &offsets(), BTreeMap::new())
            .expect("profile save");
        model.save_keymap(test_keymap("f10")).expect("keymap save");

        let stored = store
            .load_controller_data(&key)
            .expect("load")
            .expect("data");
        assert_eq!(stored.tools, table);
        assert_eq!(stored.offsets, offsets());
        assert_eq!(stored.keymap, test_keymap("f10"));
    }
}
