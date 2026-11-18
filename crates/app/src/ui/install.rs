//! Pending install staging, archive download and apply/discard.

use super::state::Ui;
use cuhub_core::hub::{Hub, HubError, RemoteInstall};
use cuhub_core::install::UpgradePlan;
use cuhub_core::mods::{DependencyIssue, InstalledMod, IssueKind};
use cuhub_core::sources::{Repo, SourceEntry, SourceKind};
use nsh_core::prelude::*;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, PartialEq, Eq)]
pub enum PendingNote {
    NexusFallback { error: String },
}

impl PendingNote {
    pub fn text(&self) -> String {
        match self {
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
    pub source: SourceKind,
    pub plan: UpgradePlan,
    pub staging: PathBuf,
    pub notes: Vec<PendingNote>,
    pub game_running: bool,
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
    Ready(PendingInstall),
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
        }) => {
            let game_running = hub.game_running()?;
            Ok(RemoteAttempt::Ready(PendingInstall {
                mod_name: name,
                source: SourceKind::Github,
                plan,
                staging,
                notes: Vec::new(),
                game_running,
            }))
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
    let archive = match source {
        SourceKind::Nexus => {
            let id =
                nexus_id.ok_or_else(|| HubError::Update(String::from("mod has no Nexus id")))?;
            match hub.download_nexus_latest(id) {
                Ok(download) => download.path,
                Err(error) => match repo {
                    Some(repo) if pinned.is_none() => {
                        notes.push(PendingNote::NexusFallback {
                            error: error.to_string(),
                        });
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
    let plan = match hub.plan_download(&archive, item, &staging) {
        Ok(plan) => plan,
        Err(HubError::Dependencies(issues)) => return Ok(RemoteAttempt::Blocked(issues)),
        Err(error) => return Err(error),
    };
    let game_running = hub.game_running()?;
    Ok(RemoteAttempt::Ready(PendingInstall {
        mod_name: item.name.clone(),
        source,
        plan,
        staging,
        notes,
        game_running,
    }))
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

fn install_one_dependency(hub: &Hub, entry: &SourceEntry) -> Result<(), HubError> {
    let RemoteInstall { staging, plan, .. } = hub.prepare_remote_install(entry)?;
    let applied = hub.apply_install(&plan);
    hub.discard_staging(&staging);
    applied.map(|_| ())
}

fn download_github_archive(
    hub: &Hub,
    repo: &Repo,
    asset: Option<&str>,
) -> Result<PathBuf, HubError> {
    let destination = hub.app_data().join("Downloads");
    hub.download_github_asset(repo, asset, &destination)
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
        } else {
            t!("notice.applying")
        });
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let mut game_wait_attempts = 0u32;
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
                        game_wait_attempts += 1;
                        if game_wait_attempts >= 60 {
                            ui.apply_status.set(String::new());
                            ui.notice.set(t!("notice.game_still_running"));
                            break;
                        }
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
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                for entry in &entries {
                    install_one_dependency(&hub, entry)?;
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
                    if accept_pending(&ui, pending) {
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
                    ui.dependency_prompt.set(None);
                    ui.notice.set(t!("dependency.installed_retry"));
                }
                Ok(Err(error)) => ui.notice.set(t!("dependency.failed", error = error)),
                Err(error) => ui.notice.set(t!("dependency.failed", error = error)),
            }
        });
    }

    pub fn dismiss_dependency_prompt(&self) {
        self.dependency_prompt.set(None);
        self.notice.set(String::new());
    }
}
