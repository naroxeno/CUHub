//! Delete confirmation flow for one mod or every mod.

use super::state::Ui;
use cuhub_core::mods::{DeleteScope, InstalledMod};
use nsh_core::prelude::*;

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

impl Ui {
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
}
