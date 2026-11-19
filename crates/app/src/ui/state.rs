//! Hub-backed UI state, its constructor and page refresh entry points.

use super::delete::DeletePrompt;
use super::install::{DependencyPrompt, PendingInstall};
use super::multiplayer::MultiplayerUi;
use super::profiles::ProfilesUi;
use super::remote::{RemoteFilter, RemoteRow, build_remote_rows};
use super::rows::CheckState;
use super::rows::LocalModRow;
use super::rows::NexusModRow;
use super::rows::build_local_rows;
use super::rows::build_nexus_rows;
use super::rows::nexus_page_count;
use super::rows::page_slice;
use super::tasks::DownloadTrack;
use crate::i18n;
use crate::setup::Setup;
use crate::shell::nav;
use cuhub_core::hub::{BepInExInstall, Hub};
use cuhub_core::install::{Flavor, Status as BepInExStatus};
use cuhub_core::metadata::MetadataMod;
use cuhub_core::mods::InstalledMod;
use cuhub_core::settings::Settings;
use nsh_core::RevealOptions;
use nsh_core::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::future::Future;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use super::batch::BatchProgress;

const STORE_PROBE_FILE: &str = ".cuhub-write-check";

#[derive(Clone)]
pub struct Ui {
    pub page: Signal<&'static str>,
    pub nav_collapsed: Signal<bool>,
    pub nav_closed_group: Signal<&'static str>,
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
    pub remote_rows: Signal<Vec<RemoteRow>>,
    pub remote_all: Signal<Vec<RemoteRow>>,
    pub remote_page: Signal<usize>,
    pub remote_total: Signal<usize>,
    pub remote_query: Signal<String>,
    pub remote_filter: Signal<RemoteFilter>,
    pub sources_ready: Signal<bool>,
    pub sources_refreshing: Signal<bool>,
    pub announcement: Signal<Option<String>>,
    pub remote_busy: Signal<Option<String>>,
    pub pending_install: Signal<Option<PendingInstall>>,
    pub dependency_prompt: Signal<Option<DependencyPrompt>>,
    pub update_busy: Signal<Option<String>>,
    pub toggle_busy: Signal<Option<String>>,
    pub bulk_busy: Signal<bool>,
    pub delete_prompt: Signal<Option<DeletePrompt>>,
    pub delete_busy: Signal<bool>,
    pub apply_running: Signal<bool>,
    pub apply_cancel: Signal<bool>,
    pub apply_status: Signal<String>,
    pub task_open: Signal<bool>,
    pub downloads: Signal<Vec<DownloadTrack>>,
    pub batch_selection: Signal<BTreeSet<String>>,
    pub batch_progress: Signal<Option<BatchProgress>>,
    pub bepinex: Signal<Option<BepInExStatus>>,
    pub bepinex_flavor: Signal<Flavor>,
    pub bepinex_busy: Signal<bool>,
    pub bepinex_pending: Signal<Option<BepInExInstall>>,
    pub bepinex_confirm: Signal<bool>,
    pub bepinex_status_text: Signal<String>,
    pub nexus_downloading: Signal<Option<String>>,
    pub nexus_list: ElementRef,
    pub metadata_synced: Signal<String>,
    pub nexus_key_saved: Signal<bool>,
    pub nexus_key_draft: Signal<String>,
    pub nexus_key_check: Signal<String>,
    pub github_token_saved: Signal<bool>,
    pub github_token_draft: Signal<String>,
    pub github_token_check: Signal<String>,
    pub store_draft: Signal<String>,
    pub store_error: Signal<String>,
    pub mp: MultiplayerUi,
    pub profiles: ProfilesUi,
    pub app_data: String,
    pub setup: Setup,
    owner: Owner,
}

impl Ui {
    pub fn new() -> Self {
        let mut hub = Hub::open();
        let setup_due = hub.settings().setup_due();
        if !setup_due && hub.settings().game_path.trim().is_empty() {
            hub.detect_game_folder();
        }
        let settings = hub.settings().clone();
        i18n::set_language(&settings.language);
        let zoom = settings.ui_scale;
        let mods = hub.scan_mods().unwrap_or_default();
        let checks = BTreeMap::new();
        let local_rows = build_local_rows(&hub, &mods, &checks);
        let nexus_rows = build_nexus_rows(hub.metadata(), &mods);
        let nexus_total = nexus_rows.len();
        let nexus_page_rows = page_slice(&nexus_rows, 0);
        let remote_all = build_remote_rows(&hub, &mods);
        let remote_total = remote_all.len();
        let remote_rows = page_slice(&remote_all, 0);
        let metadata_synced = hub
            .metadata_last_synced_label()
            .unwrap_or_else(|| t!("common.never"));
        let nexus_key_saved = hub.nexus_has_key();
        let nexus_key_check = if nexus_key_saved {
            t!("notice.saved_not_checked")
        } else {
            t!("notice.no_key_saved")
        };
        let github_token_saved = hub.github_token_saved();
        let github_token_check = if github_token_saved {
            t!("notice.saved_not_checked")
        } else {
            t!("notice.no_token_saved")
        };
        let has_sources = hub.sources().is_some();
        let announcement = hub
            .hub_config_cached()
            .config
            .current_announcement
            .map(|item| item.message);
        let app_data = hub.app_data().display().to_string();
        let store_draft = signal(settings.store_path.clone());
        let store_error = signal(String::new());
        let bepinex = hub.bepinex_status().ok();
        let owner = Owner::current();
        let setup = Setup::new(&settings, setup_due);
        let notice = if settings.game_path.trim().is_empty() {
            t!("notice.no_game_folder")
        } else {
            t!("notice.mods_found", count = mods.len())
        };
        Self {
            page: signal(nav::HOME),
            zoom: signal(zoom),
            dev: signal(false),
            taps: signal(0u32),
            last_tap: signal(None),
            notice: signal(notice),
            mp: MultiplayerUi::new(&hub),
            profiles: ProfilesUi::new(),
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
            nav_collapsed: signal(false),
            nav_closed_group: signal(""),
            remote_rows: signal(remote_rows),
            remote_all: signal(remote_all),
            remote_page: signal(0usize),
            remote_total: signal(remote_total),
            remote_query: signal(String::new()),
            remote_filter: signal(RemoteFilter::All),
            sources_ready: signal(has_sources),
            sources_refreshing: signal(false),
            announcement: signal(announcement),
            remote_busy: signal(None::<String>),
            pending_install: signal(None::<PendingInstall>),
            dependency_prompt: signal(None::<DependencyPrompt>),
            update_busy: signal(None::<String>),
            toggle_busy: signal(None::<String>),
            bulk_busy: signal(false),
            delete_prompt: signal(None::<DeletePrompt>),
            delete_busy: signal(false),
            apply_running: signal(false),
            apply_cancel: signal(false),
            apply_status: signal(String::new()),
            task_open: signal(false),
            downloads: signal(Vec::new()),
            batch_selection: signal(BTreeSet::new()),
            batch_progress: signal(None::<BatchProgress>),
            bepinex: signal(bepinex),
            bepinex_flavor: signal(Flavor::Lts),
            bepinex_busy: signal(false),
            bepinex_pending: signal_with(None::<BepInExInstall>, SignalEquals::Never),
            bepinex_confirm: signal(false),
            bepinex_status_text: signal(String::new()),
            nexus_downloading: signal(None::<String>),
            nexus_list: element_ref(),
            metadata_synced: signal(metadata_synced),
            nexus_key_saved: signal(nexus_key_saved),
            nexus_key_draft: signal(String::new()),
            nexus_key_check: signal(nexus_key_check),
            github_token_saved: signal(github_token_saved),
            github_token_draft: signal(String::new()),
            github_token_check: signal(github_token_check),
            store_draft,
            store_error,
            app_data,
            setup,
            owner,
        }
    }

    // A handler's task belongs to the view that handled it; swapping that
    // view out (a button replaced by its disabled twin) cancels the task and
    // leaves its busy signal set. Workers live with the app instead.

    pub(crate) fn spawn_worker(&self, future: impl Future<Output = ()> + 'static) {
        self.owner.run(|| spawn(future));
    }

    pub(crate) fn hub(&self) -> MutexGuard<'_, Hub> {
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
        let needs_sources = {
            let hub = self.hub();
            hub.sources_needs_refresh()
        };
        if needs_sources {
            self.refresh_sources();
        }
        self.profiles_refresh();
        self.watch_downloads();
    }

    pub fn open_link(&self, url: &str) {
        let result = {
            let hub = self.hub();
            hub.open_external_link(url)
        };
        match result {
            Ok(()) => self.notice.set(t!("notice.link_opened")),
            Err(error) => self.notice.set(t!("notice.link_failed", error = error)),
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
                self.rebuild_remote();
                self.notice.set(t!("notice.scanned", count = count));
            }
            Err(error) => self.notice.set(error.to_string()),
        }
    }

    pub(super) fn rebuild_nexus(&self, metadata: &[MetadataMod]) {
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

    pub(crate) fn save_setup_game(&self, draft: &str) -> Result<(), String> {
        if draft.trim().is_empty() {
            return Ok(());
        }
        let saved = {
            let mut hub = self.hub();
            hub.set_game_folder(Path::new(draft.trim()))
                .map(|_| hub.settings().clone())
                .map_err(|error| error.to_string())
        };
        match saved {
            Ok(settings) => {
                self.settings.set(settings);
                self.refresh_mods();
                self.refresh_bepinex();
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn detect_setup_game(&self) -> Option<String> {
        self.hub().detect_game_folder()?;
        let settings = self.hub().settings().clone();
        let path = settings.game_path.clone();
        self.settings.set(settings);
        self.refresh_mods();
        self.refresh_bepinex();
        Some(path)
    }

    pub(crate) fn save_setup_download(&self, draft: &str) -> Result<(), String> {
        let trimmed = draft.trim();
        if !trimmed.is_empty() {
            let path = Path::new(trimmed);
            if !path.is_absolute() || !path.is_dir() {
                return Err(t!("wizard.downloads.invalid"));
            }
        }
        let saved = {
            let mut hub = self.hub();
            let mut settings = hub.settings().clone();
            settings.download_path = trimmed.to_owned();
            hub.save_settings(&settings)
                .map(|()| hub.settings().clone())
                .map_err(|error| error.to_string())
        };
        match saved {
            Ok(settings) => {
                self.settings.set(settings);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn store_folder_path(&self) -> String {
        self.settings
            .get()
            .store_dir(Path::new(&self.app_data))
            .display()
            .to_string()
    }

    pub(crate) fn save_store(&self, draft: &str) -> Result<(), String> {
        let trimmed = draft.trim();
        if !trimmed.is_empty() {
            let path = Path::new(trimmed);
            if !path.is_absolute() {
                return Err(t!("settings.store_invalid"));
            }
            prepare_store_dir(path)?;
        }
        let saved = {
            let mut hub = self.hub();
            let mut settings = hub.settings().clone();
            settings.store_path = trimmed.to_owned();
            hub.save_settings(&settings)
                .map(|()| hub.settings().clone())
                .map_err(|error| t!("settings.store_save_failed", error = error.to_string()))
        };
        match saved {
            Ok(settings) => {
                self.settings.set(settings);
                self.store_draft.set(trimmed.to_owned());
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn save_home_order(&self, order: Vec<String>) {
        let saved = {
            let mut hub = self.hub();
            let mut settings = hub.settings().clone();
            settings.home_order = order;
            hub.save_settings(&settings)
                .map(|()| hub.settings().clone())
        };
        match saved {
            Ok(settings) => self.settings.set(settings),
            Err(error) => self
                .notice
                .set(t!("notice.save_settings_failed", error = error.to_string())),
        }
    }

    pub(crate) fn mark_setup_completed(&self) -> Result<(), String> {
        let saved = {
            let mut hub = self.hub();
            let mut settings = hub.settings().clone();
            settings.setup_completed = true;
            hub.save_settings(&settings)
                .map(|()| hub.settings().clone())
                .map_err(|error| error.to_string())
        };
        match saved {
            Ok(settings) => {
                self.settings.set(settings);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    pub fn set_language(&self, code: &str) {
        let code = if i18n::is_known_language(code) {
            code
        } else {
            i18n::FOLLOW_SYSTEM
        };
        i18n::set_language(code);
        let saved = {
            let mut hub = self.hub();
            let mut settings = hub.settings().clone();
            settings.language = code.to_owned();
            hub.save_settings(&settings)
                .map(|()| hub.settings().clone())
                .map_err(|error| error.to_string())
        };
        match saved {
            Ok(settings) => {
                self.settings.set(settings);
                self.reset_credential_checks();
                self.rebuild_local_rows();
                self.rebuild_remote();
                let metadata = self.hub().metadata().to_vec();
                self.rebuild_nexus(&metadata);
                self.notice.set(t!(
                    "notice.language_set",
                    language = i18n::language_label(code)
                ));
            }
            Err(error) => self
                .notice
                .set(t!("notice.save_language_failed", error = error)),
        }
    }

    fn reset_credential_checks(&self) {
        let key_saved = self.nexus_key_saved.get();
        self.nexus_key_check.set(if key_saved {
            t!("notice.saved_not_checked")
        } else {
            t!("notice.no_key_saved")
        });
        let token_saved = self.github_token_saved.get();
        self.github_token_check.set(if token_saved {
            t!("notice.saved_not_checked")
        } else {
            t!("notice.no_token_saved")
        });
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
                self.notice.set(t!("notice.ui_scale_set", level = level));
            }
            Err(error) => self
                .notice
                .set(t!("notice.save_settings_failed", error = error)),
        }
    }
}

fn prepare_store_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|error| store_dir_error(path, error))?;
    let probe = path.join(STORE_PROBE_FILE);
    let written = fs::write(&probe, b"cuhub").map_err(|error| store_dir_error(path, error));
    let _ = fs::remove_file(&probe);
    written
}

fn store_dir_error(path: &Path, error: io::Error) -> String {
    t!(
        "settings.store_unusable",
        path = path.display().to_string(),
        error = error.to_string()
    )
}
