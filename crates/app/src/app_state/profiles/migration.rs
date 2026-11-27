//! Drives migration workers and keeps each row's result visible.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cuprum_core::hub::{HubError, MigrationPreview, MigrationRow, MigrationSession};
use cuprum_core::packages::{PackageError, PackageReport};
use cuprum_core::profiles::ProfileError;
use cuprum_core::sources::SourceEntry;
use nsh_core::prelude::*;

use super::{ProfilesBusy, Ui};

#[derive(Clone, PartialEq, Eq)]
pub enum MigrationResult {
    Done(PathBuf),
    Failed(String),
    ProfileFailed(String),
}

#[derive(Clone, Copy)]
pub struct MigrationUi {
    pub session: Signal<Option<MigrationSession>>,
    pub rows: Signal<Vec<MigrationRow>>,
    pub results: Signal<BTreeMap<PathBuf, MigrationResult>>,
    pub preview: Signal<Option<MigrationPreview>>,
    pub local_confirm: Signal<bool>,
    pub error: Signal<String>,
}

impl MigrationUi {
    pub(super) fn new() -> Self {
        Self {
            session: signal(None),
            rows: signal(Vec::new()),
            results: signal(BTreeMap::new()),
            preview: signal(None),
            local_confirm: signal(false),
            error: signal(String::new()),
        }
    }

    pub fn remaining(&self) -> Vec<MigrationRow> {
        let results = self.results.get();
        self.rows
            .get()
            .into_iter()
            .filter(|row| row.local.is_ok() && actionable(results.get(&row.path)))
            .collect()
    }

    pub fn can_act(&self, path: &Path) -> bool {
        actionable(self.results.get().get(path))
    }
}

fn actionable(result: Option<&MigrationResult>) -> bool {
    !matches!(
        result,
        Some(MigrationResult::Done(_) | MigrationResult::ProfileFailed(_))
    )
}

fn commit_failed(error: &PackageError) -> bool {
    match error {
        PackageError::Profile(ProfileError::Commit { .. }) => true,
        PackageError::AfterBackup { source, .. } => commit_failed(source),
        _ => false,
    }
}

fn row_result(result: Result<PackageReport, HubError>) -> MigrationResult {
    match result {
        Ok(report) => MigrationResult::Done(report.backup.unwrap_or_default()),
        Err(HubError::Package(ref error)) if commit_failed(error) => {
            MigrationResult::ProfileFailed(error.to_string())
        }
        Err(error) => MigrationResult::Failed(error.to_string()),
    }
}

impl Ui {
    pub(crate) fn adopt_scan(&self) {
        if self.profiles.is_busy() {
            return;
        }
        let migration = self.profiles.migration;
        let preview = migration.preview.get();
        migration.preview.set(None);
        migration.session.set(None);
        migration.rows.set(Vec::new());
        migration.results.set(BTreeMap::new());
        migration.local_confirm.set(false);
        migration.error.set(String::new());
        self.profiles.busy.set(ProfilesBusy::Scan);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Some(preview) = preview {
                    hub.discard_migration_preview(&preview);
                }
                hub.scan_migration()
            })
            .await;
            match result {
                Ok(Ok((session, rows))) => {
                    migration.rows.set(rows);
                    migration.session.set(Some(session));
                }
                Ok(Err(error)) => migration.error.set(error.to_string()),
                Err(_) => migration
                    .error
                    .set(t!("profiles.error_generic").to_string()),
            }
            ui.profiles.busy.set(ProfilesBusy::None);
        });
    }

    pub(crate) fn adopt_preview_github(&self, row: MigrationRow, entry: SourceEntry) {
        let migration = self.profiles.migration;
        if !self.adopt_ready() || !migration.can_act(&row.path) || migration.preview.get().is_some()
        {
            return;
        }
        let Some(session) = migration.session.get() else {
            return;
        };
        migration.local_confirm.set(false);
        migration.error.set(String::new());
        self.profiles.busy.set(ProfilesBusy::Adopt);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let path = row.path.clone();
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.prepare_migration_reinstall(&session, &row, &entry)
            })
            .await;
            match result {
                Ok(Ok(preview)) => {
                    migration.results.update(|results| {
                        results.remove(&path);
                    });
                    migration.preview.set(Some(preview));
                }
                Ok(Err(error)) => ui.adopt_result(path, MigrationResult::Failed(error.to_string())),
                Err(_) => migration
                    .error
                    .set(t!("profiles.error_generic").to_string()),
            }
            ui.profiles.busy.set(ProfilesBusy::None);
        });
    }

    pub(crate) fn adopt_cancel_github(&self) {
        if self.profiles.is_busy() {
            return;
        }
        let migration = self.profiles.migration;
        let Some(preview) = migration.preview.get() else {
            return;
        };
        migration.preview.set(None);
        self.profiles.busy.set(ProfilesBusy::Adopt);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let _ = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.discard_migration_preview(&preview);
            })
            .await;
            ui.profiles.busy.set(ProfilesBusy::None);
        });
    }

    pub(crate) fn adopt_confirm_github(&self) {
        if !self.adopt_ready() {
            return;
        }
        let migration = self.profiles.migration;
        let (Some(mut session), Some(preview)) = (migration.session.get(), migration.preview.get())
        else {
            return;
        };
        migration.preview.set(None);
        migration.error.set(String::new());
        self.profiles.busy.set(ProfilesBusy::Adopt);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let path = preview.row().path.clone();
            let result = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let result = hub.apply_migration_reinstall(&mut session, &preview);
                (session, result)
            })
            .await;
            match result {
                Ok((session, result)) => {
                    migration.session.set(Some(session));
                    ui.adopt_result(path, row_result(result));
                }
                Err(_) => migration
                    .error
                    .set(t!("profiles.error_generic").to_string()),
            }
            ui.adopt_refresh();
        });
    }

    pub(crate) fn adopt_confirm_local(&self) {
        if !self.adopt_ready() {
            return;
        }
        let migration = self.profiles.migration;
        let Some(mut session) = migration.session.get() else {
            return;
        };
        if !migration.local_confirm.get() || migration.preview.get().is_some() {
            return;
        }
        let rows = migration.remaining();
        migration.local_confirm.set(false);
        migration.error.set(String::new());
        self.profiles.busy.set(ProfilesBusy::Adopt);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let results = hub
                    .apply_migration_local(&mut session, &rows)
                    .map(|results| {
                        rows.into_iter()
                            .zip(results)
                            .map(|(row, result)| (row.path, row_result(result)))
                            .collect::<Vec<_>>()
                    });
                (session, results)
            })
            .await;
            match result {
                Ok((session, Ok(results))) => {
                    migration.session.set(Some(session));
                    migration.results.update(|current| current.extend(results));
                }
                Ok((_, Err(error))) => migration.error.set(error.to_string()),
                Err(_) => migration
                    .error
                    .set(t!("profiles.error_generic").to_string()),
            }
            ui.adopt_refresh();
        });
    }

    fn adopt_result(&self, path: PathBuf, result: MigrationResult) {
        self.profiles.migration.results.update(|results| {
            results.insert(path, result);
        });
    }

    fn adopt_ready(&self) -> bool {
        if self.profiles.is_busy() {
            return false;
        }
        let key = if self.pending_install.get().is_some() {
            "notice.pending_first"
        } else if self.install_preparing() {
            "notice.busy"
        } else {
            return true;
        };
        self.profiles.migration.error.set(t!(key).to_string());
        false
    }

    fn adopt_refresh(&self) {
        self.profiles.busy.set(ProfilesBusy::None);
        self.profiles_refresh();
        self.refresh_mods();
    }
}
