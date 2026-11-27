use std::path::PathBuf;

use cuprum_core::hub::{Hub, HubError};
use nsh_core::file_dialog::{FileFilter, OpenDialog};
use nsh_core::prelude::*;

use super::install::{DependencyPrompt, PendingInstall, RemoteAttempt, accept_pending};
use super::state::Ui;

impl Ui {
    pub fn can_import_local(&self) -> bool {
        !self.settings.get().game_path.trim().is_empty()
            && !self.mod_files_busy()
            && !self.profiles.is_busy()
            && !self.bepinex_busy.get()
            && self.bepinex_pending.get().is_none()
            && !self.bridge_install_busy.get()
            && self.dependency_prompt.get().is_none()
    }

    pub fn pick_local_mod(&self) {
        if !self.can_import_local() {
            return;
        }
        self.local_import_busy.set(true);
        let picked = pick_paths(
            OpenDialog::files()
                .filter(FileFilter::new(
                    t!("local.import_filter"),
                    ["zip", "7z", "rar", "dll"],
                ))
                .title(t!("local.import_title")),
        );
        let ui = self.clone();
        self.spawn_worker(async move {
            match picked.await {
                Ok(Some(paths)) => {
                    if let Some(path) = paths.into_iter().next() {
                        ui.prepare_local_mod(path).await;
                    }
                }
                Ok(None) => {}
                Err(error) => ui.notice.set(t!("local.import_failed", error = error)),
            }
            ui.local_import_busy.set(false);
        });
    }

    async fn prepare_local_mod(&self, path: PathBuf) {
        self.notice.set(t!("local.import_preparing"));
        let hub = self.hub.clone();
        let result = blocking(move || {
            let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            local_pending(&hub, path)
        })
        .await;
        match result {
            Ok(Ok(RemoteAttempt::Ready(pending))) => {
                if accept_pending(self, *pending) {
                    self.clear_task_failure("task.issue_import");
                    self.notice.set(t!("local.import_ready"));
                }
            }
            Ok(Ok(RemoteAttempt::Blocked(issues))) => {
                self.dependency_prompt.set(Some(DependencyPrompt {
                    target: None,
                    issues,
                }));
                self.notice.set(t!("dependency.blocked"));
            }
            Ok(Err(error)) => {
                self.task_failed("task.issue_import", &error);
                self.notice.set(t!("local.import_failed", error = error));
            }
            Err(error) => {
                self.task_failed_with("task.issue_import", "task.reason_failed");
                self.notice.set(t!("local.import_failed", error = error));
            }
        }
    }
}

fn local_pending(hub: &Hub, path: PathBuf) -> Result<RemoteAttempt, HubError> {
    let game_running = hub.game_running()?;
    let package = match hub.prepare_local_import(&path) {
        Ok(package) => package,
        Err(HubError::Dependencies(issues)) => return Ok(RemoteAttempt::Blocked(issues)),
        Err(error) => return Err(error),
    };
    let mod_name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(&package.mod_id);
    Ok(RemoteAttempt::Ready(Box::new(PendingInstall {
        mod_name: mod_name.to_owned(),
        source: None,
        plan: package.preview(),
        staging: package.staging.clone(),
        package,
        notes: Vec::new(),
        game_running,
    })))
}
