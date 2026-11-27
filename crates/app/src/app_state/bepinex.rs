//! BepInEx status, install and uninstall flow.

use super::state::Ui;
use super::tasks::DownloadTracker;
use cuprum_core::hub::HubError;
use cuprum_core::install::{Flavor, Mode};
use nsh_core::prelude::*;
use std::sync::Arc;

impl Ui {
    pub fn refresh_bepinex(&self) {
        let hub = Arc::clone(&self.hub);
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.bepinex_status()
            })
            .await;
            match result {
                Ok(Ok(status)) => {
                    ui.bepinex_status_text.set(String::new());
                    ui.bepinex.set(Some(status));
                }
                Ok(Err(HubError::NoGameFolder)) => {
                    ui.bepinex_status_text.set(String::new());
                    ui.bepinex.set(None);
                }
                Ok(Err(error)) => {
                    ui.bepinex.set(None);
                    ui.bepinex_status_text.set(error.to_string());
                }
                Err(error) => ui
                    .bepinex_status_text
                    .set(t!("notice.bepinex_worker_stopped", error = error)),
            }
        });
    }

    pub fn set_bepinex_flavor(&self, flavor: Flavor) {
        self.bepinex_flavor.set(flavor);
        self.bepinex_status_text.set(String::new());
    }

    pub fn prepare_bepinex(&self, mode: Mode) {
        if self.bepinex_busy.get() || self.bepinex_pending.get().is_some() {
            return;
        }
        self.bepinex_busy.set(true);
        self.bepinex_status_text.set(t!("notice.bepinex_preparing"));
        let flavor = self.bepinex_flavor.get();
        let label = t!("task.bepinex_title");
        let hub = Arc::clone(&self.hub);
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let mut tracker = DownloadTracker::start(label);
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.prepare_bepinex_with_progress(flavor, mode, &mut |progress| {
                    tracker.report(progress)
                })
            })
            .await;
            match result {
                Ok(Ok(install)) => {
                    ui.clear_task_failure("task.bepinex_title");
                    ui.bepinex_status_text.set(String::new());
                    ui.bepinex_pending.set(Some(install));
                }
                Ok(Err(error @ HubError::GameRunning)) => {
                    ui.task_failed("task.bepinex_title", &error);
                    ui.bepinex_status_text
                        .set(t!("notice.bepinex_game_running"));
                }
                Ok(Err(error @ HubError::NoGameFolder)) => {
                    ui.task_failed("task.bepinex_title", &error);
                    ui.bepinex_status_text
                        .set(t!("notice.bepinex_no_game_folder"));
                }
                Ok(Err(error)) => {
                    ui.task_failed("task.bepinex_title", &error);
                    ui.bepinex_status_text.set(error.to_string());
                }
                Err(error) => {
                    ui.task_failed_with("task.bepinex_title", "task.reason_failed");
                    ui.bepinex_status_text
                        .set(t!("notice.bepinex_worker_stopped", error = error));
                }
            }
            ui.bepinex_busy.set(false);
        });
    }

    pub fn apply_bepinex(&self) {
        let Some(install) = self.bepinex_pending.get() else {
            return;
        };
        if self.bepinex_busy.get() {
            return;
        }
        self.bepinex_busy.set(true);
        self.bepinex_status_text.set(t!("notice.bepinex_applying"));
        let hub = Arc::clone(&self.hub);
        let ui = self.clone();
        self.spawn_worker(async move {
            let plan = install.plan.clone();
            let apply_hub = Arc::clone(&hub);
            let applied = blocking(move || {
                let hub = apply_hub
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.apply_install(&plan)
            })
            .await;
            match applied {
                Ok(Ok(_)) => {
                    let finish_hub = Arc::clone(&hub);
                    let finished = blocking(move || {
                        let hub = finish_hub
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        hub.finish_bepinex(&install)
                    })
                    .await;
                    match finished {
                        Ok(Ok(_)) => {
                            ui.clear_task_failure("task.bepinex_title");
                            ui.bepinex_pending.set(None);
                            ui.bepinex_status_text.set(String::new());
                            ui.notice.set(t!("notice.bepinex_installed"));
                            ui.refresh_bepinex();
                            ui.refresh_mods();
                            ui.refresh_bridge_install();
                        }
                        Ok(Err(error)) => {
                            ui.task_failed("task.bepinex_title", &error);
                            ui.bepinex_status_text.set(error.to_string());
                        }
                        Err(error) => {
                            ui.task_failed_with("task.bepinex_title", "task.reason_failed");
                            ui.bepinex_status_text
                                .set(t!("notice.bepinex_worker_stopped", error = error));
                        }
                    }
                }
                Ok(Err(error @ HubError::GameRunning)) => {
                    ui.task_failed("task.bepinex_title", &error);
                    ui.bepinex_status_text
                        .set(t!("notice.bepinex_game_running"));
                }
                Ok(Err(error)) => {
                    ui.task_failed("task.bepinex_title", &error);
                    ui.bepinex_status_text.set(error.to_string());
                }
                Err(error) => {
                    ui.task_failed_with("task.bepinex_title", "task.reason_failed");
                    ui.bepinex_status_text
                        .set(t!("notice.bepinex_worker_stopped", error = error));
                }
            }
            ui.bepinex_busy.set(false);
        });
    }

    pub fn cancel_bepinex(&self) {
        if self.bepinex_busy.get() {
            return;
        }
        let Some(install) = self.bepinex_pending.get() else {
            return;
        };
        {
            let hub = self.hub();
            hub.discard_staging(&install.staging);
        }
        self.bepinex_pending.set(None);
        self.bepinex_status_text.set(String::new());
        self.notice.set(t!("notice.bepinex_cancelled"));
    }
}
