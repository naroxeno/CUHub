//! Settings model, tolerant loading and atomic saving. No UI state.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const SETTINGS_FILE: &str = "Settings.json";
const DEFAULT_BACKUP_DIR: &str = "Backups";
const DEFAULT_STORE_DIR: &str = "Store";
const DEFAULT_PROTECTED_DIR: &str = "ProtectedFiles";

pub const DEFAULT_UI_SCALE: u8 = 2;
const MIN_UI_SCALE: u8 = 1;
const MAX_UI_SCALE: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub game_path: String,
    pub download_path: String,
    pub backup_path: String,
    pub store_path: String,
    pub protected_files_path: String,
    pub disable_auto_delete_imported_parent_files: bool,
    pub ui_scale: u8,
    pub language: String,
    pub high_contrast_mode: bool,
    pub easter_eggs_enabled: bool,
    pub easter_eggs_preference_initialized: bool,
    pub local_mods_share_column_collapsed: bool,
    pub local_mods_share_column_visible: bool,
    pub dashboard_list_view: bool,
    pub dashboard_show_adult_mods: bool,
    pub ignored_dependency_names: Vec<String>,
    pub home_order: Vec<String>,
    pub setup_completed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            game_path: String::new(),
            download_path: crate::platform::downloads_dir()
                .to_string_lossy()
                .into_owned(),
            backup_path: DEFAULT_BACKUP_DIR.to_owned(),
            store_path: String::new(),
            protected_files_path: DEFAULT_PROTECTED_DIR.to_owned(),
            disable_auto_delete_imported_parent_files: false,
            ui_scale: DEFAULT_UI_SCALE,
            language: "system".to_owned(),
            high_contrast_mode: false,
            easter_eggs_enabled: false,
            easter_eggs_preference_initialized: false,
            local_mods_share_column_collapsed: false,
            local_mods_share_column_visible: true,
            dashboard_list_view: false,
            dashboard_show_adult_mods: false,
            ignored_dependency_names: Vec::new(),
            home_order: Vec::new(),
            setup_completed: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSettings {
    pub settings: Settings,
    pub created: bool,
    pub warning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    file: PathBuf,
}

impl SettingsStore {
    pub fn new(app_data: impl Into<PathBuf>) -> Self {
        let file = app_data.into().join(SETTINGS_FILE);
        Self { file }
    }

    pub fn path(&self) -> &Path {
        &self.file
    }

    pub fn load(&self) -> LoadedSettings {
        match fs::read_to_string(&self.file) {
            Ok(raw) => match serde_json::from_str::<Settings>(&raw) {
                Ok(mut settings) => {
                    let warning = settings.clamp_ui_scale().map(|requested| {
                        format!(
                            "uiScale {requested} is outside {MIN_UI_SCALE}..{MAX_UI_SCALE}, using {}",
                            settings.ui_scale
                        )
                    });
                    LoadedSettings {
                        settings,
                        created: false,
                        warning,
                    }
                }
                Err(error) => LoadedSettings {
                    settings: Settings::default(),
                    created: false,
                    warning: Some(format!("Could not read {}: {error}", self.file.display())),
                },
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let settings = Settings::default();
                match self.save(&settings) {
                    Ok(()) => LoadedSettings {
                        settings,
                        created: true,
                        warning: None,
                    },
                    Err(error) => LoadedSettings {
                        settings,
                        created: false,
                        warning: Some(format!("Could not create {}: {error}", self.file.display())),
                    },
                }
            }
            Err(error) => LoadedSettings {
                settings: Settings::default(),
                created: false,
                warning: Some(format!("Could not read {}: {error}", self.file.display())),
            },
        }
    }

    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        let mut settings = settings.clone();
        settings.clamp_ui_scale();
        let json = serde_json::to_string_pretty(&settings).map_err(io::Error::other)?;
        crate::atomic::replace(&self.file, format!("{json}\n").as_bytes())
    }
}

impl Settings {
    pub fn clamp_ui_scale(&mut self) -> Option<u8> {
        let requested = self.ui_scale;
        self.ui_scale = requested.clamp(MIN_UI_SCALE, MAX_UI_SCALE);
        (self.ui_scale != requested).then_some(requested)
    }

    pub fn store_dir(&self, app_data: &Path) -> PathBuf {
        let configured = self.store_path.trim();
        if configured.is_empty() {
            app_data.join(DEFAULT_STORE_DIR)
        } else {
            PathBuf::from(configured)
        }
    }

    pub fn setup_due(&self) -> bool {
        !self.setup_completed && self.game_path.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    #[test]
    fn missing_file_is_created_with_defaults() {
        let dir = temp_dir("settings-missing");
        let store = SettingsStore::new(&dir);
        let loaded = store.load();
        assert!(loaded.created);
        assert!(loaded.warning.is_none());
        assert_eq!(loaded.settings, Settings::default());
        assert!(store.path().is_file());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn parse_error_is_reported_and_file_kept() {
        let dir = temp_dir("settings-broken");
        let store = SettingsStore::new(&dir);
        fs::write(store.path(), "{not json").unwrap();
        let loaded = store.load();
        assert!(!loaded.created);
        assert!(loaded.warning.is_some());
        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(fs::read_to_string(store.path()).unwrap(), "{not json");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn save_round_trips_with_camel_case_keys() {
        let dir = temp_dir("settings-roundtrip");
        let store = SettingsStore::new(&dir);
        let settings = Settings {
            game_path: "/games/Casualties Unknown".to_owned(),
            ui_scale: 3,
            language: "zh-CN".to_owned(),
            ignored_dependency_names: vec!["SomeDep".to_owned()],
            home_order: vec!["mods".to_owned(), "game".to_owned()],
            setup_completed: true,
            ..Settings::default()
        };
        store.save(&settings).unwrap();
        let raw = fs::read_to_string(store.path()).unwrap();
        assert!(raw.contains("gamePath"));
        assert!(raw.contains("ignoredDependencyNames"));
        assert!(raw.contains("homeOrder"));
        assert!(!raw.contains("home_order"));
        assert!(raw.contains("\"language\": \"zh-CN\""));
        assert!(raw.contains("\"setupCompleted\": true"));
        assert!(!raw.contains("game_path"));
        assert!(raw.ends_with('\n'));
        let loaded = store.load();
        assert_eq!(loaded.settings, settings);
        assert!(loaded.warning.is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unknown_and_legacy_keys_are_ignored() {
        let dir = temp_dir("settings-legacy");
        let store = SettingsStore::new(&dir);
        fs::write(
            store.path(),
            r#"{"gamePath":"/g","ThemeColoursInitialized":true,"textSize":14}"#,
        )
        .unwrap();
        let loaded = store.load();
        assert_eq!(loaded.settings.game_path, "/g");
        assert!(loaded.warning.is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ui_scale_is_clamped_on_load() {
        let dir = temp_dir("settings-clamp");
        let store = SettingsStore::new(&dir);
        fs::write(store.path(), r#"{"uiScale":9}"#).unwrap();
        let loaded = store.load();
        assert_eq!(loaded.settings.ui_scale, MAX_UI_SCALE);
        assert!(loaded.warning.is_some());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn setup_is_due_without_a_game_folder_or_completion() {
        let mut settings = Settings::default();
        assert!(settings.setup_due());
        settings.game_path = "/games/Casualties Unknown".to_owned();
        assert!(!settings.setup_due());
        settings.game_path.clear();
        settings.setup_completed = true;
        assert!(!settings.setup_due());
    }

    #[test]
    fn legacy_json_without_setup_completed_starts_setup() {
        let dir = temp_dir("settings-setup-legacy");
        let store = SettingsStore::new(&dir);
        fs::write(store.path(), r#"{"gamePath":""}"#).unwrap();
        let loaded = store.load();
        assert!(!loaded.settings.setup_completed);
        assert!(loaded.settings.setup_due());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn default_store_lives_under_app_data() {
        let dir = temp_dir("settings-store-default");
        let settings = Settings::default();
        assert_eq!(settings.store_dir(&dir), dir.join("Store"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn configured_store_wins_and_blank_falls_back() {
        let dir = temp_dir("settings-store-custom");
        let mut settings = Settings::default();
        settings.store_path = "  D:/Mods/Store  ".to_owned();
        assert_eq!(settings.store_dir(&dir), PathBuf::from("D:/Mods/Store"));
        settings.store_path = "   ".to_owned();
        assert_eq!(settings.store_dir(&dir), dir.join("Store"));
        let _ = fs::remove_dir_all(dir);
    }
}
