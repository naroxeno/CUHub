//! Pending install staging, archive download and apply/discard.

use super::state::Ui;
use super::tasks::DownloadTracker;
use cuprum_core::hub::{Hub, HubError, RemoteInstall};
use cuprum_core::install::UpgradePlan;
use cuprum_core::mods::{DependencyIssue, InstalledMod, IssueKind};
use cuprum_core::network::DownloadProgress;
use cuprum_core::packages::PackagePlan;
use cuprum_core::sources::{Repo, SourceEntry, SourceKind};
use nsh_core::prelude::*;
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq)]
pub enum PendingNote {
    NexusFallback { error: String },
    ThunderstoreDependency(cuprum_core::hub::PlannedDependency),
}

impl PendingNote {
    pub fn text(&self) -> String {
        match self {
            Self::ThunderstoreDependency(entry) => super::tasks::thunderstore_dependency_line(entry).0,
            Self::NexusFallback { error } => {
                t!("pending.nexus_fallback", error = error.clone())
            }
        }
    }
}

pub(super) fn accept_pending(ui: &Ui, pending: PendingInstall) -> bool {
    if ui.pending_install.get().is_some() {
        ui.hub().discard_staging(&pending.staging);
        ui.notice.set(t!("notice.pending_first"));
        return false;
    }
    ui.pending_install.set(Some(pending));
    ui.apply_status.set(String::new());
    true
}

#[derive(Clone, PartialEq, Eq)]
pub struct PendingInstall {
    pub mod_name: String,
    pub plan: UpgradePlan,
    pub package: PackagePlan,
    pub staging: PathBuf,
    pub notes: Vec<PendingNote>,
    pub game_running: bool,
}

impl PendingInstall {
    pub(super) fn is_local(&self) -> bool {
        self.package.source == "local"
    }

    fn is_thunderstore(&self) -> bool {
        self.package
            .source
            .starts_with(cuprum_core::thunderstore::SOURCE_PREFIX)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct DependencyPrompt {
    pub target: Option<SourceEntry>,
    pub issues: Vec<DependencyIssue>,
}

impl DependencyPrompt {
    pub fn can_install(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.kind == IssueKind::Missing && issue.entry.is_some())
    }

    pub fn has_manual(&self) -> bool {
        self.issues.iter().any(|issue| {
            issue.kind == IssueKind::Outdated
                || (issue.kind == IssueKind::Missing && issue.entry.is_none())
        })
    }
}

pub(super) enum RemoteAttempt {
    Ready(Box<PendingInstall>),
    Blocked(Vec<DependencyIssue>),
}

pub(super) fn prepare_remote_pending(
    hub: &Hub,
    entry: SourceEntry,
) -> Result<RemoteAttempt, HubError> {
    match hub.prepare_remote_install(&entry) {
        Ok(RemoteInstall {
            name,
            staging,
            plan,
            package,
        }) => {
            let game_running = hub.game_running()?;
            Ok(RemoteAttempt::Ready(Box::new(PendingInstall {
                mod_name: name,
                plan,
                package,
                staging,
                notes: Vec::new(),
                game_running,
            })))
        }
        Err(HubError::Dependencies(issues)) => Ok(RemoteAttempt::Blocked(issues)),
        Err(error) => Err(error),
    }
}

pub(super) fn prepare_update(
    hub: &Hub,
    item: &InstalledMod,
    pinned: Option<SourceKind>,
    effective: Option<SourceKind>,
    repo: Option<&Repo>,
    asset: Option<&str>,
    nexus_id: Option<&str>,
) -> Result<RemoteAttempt, HubError> {
    let mut source = effective
        .unwrap_or_else(|| choose_update_source(hub, pinned, nexus_id.is_some(), repo.is_some()));
    let mut notes = Vec::new();
    let mut tracker = DownloadTracker::start(item.name.clone());
    let archive = match source {
        SourceKind::Nexus => {
            let id =
                nexus_id.ok_or_else(|| HubError::Update(String::from("mod has no Nexus id")))?;
            match hub.download_nexus_latest_cached_with_progress(id, &mut |progress| {
                tracker.report(progress)
            }) {
                Ok(download) => download.path,
                Err(error) => match repo {
                    Some(repo) if pinned.is_none() => {
                        notes.push(PendingNote::NexusFallback {
                            error: error.to_string(),
                        });
                        source = SourceKind::Github;
                        download_github_archive(hub, repo, asset, &mut |progress| {
                            tracker.report(progress)
                        })?
                    }
                    _ => return Err(error),
                },
            }
        }
        SourceKind::Github => {
            let repo =
                repo.ok_or_else(|| HubError::Update(String::from("mod has no GitHub repository")))?;
            download_github_archive(hub, repo, asset, &mut |progress| tracker.report(progress))?
        }
    };
    let staging = hub.staging_root();
    let label = source_label(source, repo, nexus_id)?;
    let package = match hub.prepare_package_update(&archive, item, &staging, &label) {
        Ok(package) => package,
        Err(HubError::Dependencies(issues)) => return Ok(RemoteAttempt::Blocked(issues)),
        Err(error) => return Err(error),
    };
    let plan = package.preview();
    let game_running = hub.game_running()?;
    Ok(RemoteAttempt::Ready(Box::new(PendingInstall {
        mod_name: item.name.clone(),
        plan,
        package,
        staging,
        notes,
        game_running,
    })))
}

fn source_label(
    source: SourceKind,
    repo: Option<&Repo>,
    nexus_id: Option<&str>,
) -> Result<String, HubError> {
    match source {
        SourceKind::Github => {
            let repo =
                repo.ok_or_else(|| HubError::Update(String::from("mod has no GitHub repository")))?;
            Ok(format!("github:{}", repo.slug()))
        }
        SourceKind::Nexus => {
            let id =
                nexus_id.ok_or_else(|| HubError::Update(String::from("mod has no Nexus id")))?;
            Ok(format!("nexus:{id}"))
        }
    }
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

fn install_one_dependency(hub: &mut Hub, entry: &SourceEntry) -> Result<(), HubError> {
    let RemoteInstall {
        staging, package, ..
    } = hub.prepare_remote_install(entry)?;
    let applied = hub.apply_package(&package);
    hub.discard_staging(&staging);
    applied.map(|_| ())
}

fn download_github_archive(
    hub: &Hub,
    repo: &Repo,
    asset: Option<&str>,
    progress: &mut dyn FnMut(DownloadProgress),
) -> Result<PathBuf, HubError> {
    let destination = hub.download_cache();
    hub.download_github_asset_with_progress(repo, asset, &destination, progress)
}

impl Ui {
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
        } else if pending.is_local() {
            t!("local.import_applying")
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
                let package = pending.package.clone();
                let worker_hub = hub.clone();
                let result = blocking(move || {
                    let mut hub = worker_hub
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    hub.apply_package(&package)
                })
                .await;
                let operation = if pending.is_local() {
                    "task.issue_import"
                } else {
                    "task.issue_install"
                };
                match result {
                    Ok(Ok(report)) => {
                        ui.clear_task_failure(operation);
                        let staging = pending.staging.clone();
                        let cleanup_hub = hub.clone();
                        let _ = blocking(move || {
                            let hub = cleanup_hub
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            hub.discard_staging(&staging);
                        })
                        .await;
                        let count = report.files;
                        let backup = report
                            .backup
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| t!("common.none"));
                        let name = pending.mod_name.clone();
                        ui.pending_install.set(None);
                        ui.apply_status.set(String::new());
                        ui.refresh_mods();
                        if pending.is_local() || pending.is_thunderstore() {
                            ui.profiles_refresh();
                            let key = if pending.is_local() {
                                "local.import_installed"
                            } else {
                                "remote.thunderstore_installed"
                            };
                            ui.notice
                                .set(t!(key, name = name, profile = report.profile));
                        } else {
                            ui.notice.set(t!(
                                "notice.updated",
                                name = name,
                                count = count,
                                backup = backup
                            ));
                        }
                        break;
                    }
                    Ok(Err(HubError::GameRunning)) => {
                        ui.pending_install.set(Some(PendingInstall {
                            game_running: true,
                            ..pending.clone()
                        }));
                        ui.apply_status.set(String::new());
                        ui.notice.set(if pending.is_local() {
                            t!("local.import_game_running")
                        } else {
                            t!("notice.game_still_running")
                        });
                        break;
                    }
                    Ok(Err(error)) => {
                        ui.task_failed(operation, &error);
                        ui.apply_status.set(String::new());
                        ui.notice.set(error.to_string());
                        break;
                    }
                    Err(error) => {
                        ui.task_failed_with(operation, "task.reason_failed");
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
        self.notice.set(if pending.is_local() {
            t!("local.import_discarded")
        } else {
            t!("notice.discarded")
        });
    }

    pub fn install_dependencies(&self) {
        let Some(prompt) = self.dependency_prompt.get() else {
            return;
        };
        if self.update_busy.get().is_some() || self.apply_running.get() || self.profiles.is_busy() {
            self.notice.set(t!("notice.busy"));
            return;
        }
        if self.remote_busy.get().is_some() {
            return;
        }
        let entries: Vec<SourceEntry> = prompt
            .issues
            .iter()
            .filter(|issue| issue.kind == IssueKind::Missing)
            .filter_map(|issue| issue.entry.clone())
            .collect();
        if entries.is_empty() {
            return;
        }
        let target = prompt.target.clone();
        let fallback_target = prompt.target.clone();
        self.remote_busy.set(Some(String::from("dependencies")));
        self.notice.set(t!("dependency.installing"));
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || -> Result<Option<RemoteAttempt>, HubError> {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                for entry in &entries {
                    install_one_dependency(&mut hub, entry)?;
                }
                match target {
                    Some(target) => prepare_remote_pending(&hub, target).map(Some),
                    None => Ok(None),
                }
            })
            .await;
            ui.remote_busy.set(None);
            match result {
                Ok(Ok(Some(RemoteAttempt::Ready(pending)))) => {
                    let name = pending.mod_name.clone();
                    if accept_pending(&ui, *pending) {
                        ui.clear_task_failure("task.dependency_title");
                        ui.dependency_prompt.set(None);
                        ui.notice.set(t!("remote.ready", name = name));
                    }
                }
                Ok(Ok(Some(RemoteAttempt::Blocked(issues)))) => {
                    ui.dependency_prompt.set(Some(DependencyPrompt {
                        target: fallback_target,
                        issues,
                    }));
                    ui.notice.set(t!("dependency.still_blocked"));
                }
                Ok(Ok(None)) => {
                    ui.clear_task_failure("task.dependency_title");
                    ui.dependency_prompt.set(None);
                    ui.notice.set(t!("dependency.installed_retry"));
                }
                Ok(Err(error)) => {
                    ui.task_failed("task.dependency_title", &error);
                    ui.notice.set(t!("dependency.failed", error = error));
                }
                Err(error) => {
                    ui.task_failed_with("task.dependency_title", "task.reason_failed");
                    ui.notice.set(t!("dependency.failed", error = error));
                }
            }
        });
    }

    pub fn dismiss_dependency_prompt(&self) {
        self.dependency_prompt.set(None);
        self.notice.set(String::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_label_carries_the_repository_slug() {
        let repo = Repo::parse("CNCUMC/Bark").unwrap();
        assert_eq!(
            source_label(SourceKind::Github, Some(&repo), None).unwrap(),
            "github:CNCUMC/Bark"
        );
    }

    #[test]
    fn nexus_label_carries_the_mod_id() {
        assert_eq!(
            source_label(SourceKind::Nexus, None, Some("12345")).unwrap(),
            "nexus:12345"
        );
    }

    #[test]
    fn github_label_requires_a_repository() {
        assert!(matches!(
            source_label(SourceKind::Github, None, None),
            Err(HubError::Update(_))
        ));
    }

    #[test]
    fn nexus_label_requires_a_mod_id() {
        assert!(matches!(
            source_label(SourceKind::Nexus, None, None),
            Err(HubError::Update(_))
        ));
    }
}
