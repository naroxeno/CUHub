//! Bridge install entry: bundled archive lookup, zip picker and the Hub install worker.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cuhub_core::hub::{BridgeInstallStatus, HubError};
use nsh_core::file_dialog::{FileFilter, OpenDialog};
use nsh_core::prelude::*;

use super::state::Ui;

const ARCHIVE_NAME: &str = "CuhubBridge.zip";
const WORKSPACE_ARCHIVE: &str = "dist/bridge/CuhubBridge.zip";

impl Ui {
    pub fn refresh_bridge_install(&self) {
        let hub = Arc::clone(&self.hub);
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.bridge_install_status()
            })
            .await;
            match result {
                Ok(Ok(status)) => ui.bridge_install.set(Some(status)),
                Ok(Err(_)) | Err(_) => ui.bridge_install.set(None),
            }
        });
    }

    pub fn start_bridge_install(&self) {
        if self.bridge_install_busy.get() {
            return;
        }
        if self.settings.get().game_path.trim().is_empty() {
            self.bridge_install_text
                .set(t!("local.bridge_no_game_folder"));
            return;
        }
        match bundled_archive() {
            Some(archive) => self.begin_bridge_install(archive),
            None => self.pick_bridge_archive(),
        }
    }

    pub fn bridge_install_label(&self) -> String {
        match self.bridge_install.get() {
            Some(status) if status.installed => match status.version {
                Some(version) => t!("local.bridge_status_installed", version = version),
                None => t!("local.bridge_status_installed_unknown"),
            },
            _ => t!("local.bridge_status_not_installed"),
        }
    }

    fn begin_bridge_install(&self, archive: PathBuf) {
        if self.bridge_install_busy.get() {
            return;
        }
        announce(self, &archive);
        let ui = self.clone();
        self.spawn_worker(async move {
            install_archive(ui, archive).await;
        });
    }

    fn pick_bridge_archive(&self) {
        let ui = self.clone();
        let picked = pick_paths(
            OpenDialog::files()
                .filter(FileFilter::new("CuhubBridge", ["zip"]))
                .title(t!("local.bridge_pick_title")),
        );
        self.spawn_worker(async move {
            match picked.await {
                Ok(Some(paths)) => {
                    let Some(archive) = paths.into_iter().next() else {
                        return;
                    };
                    if ui.bridge_install_busy.get() {
                        return;
                    }
                    announce(&ui, &archive);
                    install_archive(ui, archive).await;
                }
                Ok(None) => {}
                Err(error) => ui
                    .bridge_install_text
                    .set(t!("local.bridge_picker_failed", error = error)),
            }
        });
    }
}

fn announce(ui: &Ui, archive: &Path) {
    let name = archive
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| archive.display().to_string());
    ui.bridge_install_busy.set(true);
    ui.bridge_install_text
        .set(t!("local.bridge_installing_from", name = name));
}

async fn install_archive(ui: Ui, archive: PathBuf) {
    let hub = Arc::clone(&ui.hub);
    let result = blocking(move || {
        let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        hub.install_bridge(&archive)
    })
    .await;
    match result {
        Ok(Ok(report)) => {
            ui.bridge_install.set(Some(BridgeInstallStatus {
                installed: true,
                version: report.version.clone(),
                target: report.target,
            }));
            ui.bridge_install_text.set(String::new());
            ui.refresh_mods();
            ui.notice.set(match report.version {
                Some(version) => t!("local.bridge_installed_notice", version = version),
                None => t!("local.bridge_installed_notice_unknown"),
            });
        }
        Ok(Err(HubError::GameRunning)) => {
            ui.bridge_install_text.set(t!("local.bridge_game_running"))
        }
        Ok(Err(HubError::NoGameFolder)) => ui
            .bridge_install_text
            .set(t!("local.bridge_no_game_folder")),
        Ok(Err(error)) => ui.bridge_install_text.set(error.to_string()),
        Err(error) => ui
            .bridge_install_text
            .set(t!("local.bridge_worker_stopped", error = error)),
    }
    ui.bridge_install_busy.set(false);
}

fn bundled_archive() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join(ARCHIVE_NAME));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(WORKSPACE_ARCHIVE));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(WORKSPACE_ARCHIVE),
    );
    candidates.into_iter().find(|path| path.is_file())
}
