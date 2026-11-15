//! Application facade: owns settings, logs, catalogues and the cached community metadata.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::catalogs::Catalogs;
use crate::github::{self, CheckFailure, RateLimit, ReleaseAsset, ReleaseInfo};
use crate::install::{self, UpgradePlan};
use crate::launch::{self, LaunchError};
use crate::logs::Logs;
use crate::metadata::{self, HttpState, MetadataCache, MetadataMod};
use crate::mods::{self, InstalledMod, ModsError};
use crate::network::{self, Fetch, Network, NetworkError};
use crate::nexus::{Nexus, NexusAccount, NexusDownload, NexusError};
use crate::nexus_auth::{GithubTokenStore, KeyStoreError};
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
    Credentials(KeyStoreError),
    MissingGithubToken,
    GithubTokenRejected,
    GithubResponse(String),
    Json(serde_json::Error),
    UnexpectedContent,
    NetworkUnavailable,
    Source(ChoiceError),
    Sources(sources::LoadError),
    Install(String),
    Plan(String),
    Apply(String),
    GameRunning,
    Github(String),
    Update(String),
    Io(io::Error),
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
            Self::Credentials(error) => error.fmt(formatter),
            Self::MissingGithubToken => formatter.write_str(
                "No GitHub token is saved. Add one in Settings to raise the GitHub API rate limit.",
            ),
            Self::GithubTokenRejected => {
                formatter.write_str("GitHub rejected the token. Save a valid one in Settings.")
            }
            Self::GithubResponse(message) => {
                write!(
                    formatter,
                    "GitHub returned an unexpected response: {message}"
                )
            }
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
            Self::Install(message) => write!(formatter, "Could not unpack the update: {message}"),
            Self::Plan(message) => write!(formatter, "The update cannot be installed: {message}"),
            Self::Apply(message) => write!(formatter, "The update failed: {message}"),
            Self::GameRunning => {
                write!(
                    formatter,
                    "Close Casualties Unknown before changing mod files."
                )
            }
            Self::Github(message) => write!(formatter, "GitHub update failed: {message}"),
            Self::Update(message) => write!(formatter, "Could not prepare the update: {message}"),
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
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
            Self::Credentials(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Install(_)
            | Self::Plan(_)
            | Self::Apply(_)
            | Self::GameRunning
            | Self::Github(_)
            | Self::Update(_)
            | Self::MissingGithubToken
            | Self::GithubTokenRejected
            | Self::GithubResponse(_) => None,
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

impl From<KeyStoreError> for HubError {
    fn from(error: KeyStoreError) -> Self {
        Self::Credentials(error)
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
    github_tokens: GithubTokenStore,
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

        let github_tokens = GithubTokenStore::new(app_data.clone());

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
            github_tokens,
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

    pub fn github_token_saved(&self) -> bool {
        self.github_tokens.is_configured()
    }

    pub fn save_github_token(&mut self, token: &str) -> Result<(), HubError> {
        self.github_tokens.save(token)?;
        self.logs.activity("GitHub", "Personal GitHub token saved.");
        Ok(())
    }

    pub fn clear_github_token(&mut self) -> Result<(), HubError> {
        self.github_tokens.clear()?;
        self.logs
            .activity("GitHub", "Personal GitHub token cleared.");
        Ok(())
    }

    pub fn validate_github_token(&self) -> Result<RateLimit, HubError> {
        let network = self.network.as_ref().ok_or(HubError::NetworkUnavailable)?;
        let token = self
            .github_tokens
            .load()
            .ok_or(HubError::MissingGithubToken)?;
        let authorization = format!("Bearer {token}");
        let headers = [
            ("Accept", "application/vnd.github+json"),
            ("X-GitHub-Api-Version", "2022-11-28"),
            ("Authorization", authorization.as_str()),
        ];
        let body = match network.get_with_headers(
            "https://api.github.com/rate_limit",
            &headers,
            CHECK_MAX_BODY_BYTES,
        ) {
            Ok(Fetch::Fetched { body, .. }) => body,
            Ok(Fetch::NotModified) => {
                return Err(HubError::GithubResponse(String::from(
                    "the rate endpoint answered 304",
                )));
            }
            Err(NetworkError::HttpStatus { status: 401, .. }) => {
                return Err(HubError::GithubTokenRejected);
            }
            Err(error) => return Err(HubError::Network(error)),
        };
        let text =
            String::from_utf8(body).map_err(|error| HubError::GithubResponse(error.to_string()))?;
        let limit = github::parse_rate_limit(&text)
            .map_err(|error| HubError::GithubResponse(error.to_string()))?;
        self.logs.activity(
            "GitHub",
            &format!(
                "Token accepted; {} of {} requests left.",
                limit.remaining, limit.limit
            ),
        );
        Ok(limit)
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

    pub fn set_mod_enabled(&self, item: &InstalledMod, enabled: bool) -> Result<usize, HubError> {
        let plugins = self.plugins_dir()?;
        let changed = mods::set_enabled(&plugins, item, enabled)?;
        self.logs.activity(
            "Mods",
            &format!(
                "{} {} ({} DLL file(s), {}).",
                if enabled { "Enabled" } else { "Disabled" },
                item.name,
                changed,
                item.source_path.display()
            ),
        );
        Ok(changed)
    }

    pub fn set_all_mods_enabled(&self, enabled: bool) -> Result<usize, HubError> {
        let plugins = self.plugins_dir()?;
        let changed = mods::set_all_enabled(&plugins, enabled)?;
        self.logs.activity(
            "Mods",
            &format!(
                "{} {changed} DLL file(s) in {}.",
                if enabled { "Enabled" } else { "Disabled" },
                plugins.display()
            ),
        );
        Ok(changed)
    }

    pub fn delete_scope(&self, item: &InstalledMod) -> Result<mods::DeleteScope, HubError> {
        let plugins = self.plugins_dir()?;
        Ok(mods::delete_scope(&plugins, item)?)
    }

    pub fn delete_mod(&self, item: &InstalledMod) -> Result<usize, HubError> {
        self.ensure_game_closed()?;
        let plugins = self.plugins_dir()?;
        let removed = mods::delete_mod(&plugins, item)?;
        self.logs.activity(
            "Mods",
            &format!(
                "Deleted {} ({} file(s), {}).",
                item.name,
                removed,
                item.source_path.display()
            ),
        );
        Ok(removed)
    }

    pub fn delete_all_scope(&self) -> Result<mods::DeleteScope, HubError> {
        let plugins = self.plugins_dir()?;
        Ok(mods::delete_all_scope(&plugins)?)
    }

    pub fn delete_all_mods(&self) -> Result<usize, HubError> {
        self.ensure_game_closed()?;
        let plugins = self.plugins_dir()?;
        let removed = mods::delete_all(&plugins)?;
        self.logs.activity(
            "Mods",
            &format!("Deleted {removed} file(s) from {}.", plugins.display()),
        );
        Ok(removed)
    }

    fn ensure_game_closed(&self) -> Result<(), HubError> {
        if self.game_running()? {
            return Err(HubError::GameRunning);
        }
        Ok(())
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
        let token = self.github_tokens.load();
        let token = token.as_deref();
        match fetch_release(network, token, &latest) {
            Ok(release) => Ok(CheckOutcome::Release(release)),
            Err(CheckFailure::Missing) => {
                let list = format!(
                    "https://api.github.com/repos/{}/{}/releases?per_page=1",
                    repo.owner(),
                    repo.name()
                );
                match fetch_release_list(network, token, &list) {
                    Ok(release) => Ok(CheckOutcome::Release(release)),
                    Err(failure) => Ok(CheckOutcome::Failure(failure)),
                }
            }
            Err(failure) => Ok(CheckOutcome::Failure(failure)),
        }
    }

    pub fn game_running(&self) -> Result<bool, HubError> {
        let game = self.game_folder().ok_or(HubError::NoGameFolder)?;
        install::game_running(&game).map_err(HubError::Io)
    }

    pub fn backup_root(&self) -> PathBuf {
        let configured = self.settings.backup_path.trim();
        let path = if configured.is_empty() {
            PathBuf::from("Backups")
        } else {
            PathBuf::from(configured)
        };
        if path.is_absolute() {
            path
        } else {
            self.app_data.join(path)
        }
    }

    pub fn staging_root(&self) -> PathBuf {
        let stamp = Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or_default()
            .to_string();
        self.app_data
            .join("Staging")
            .join(format!("{stamp}-{}", std::process::id()))
    }

    pub fn plan_download(
        &self,
        download: &Path,
        installed: &InstalledMod,
        staging: &Path,
    ) -> Result<UpgradePlan, HubError> {
        let plugins = self.plugins_dir()?;
        let root = if installed.source_path.is_dir() {
            installed.source_path.clone()
        } else {
            installed
                .source_path
                .parent()
                .ok_or_else(|| HubError::Install(String::from("mod path has no parent")))?
                .to_path_buf()
        };
        let staged = if install::is_dll_file(download) {
            install::stage_dll(download, staging)
        } else {
            install::stage_archive(download, staging)
        };
        let staged = match staged {
            Ok(staged) => staged,
            Err(error) => {
                let _ = fs::remove_dir_all(staging);
                return Err(HubError::Install(error.to_string()));
            }
        };
        let target = install::PlanTarget {
            root: &root,
            plugins: &plugins,
            owned_root: &installed.source_path,
            owned_files: &installed.dll_paths,
        };
        match install::plan_payload(&staged, target) {
            Ok(plan) => Ok(plan),
            Err(error) => {
                let _ = fs::remove_dir_all(&staged.staging_root);
                Err(HubError::Plan(error.to_string()))
            }
        }
    }

    pub fn discard_staging(&self, staging: &Path) {
        if let Err(error) = fs::remove_dir_all(staging)
            && error.kind() != io::ErrorKind::NotFound
        {
            self.logs.error("Install", &error);
        }
    }

    pub fn apply_install(&self, plan: &UpgradePlan) -> Result<install::ApplyReport, HubError> {
        self.ensure_game_closed()?;
        let report = install::apply_upgrade(plan, &self.backup_root())
            .map_err(|error| HubError::Apply(error.to_string()))?;
        self.logs.activity(
            "Install",
            &format!(
                "Installed {} file(s); backup at {}.",
                report.replaced.len() + report.created.len(),
                report
                    .backup
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| String::from("none"))
            ),
        );
        Ok(report)
    }

    pub fn download_github_asset(
        &self,
        repo: &Repo,
        requested: Option<&str>,
        destination: &Path,
    ) -> Result<PathBuf, HubError> {
        let release = match self.check_release(repo)? {
            CheckOutcome::Release(release) => release,
            CheckOutcome::Failure(failure) => return Err(HubError::Github(failure.summary())),
        };
        let asset = select_github_asset(&release.assets, requested).ok_or_else(|| {
            HubError::Github(match requested {
                Some(name) => format!("release has no asset named {name}"),
                None => String::from("release has no zip, 7z, rar or DLL asset"),
            })
        })?;
        let name = safe_asset_name(&asset.name)
            .ok_or_else(|| HubError::Github(String::from("release asset has an unsafe name")))?;
        fs::create_dir_all(destination).map_err(HubError::Io)?;
        let target = unique_destination(destination, &name);
        let network = self.network.as_ref().ok_or(HubError::NetworkUnavailable)?;
        network
            .download_with_timeout(
                &asset.download_url,
                &target,
                install::MAX_ARCHIVE_BYTES,
                Duration::from_secs(300),
            )
            .map_err(HubError::Network)?;
        self.logs.activity(
            "GitHub",
            &format!("Downloaded {} to {}.", asset.name, target.display()),
        );
        Ok(target)
    }

    pub fn log_entries(&self) -> Vec<String> {
        self.logs.entries()
    }

    pub fn logs(&self) -> &Logs {
        &self.logs
    }
}

fn select_github_asset<'a>(
    assets: &'a [ReleaseAsset],
    requested: Option<&str>,
) -> Option<&'a ReleaseAsset> {
    if let Some(name) = requested {
        return assets
            .iter()
            .find(|asset| asset.name.eq_ignore_ascii_case(name));
    }
    assets.iter().find(|asset| is_installable_name(&asset.name))
}

fn is_installable_name(name: &str) -> bool {
    matches!(
        Path::new(name)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("zip" | "7z" | "rar" | "dll")
    )
}

fn safe_asset_name(name: &str) -> Option<String> {
    if name.is_empty()
        || name.chars().count() > 200
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.chars().any(char::is_control)
    {
        return None;
    }
    let file_name = Path::new(name).file_name()?.to_str()?;
    if file_name.is_empty() {
        None
    } else {
        Some(file_name.to_string())
    }
}

fn unique_destination(directory: &Path, name: &str) -> PathBuf {
    let candidate = directory.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("update");
    let extension = Path::new(name).extension().and_then(|value| value.to_str());
    for index in 1..1000 {
        let candidate_name = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        let candidate = directory.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{stem}-{}.archive", std::process::id()))
}

fn fetch_release(
    network: &Network,
    token: Option<&str>,
    url: &str,
) -> Result<ReleaseInfo, CheckFailure> {
    let body = fetch_json(network, token, url)?;
    github::parse_release(&body).map_err(|error| CheckFailure::Unreadable {
        detail: error.to_string(),
    })
}

fn fetch_release_list(
    network: &Network,
    token: Option<&str>,
    url: &str,
) -> Result<ReleaseInfo, CheckFailure> {
    let body = fetch_json(network, token, url)?;
    match github::parse_release_list(&body) {
        Ok(release) => Ok(release),
        Err(github::ReleaseError::NoReleases) => Err(CheckFailure::Missing),
        Err(error) => Err(CheckFailure::Unreadable {
            detail: error.to_string(),
        }),
    }
}

fn fetch_json(network: &Network, token: Option<&str>, url: &str) -> Result<String, CheckFailure> {
    let authorization = token.map(|token| format!("Bearer {token}"));
    let mut headers = GITHUB_HEADERS.to_vec();
    if let Some(value) = authorization.as_deref() {
        headers.push(("Authorization", value));
    }
    match network.get_with_headers(url, &headers, CHECK_MAX_BODY_BYTES) {
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
