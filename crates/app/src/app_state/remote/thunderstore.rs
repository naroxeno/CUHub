use cuprum_core::thunderstore::{Catalogue, Package};
use nsh_core::prelude::*;

use super::{DependencyPrompt, PendingInstall, RemoteRow, Ui, accept_pending};
use crate::app_state::install::PendingNote;
use crate::app_state::tasks::DownloadTracker;
use cuprum_core::hub::{DependencyPlan, HubError};

pub(in crate::app_state) fn append_thunderstore(
    rows: &mut Vec<RemoteRow>,
    packages: &[Package],
    installed: &[(String, String)],
) {
    rows.extend(packages.iter().map(|package| RemoteRow {
        key: format!("thunderstore:{}", package.full_name),
        github: None,
        nexus: None,
        installed: None,
        thunderstore: Some(package.clone()),
        thunderstore_version: installed.iter().find_map(|(name, version)| {
            name.eq_ignore_ascii_case(&package.full_name)
                .then(|| version.clone())
        }),
    }));
}

enum Prepared {
    Preview(DependencyPlan),
    Single(Box<PendingInstall>),
}

impl Ui {
    pub fn apply_thunderstore_dependencies(&self, confirm_local: bool) {
        if self.mod_files_busy()
            || self.profiles.is_busy()
            || self.dependency_prompt.get().is_some()
        {
            self.notice.set(t!("notice.busy"));
            return;
        }
        let Some(plan) = self.thunderstore_dependency_plan.get() else {
            return;
        };
        if !plan.installable() {
            return;
        }
        self.clear_task_failure("task.issue_install");
        let profile = plan.profile.clone();
        let name = plan
            .entries
            .last()
            .map(|entry| entry.package.clone())
            .unwrap_or_default();
        self.remote_busy.set(Some(format!("thunderstore:{name}")));
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let mut tracker = DownloadTracker::start(name.clone());
            let result = blocking(move || {
                hub.lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .apply_thunderstore_dependencies(&plan, confirm_local, &mut |progress| {
                        tracker.report(progress)
                    })
            })
            .await;
            ui.remote_busy.set(None);
            let interrupted = ui.hub().interrupted_thunderstore_plan();
            ui.thunderstore_recovery
                .set(matches!(interrupted, Ok(Some(_))));
            if let Ok(Some(plan)) = interrupted {
                ui.thunderstore_dependency_plan.set(Some(plan));
            }
            match result {
                Ok(Ok(())) => {
                    if !ui.thunderstore_recovery.get() {
                        ui.thunderstore_dependency_plan.set(None);
                    }
                    ui.clear_task_failure("task.issue_install");
                    ui.refresh_mods();
                    ui.profiles_refresh();
                    ui.notice.set(t!(
                        "remote.thunderstore_installed",
                        name = name,
                        profile = profile
                    ));
                }
                Ok(Err(error)) => {
                    ui.task_failed("task.issue_install", &error);
                    ui.thunderstore_install_error.set(Some(error.to_string()));
                    ui.notice.set(t!("remote.failed", error = error));
                }
                Err(error) => {
                    ui.task_failed_with("task.issue_install", "task.reason_failed");
                    ui.thunderstore_install_error.set(Some(error.to_string()));
                    ui.notice.set(t!("remote.failed", error = error));
                }
            }
        });
    }

    pub fn refresh_thunderstore(&self) {
        if self.thunderstore_refreshing.get() {
            return;
        }
        self.thunderstore_refreshing.set(true);
        let catalogue = Catalogue::new(std::path::Path::new(&self.app_data));
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || catalogue.refresh()).await;
            match result {
                Ok(Ok(packages)) => {
                    ui.thunderstore_packages.set(packages);
                    ui.thunderstore_error.set(None);
                    ui.rebuild_remote();
                }
                Ok(Err(error)) => ui.thunderstore_error.set(Some(error.to_string())),
                Err(error) => ui.thunderstore_error.set(Some(error.to_string())),
            }
            ui.thunderstore_refreshing.set(false);
        });
    }

    pub fn install_thunderstore(&self, id: String, name: String, version: String) {
        if self.install_preparing() || self.profiles.is_busy() {
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
        if self.thunderstore_recovery.get() {
            self.task_focus.set(Some("thunderstore-dependencies"));
            self.task_open.set(true);
            return;
        }
        self.thunderstore_dependency_plan.set(None);
        self.remote_busy.set(Some(format!("thunderstore:{id}")));
        self.notice.set(t!("remote.preparing", name = name.clone()));
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let mut tracker = DownloadTracker::start(name.clone());
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(receipt) = hub.interrupted_thunderstore_plan()? {
                    return Ok(Prepared::Preview(receipt));
                }
                let mut dependencies = hub.plan_thunderstore_dependencies(&id, &version)?;
                if !dependencies.ready_for_single_install() {
                    return Ok(Prepared::Preview(dependencies));
                }
                dependencies.entries.pop();
                let game_running = hub.game_running()?;
                let package = hub.prepare_thunderstore(&id, &version, &mut |progress| {
                    tracker.report(progress)
                })?;
                Ok::<_, HubError>(Prepared::Single(Box::new(PendingInstall {
                    mod_name: name,
                    plan: package.preview(),
                    staging: package.staging.clone(),
                    package,
                    notes: dependencies
                        .entries
                        .into_iter()
                        .map(PendingNote::ThunderstoreDependency)
                        .collect(),
                    game_running,
                })))
            })
            .await;
            ui.remote_busy.set(None);
            match result {
                Ok(Ok(Prepared::Preview(plan))) => {
                    let recovering = ui
                        .hub()
                        .interrupted_thunderstore_plan()
                        .is_ok_and(|p| p.is_some());
                    ui.thunderstore_recovery.set(recovering);
                    ui.clear_task_failure("task.issue_install");
                    ui.thunderstore_dependency_plan.set(Some(plan));
                    ui.task_focus.set(Some("thunderstore-dependencies"));
                    ui.task_open.set(true);
                }
                Ok(Ok(Prepared::Single(pending))) => {
                    ui.clear_task_failure("task.issue_install");
                    let name = pending.mod_name.clone();
                    if accept_pending(&ui, *pending) {
                        ui.notice.set(t!("remote.ready", name = name));
                    }
                }
                Ok(Err(cuprum_core::hub::HubError::Dependencies(issues))) => {
                    ui.dependency_prompt.set(Some(DependencyPrompt {
                        target: None,
                        issues,
                    }));
                    ui.notice.set(t!("dependency.blocked"));
                }
                Ok(Err(error)) => {
                    ui.task_failed("task.issue_install", &error);
                    ui.thunderstore_install_error.set(Some(error.to_string()));
                    ui.notice.set(t!("remote.failed", error = error));
                }
                Err(error) => {
                    ui.task_failed_with("task.issue_install", "task.reason_failed");
                    ui.thunderstore_install_error.set(Some(error.to_string()));
                    ui.notice.set(t!("remote.failed", error = error));
                }
            }
        });
    }
}
