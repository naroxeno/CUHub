//! Remote manifest rows plus the install and update workers behind the remote page.

use super::install::{
    DependencyPrompt, RemoteAttempt, accept_pending, prepare_remote_pending, prepare_update,
};
use super::rows::dll_stems;
use super::rows::{nexus_page_count, page_slice, source_key};
use super::state::Ui;
use cuhub_core::hub::Hub;
use cuhub_core::mods::InstalledMod;
use cuhub_core::sources::{LocalMod, SourceEntry, SourceKind};
use nsh_core::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteFilter {
    All,
    Installed,
    Missing,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RemoteRow {
    pub key: String,
    pub entry: SourceEntry,
    pub installed: Option<InstalledMod>,
}

impl RemoteRow {
    pub fn row_key(&self) -> String {
        format!(
            "{}|{}|{}",
            self.key,
            self.is_installed(),
            self.installed
                .as_ref()
                .and_then(|item| item.installed_version.clone())
                .unwrap_or_default()
        )
    }

    pub fn name(&self) -> &str {
        &self.entry.name
    }

    pub fn repo(&self) -> String {
        self.entry.repo.slug()
    }

    pub fn is_installed(&self) -> bool {
        self.installed.is_some()
    }
}

pub(super) fn build_remote_rows(hub: &Hub, mods: &[InstalledMod]) -> Vec<RemoteRow> {
    let entries = hub
        .sources()
        .map(|manifest| manifest.entries.as_slice())
        .unwrap_or(&[]);
    entries
        .iter()
        .map(|entry| RemoteRow {
            key: format!("{}|{}", entry.name, entry.repo.slug()),
            entry: entry.clone(),
            installed: best_installed(entry, mods),
        })
        .collect()
}

fn best_installed(entry: &SourceEntry, mods: &[InstalledMod]) -> Option<InstalledMod> {
    mods.iter()
        .filter_map(|item| {
            let stems = dll_stems(item);
            entry
                .matcher
                .matches(&LocalMod {
                    guid: item.guid.as_deref(),
                    dll_stems: stems,
                    name: &item.name,
                })
                .map(|kind| (kind, item))
        })
        .min_by_key(|(kind, _)| *kind)
        .map(|(_, item)| item.clone())
}

impl Ui {
    pub fn rebuild_remote(&self) {
        let rows = {
            let hub = self.hub();
            build_remote_rows(&hub, &self.mods.get())
        };
        self.remote_all.set(rows);
        self.refresh_remote_view();
    }

    pub fn set_remote_query(&self, value: String) {
        self.remote_query.set(value);
        self.remote_page.set(0);
        self.refresh_remote_view();
    }

    pub fn set_remote_filter(&self, filter: RemoteFilter) {
        self.remote_filter.set(filter);
        self.remote_page.set(0);
        self.refresh_remote_view();
    }

    pub fn set_remote_page(&self, page: usize) {
        self.remote_page.set(page);
        self.refresh_remote_view();
    }

    pub fn sources_path(&self) -> String {
        std::path::Path::new(&self.app_data)
            .join("Sources")
            .join("manifest.json")
            .display()
            .to_string()
    }

    pub fn run_remote(&self, row: RemoteRow) {
        if row.is_installed() {
            self.update_remote(row);
        } else {
            self.install_remote(row);
        }
    }

    fn refresh_remote_view(&self) {
        let all = self.remote_all.get();
        let query = self.remote_query.get().trim().to_lowercase();
        let filter = self.remote_filter.get();
        let rows: Vec<RemoteRow> = all
            .into_iter()
            .filter(|row| {
                let matches_query = query.is_empty()
                    || row.name().to_lowercase().contains(&query)
                    || row.repo().to_lowercase().contains(&query);
                let matches_filter = match filter {
                    RemoteFilter::All => true,
                    RemoteFilter::Installed => row.is_installed(),
                    RemoteFilter::Missing => !row.is_installed(),
                };
                matches_query && matches_filter
            })
            .collect();
        let total = rows.len();
        let page = self.remote_page.get().min(nexus_page_count(total) - 1);
        self.remote_page.set(page);
        self.remote_total.set(total);
        self.remote_rows.set(page_slice(&rows, page));
    }

    fn install_remote(&self, row: RemoteRow) {
        if self.remote_busy.get().is_some() {
            return;
        }
        if self.pending_install.get().is_some() {
            self.notice.set(t!("notice.pending_first"));
            return;
        }
        if self.update_busy.get().is_some() || self.apply_running.get() || self.profiles.is_busy() {
            self.notice.set(t!("notice.busy"));
            return;
        }
        if self.dependency_prompt.get().is_some() {
            self.notice.set(t!("dependency.resolve_first"));
            return;
        }
        let key = row.key.clone();
        let name = row.entry.name.clone();
        self.remote_busy.set(Some(key));
        self.notice.set(t!("remote.preparing", name = name));
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let entry = row.entry;
            let target = entry.clone();
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                prepare_remote_pending(&hub, entry)
            })
            .await;
            ui.remote_busy.set(None);
            match result {
                Ok(Ok(RemoteAttempt::Ready(pending))) => {
                    let name = pending.mod_name.clone();
                    if accept_pending(&ui, pending) {
                        ui.notice.set(t!("remote.ready", name = name));
                    }
                }
                Ok(Ok(RemoteAttempt::Blocked(issues))) => {
                    ui.dependency_prompt.set(Some(DependencyPrompt {
                        target: Some(target),
                        issues,
                    }));
                    ui.notice.set(t!("dependency.blocked"));
                }
                Ok(Err(error)) => ui.notice.set(t!("remote.failed", error = error)),
                Err(error) => ui.notice.set(t!("remote.failed", error = error)),
            }
        });
    }

    fn update_remote(&self, row: RemoteRow) {
        if self.remote_busy.get().is_some() {
            return;
        }
        if self.pending_install.get().is_some() {
            self.notice.set(t!("notice.pending_first"));
            return;
        }
        if self.update_busy.get().is_some() || self.apply_running.get() || self.profiles.is_busy() {
            self.notice.set(t!("notice.busy"));
            return;
        }
        let Some(item) = row.installed.clone() else {
            return;
        };
        let key = row.key.clone();
        let name = item.name.clone();
        let pinned = {
            let hub = self.hub();
            hub.source_choice(&source_key(&item))
        };
        let repo = row.entry.repo.clone();
        let asset = row.entry.asset.clone();
        self.remote_busy.set(Some(key));
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
                    Some(SourceKind::Github),
                    Some(&repo),
                    asset.as_deref(),
                    None,
                )
            })
            .await;
            ui.remote_busy.set(None);
            match result {
                Ok(Ok(RemoteAttempt::Ready(pending))) => {
                    let name = pending.mod_name.clone();
                    let files = pending.plan.files.len();
                    if accept_pending(&ui, pending) {
                        ui.notice
                            .set(t!("notice.update_ready", name = name, files = files));
                    }
                }
                Ok(Ok(RemoteAttempt::Blocked(issues))) => {
                    ui.dependency_prompt.set(Some(DependencyPrompt {
                        target: None,
                        issues,
                    }));
                    ui.notice.set(t!("dependency.blocked"));
                }
                Ok(Err(error)) => ui.notice.set(error.to_string()),
                Err(error) => ui
                    .notice
                    .set(t!("notice.update_worker_stopped", error = error)),
            }
        });
    }
}
