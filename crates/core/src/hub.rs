//! Application facade: owns settings, logs, catalogues and the cached community metadata.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;

use chrono::{DateTime, Utc};

use crate::catalogs::Catalogs;
use crate::github::{self, CheckFailure, ReleaseInfo};
use crate::launch::{self, LaunchError};
use crate::logs::Logs;
use crate::metadata::{self, HttpState, MetadataCache, MetadataMod};
use crate::mods::{self, InstalledMod, ModsError};
use crate::network::{self, Fetch, Network, NetworkError};
use crate::nexus::{Nexus, NexusAccount, NexusDownload, NexusError};
use crate::paths::{self, PathError};
use crate::platform;
use crate::settings::{Settings, SettingsStore};
use crate::source_choice::{ChoiceError, SourceChoices};
use crate::sources::{self, Repo, SourceKind, SourceManifest};
use crate::steam;

#[derive(Debug)]
pub enum HubError {
    NoGameFolder,
    Path(PathError),
    Mods(ModsError),
    Launch(LaunchError),
    MissingMetadataLink,
    UnsafeMetadataLink,
    OpenLink(io::Error),
    Settings(io::Error),
    Network(NetworkError),
    Cache(metadata::MetadataCacheError),
    Nexus(NexusError),
    Json(serde_json::Error),
    UnexpectedContent,
    NetworkUnavailable,
    Source(ChoiceError),
    Sources(sources::LoadError),
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
            Self::Launch(error) => error.fmt(formatter),
            Self::MissingMetadataLink => {
                write!(formatter, "This catalogue entry has no Nexus page link.")
            }
            Self::UnsafeMetadataLink => write!(
                formatter,
                "The catalogue link is not an HTTPS URL, so it was not opened."
            ),
            Self::OpenLink(error) => write!(formatter, "Could not open the Nexus page: {error}"),
            Self::Settings(error) => write!(formatter, "Could not save settings: {error}"),
            Self::Network(error) => error.fmt(formatter),
            Self::Cache(error) => error.fmt(formatter),
            Self::Nexus(error) => error.fmt(formatter),
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
            Self::Source(error) => error.fmt(formatter),
            Self::Sources(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for HubError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoGameFolder | Self::UnexpectedContent | Self::NetworkUnavailable => None,
            Self::Source(error) => Some(error),
            Self::Sources(error) => Some(error),
            Self::Path(error) => Some(error),
            Self::Mods(error) => Some(error),
            Self::Launch(error) => Some(error),
            Self::MissingMetadataLink | Self::UnsafeMetadataLink => None,
            Self::OpenLink(error) => Some(error),
            Self::Settings(error) => Some(error),
            Self::Network(error) => Some(error),
            Self::Cache(error) => Some(error),
            Self::Nexus(error) => Some(error),
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

impl From<LaunchError> for HubError {
    fn from(error: LaunchError) -> Self {
        Self::Launch(error)
    }
}

impl From<NetworkError> for HubError {
    fn from(error: NetworkError) -> Self {
        Self::Network(error)
    }
}

impl From<NexusError> for HubError {
    fn from(error: NexusError) -> Self {
        Self::Nexus(error)
    }
}

impl From<metadata::MetadataCacheError> for HubError {
    fn from(error: metadata::MetadataCacheError) -> Self {
        Self::Cache(error)
    }
}

impl From<ChoiceError> for HubError {
    fn from(error: ChoiceError) -> Self {
        Self::Source(error)
    }
}

impl From<sources::LoadError> for HubError {
    fn from(error: sources::LoadError) -> Self {
        Self::Sources(error)
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

#[derive(Debug, Clone)]
pub enum CheckOutcome {
    Release(ReleaseInfo),
    Failure(CheckFailure),
}

const CHECK_MAX_BODY_BYTES: u64 = 1024 * 1024;

const GITHUB_HEADERS: [(&str, &str); 2] = [
    ("Accept", "application/vnd.github+json"),
    ("X-GitHub-Api-Version", "2022-11-28"),
];

pub struct Hub {
    app_data: PathBuf,
    settings_store: SettingsStore,
    settings: Settings,
    logs: Logs,
    catalogs: Catalogs,
    metadata_cache: MetadataCache,
    metadata: Vec<MetadataMod>,
    network: Option<Network>,
    nexus: Option<Nexus>,
    sources: Option<SourceManifest>,
    source_choices: SourceChoices,
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

        let nexus = network
            .as_ref()
            .map(|network| Nexus::new(network.clone(), app_data.clone()));

        let sources = match sources::load_manifest_optional(&sources::manifest_path(&app_data)) {
            Ok(manifest) => manifest,
            Err(error) => {
                logs.error("Sources", &io::Error::other(error.to_string()));
                None
            }
        };
        let source_choices = match SourceChoices::load(&app_data) {
            Ok(choices) => choices,
            Err(error) => {
                logs.error("Sources", &io::Error::other(error.to_string()));
                SourceChoices::empty(&app_data)
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
            nexus,
            sources,
            source_choices,
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

    pub fn launch_game(&self) -> Result<String, HubError> {
        let game = self.game_folder().ok_or(HubError::NoGameFolder)?;
        let app_id = launch::launch_game(&game)?;
        self.logs.activity(
            "Game",
            &format!("Asked Steam to launch Casualties Unknown (app {app_id})."),
        );
        Ok(app_id)
    }

    pub fn nexus_has_key(&self) -> bool {
        self.nexus.as_ref().is_some_and(|nexus| nexus.has_key())
    }

    pub fn nexus(&self) -> Result<&Nexus, HubError> {
        self.nexus.as_ref().ok_or(HubError::NetworkUnavailable)
    }

    pub fn save_nexus_key(&mut self, key: &str) -> Result<(), HubError> {
        let nexus = self.nexus.as_ref().ok_or(HubError::NetworkUnavailable)?;
        nexus.save_key(key)?;
        self.logs.activity("Nexus", "Personal Nexus API key saved.");
        Ok(())
    }

    pub fn clear_nexus_key(&mut self) -> Result<(), HubError> {
        let nexus = self.nexus.as_ref().ok_or(HubError::NetworkUnavailable)?;
        nexus.clear_key()?;
        self.logs
            .activity("Nexus", "Personal Nexus API key cleared.");
        Ok(())
    }

    pub fn validate_nexus_key(&self) -> Result<NexusAccount, HubError> {
        Ok(self.nexus()?.validate_key()?)
    }

    pub fn download_nexus_latest(&self, mod_id: &str) -> Result<NexusDownload, HubError> {
        let destination = if self.settings.download_path.trim().is_empty() {
            platform::downloads_dir()
        } else {
            PathBuf::from(self.settings.download_path.trim())
        };
        let download = self.nexus()?.download_latest(mod_id, &destination)?;
        self.logs.activity(
            "Nexus",
            &format!(
                "Downloaded {} for mod {mod_id} to {}.",
                download.file_name,
                download.path.display()
            ),
        );
        Ok(download)
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

    pub fn metadata_needs_refresh(&self) -> bool {
        self.metadata.is_empty() || self.metadata_cache.is_stale()
    }

    pub fn open_nexus_download_page(&self, mod_id: &str) -> Result<(), HubError> {
        let url = self
            .metadata
            .iter()
            .find(|entry| entry.id == mod_id)
            .and_then(|entry| entry.nexus_download_page_url())
            .ok_or(HubError::MissingMetadataLink)?;
        if !network::is_https_url(&url) {
            return Err(HubError::UnsafeMetadataLink);
        }
        let mut child = platform::open_url(&url).map_err(HubError::OpenLink)?;
        thread::spawn(move || {
            let _ = child.wait();
        });
        self.logs.activity(
            "Nexus",
            &format!("Opened the Nexus download page for mod {mod_id}."),
        );
        Ok(())
    }

    pub fn refresh_metadata(&mut self, force: bool) -> Result<MetadataRefresh, HubError> {
        let network = self.network.as_ref().ok_or(HubError::NetworkUnavailable)?;
        let state = if force {
            HttpState::default()
        } else {
            self.metadata_cache.load_state()
        };
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

    pub fn sources(&self) -> Option<&SourceManifest> {
        self.sources.as_ref()
    }

    pub fn source_choice(&self, key: &str) -> Option<SourceKind> {
        self.source_choices.choice(key)
    }

    pub fn pin_source(&mut self, key: &str, kind: SourceKind) -> Result<(), HubError> {
        self.source_choices.set(key, kind)?;
        self.logs
            .activity("Sources", &format!("Source choice saved for {key}."));
        Ok(())
    }

    pub fn clear_source_pin(&mut self, key: &str) -> Result<(), HubError> {
        self.source_choices.clear(key)?;
        self.logs
            .activity("Sources", &format!("Source choice cleared for {key}."));
        Ok(())
    }

    pub fn check_release(&self, repo: &Repo) -> Result<CheckOutcome, HubError> {
        let network = self.network.as_ref().ok_or(HubError::NetworkUnavailable)?;
        let latest = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            repo.owner(),
            repo.name()
        );
        match fetch_release(network, &latest) {
            Ok(release) => Ok(CheckOutcome::Release(release)),
            Err(CheckFailure::Missing) => {
                let list = format!(
                    "https://api.github.com/repos/{}/{}/releases?per_page=1",
                    repo.owner(),
                    repo.name()
                );
                match fetch_release_list(network, &list) {
                    Ok(release) => Ok(CheckOutcome::Release(release)),
                    Err(failure) => Ok(CheckOutcome::Failure(failure)),
                }
            }
            Err(failure) => Ok(CheckOutcome::Failure(failure)),
        }
    }

    pub fn log_entries(&self) -> Vec<String> {
        self.logs.entries()
    }

    pub fn logs(&self) -> &Logs {
        &self.logs
    }
}

fn fetch_release(network: &Network, url: &str) -> Result<ReleaseInfo, CheckFailure> {
    let body = fetch_json(network, url)?;
    github::parse_release(&body).map_err(|error| CheckFailure::Unreadable {
        detail: error.to_string(),
    })
}

fn fetch_release_list(network: &Network, url: &str) -> Result<ReleaseInfo, CheckFailure> {
    let body = fetch_json(network, url)?;
    match github::parse_release_list(&body) {
        Ok(release) => Ok(release),
        Err(github::ReleaseError::NoReleases) => Err(CheckFailure::Missing),
        Err(error) => Err(CheckFailure::Unreadable {
            detail: error.to_string(),
        }),
    }
}

fn fetch_json(network: &Network, url: &str) -> Result<String, CheckFailure> {
    match network.get_with_headers(url, &GITHUB_HEADERS, CHECK_MAX_BODY_BYTES) {
        Ok(Fetch::Fetched { body, .. }) => {
            String::from_utf8(body).map_err(|error| CheckFailure::Unreadable {
                detail: error.to_string(),
            })
        }
        Ok(Fetch::NotModified) => Err(CheckFailure::Unreadable {
            detail: String::from("GitHub answered with an unexpected 304."),
        }),
        Err(error) => Err(check_failure(&error)),
    }
}

fn check_failure(error: &NetworkError) -> CheckFailure {
    match error {
        NetworkError::HttpStatus { status, .. } => CheckFailure::from_status(*status, None),
        other => CheckFailure::Network {
            detail: other.to_string(),
        },
    }
}
