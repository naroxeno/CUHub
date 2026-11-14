//! Application facade: owns settings, logs, catalogues and the cached community metadata.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::catalogs::Catalogs;
use crate::logs::Logs;
use crate::metadata::{self, HttpState, MetadataCache, MetadataMod};
use crate::mods::{self, InstalledMod, ModsError};
use crate::network::{self, Fetch, Network, NetworkError};
use crate::paths::{self, PathError};
use crate::platform;
use crate::settings::{Settings, SettingsStore};
use crate::steam;

#[derive(Debug)]
pub enum HubError {
    NoGameFolder,
    Path(PathError),
    Mods(ModsError),
    Settings(io::Error),
    Network(NetworkError),
    Cache(metadata::MetadataCacheError),
    Json(serde_json::Error),
    UnexpectedContent,
    NetworkUnavailable,
}

impl std::fmt::Display for HubError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoGameFolder => write!(
                formatter,
                "Choose the Casualties Unknown game folder first."
            ),
            Self::Path(error) => error.fmt(formatter),
            Self::Mods(error) => error.fmt(formatter),
            Self::Settings(error) => write!(formatter, "Could not save settings: {error}"),
            Self::Network(error) => error.fmt(formatter),
            Self::Cache(error) => error.fmt(formatter),
            Self::Json(error) => write!(
                formatter,
                "The community catalogue is not valid JSON: {error}"
            ),
            Self::UnexpectedContent => {
                write!(formatter, "The community catalogue is not valid UTF-8.")
            }
            Self::NetworkUnavailable => {
                write!(formatter, "The network client could not be created.")
            }
        }
    }
}

impl std::error::Error for HubError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoGameFolder | Self::UnexpectedContent | Self::NetworkUnavailable => None,
            Self::Path(error) => Some(error),
            Self::Mods(error) => Some(error),
            Self::Settings(error) => Some(error),
            Self::Network(error) => Some(error),
            Self::Cache(error) => Some(error),
            Self::Json(error) => Some(error),
        }
    }
}

impl From<PathError> for HubError {
    fn from(error: PathError) -> Self {
        Self::Path(error)
    }
}

impl From<ModsError> for HubError {
    fn from(error: ModsError) -> Self {
        Self::Mods(error)
    }
}

impl From<NetworkError> for HubError {
    fn from(error: NetworkError) -> Self {
        Self::Network(error)
    }
}

impl From<metadata::MetadataCacheError> for HubError {
    fn from(error: metadata::MetadataCacheError) -> Self {
        Self::Cache(error)
    }
}

impl From<serde_json::Error> for HubError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataRefresh {
    NotModified,
    Updated(usize),
}

pub struct Hub {
    app_data: PathBuf,
    settings_store: SettingsStore,
    settings: Settings,
    logs: Logs,
    catalogs: Catalogs,
    metadata_cache: MetadataCache,
    metadata: Vec<MetadataMod>,
    network: Option<Network>,
}

impl Hub {
    pub fn open() -> Self {
        let app_data = platform::app_data_root();
        let _ = fs::create_dir_all(&app_data);
        let settings_store = SettingsStore::new(app_data.clone());
        let loaded = settings_store.load();
        let logs = Logs::start(&app_data);
        if let Some(warning) = &loaded.warning {
            logs.error("Settings", &io::Error::other(warning.clone()));
        }
        logs.info(&format!("CUHub {} started.", env!("CARGO_PKG_VERSION")));

        let catalogs = Catalogs::load(&app_data);
        for error in &catalogs.errors {
            logs.error("Catalog", error);
        }

        let metadata_cache = MetadataCache::new(&app_data);
        let metadata = metadata_cache.load_cached().unwrap_or_default();
        if !metadata.is_empty() {
            logs.info(&format!(
                "Loaded {} cached catalogue entries.",
                metadata.len()
            ));
        }

        let network = match Network::new() {
            Ok(network) => Some(network),
            Err(error) => {
                logs.error("Network", &error);
                None
            }
        };

        Self {
            app_data,
            settings_store,
            settings: loaded.settings,
            logs,
            catalogs,
            metadata_cache,
            metadata,
            network,
        }
    }

    pub fn app_data(&self) -> &Path {
        &self.app_data
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn settings_path(&self) -> &Path {
        self.settings_store.path()
    }

    pub fn save_settings(&mut self, settings: &Settings) -> io::Result<()> {
        let mut normalized = settings.clone();
        normalized.clamp_ui_scale();
        self.settings_store.save(&normalized)?;
        self.settings = normalized;
        self.logs.activity("Settings", "Settings saved.");
        Ok(())
    }

    pub fn game_folder(&self) -> Option<PathBuf> {
        let path = self.settings.game_path.trim();
        (!path.is_empty()).then(|| PathBuf::from(path))
    }

    pub fn set_game_folder(&mut self, selected: &Path) -> Result<PathBuf, HubError> {
        let root = paths::game_paths(selected)?;
        let mut settings = self.settings.clone();
        settings.game_path = root.to_string_lossy().into_owned();
        self.save_settings(&settings).map_err(HubError::Settings)?;
        self.logs
            .activity("Game", &format!("Game folder set to {}.", root.display()));
        Ok(root)
    }

    pub fn detect_game_folder(&mut self) -> Option<PathBuf> {
        let Some(root) = steam::find_game_install() else {
            self.logs
                .info("No Casualties Unknown install was found in the Steam libraries.");
            return None;
        };
        match self.set_game_folder(&root) {
            Ok(path) => Some(path),
            Err(error) => {
                self.logs.error("Game", &error);
                None
            }
        }
    }

    pub fn plugins_dir(&self) -> Result<PathBuf, HubError> {
        let game = self.game_folder().ok_or(HubError::NoGameFolder)?;
        let root = paths::game_paths(&game)?;
        Ok(mods::plugins_dir(&root)?)
    }

    pub fn scan_mods(&self) -> Result<Vec<InstalledMod>, HubError> {
        let plugins = self.plugins_dir()?;
        let mods = mods::scan(
            &plugins,
            &self.metadata,
            &self.catalogs,
            &self.settings.ignored_dependency_names,
        )?;
        self.logs.activity(
            "Mods",
            &format!("Scanned {} mod(s) in {}.", mods.len(), plugins.display()),
        );
        Ok(mods)
    }

    pub fn metadata(&self) -> &[MetadataMod] {
        &self.metadata
    }

    pub fn metadata_last_synced(&self) -> Option<DateTime<Utc>> {
        fs::metadata(self.metadata_cache.cache_path())
            .ok()?
            .modified()
            .ok()
            .map(DateTime::<Utc>::from)
    }

    pub fn metadata_last_synced_label(&self) -> Option<String> {
        self.metadata_last_synced()
            .map(|synced| synced.format("%Y-%m-%d %H:%M").to_string())
    }

    pub fn refresh_metadata(&mut self) -> Result<MetadataRefresh, HubError> {
        let network = self.network.as_ref().ok_or(HubError::NetworkUnavailable)?;
        let state = self.metadata_cache.load_state();
        let fetch = network.get(
            metadata::METADATA_URL,
            state.etag.as_deref(),
            state.last_modified.as_deref(),
            network::DEFAULT_MAX_BODY_BYTES,
        )?;
        match fetch {
            Fetch::NotModified => {
                self.logs
                    .activity("Metadata", "The community catalogue is already current.");
                Ok(MetadataRefresh::NotModified)
            }
            Fetch::Fetched {
                body,
                etag,
                last_modified,
            } => {
                let raw = String::from_utf8(body).map_err(|_| HubError::UnexpectedContent)?;
                let mods = metadata::parse_mods(&raw)?;
                let count = mods.len();
                self.metadata_cache.store(
                    &raw,
                    &HttpState {
                        etag,
                        last_modified,
                    },
                )?;
                self.metadata = mods;
                self.logs.activity(
                    "Metadata",
                    &format!("Community catalogue refreshed with {count} entries."),
                );
                Ok(MetadataRefresh::Updated(count))
            }
        }
    }

    pub fn log_entries(&self) -> Vec<String> {
        self.logs.entries()
    }

    pub fn logs(&self) -> &Logs {
        &self.logs
    }
}
