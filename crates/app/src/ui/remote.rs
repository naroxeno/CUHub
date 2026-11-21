//! Remote manifest rows plus the install and update workers behind the remote page.

use super::install::{
    DependencyPrompt, PendingInstall, RemoteAttempt, accept_pending, prepare_remote_pending,
    prepare_update,
};
use super::rows::dll_stems;
use super::rows::{NexusModRow, nexus_page_count, page_slice};
use super::state::Ui;
use cuhub_core::hub::{CheckOutcome, Hub, HubError};
use cuhub_core::mods::InstalledMod;
use cuhub_core::sources::{LocalMod, Repo, SourceEntry, SourceKind};
use nsh_core::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteFilter {
    All,
    Installed,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteSource {
    All,
    Github,
    Nexus,
}

#[derive(Clone, PartialEq, Eq)]
pub struct RemoteRow {
    pub key: String,
    pub github: Option<SourceEntry>,
    pub nexus: Option<NexusModRow>,
    pub installed: Option<InstalledMod>,
}

impl RemoteRow {
    pub fn row_key(&self) -> String {
        format!(
            "{}|{:?}|{:?}|{:?}|{:?}",
            self.key,
            self.github,
            self.nexus
                .as_ref()
                .map(|row| (&row.version, &row.page_url, &row.download_id)),
            self.installed
                .as_ref()
                .map(|item| (item.row_key(), &item.guid)),
            self.nexus.as_ref().map(|row| &row.name)
        )
    }

    pub fn name(&self) -> &str {
        self.github
            .as_ref()
            .map(|entry| entry.name.as_str())
            .or_else(|| self.nexus.as_ref().map(|entry| entry.name.as_str()))
            .unwrap_or_default()
    }

    pub fn is_installed(&self) -> bool {
        self.installed.is_some()
    }
}

pub(super) fn merge_catalogues(
    entries: &[SourceEntry],
    nexus: &mut [NexusModRow],
    mods: &[InstalledMod],
) -> Vec<RemoteRow> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut claims = BTreeMap::new();
    for id in entries.iter().filter_map(|entry| entry.nexus_id) {
        *claims.entry(id).or_insert(0) += 1;
    }
    let mut by_id = BTreeMap::new();
    for (index, row) in nexus.iter().enumerate() {
        if let Some(id) = row
            .download_id
            .as_deref()
            .and_then(|id| id.parse::<u64>().ok())
        {
            by_id
                .entry(id)
                .and_modify(|value| *value = None)
                .or_insert(Some(index));
        }
    }
    let mut linked = BTreeSet::new();
    let mut rows: Vec<_> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let match_index = entry
                .nexus_id
                .filter(|id| claims.get(id) == Some(&1))
                .and_then(|id| by_id.get(&id).copied().flatten());
            let mut installed = best_installed(entry, mods);
            let nexus = match_index.map(|index| {
                linked.insert(index);
                let row = &mut nexus[index];
                installed = mods
                    .iter()
                    .find(|item| item.metadata_id.as_deref() == Some(row.id.as_str()))
                    .cloned()
                    .or(installed.take());
                row.status = installed.as_ref().map(|item| item.status);
                row.installed_version = installed
                    .as_ref()
                    .and_then(|item| item.installed_version.clone());
                row.clone()
            });
            RemoteRow {
                key: format!("github:{}:{index}", entry.repo.slug()),
                github: Some(entry.clone()),
                nexus,
                installed,
            }
        })
        .collect();
    rows.extend(
        nexus
            .iter()
            .enumerate()
            .filter(|(index, _)| !linked.contains(index))
            .map(|(index, row)| RemoteRow {
                key: format!("nexus:{}:{index}", row.id),
                github: None,
                nexus: Some(row.clone()),
                installed: mods
                    .iter()
                    .find(|item| item.metadata_id.as_deref() == Some(row.id.as_str()))
                    .cloned(),
            }),
    );
    rows
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

fn check_catalogue_package(
    hub: &Hub,
    attempt: RemoteAttempt,
    expected_guid: Option<&str>,
) -> Result<RemoteAttempt, HubError> {
    if let (RemoteAttempt::Ready(pending), Some(guid)) = (&attempt, expected_guid) {
        hub.check_package_guid(&pending.package, guid)?;
    }
    Ok(attempt)
}

impl Ui {
    pub fn rebuild_remote(&self) {
        let metadata = self.hub().metadata().to_vec();
        self.rebuild_nexus(&metadata);
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

    pub fn set_remote_source(&self, source: RemoteSource) {
        self.remote_source.set(source);
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
        let Some(entry) = row.github.clone() else {
            return;
        };
        if row.is_installed() {
            self.update_remote(row, entry);
        } else {
            self.install_remote(row.key, entry);
        }
    }

    pub(super) fn refresh_remote_view(&self) {
        let all = self.remote_all.get();
        let query = self.remote_query.get().trim().to_lowercase();
        let filter = self.remote_filter.get();
        let source = self.remote_source.get();
        let adult = self.settings.get().dashboard_show_adult_mods;
        let rows: Vec<RemoteRow> = all
            .into_iter()
            .filter(|row| {
                let matches_query = query.is_empty()
                    || row.name().to_lowercase().contains(&query)
                    || row
                        .github
                        .as_ref()
                        .is_some_and(|entry| entry.repo.slug().to_lowercase().contains(&query))
                    || row.nexus.as_ref().is_some_and(|entry| {
                        [&entry.name, &entry.author, &entry.description]
                            .iter()
                            .any(|text| text.to_lowercase().contains(&query))
                    });
                let matches_source = match source {
                    RemoteSource::All => true,
                    RemoteSource::Github => row.github.is_some(),
                    RemoteSource::Nexus => row.nexus.is_some(),
                };
                let matches_filter = match filter {
                    RemoteFilter::All => true,
                    RemoteFilter::Installed => row.is_installed(),
                    RemoteFilter::Missing => !row.is_installed(),
                };
                matches_query
                    && matches_filter
                    && matches_source
                    && (adult || !row.nexus.as_ref().is_some_and(|entry| entry.adult))
            })
            .collect();
        let total = rows.len();
        let page = self.remote_page.get().min(nexus_page_count(total) - 1);
        self.remote_page.set(page);
        self.remote_total.set(total);
        self.remote_rows.set(page_slice(&rows, page));
    }

    pub fn check_remote_release(&self, repo: Repo) {
        let slug = repo.slug();
        let mut versions = self.remote_versions.get();
        if versions.get(&slug).is_some_and(Option::is_none) {
            return;
        }
        versions.insert(slug.clone(), None);
        self.remote_versions.set(versions);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                match hub.check_release(&repo)? {
                    CheckOutcome::Release(release) => Ok(release.tag),
                    CheckOutcome::Failure(failure) => Err(HubError::Update(failure.summary())),
                }
            })
            .await;
            let mut versions = ui.remote_versions.get();
            match result {
                Ok(Ok(tag)) => {
                    versions.insert(slug, Some(tag));
                }
                Ok(Err(error)) => {
                    versions.remove(&slug);
                    ui.notice.set(error.to_string());
                }
                Err(error) => {
                    versions.remove(&slug);
                    ui.notice.set(error.to_string());
                }
            }
            ui.remote_versions.set(versions);
        });
    }

    pub fn prepare_catalogue_nexus(&self, id: String) {
        if self.install_preparing() || self.apply_running.get() || self.profiles.is_busy() {
            self.notice.set(t!("notice.busy"));
            return;
        }
        if self.pending_install.get().is_some() {
            self.notice.set(t!("notice.pending_first"));
            return;
        }
        if self.dependency_prompt.get().is_some() {
            self.notice.set(t!("dependency.resolve_first"));
            return;
        }
        let Some(row) = self
            .remote_all
            .get()
            .into_iter()
            .find(|row| row.nexus.as_ref().is_some_and(|entry| entry.id == id))
        else {
            return;
        };
        let Some(nexus) = row.nexus else {
            return;
        };
        if nexus.adult && !self.settings.get().dashboard_show_adult_mods {
            return;
        }
        let Some(id) = nexus.download_id.and_then(|id| id.parse::<u64>().ok()) else {
            return;
        };
        let expected_guid = row
            .installed
            .as_ref()
            .and_then(|item| item.guid.clone())
            .or_else(|| {
                row.github
                    .as_ref()
                    .and_then(|entry| entry.matcher.guid.clone())
            });
        let name = row
            .github
            .as_ref()
            .map_or(nexus.name, |entry| entry.name.clone());
        self.remote_busy.set(Some(row.key));
        self.notice.set(t!("remote.preparing", name = name.clone()));
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let id_string = id.to_string();
                if let Some(installed) = row.installed {
                    let attempt = prepare_update(
                        &hub,
                        &installed,
                        Some(SourceKind::Nexus),
                        Some(SourceKind::Nexus),
                        None,
                        None,
                        Some(&id_string),
                    )?;
                    return check_catalogue_package(&hub, attempt, expected_guid.as_deref());
                }
                let game_running = hub.game_running()?;
                let download = hub.download_nexus_latest_cached(&id_string)?;
                let package = match hub.prepare_nexus_package(&download.path, id, &name) {
                    Ok(package) => package,
                    Err(HubError::Dependencies(issues)) => {
                        return Ok(RemoteAttempt::Blocked(issues));
                    }
                    Err(error) => return Err(error),
                };
                if let Some(guid) = expected_guid.as_deref() {
                    hub.check_package_guid(&package, guid)?;
                }
                Ok(RemoteAttempt::Ready(Box::new(PendingInstall {
                    mod_name: name,
                    source: SourceKind::Nexus,
                    plan: package.preview(),
                    staging: package.staging.clone(),
                    package,
                    notes: Vec::new(),
                    game_running,
                })))
            })
            .await;
            ui.remote_busy.set(None);
            match result {
                Ok(Ok(RemoteAttempt::Ready(pending))) => {
                    let name = pending.mod_name.clone();
                    if accept_pending(&ui, *pending) {
                        ui.notice.set(t!("remote.ready", name = name));
                    }
                }
                Ok(Ok(RemoteAttempt::Blocked(issues))) => {
                    ui.dependency_prompt.set(Some(DependencyPrompt {
                        target: None,
                        issues,
                    }));
                    ui.notice.set(t!("dependency.blocked"));
                }
                Ok(Err(error)) => ui.notice.set(t!("remote.failed", error = error)),
                Err(error) => ui.notice.set(t!("remote.failed", error = error)),
            }
        });
    }

    fn install_remote(&self, key: String, entry: SourceEntry) {
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
        let name = entry.name.clone();
        self.remote_busy.set(Some(key));
        self.notice.set(t!("remote.preparing", name = name));
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let target = entry.clone();
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let expected_guid = entry.matcher.guid.clone();
                let attempt = prepare_remote_pending(&hub, entry)?;
                check_catalogue_package(&hub, attempt, expected_guid.as_deref())
            })
            .await;
            ui.remote_busy.set(None);
            match result {
                Ok(Ok(RemoteAttempt::Ready(pending))) => {
                    let name = pending.mod_name.clone();
                    if accept_pending(&ui, *pending) {
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

    fn update_remote(&self, row: RemoteRow, entry: SourceEntry) {
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
        let expected_guid = item.guid.clone().or_else(|| entry.matcher.guid.clone());
        let repo = entry.repo;
        let asset = entry.asset;
        self.remote_busy.set(Some(key));
        self.notice.set(t!("notice.preparing_update", name = name));
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let attempt = prepare_update(
                    &hub,
                    &item,
                    Some(SourceKind::Github),
                    Some(SourceKind::Github),
                    Some(&repo),
                    asset.as_deref(),
                    None,
                )?;
                check_catalogue_package(&hub, attempt, expected_guid.as_deref())
            })
            .await;
            ui.remote_busy.set(None);
            match result {
                Ok(Ok(RemoteAttempt::Ready(pending))) => {
                    let name = pending.mod_name.clone();
                    let files = pending.plan.files.len();
                    if accept_pending(&ui, *pending) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::rows::build_nexus_rows;
    use cuhub_core::{metadata::parse_mods, sources::SourceManifest};

    #[test]
    fn catalogue_links_explicit_ids_and_keeps_unlinked_names_separate() {
        let manifest = SourceManifest::parse(
            r#"{
            "schemaVersion":1,"generatedAt":"2026-11-21T00:00:00Z","mods":[
                {"name":"Linked","repo":"a/linked","nexusId":67,"match":{"guid":"linked"}},
                {"name":"Same name","repo":"a/unlinked","match":{"name":["Same name"]}}
            ]
        }"#,
        )
        .unwrap();
        let metadata = parse_mods(
            r#"[
            {"Id":"nexus-67","Name":"Linked on Nexus","Author":"A","Version":"2.0"},
            {"Id":"nexus-68","Name":"Same name","Author":"B","Version":"3.0"}
        ]"#,
        )
        .unwrap();
        let mut nexus = build_nexus_rows(&metadata, &[]);
        let rows = merge_catalogues(&manifest.entries, &mut nexus, &[]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].nexus.as_ref().unwrap().version, "2.0");
        assert!(rows[1].nexus.is_none());
        assert!(rows[2].github.is_none());
        assert_eq!(rows[1].name(), rows[2].name());

        let mut ambiguous = manifest.entries.clone();
        ambiguous[1].nexus_id = Some(67);
        let rows = merge_catalogues(&ambiguous, &mut nexus, &[]);
        assert_eq!(rows.len(), 4);
        assert!(
            rows.iter()
                .all(|row| row.github.is_none() || row.nexus.is_none())
        );
    }
}
