//! Mod list actions: toggles, source checks and pins, details and launch.

use super::install::{DependencyPrompt, RemoteAttempt, accept_pending, prepare_update};
use super::rows::CheckState;
use super::rows::build_local_rows;
use super::rows::failure_notice;
use super::rows::release_notice;
use super::rows::source_kind_label;
use super::state::Ui;
use cuhub_core::github::CheckFailure;
use cuhub_core::hub::CheckOutcome;
use cuhub_core::sources::{Repo, SourceKind};
use nsh_core::prelude::*;

impl Ui {
    pub fn rebuild_local_rows(&self) {
        let rows = {
            let hub = self.hub();
            build_local_rows(&hub, &self.mods.get(), &self.checks.get())
        };
        self.local_rows.set(rows);
    }

    pub fn set_mod_enabled(&self, key: String, enabled: bool) {
        if self.pending_install.get().is_some() {
            self.notice.set(t!("notice.pending_first"));
            return;
        }
        if self.install_preparing() {
            self.notice.set(t!("notice.busy"));
            return;
        }
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
        if self.pending_install.get().is_some() {
            self.notice.set(t!("notice.pending_first"));
            return;
        }
        if self.install_preparing() {
            self.notice.set(t!("notice.busy"));
            return;
        }
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

    pub fn install_preparing(&self) -> bool {
        self.update_busy.get().is_some()
            || self.remote_busy.get().is_some()
            || self.apply_running.get()
    }

    pub fn mod_files_busy(&self) -> bool {
        self.toggle_busy.get().is_some()
            || self.bulk_busy.get()
            || self.delete_busy.get()
            || self.delete_prompt.get().is_some()
            || self.pending_install.get().is_some()
            || self.install_preparing()
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
        if self.remote_busy.get().is_some()
            || self.apply_running.get()
            || self.dependency_prompt.get().is_some()
            || self.profiles.is_busy()
        {
            self.notice.set(t!("notice.busy"));
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
            ui.update_busy.set(None);
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
                    Some(kind) => t!("notice.pinned", kind = source_kind_label(kind)),
                    None => t!("notice.pin_auto"),
                });
            }
            Err(error) => self
                .notice
                .set(t!("notice.save_source_failed", error = error)),
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
                self.refresh_bepinex();
            }
            None => self.notice.set(t!("notice.game_not_found")),
        }
    }
}
