//! Zoom-aware pixel primitives and the app's hub-backed state.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use cuhub_core::github::{CheckFailure, ReleaseInfo};
use cuhub_core::hub::{CheckOutcome, Hub, HubError, MetadataRefresh};
use cuhub_core::metadata::MetadataMod;
use cuhub_core::mods::{InstalledMod, ModStatus};
use cuhub_core::settings::Settings;
use cuhub_core::source_choice::mod_key;
use cuhub_core::sources::{self, LocalMod, MatchKind, Repo, SourceEntry, SourceKind};
use cuhub_core::version::HubVersion;
use nsh_core::RevealOptions;
use nsh_core::element::FontAsset;
use nsh_core::prelude::*;

use crate::palette::*;

pub const PIXEL_FONT: FontAsset = asset!("fonts/fusion-pixel-12px-proportional-zh_hans.otf");

pub const PAGES: [&str; 6] = [
    "Nexus Dashboard",
    "Local Mods",
    "Multiplayer",
    "Skins & Backups",
    "Hub Home",
    "Settings",
];

pub const NEXUS_PAGE_SIZE: usize = 50;

#[derive(Clone, PartialEq, Eq)]
pub struct NexusModRow {
    pub id: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub downloads: String,
    pub adult: bool,
    pub has_page: bool,
    pub can_download: bool,
    pub status: Option<ModStatus>,
    pub installed_version: Option<String>,
}

impl NexusModRow {
    pub fn row_key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.id,
            self.name,
            self.author,
            self.version,
            self.downloads,
            self.state_label(),
            self.installed_version.as_deref().unwrap_or(""),
            self.adult,
            self.has_page
        )
    }

    pub fn state_label(&self) -> &'static str {
        self.status.map(ModStatus::label).unwrap_or("Not installed")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SourceRow {
    pub text: String,
    pub warning: bool,
    pub can_check: bool,
    pub repo: Option<Repo>,
    pub options: Vec<SourceKind>,
    pub pinned: Option<SourceKind>,
}

impl SourceRow {
    pub fn can_pin(&self) -> bool {
        self.options.len() > 1 || self.pinned.is_some()
    }

    pub fn pin_label(&self) -> &'static str {
        match self.pinned {
            None => "Pin: auto",
            Some(SourceKind::Github) => "Pin: GitHub",
            Some(SourceKind::Nexus) => "Pin: Nexus",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct LocalModRow {
    pub key: String,
    pub source_key: String,
    pub item: InstalledMod,
    pub source: SourceRow,
}

impl LocalModRow {
    pub fn row_key(&self) -> String {
        self.key.clone()
    }
}

#[derive(Clone, PartialEq)]
pub enum CheckState {
    Checking,
    Release(ReleaseInfo),
    Failed(CheckFailure),
}

#[derive(Clone)]
pub struct Ui {
    pub page: Signal<usize>,
    pub zoom: Signal<u8>,
    pub dev: Signal<bool>,
    pub taps: Signal<u32>,
    pub last_tap: Signal<Option<Instant>>,
    pub notice: Signal<String>,
    pub hub: Arc<Mutex<Hub>>,
    pub settings: Signal<Settings>,
    pub mods: Signal<Vec<InstalledMod>>,
    pub nexus_rows: Signal<Vec<NexusModRow>>,
    pub nexus_page_rows: Signal<Vec<NexusModRow>>,
    pub nexus_page: Signal<usize>,
    pub nexus_total: Signal<usize>,
    pub nexus_refreshing: Signal<bool>,
    pub nexus_error: Signal<String>,
    pub local_rows: Signal<Vec<LocalModRow>>,
    pub checks: Signal<BTreeMap<String, CheckState>>,
    pub nexus_downloading: Signal<Option<String>>,
    pub nexus_list: ElementRef,
    pub metadata_synced: Signal<String>,
    pub nexus_key_saved: Signal<bool>,
    pub nexus_key_draft: Signal<String>,
    pub nexus_key_check: Signal<String>,
    pub app_data: String,
}

impl Ui {
    pub fn new() -> Self {
        let mut hub = Hub::open();
        if hub.settings().game_path.trim().is_empty() {
            hub.detect_game_folder();
        }
        let settings = hub.settings().clone();
        let zoom = settings.ui_scale;
        let mods = hub.scan_mods().unwrap_or_default();
        let checks = BTreeMap::new();
        let local_rows = build_local_rows(&hub, &mods, &checks);
        let nexus_rows = build_nexus_rows(hub.metadata(), &mods);
        let nexus_total = nexus_rows.len();
        let nexus_page_rows = page_slice(&nexus_rows, 0);
        let metadata_synced = hub
            .metadata_last_synced_label()
            .unwrap_or_else(|| String::from("Never"));
        let nexus_key_saved = hub.nexus_has_key();
        let nexus_key_check = if nexus_key_saved {
            String::from("Saved; not checked.")
        } else {
            String::from("No key saved.")
        };
        let app_data = hub.app_data().display().to_string();
        let notice = if settings.game_path.trim().is_empty() {
            String::from("No game folder set. Open Settings to detect it.")
        } else {
            format!("{} mod(s) found in Plugins.", mods.len())
        };
        Self {
            page: signal(0usize),
            zoom: signal(zoom),
            dev: signal(false),
            taps: signal(0u32),
            last_tap: signal(None),
            notice: signal(notice),
            hub: Arc::new(Mutex::new(hub)),
            settings: signal(settings),
            mods: signal(mods),
            nexus_rows: signal(nexus_rows),
            nexus_page_rows: signal(nexus_page_rows),
            nexus_page: signal(0usize),
            nexus_total: signal(nexus_total),
            nexus_refreshing: signal(false),
            nexus_error: signal(String::new()),
            local_rows: signal(local_rows),
            checks: signal(checks),
            nexus_downloading: signal(None::<String>),
            nexus_list: element_ref(),
            metadata_synced: signal(metadata_synced),
            nexus_key_saved: signal(nexus_key_saved),
            nexus_key_draft: signal(String::new()),
            nexus_key_check: signal(nexus_key_check),
            app_data,
        }
    }

    fn hub(&self) -> MutexGuard<'_, Hub> {
        self.hub
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn start_background_tasks(&self) {
        let needs_refresh = {
            let hub = self.hub();
            hub.metadata_needs_refresh()
        };
        if needs_refresh {
            self.refresh_metadata(false);
        }
    }

    pub fn refresh_mods(&self) {
        let (result, metadata) = {
            let hub = self.hub();
            (hub.scan_mods(), hub.metadata().to_vec())
        };
        match result {
            Ok(mods) => {
                let count = mods.len();
                self.mods.set(mods);
                self.rebuild_nexus(&metadata);
                self.rebuild_local_rows();
                self.notice
                    .set(format!("Scanned {count} mod(s) in Plugins."));
            }
            Err(error) => self.notice.set(error.to_string()),
        }
    }

    fn rebuild_nexus(&self, metadata: &[MetadataMod]) {
        let rows = build_nexus_rows(metadata, &self.mods.get());
        let total = rows.len();
        let page = self.nexus_page.get();
        self.nexus_rows.set(rows);
        self.nexus_total.set(total);
        self.set_nexus_page(page);
    }

    pub fn set_nexus_page(&self, page: usize) {
        let rows = self.nexus_rows.get();
        let previous = self.nexus_page.get();
        let page = page.min(nexus_page_count(rows.len()) - 1);
        self.nexus_page.set(page);
        self.nexus_page_rows.set(page_slice(&rows, page));
        if previous != page {
            self.nexus_list
                .reveal(RevealOptions::default().animated(false));
        }
    }

    pub fn refresh_metadata(&self, force: bool) {
        if self.nexus_refreshing.get() {
            return;
        }
        self.nexus_refreshing.set(true);
        self.nexus_error.set(String::new());
        let hub = self.hub.clone();
        let ui = self.clone();
        spawn(async move {
            let result = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let refresh = hub.refresh_metadata(force)?;
                let metadata = hub.metadata().to_vec();
                let mods = hub.scan_mods().ok();
                let synced = hub.metadata_last_synced_label();
                Ok::<_, HubError>((refresh, metadata, mods, synced))
            })
            .await;
            match result {
                Ok(Ok((refresh, metadata, mods, synced))) => {
                    if let Some(mods) = mods {
                        ui.mods.set(mods);
                    }
                    ui.rebuild_nexus(&metadata);
                    ui.rebuild_local_rows();
                    ui.metadata_synced
                        .set(synced.unwrap_or_else(|| String::from("Never")));
                    ui.nexus_refreshing.set(false);
                    match refresh {
                        MetadataRefresh::Updated(count) => ui
                            .notice
                            .set(format!("Community catalogue refreshed: {count} entries.")),
                        MetadataRefresh::NotModified => ui
                            .notice
                            .set(String::from("The community catalogue is already current.")),
                    }
                }
                Ok(Err(error)) => {
                    ui.nexus_error.set(error.to_string());
                    ui.nexus_refreshing.set(false);
                    ui.notice.set(format!(
                        "Could not refresh the community catalogue: {error}"
                    ));
                }
                Err(error) => {
                    ui.nexus_error
                        .set(String::from("The catalogue worker stopped."));
                    ui.nexus_refreshing.set(false);
                    ui.notice.set(format!(
                        "Could not refresh the community catalogue: {error}"
                    ));
                }
            }
        });
    }

    pub fn download_nexus(&self, mod_id: String) {
        if self.nexus_downloading.get().is_some() {
            self.notice
                .set(String::from("A Nexus download is already running."));
            return;
        }
        if mod_id.trim().is_empty() {
            self.notice
                .set(String::from("This catalogue entry has no Nexus mod id."));
            return;
        }
        self.nexus_downloading.set(Some(mod_id.clone()));
        self.notice
            .set(String::from("Downloading the latest Nexus file…"));
        let hub = self.hub.clone();
        let ui = self.clone();
        spawn(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.download_nexus_latest(&mod_id)
            })
            .await;
            ui.nexus_downloading.set(None);
            match result {
                Ok(Ok(download)) => ui.notice.set(format!(
                    "Downloaded {} to {}.",
                    download.file_name,
                    download.path.display()
                )),
                Ok(Err(error)) => ui
                    .notice
                    .set(format!("Could not download from Nexus: {error}")),
                Err(error) => ui
                    .notice
                    .set(format!("Could not download from Nexus: {error}")),
            }
        });
    }

    pub fn rebuild_local_rows(&self) {
        let rows = {
            let hub = self.hub();
            build_local_rows(&hub, &self.mods.get(), &self.checks.get())
        };
        self.local_rows.set(rows);
    }

    pub fn check_source(&self, key: String, repo: Repo) {
        let mut checks = self.checks.get();
        if checks.get(&key) == Some(&CheckState::Checking) {
            return;
        }
        checks.insert(key.clone(), CheckState::Checking);
        self.checks.set(checks);
        self.rebuild_local_rows();
        self.notice
            .set(format!("Checking GitHub for {}…", repo.slug()));
        let hub = self.hub.clone();
        let ui = self.clone();
        spawn(async move {
            let target = repo.clone();
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.check_release(&target)
            })
            .await;
            let mut checks = ui.checks.get();
            match result {
                Ok(Ok(CheckOutcome::Release(release))) => {
                    ui.notice.set(release_notice(&repo, &release));
                    checks.insert(key.clone(), CheckState::Release(release));
                }
                Ok(Ok(CheckOutcome::Failure(failure))) => {
                    ui.notice.set(failure_notice(&failure));
                    checks.insert(key.clone(), CheckState::Failed(failure));
                }
                Ok(Err(error)) => {
                    ui.notice.set(error.to_string());
                    checks.insert(
                        key.clone(),
                        CheckState::Failed(CheckFailure::Network {
                            detail: error.to_string(),
                        }),
                    );
                }
                Err(error) => {
                    ui.notice.set(format!("The GitHub check stopped: {error}"));
                    checks.insert(
                        key.clone(),
                        CheckState::Failed(CheckFailure::Network {
                            detail: error.to_string(),
                        }),
                    );
                }
            }
            ui.checks.set(checks);
            ui.rebuild_local_rows();
        });
    }

    pub fn cycle_source_pin(&self, key: String, options: Vec<SourceKind>) {
        let current = {
            let hub = self.hub();
            hub.source_choice(&key)
        };
        let next = match current {
            None => options.first().copied(),
            Some(kind) => options
                .iter()
                .position(|option| *option == kind)
                .and_then(|index| options.get(index + 1).copied()),
        };
        self.set_source_pin(key, next);
    }

    pub fn set_source_pin(&self, key: String, kind: Option<SourceKind>) {
        let result = {
            let mut hub = self.hub();
            match kind {
                Some(kind) => hub.pin_source(&key, kind),
                None => hub.clear_source_pin(&key),
            }
        };
        match result {
            Ok(()) => {
                self.rebuild_local_rows();
                self.notice.set(match kind {
                    Some(kind) => format!("Pinned the source to {}.", source_kind_label(kind)),
                    None => String::from("Source choice set to automatic."),
                });
            }
            Err(error) => self
                .notice
                .set(format!("Could not save the source choice: {error}")),
        }
    }

    pub fn open_nexus_download(&self, mod_id: String) {
        let result = {
            let hub = self.hub();
            hub.open_nexus_download_page(&mod_id)
        };
        match result {
            Ok(()) => self.notice.set(String::from(
                "Opened the Nexus download page in the browser.",
            )),
            Err(error) => self
                .notice
                .set(format!("Could not open the Nexus page: {error}")),
        }
    }

    pub fn show_mod_details(&self, name: &str) {
        self.notice
            .set(format!("Mod details for {name} are not implemented yet."));
    }

    pub fn launch_game(&self) {
        let result = {
            let hub = self.hub();
            hub.launch_game()
        };
        match result {
            Ok(app_id) => self.notice.set(format!(
                "Asked Steam to launch Casualties Unknown (app {app_id})."
            )),
            Err(error) => self.notice.set(format!("Could not launch: {error}")),
        }
    }

    pub fn detect_game(&self) {
        let found = {
            let mut hub = self.hub();
            hub.detect_game_folder()
        };
        match found {
            Some(path) => {
                let settings = {
                    let hub = self.hub();
                    hub.settings().clone()
                };
                self.settings.set(settings);
                self.notice
                    .set(format!("Game folder set to {}.", path.display()));
                self.refresh_mods();
            }
            None => self.notice.set(String::from(
                "Casualties Unknown was not found in the Steam libraries.",
            )),
        }
    }

    pub fn save_nexus_key(&self) {
        let key = self.nexus_key_draft.get();
        if key.trim().is_empty() {
            self.notice
                .set(String::from("Paste a Nexus API key first."));
            return;
        }
        let result = {
            let mut hub = self.hub();
            hub.save_nexus_key(&key)
        };
        match result {
            Ok(()) => {
                self.nexus_key_saved.set(true);
                self.nexus_key_draft.set(String::new());
                self.notice
                    .set(String::from("Nexus API key saved on this machine."));
            }
            Err(error) => self
                .notice
                .set(format!("Could not save the Nexus API key: {error}")),
        }
    }

    pub fn validate_nexus_key(&self) {
        if !self.nexus_key_saved.get() {
            self.nexus_key_check.set(String::from("No key saved."));
            self.notice.set(String::from("Save a Nexus API key first."));
            return;
        }
        self.nexus_key_check
            .set(String::from("Checking with Nexus…"));
        let hub = self.hub.clone();
        let saved = self.nexus_key_saved;
        let check = self.nexus_key_check;
        let notice = self.notice;
        spawn(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.validate_nexus_key()
            })
            .await;
            match result {
                Ok(Ok(account)) => {
                    saved.set(true);
                    if account.is_premium {
                        check.set(String::from("Nexus account: Premium."));
                        notice.set(String::from("Nexus accepted the key: Premium account."));
                    } else {
                        check.set(String::from(
                            "Nexus account: Standard (direct downloads need Premium).",
                        ));
                        notice.set(String::from(
                            "Nexus accepted the key, but this account is not Premium.",
                        ));
                    }
                }
                Ok(Err(error)) => {
                    check.set(format!("Nexus check failed: {error}"));
                    notice.set(format!("Could not check the Nexus key: {error}"));
                }
                Err(error) => {
                    check.set(String::from("Nexus check did not finish."));
                    notice.set(format!("Could not check the Nexus key: {error}"));
                }
            }
        });
    }

    pub fn clear_nexus_key(&self) {
        let result = {
            let mut hub = self.hub();
            hub.clear_nexus_key()
        };
        match result {
            Ok(()) => {
                self.nexus_key_saved.set(false);
                self.nexus_key_draft.set(String::new());
                self.notice.set(String::from("Nexus API key cleared."));
            }
            Err(error) => self
                .notice
                .set(format!("Could not clear the Nexus API key: {error}")),
        }
    }

    pub fn set_ui_scale(&self, level: u8) {
        let result = {
            let mut hub = self.hub();
            let mut settings = hub.settings().clone();
            if settings.ui_scale == level {
                Ok(settings)
            } else {
                settings.ui_scale = level;
                match hub.save_settings(&settings) {
                    Ok(()) => Ok(hub.settings().clone()),
                    Err(error) => Err(error.to_string()),
                }
            }
        };
        match result {
            Ok(settings) => {
                self.zoom.set(settings.ui_scale);
                self.settings.set(settings);
                self.notice.set(format!("Interface scale set to {level}x."));
            }
            Err(error) => self.notice.set(format!("Could not save settings: {error}")),
        }
    }
}

struct SourceInput<'a> {
    entries: &'a [SourceEntry],
    item: &'a InstalledMod,
    check: Option<&'a CheckState>,
    pinned: Option<SourceKind>,
}

fn build_local_rows(
    hub: &Hub,
    mods: &[InstalledMod],
    checks: &BTreeMap<String, CheckState>,
) -> Vec<LocalModRow> {
    let entries = hub
        .sources()
        .map(|manifest| manifest.entries.as_slice())
        .unwrap_or(&[]);
    mods.iter()
        .map(|item| {
            let key = source_key(item);
            let check = checks.get(&key);
            let pinned = hub.source_choice(&key);
            let source = build_source_row(SourceInput {
                entries,
                item,
                check,
                pinned,
            });
            LocalModRow {
                key: format!("{key}|{}|{}", check_tag(check), pin_tag(pinned)),
                source_key: key,
                item: item.clone(),
                source,
            }
        })
        .collect()
}

fn build_source_row(input: SourceInput<'_>) -> SourceRow {
    let stems = dll_stems(input.item);
    let local = LocalMod {
        guid: input.item.guid.as_deref(),
        dll_stems: stems,
        name: &input.item.name,
    };
    let github = match input.check {
        Some(CheckState::Release(release)) => Some(release.version.clone()),
        _ => None,
    };
    let nexus = input
        .item
        .expected_version
        .as_deref()
        .and_then(HubVersion::parse);
    let resolution = sources::resolve_mod(input.entries, &local, github, nexus, input.pinned);
    let Some(matched) = resolution.matched else {
        let text = match input.pinned {
            Some(kind) => format!(
                "Sources: not listed in the GitHub manifest. Pinned to {}.",
                source_kind_label(kind)
            ),
            None => String::from("Sources: not listed in the GitHub manifest."),
        };
        return SourceRow {
            text,
            warning: false,
            can_check: false,
            repo: None,
            options: Vec::new(),
            pinned: input.pinned,
        };
    };
    let repo = &matched.entry.repo;
    let mut parts = vec![
        format!("GitHub {}", repo.slug()),
        String::from(match_label(matched.kind)),
    ];
    match input.check {
        None => parts.push(String::from("not checked")),
        Some(CheckState::Checking) => parts.push(String::from("checking…")),
        Some(CheckState::Release(release)) => {
            parts.push(release.tag.clone());
            if release.prerelease {
                parts.push(String::from("prerelease only"));
            }
        }
        Some(CheckState::Failed(failure)) => parts.push(failure.summary()),
    }
    if let Some(effective) = &resolution.effective {
        parts.push(format!(
            "uses {}",
            source_label(effective.kind, input.check, input.item)
        ));
    }
    if resolution.pinned.is_some() {
        parts.push(String::from("pinned"));
    }
    let warning = matched.kind == MatchKind::Name
        || matches!(input.check, Some(CheckState::Release(release)) if release.prerelease);
    let options: Vec<SourceKind> = resolution
        .options
        .iter()
        .map(|option| option.kind)
        .collect();
    SourceRow {
        text: parts.join(" · "),
        warning,
        can_check: !matches!(input.check, Some(CheckState::Checking)),
        repo: Some(repo.clone()),
        options,
        pinned: resolution.pinned,
    }
}

fn source_key(item: &InstalledMod) -> String {
    let stems = dll_stems(item);
    mod_key(item.guid.as_deref(), &stems)
        .unwrap_or_else(|| format!("name:{}", item.name.to_lowercase()))
}

fn dll_stems(item: &InstalledMod) -> Vec<&str> {
    item.dll_paths
        .iter()
        .filter_map(|path| path.file_stem().and_then(|stem| stem.to_str()))
        .collect()
}

fn check_tag(check: Option<&CheckState>) -> String {
    match check {
        None => String::from("none"),
        Some(CheckState::Checking) => String::from("checking"),
        Some(CheckState::Release(release)) => format!("release:{}", release.tag),
        Some(CheckState::Failed(failure)) => format!("failed:{}", failure.summary()),
    }
}

fn pin_tag(pinned: Option<SourceKind>) -> &'static str {
    match pinned {
        None => "auto",
        Some(SourceKind::Github) => "github",
        Some(SourceKind::Nexus) => "nexus",
    }
}

fn source_kind_label(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Github => "GitHub",
        SourceKind::Nexus => "Nexus",
    }
}

fn source_label(kind: SourceKind, check: Option<&CheckState>, item: &InstalledMod) -> String {
    match kind {
        SourceKind::Github => match check {
            Some(CheckState::Release(release)) => format!("GitHub {}", release.tag),
            _ => String::from("GitHub (not checked)"),
        },
        SourceKind::Nexus => item
            .expected_version
            .clone()
            .map(|version| format!("Nexus {version}"))
            .unwrap_or_else(|| String::from("Nexus")),
    }
}

fn match_label(kind: MatchKind) -> &'static str {
    match kind {
        MatchKind::Guid => "GUID match",
        MatchKind::Dll => "DLL match",
        MatchKind::Name => "name match",
    }
}

fn release_notice(repo: &Repo, release: &ReleaseInfo) -> String {
    if release.prerelease {
        format!(
            "{} only has a prerelease: {}. Review it before updating.",
            repo.slug(),
            release.tag
        )
    } else {
        format!("{} has release {}.", repo.slug(), release.tag)
    }
}

fn failure_notice(failure: &CheckFailure) -> String {
    match failure.detail() {
        Some(detail) => format!("{} {detail}", failure.summary()),
        None => failure.summary(),
    }
}

fn build_nexus_rows(metadata: &[MetadataMod], mods: &[InstalledMod]) -> Vec<NexusModRow> {
    metadata
        .iter()
        .map(|entry| {
            let installed = mods
                .iter()
                .find(|installed| installed.metadata_id.as_deref() == Some(entry.id.as_str()));
            NexusModRow {
                id: entry.id.clone(),
                name: if entry.name.trim().is_empty() {
                    String::from("Unknown mod")
                } else {
                    entry.name.clone()
                },
                author: if entry.author.trim().is_empty() {
                    String::from("Unknown author")
                } else {
                    entry.author.clone()
                },
                version: if entry.version.trim().is_empty() {
                    String::from("Unknown")
                } else {
                    entry.version.clone()
                },
                downloads: entry.total_downloads_label(),
                adult: entry.is_adult_content(),
                has_page: entry.nexus_download_page_url().is_some(),
                can_download: !entry.id.trim().is_empty(),
                status: installed.map(|installed| installed.status),
                installed_version: installed
                    .and_then(|installed| installed.installed_version.clone()),
            }
        })
        .collect()
}

fn page_slice(rows: &[NexusModRow], page: usize) -> Vec<NexusModRow> {
    let start = page * NEXUS_PAGE_SIZE;
    if start >= rows.len() {
        return Vec::new();
    }
    let end = (start + NEXUS_PAGE_SIZE).min(rows.len());
    rows[start..end].to_vec()
}

pub fn nexus_page_count(total: usize) -> usize {
    total.div_ceil(NEXUS_PAGE_SIZE).max(1)
}

pub fn scaled(zoom: &Signal<u8>, base: f32) -> impl Fn() -> Points + Clone + use<> {
    let zoom = *zoom;
    move || px(base * zoom.get() as f32)
}

pub fn pix(s: impl IntoReactive<SharedString>, base: f32, color: u32, zoom: &Signal<u8>) -> Text {
    pix_c(s, base, rgb(color), zoom)
}

pub fn pix_c(
    s: impl IntoReactive<SharedString>,
    base: f32,
    color: impl IntoReactive<AdaptiveColor>,
    zoom: &Signal<u8>,
) -> Text {
    text(s)
        .text_size(scaled(zoom, base))
        .line_height(scaled(zoom, base * 4.0 / 3.0))
        .font_family(PIXEL_FONT.family)
        .text_color(color)
}

pub fn panel(zoom: &Signal<u8>, title: &'static str, body: impl IntoElement) -> View {
    let z = *zoom;
    view()
        .flex_col()
        .bg(rgb(SURFACE))
        .border_l(scaled(&z, 1.0))
        .border_r(scaled(&z, 1.0))
        .border_b(scaled(&z, 1.0))
        .border_color(rgb(BORDER))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_center()
                .h(scaled(&z, 1.0))
                .child(view().flex_1().h(scaled(&z, 1.0)).bg(rgb(BORDER)))
                .child(
                    view()
                        .px(scaled(&z, 8.0))
                        .bg(rgb(SURFACE))
                        .child(pix(title, 12.0, TEXT, &z)),
                )
                .child(view().flex_1().h(scaled(&z, 1.0)).bg(rgb(BORDER))),
        )
        .child(
            view()
                .flex_col()
                .flex_1()
                .pt(scaled(&z, 9.0))
                .px(scaled(&z, 2.0))
                .pb(scaled(&z, 2.0))
                .child(
                    view()
                        .flex_col()
                        .flex_1()
                        .p(scaled(&z, 8.0))
                        .border(scaled(&z, 1.0))
                        .border_color(rgb(BORDER))
                        .child(body),
                ),
        )
}

pub fn button(
    zoom: &Signal<u8>,
    label: &'static str,
    enabled: bool,
    on_press: impl Fn() + 'static,
) -> Text {
    let z = *zoom;
    let mut button = pix(label, 12.0, if enabled { TEXT } else { TEXT_DIM }, &z)
        .px(scaled(&z, 10.0))
        .py(scaled(&z, 4.0))
        .bg(rgb(SURFACE_RAISED))
        .border(scaled(&z, 1.0))
        .border_color(if enabled {
            rgb(BORDER)
        } else {
            rgb(BORDER_DIM)
        });
    if enabled {
        button = button
            .hover(|s| s.bg(rgb(TEXT)).text_color(rgb(BG)))
            .active(|s| s.bg(rgb(ACCENT_PRESS)).text_color(rgb(TEXT)))
            .on_press(on_press);
    }
    button
}
