//! Zoom-aware pixel primitives, motion helpers and the app's hub-backed state.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use cuhub_core::github::{CheckFailure, ReleaseInfo};
use cuhub_core::hub::{CheckOutcome, Hub, HubError, MetadataRefresh};
use cuhub_core::install::UpgradePlan;
use cuhub_core::metadata::MetadataMod;
use cuhub_core::mods::{DeleteScope, InstalledMod, ModStatus};
use cuhub_core::settings::Settings;
use cuhub_core::source_choice::mod_key;
use cuhub_core::sources::{self, LocalMod, MatchKind, Repo, SourceEntry, SourceKind};
use cuhub_core::version::HubVersion;
use nsh_core::RevealOptions;
use nsh_core::element::FontAsset;
use nsh_core::prelude::*;

use crate::i18n;
use crate::palette::*;

pub use crate::i18n::PAGE_KEYS as PAGES;

pub const PIXEL_FONT: FontAsset = asset!("fonts/fusion-pixel-12px-proportional-zh_hans.otf");

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
            self.state_key(),
            self.installed_version.as_deref().unwrap_or(""),
            self.adult,
            self.has_page
        )
    }

    pub fn state_key(&self) -> &'static str {
        self.status
            .map(i18n::status_key)
            .unwrap_or("status.not_installed")
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
    pub effective: Option<SourceKind>,
    pub release_version: Option<HubVersion>,
    pub asset: Option<String>,
}

impl SourceRow {
    pub fn can_pin(&self) -> bool {
        self.options.len() > 1 || self.pinned.is_some()
    }

    pub fn pin_key(&self) -> &'static str {
        match self.pinned {
            None => "local.pin_auto",
            Some(SourceKind::Github) => "local.pin_github",
            Some(SourceKind::Nexus) => "local.pin_nexus",
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

    pub fn can_update(&self) -> bool {
        if self.item.disabled {
            return false;
        }
        let has_source = self.item.metadata_id.is_some() || self.source.repo.is_some();
        if !has_source {
            return false;
        }
        if self.item.status == ModStatus::OutOfDate {
            return true;
        }
        if self.source.effective != Some(SourceKind::Github) {
            return false;
        }
        let Some(release) = self.source.release_version.as_ref() else {
            return false;
        };
        self.item
            .installed_version
            .as_deref()
            .and_then(HubVersion::parse)
            .is_some_and(|installed| installed < *release)
    }
}

#[derive(Clone, PartialEq)]
pub enum CheckState {
    Checking,
    Release(ReleaseInfo),
    Failed(CheckFailure),
}

#[derive(Clone, PartialEq, Eq)]
pub struct PendingInstall {
    pub mod_name: String,
    pub source: SourceKind,
    pub plan: UpgradePlan,
    pub staging: PathBuf,
    pub notes: Vec<String>,
    pub game_running: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub enum DeleteTarget {
    Mod(Box<InstalledMod>),
    All,
}

impl DeleteTarget {
    pub fn label(&self) -> String {
        match self {
            DeleteTarget::Mod(item) => item.name.clone(),
            DeleteTarget::All => t!("delete.all_mods"),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct DeletePrompt {
    pub target: DeleteTarget,
    pub scope: DeleteScope,
}

impl DeletePrompt {
    pub fn title_key(&self) -> &'static str {
        match &self.target {
            DeleteTarget::Mod(_) => "panel.delete_mod",
            DeleteTarget::All => "panel.delete_all",
        }
    }

    pub fn action_key(&self) -> &'static str {
        match &self.target {
            DeleteTarget::Mod(_) => "common.delete",
            DeleteTarget::All => "common.delete_all",
        }
    }

    pub fn scope_note_key(&self) -> &'static str {
        match &self.target {
            DeleteTarget::Mod(_) => "delete.scope_mod",
            DeleteTarget::All => "delete.scope_all",
        }
    }
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
    pub pending_install: Signal<Option<PendingInstall>>,
    pub update_busy: Signal<Option<String>>,
    pub toggle_busy: Signal<Option<String>>,
    pub bulk_busy: Signal<bool>,
    pub delete_prompt: Signal<Option<DeletePrompt>>,
    pub delete_busy: Signal<bool>,
    pub apply_running: Signal<bool>,
    pub apply_cancel: Signal<bool>,
    pub apply_status: Signal<String>,
    pub nexus_downloading: Signal<Option<String>>,
    pub nexus_list: ElementRef,
    pub metadata_synced: Signal<String>,
    pub nexus_key_saved: Signal<bool>,
    pub nexus_key_draft: Signal<String>,
    pub nexus_key_check: Signal<String>,
    pub github_token_saved: Signal<bool>,
    pub github_token_draft: Signal<String>,
    pub github_token_check: Signal<String>,
    pub app_data: String,
    owner: Owner,
}

impl Ui {
    pub fn new() -> Self {
        let mut hub = Hub::open();
        if hub.settings().game_path.trim().is_empty() {
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
        let app_data = hub.app_data().display().to_string();
        let owner = Owner::current();
        let notice = if settings.game_path.trim().is_empty() {
            t!("notice.no_game_folder")
        } else {
            t!("notice.mods_found", count = mods.len())
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
            pending_install: signal(None::<PendingInstall>),
            update_busy: signal(None::<String>),
            toggle_busy: signal(None::<String>),
            bulk_busy: signal(false),
            delete_prompt: signal(None::<DeletePrompt>),
            delete_busy: signal(false),
            apply_running: signal(false),
            apply_cancel: signal(false),
            apply_status: signal(String::new()),
            nexus_downloading: signal(None::<String>),
            nexus_list: element_ref(),
            metadata_synced: signal(metadata_synced),
            nexus_key_saved: signal(nexus_key_saved),
            nexus_key_draft: signal(String::new()),
            nexus_key_check: signal(nexus_key_check),
            github_token_saved: signal(github_token_saved),
            github_token_draft: signal(String::new()),
            github_token_check: signal(github_token_check),
            app_data,
            owner,
        }
    }

    // A handler's task belongs to the view that handled it; swapping that
    // view out (a button replaced by its disabled twin) cancels the task and
    // leaves its busy signal set. Workers live with the app instead.
    fn spawn_worker(&self, future: impl Future<Output = ()> + 'static) {
        self.owner.run(|| spawn(future));
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
                self.notice.set(t!("notice.scanned", count = count));
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
        self.spawn_worker(async move {
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
                        .set(synced.unwrap_or_else(|| t!("common.never")));
                    ui.nexus_refreshing.set(false);
                    match refresh {
                        MetadataRefresh::Updated(count) => ui
                            .notice
                            .set(t!("notice.catalogue_refreshed", count = count)),
                        MetadataRefresh::NotModified => {
                            ui.notice.set(t!("notice.catalogue_current"))
                        }
                    }
                }
                Ok(Err(error)) => {
                    ui.nexus_error.set(error.to_string());
                    ui.nexus_refreshing.set(false);
                    ui.notice
                        .set(t!("notice.catalogue_refresh_failed", error = error));
                }
                Err(error) => {
                    ui.nexus_error.set(t!("notice.catalogue_worker_stopped"));
                    ui.nexus_refreshing.set(false);
                    ui.notice
                        .set(t!("notice.catalogue_refresh_failed", error = error));
                }
            }
        });
    }

    pub fn download_nexus(&self, mod_id: String) {
        if self.nexus_downloading.get().is_some() {
            self.notice.set(t!("notice.nexus_download_running"));
            return;
        }
        if mod_id.trim().is_empty() {
            self.notice.set(t!("notice.no_nexus_id"));
            return;
        }
        self.nexus_downloading.set(Some(mod_id.clone()));
        self.notice.set(t!("notice.nexus_downloading"));
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.download_nexus_latest(&mod_id)
            })
            .await;
            ui.nexus_downloading.set(None);
            match result {
                Ok(Ok(download)) => ui.notice.set(t!(
                    "notice.downloaded",
                    file = download.file_name,
                    path = download.path.display().to_string()
                )),
                Ok(Err(error)) => ui
                    .notice
                    .set(t!("notice.nexus_download_failed", error = error)),
                Err(error) => ui
                    .notice
                    .set(t!("notice.nexus_download_failed", error = error)),
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

    pub fn set_mod_enabled(&self, key: String, enabled: bool) {
        if self.toggle_busy.get().is_some() || self.bulk_busy.get() {
            self.notice.set(t!("notice.change_busy"));
            return;
        }
        let row = self
            .local_rows
            .get()
            .into_iter()
            .find(|row| row.source_key == key);
        let Some(row) = row else {
            self.notice.set(t!("notice.mod_gone"));
            return;
        };
        let item = row.item.clone();
        let name = item.name.clone();
        self.toggle_busy.set(Some(key));
        self.notice.set(if enabled {
            t!("notice.enabling", name = name)
        } else {
            t!("notice.disabling", name = name)
        });
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.set_mod_enabled(&item, enabled)
            })
            .await;
            ui.toggle_busy.set(None);
            match result {
                Ok(Ok(count)) => {
                    ui.refresh_mods();
                    ui.notice.set(if enabled {
                        t!("notice.enabled", name = name, count = count)
                    } else {
                        t!("notice.disabled", name = name, count = count)
                    });
                }
                Ok(Err(error)) => ui.notice.set(if enabled {
                    t!("notice.enable_failed", name = name, error = error)
                } else {
                    t!("notice.disable_failed", name = name, error = error)
                }),
                Err(error) => ui
                    .notice
                    .set(t!("notice.toggle_worker_stopped", error = error)),
            }
        });
    }

    pub fn set_all_mods_enabled(&self, enabled: bool) {
        if self.toggle_busy.get().is_some() || self.bulk_busy.get() {
            self.notice.set(t!("notice.change_busy"));
            return;
        }
        if self.mods.get().is_empty() {
            self.notice.set(t!("notice.no_mods_to_change"));
            return;
        }
        self.bulk_busy.set(true);
        self.notice.set(if enabled {
            t!("notice.enabling_all")
        } else {
            t!("notice.disabling_all")
        });
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.set_all_mods_enabled(enabled)
            })
            .await;
            ui.bulk_busy.set(false);
            match result {
                Ok(Ok(count)) => {
                    ui.refresh_mods();
                    ui.notice.set(if enabled {
                        t!("notice.enabled_all", count = count)
                    } else {
                        t!("notice.disabled_all", count = count)
                    });
                }
                Ok(Err(error)) => ui.notice.set(if enabled {
                    t!("notice.enable_all_failed", error = error)
                } else {
                    t!("notice.disable_all_failed", error = error)
                }),
                Err(error) => ui
                    .notice
                    .set(t!("notice.toggle_worker_stopped", error = error)),
            }
        });
    }

    pub fn mod_files_busy(&self) -> bool {
        self.toggle_busy.get().is_some()
            || self.bulk_busy.get()
            || self.delete_busy.get()
            || self.delete_prompt.get().is_some()
    }

    pub fn request_delete_mod(&self, key: String) {
        let row = self
            .local_rows
            .get()
            .into_iter()
            .find(|row| row.source_key == key);
        let Some(row) = row else {
            self.notice.set(t!("notice.mod_gone"));
            return;
        };
        self.start_delete(DeleteTarget::Mod(Box::new(row.item.clone())));
    }

    pub fn request_delete_all(&self) {
        self.start_delete(DeleteTarget::All);
    }

    fn start_delete(&self, target: DeleteTarget) {
        if self.delete_prompt.get().is_some() {
            return;
        }
        if self.delete_busy.get() || self.toggle_busy.get().is_some() || self.bulk_busy.get() {
            self.notice.set(t!("notice.file_change_busy"));
            return;
        }
        let label = target.label();
        self.delete_busy.set(true);
        self.notice
            .set(t!("notice.collecting_delete", label = label));
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let scope_target = target.clone();
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                match &scope_target {
                    DeleteTarget::Mod(item) => hub.delete_scope(item),
                    DeleteTarget::All => hub.delete_all_scope(),
                }
            })
            .await;
            ui.delete_busy.set(false);
            match result {
                Ok(Ok(scope)) => {
                    if scope.roots.is_empty() {
                        ui.notice.set(t!("notice.plugins_empty"));
                        return;
                    }
                    ui.notice.set(t!(
                        "notice.delete_scope",
                        label = label,
                        files = scope.files,
                        paths = scope.roots.len()
                    ));
                    ui.delete_prompt.set(Some(DeletePrompt { target, scope }));
                }
                Ok(Err(error)) => {
                    ui.notice
                        .set(t!("notice.list_failed", label = label, error = error))
                }
                Err(error) => ui
                    .notice
                    .set(t!("notice.delete_worker_stopped", error = error)),
            }
        });
    }

    pub fn confirm_delete(&self) {
        let Some(prompt) = self.delete_prompt.get() else {
            return;
        };
        if self.delete_busy.get() {
            return;
        }
        let label = prompt.target.label();
        let failure = match &prompt.target {
            DeleteTarget::Mod(item) => t!(
                "notice.delete_mod_failed",
                name = item.name,
                path = item.source_path.display().to_string()
            ),
            DeleteTarget::All => t!("notice.delete_all_failed"),
        };
        self.delete_busy.set(true);
        self.notice.set(t!("notice.deleting", label = label));
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let target = prompt.target.clone();
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                match &target {
                    DeleteTarget::Mod(item) => hub.delete_mod(item),
                    DeleteTarget::All => hub.delete_all_mods(),
                }
            })
            .await;
            ui.delete_busy.set(false);
            ui.delete_prompt.set(None);
            match result {
                Ok(Ok(count)) => {
                    ui.refresh_mods();
                    ui.notice.set(t!("notice.deleted", count = count));
                }
                Ok(Err(error)) => {
                    ui.notice
                        .set(t!("notice.delete_failed", failure = failure, error = error))
                }
                Err(error) => ui
                    .notice
                    .set(t!("notice.delete_worker_stopped", error = error)),
            }
        });
    }

    pub fn cancel_delete(&self) {
        if self.delete_busy.get() || self.delete_prompt.get().is_none() {
            return;
        }
        self.delete_prompt.set(None);
        self.notice.set(t!("notice.delete_cancelled"));
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
            .set(t!("notice.checking_github_repo", repo = repo.slug()));
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
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
                    ui.notice
                        .set(t!("notice.check_worker_stopped", error = error));
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

    pub fn update_source(&self, key: String) {
        if self.update_busy.get().is_some() {
            self.notice.set(t!("notice.update_preparing"));
            return;
        }
        if self.pending_install.get().is_some() {
            self.notice.set(t!("notice.pending_first"));
            return;
        }
        let row = self
            .local_rows
            .get()
            .into_iter()
            .find(|row| row.source_key == key);
        let Some(row) = row else {
            self.notice.set(t!("notice.mod_gone"));
            return;
        };
        if !row.can_update() {
            self.notice.set(t!("notice.not_ready_for_update"));
            return;
        }
        let item = row.item.clone();
        let pinned = row.source.pinned;
        let effective = row.source.effective;
        let repo = row.source.repo.clone();
        let asset = row.source.asset.clone();
        let nexus_id = item.metadata_id.clone();
        let name = item.name.clone();
        self.update_busy.set(Some(key));
        self.notice.set(t!("notice.preparing_update", name = name));
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                prepare_update(
                    &hub,
                    &item,
                    pinned,
                    effective,
                    repo.as_ref(),
                    asset.as_deref(),
                    nexus_id.as_deref(),
                )
            })
            .await;
            match result {
                Ok(Ok(pending)) => {
                    let name = pending.mod_name.clone();
                    let files = pending.plan.files.len();
                    ui.pending_install.set(Some(pending));
                    ui.apply_status.set(String::new());
                    ui.notice
                        .set(t!("notice.update_ready", name = name, files = files));
                }
                Ok(Err(error)) => ui.notice.set(error.to_string()),
                Err(error) => ui
                    .notice
                    .set(t!("notice.update_worker_stopped", error = error)),
            }
            ui.update_busy.set(None);
        });
    }

    pub fn apply_pending(&self) {
        let Some(pending) = self.pending_install.get() else {
            return;
        };
        if self.apply_running.get() {
            return;
        }
        self.apply_running.set(true);
        self.apply_cancel.set(false);
        self.apply_status.set(if pending.game_running {
            t!("notice.waiting_for_game")
        } else {
            t!("notice.applying")
        });
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            loop {
                if ui.apply_cancel.get() {
                    let staging = pending.staging.clone();
                    let hub_for_cleanup = hub.clone();
                    let _ = blocking(move || {
                        let hub = hub_for_cleanup
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        hub.discard_staging(&staging);
                    })
                    .await;
                    ui.pending_install.set(None);
                    ui.apply_status.set(t!("notice.update_cancelled_status"));
                    ui.notice.set(t!("notice.update_cancelled"));
                    break;
                }
                let plan = pending.plan.clone();
                let worker_hub = hub.clone();
                let result = blocking(move || {
                    let hub = worker_hub
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    hub.apply_install(&plan)
                })
                .await;
                match result {
                    Ok(Ok(report)) => {
                        let staging = pending.staging.clone();
                        let cleanup_hub = hub.clone();
                        let _ = blocking(move || {
                            let hub = cleanup_hub
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            hub.discard_staging(&staging);
                        })
                        .await;
                        let count = report.replaced.len() + report.created.len();
                        let backup = report
                            .backup
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| t!("common.none"));
                        let name = pending.mod_name.clone();
                        ui.pending_install.set(None);
                        ui.apply_status.set(String::new());
                        ui.refresh_mods();
                        ui.notice.set(t!(
                            "notice.updated",
                            name = name,
                            count = count,
                            backup = backup
                        ));
                        break;
                    }
                    Ok(Err(HubError::GameRunning)) => {
                        ui.apply_status.set(t!("notice.waiting_for_game"));
                        let _ = blocking(|| std::thread::sleep(Duration::from_secs(3))).await;
                    }
                    Ok(Err(error)) => {
                        ui.apply_status.set(String::new());
                        ui.notice.set(error.to_string());
                        break;
                    }
                    Err(error) => {
                        ui.apply_status.set(String::new());
                        ui.notice
                            .set(t!("notice.install_worker_stopped", error = error));
                        break;
                    }
                }
            }
            ui.apply_running.set(false);
        });
    }

    pub fn discard_pending(&self) {
        if self.apply_running.get() {
            self.apply_cancel.set(true);
            self.apply_status.set(t!("notice.cancelling"));
            return;
        }
        let Some(pending) = self.pending_install.get() else {
            return;
        };
        {
            let hub = self.hub();
            hub.discard_staging(&pending.staging);
        }
        self.pending_install.set(None);
        self.apply_status.set(String::new());
        self.notice.set(t!("notice.discarded"));
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
                    Some(kind) => t!("notice.pinned", kind = source_kind_label(kind)),
                    None => t!("notice.pin_auto"),
                });
            }
            Err(error) => self
                .notice
                .set(t!("notice.save_source_failed", error = error)),
        }
    }

    pub fn open_nexus_download(&self, mod_id: String) {
        let result = {
            let hub = self.hub();
            hub.open_nexus_download_page(&mod_id)
        };
        match result {
            Ok(()) => self.notice.set(t!("notice.opened_nexus")),
            Err(error) => self
                .notice
                .set(t!("notice.open_nexus_failed", error = error)),
        }
    }

    pub fn show_mod_details(&self, name: &str) {
        self.notice.set(t!("notice.details_todo", name = name));
    }

    pub fn launch_game(&self) {
        let result = {
            let hub = self.hub();
            hub.launch_game()
        };
        match result {
            Ok(app_id) => self.notice.set(t!("notice.launched", app = app_id)),
            Err(error) => self.notice.set(t!("notice.launch_failed", error = error)),
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
                self.notice.set(t!(
                    "notice.game_folder_set",
                    path = path.display().to_string()
                ));
                self.refresh_mods();
            }
            None => self.notice.set(t!("notice.game_not_found")),
        }
    }

    pub fn save_nexus_key(&self) {
        let key = self.nexus_key_draft.get();
        if key.trim().is_empty() {
            self.notice.set(t!("notice.paste_nexus_key"));
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
                self.notice.set(t!("notice.nexus_key_saved"));
            }
            Err(error) => self
                .notice
                .set(t!("notice.save_nexus_key_failed", error = error)),
        }
    }

    pub fn validate_nexus_key(&self) {
        if !self.nexus_key_saved.get() {
            self.nexus_key_check.set(t!("notice.no_key_saved"));
            self.notice.set(t!("notice.save_nexus_key_first"));
            return;
        }
        self.nexus_key_check.set(t!("notice.checking_nexus"));
        let hub = self.hub.clone();
        let saved = self.nexus_key_saved;
        let check = self.nexus_key_check;
        let notice = self.notice;
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.validate_nexus_key()
            })
            .await;
            match result {
                Ok(Ok(account)) => {
                    saved.set(true);
                    if account.is_premium {
                        check.set(t!("notice.nexus_premium"));
                        notice.set(t!("notice.nexus_accepted_premium"));
                    } else {
                        check.set(t!("notice.nexus_standard"));
                        notice.set(t!("notice.nexus_accepted_standard"));
                    }
                }
                Ok(Err(error)) => {
                    check.set(t!("notice.nexus_check_failed", error = error));
                    notice.set(t!("notice.check_nexus_key_failed", error = error));
                }
                Err(error) => {
                    check.set(t!("notice.nexus_check_unfinished"));
                    notice.set(t!("notice.check_nexus_key_failed", error = error));
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
                self.notice.set(t!("notice.nexus_key_cleared"));
            }
            Err(error) => self
                .notice
                .set(t!("notice.clear_nexus_key_failed", error = error)),
        }
    }

    pub fn save_github_token(&self) {
        let token = self.github_token_draft.get();
        if token.trim().is_empty() {
            self.notice.set(t!("notice.paste_github_token"));
            return;
        }
        let result = {
            let mut hub = self.hub();
            hub.save_github_token(&token)
        };
        match result {
            Ok(()) => {
                self.github_token_saved.set(true);
                self.github_token_draft.set(String::new());
                self.notice.set(t!("notice.github_token_saved"));
            }
            Err(error) => self
                .notice
                .set(t!("notice.save_github_token_failed", error = error)),
        }
    }

    pub fn validate_github_token(&self) {
        if !self.github_token_saved.get() {
            self.github_token_check.set(t!("notice.no_token_saved"));
            self.notice.set(t!("notice.save_github_token_first"));
            return;
        }
        self.github_token_check.set(t!("notice.checking_github"));
        let hub = self.hub.clone();
        let saved = self.github_token_saved;
        let check = self.github_token_check;
        let notice = self.notice;
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.validate_github_token()
            })
            .await;
            match result {
                Ok(Ok(limit)) => {
                    saved.set(true);
                    check.set(t!(
                        "notice.github_token_accepted",
                        remaining = limit.remaining,
                        limit = limit.limit
                    ));
                    notice.set(t!(
                        "notice.github_token_accepted_notice",
                        remaining = limit.remaining,
                        limit = limit.limit
                    ));
                }
                Ok(Err(error)) => {
                    check.set(t!("notice.github_check_failed", error = error));
                    notice.set(t!("notice.check_github_token_failed", error = error));
                }
                Err(error) => {
                    check.set(t!("notice.github_check_unfinished"));
                    notice.set(t!("notice.check_github_token_failed", error = error));
                }
            }
        });
    }

    pub fn clear_github_token(&self) {
        let result = {
            let mut hub = self.hub();
            hub.clear_github_token()
        };
        match result {
            Ok(()) => {
                self.github_token_saved.set(false);
                self.github_token_draft.set(String::new());
                self.github_token_check.set(t!("notice.no_token_saved"));
                self.notice.set(t!("notice.github_token_cleared"));
            }
            Err(error) => self
                .notice
                .set(t!("notice.clear_github_token_failed", error = error)),
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

fn prepare_update(
    hub: &Hub,
    item: &InstalledMod,
    pinned: Option<SourceKind>,
    effective: Option<SourceKind>,
    repo: Option<&Repo>,
    asset: Option<&str>,
    nexus_id: Option<&str>,
) -> Result<PendingInstall, HubError> {
    let mut source = effective
        .unwrap_or_else(|| choose_update_source(hub, pinned, nexus_id.is_some(), repo.is_some()));
    let mut notes = Vec::new();
    let archive = match source {
        SourceKind::Nexus => {
            let id =
                nexus_id.ok_or_else(|| HubError::Update(String::from("mod has no Nexus id")))?;
            match hub.download_nexus_latest(id) {
                Ok(download) => download.path,
                Err(error) => match repo {
                    Some(repo) if pinned.is_none() => {
                        notes.push(t!("pending.nexus_fallback", error = error));
                        source = SourceKind::Github;
                        download_github_archive(hub, repo, asset)?
                    }
                    _ => return Err(error),
                },
            }
        }
        SourceKind::Github => {
            let repo =
                repo.ok_or_else(|| HubError::Update(String::from("mod has no GitHub repository")))?;
            download_github_archive(hub, repo, asset)?
        }
    };
    let staging = hub.staging_root();
    let plan = hub.plan_download(&archive, item, &staging)?;
    let game_running = hub.game_running()?;
    Ok(PendingInstall {
        mod_name: item.name.clone(),
        source,
        plan,
        staging,
        notes,
        game_running,
    })
}

fn choose_update_source(
    hub: &Hub,
    pinned: Option<SourceKind>,
    has_nexus: bool,
    has_github: bool,
) -> SourceKind {
    if let Some(kind) = pinned {
        return kind;
    }
    let premium = hub.nexus_has_key()
        && hub
            .validate_nexus_key()
            .map(|account| account.is_premium)
            .unwrap_or(false);
    if premium && has_nexus {
        SourceKind::Nexus
    } else if has_github {
        SourceKind::Github
    } else {
        SourceKind::Nexus
    }
}

fn download_github_archive(
    hub: &Hub,
    repo: &Repo,
    asset: Option<&str>,
) -> Result<PathBuf, HubError> {
    let destination = hub.app_data().join("Downloads");
    hub.download_github_asset(repo, asset, &destination)
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
                key: format!(
                    "{key}|{}|{}|{}",
                    check_tag(check),
                    pin_tag(pinned),
                    item.disabled
                ),
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
            Some(kind) => t!("source.not_listed_pinned", kind = source_kind_label(kind)),
            None => t!("source.not_listed"),
        };
        return SourceRow {
            text,
            warning: false,
            can_check: false,
            repo: None,
            options: Vec::new(),
            pinned: input.pinned,
            effective: None,
            release_version: None,
            asset: None,
        };
    };
    let repo = &matched.entry.repo;
    let mut parts = vec![
        t!("source.github", repo = repo.slug()),
        t!(match_key(matched.kind)),
    ];
    match input.check {
        None => parts.push(t!("source.not_checked")),
        Some(CheckState::Checking) => parts.push(t!("source.checking")),
        Some(CheckState::Release(release)) => {
            parts.push(release.tag.clone());
            if release.prerelease {
                parts.push(t!("source.prerelease_only"));
            }
        }
        Some(CheckState::Failed(failure)) => parts.push(failure.summary()),
    }
    if let Some(effective) = &resolution.effective {
        parts.push(t!(
            "source.uses",
            source = source_label(effective.kind, input.check, input.item)
        ));
    }
    if resolution.pinned.is_some() {
        parts.push(t!("source.pinned"));
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
        effective: resolution.effective.as_ref().map(|option| option.kind),
        release_version: match input.check {
            Some(CheckState::Release(release)) => Some(release.version.clone()),
            _ => None,
        },
        asset: matched.entry.asset.clone(),
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
            Some(CheckState::Release(release)) => t!("source.github", repo = release.tag),
            _ => t!("source.github_not_checked"),
        },
        SourceKind::Nexus => item
            .expected_version
            .clone()
            .map(|version| format!("Nexus {version}"))
            .unwrap_or_else(|| String::from("Nexus")),
    }
}

fn match_key(kind: MatchKind) -> &'static str {
    match kind {
        MatchKind::Guid => "source.match_guid",
        MatchKind::Dll => "source.match_dll",
        MatchKind::Name => "source.match_name",
    }
}

fn release_notice(repo: &Repo, release: &ReleaseInfo) -> String {
    if release.prerelease {
        t!(
            "notice.release_prerelease",
            repo = repo.slug(),
            tag = release.tag
        )
    } else {
        t!(
            "notice.release_found",
            repo = repo.slug(),
            tag = release.tag
        )
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
                    t!("nexus.unknown_mod")
                } else {
                    entry.name.clone()
                },
                author: if entry.author.trim().is_empty() {
                    t!("nexus.unknown_author")
                } else {
                    entry.author.clone()
                },
                version: if entry.version.trim().is_empty() {
                    t!("nexus.unknown_version")
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

pub fn panel(
    zoom: &Signal<u8>,
    title: impl IntoReactive<SharedString>,
    body: impl IntoElement,
) -> View {
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
    label: impl IntoReactive<SharedString>,
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
    button.flex_shrink_0()
}

pub fn danger_button(
    zoom: &Signal<u8>,
    label: impl IntoReactive<SharedString>,
    enabled: bool,
    on_press: impl Fn() + 'static,
) -> Text {
    let z = *zoom;
    let mut button = pix(label, 12.0, if enabled { DANGER } else { TEXT_DIM }, &z)
        .px(scaled(&z, 10.0))
        .py(scaled(&z, 4.0))
        .bg(rgb(SURFACE_RAISED))
        .border(scaled(&z, 1.0))
        .border_color(if enabled {
            rgb(DANGER)
        } else {
            rgb(BORDER_DIM)
        });
    if enabled {
        button = button
            .hover(|s| s.bg(rgb(DANGER)).text_color(rgb(BG)))
            .active(|s| s.bg(rgb(DANGER)).text_color(rgb(BG)))
            .on_press(on_press);
    }
    button.flex_shrink_0()
}

pub fn rise_in() -> Motion {
    motion().entering(MotionPreset::FadeInUp)
}

pub fn fade_in() -> Motion {
    motion().entering(MotionPreset::FadeIn)
}

pub fn spinner(zoom: &Signal<u8>) -> View {
    let z = *zoom;
    let mut dots = view().flex_row().items_center().gap(scaled(&z, 2.0));
    for step in 0..4 {
        dots = dots.child(
            view()
                .w(scaled(&z, 3.0))
                .h(scaled(&z, 3.0))
                .bg(rgb(ACCENT))
                .animation(
                    KeyframeAnimation::new(Keyframes::Pulse)
                        .duration(640.0)
                        .delay(step as f32 * 120.0)
                        .frame_rate(20.0),
                ),
        );
    }
    dots
}
