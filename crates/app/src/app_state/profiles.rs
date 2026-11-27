//! Profile and content-library state: rows, drafts and the workers behind them.

mod migration;
use cuprum_core::hub::{Hub, HubError};
use cuprum_core::profiles::{ApplyOp, Profile, ProfileEntry, ProfileError};
pub(crate) use migration::MigrationResult;
use nsh_core::prelude::*;
use std::collections::BTreeSet;
use std::io;

use super::Ui;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProfilesBusy {
    None,
    Refresh,
    Create,
    Plan,
    Apply,
    Remove,
    Rename,
    Scan,
    Adopt,
}

#[derive(Clone, PartialEq)]
pub struct ProfileRow {
    pub name: String,
    pub active: bool,
    pub entries: usize,
}

#[derive(Clone, Copy)]
pub struct ProfilesUi {
    pub rows: Signal<Vec<ProfileRow>>,
    pub name: Signal<String>,
    pub busy: Signal<ProfilesBusy>,
    pub error: Signal<(&'static str, String)>,
    pub create_confirm: Signal<String>,
    pub create_snapshot: Signal<bool>,
    pub apply_name: Signal<String>,
    pub apply_count: Signal<usize>,
    apply_ops: Signal<Vec<ApplyOp>>,
    pub remove_name: Signal<String>,
    pub rename_from: Signal<String>,
    pub rename_name: Signal<String>,
    pub migration: migration::MigrationUi,
}

impl ProfilesUi {
    pub fn new() -> Self {
        Self {
            rows: signal(Vec::new()),
            name: signal(String::new()),
            busy: signal(ProfilesBusy::None),
            error: signal(("", String::new())),
            create_confirm: signal(String::new()),
            create_snapshot: signal(false),
            apply_name: signal(String::new()),
            apply_count: signal(0usize),
            apply_ops: signal(Vec::new()),
            remove_name: signal(String::new()),
            rename_from: signal(String::new()),
            rename_name: signal(String::new()),
            migration: migration::MigrationUi::new(),
        }
    }

    fn set_error(&self, key: &'static str) {
        self.error.set((key, String::new()));
    }

    pub fn is_busy(&self) -> bool {
        self.busy.get() != ProfilesBusy::None
    }
}

impl Default for ProfilesUi {
    fn default() -> Self {
        Self::new()
    }
}

enum CreateOutcome {
    Saved,
    Exists,
    InvalidName,
    Failed(&'static str),
}

fn profile_rows(hub: &Hub) -> Vec<ProfileRow> {
    let active = hub.profiles().active().unwrap_or_default();
    hub.profiles()
        .list()
        .into_iter()
        .map(|name| {
            let entries = hub
                .profiles()
                .load(&name)
                .map(|profile| profile.entries.len())
                .unwrap_or(0);
            ProfileRow {
                name: name.clone(),
                active: name == active,
                entries,
            }
        })
        .collect()
}

fn write_profile(hub: &Hub, name: &str, snapshot: bool) -> io::Result<()> {
    let mut profile = Profile::new(name);
    if snapshot {
        let mut seen = BTreeSet::new();
        for (_, entry) in hub.links().entries() {
            if !seen.insert(entry.mod_id.clone()) {
                continue;
            }
            profile.entries.push(ProfileEntry {
                mod_id: entry.mod_id.clone(),
                version: entry.version.clone(),
                enabled: true,
                source: None,
                link: Some(entry.kind),
            });
        }
    }
    profile.active = hub.profiles().active().as_deref() == Some(name);
    hub.profiles().save(&profile)
}

fn create_profile(hub: &Hub, name: &str, snapshot: bool, overwrite: bool) -> CreateOutcome {
    if !overwrite && hub.profiles().load(name).is_some() {
        return CreateOutcome::Exists;
    }
    match write_profile(hub, name, snapshot) {
        Ok(()) => CreateOutcome::Saved,
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => CreateOutcome::InvalidName,
        Err(_) => CreateOutcome::Failed("profiles.error_io"),
    }
}

fn error_key(error: &HubError) -> &'static str {
    match error {
        HubError::NoGameFolder | HubError::Path(_) => "profiles.error_no_game",
        HubError::ProfileNotFound(_) => "profiles.error_not_found",
        HubError::Profile(_) => "profiles.error_profile",
        HubError::GameRunning => "profiles.error_game_running",
        HubError::Io(_) => "profiles.error_io",
        _ => "profiles.error_generic",
    }
}

fn profile_apply_error(error: HubError) -> (&'static str, String) {
    match error {
        HubError::Profile(ProfileError::Commit { source, .. }) => {
            ("profiles.error_activate_after_apply", source.to_string())
        }
        HubError::Profile(ref cause) => ("", cause.to_string()),
        _ => (error_key(&error), String::new()),
    }
}

impl Ui {
    pub(crate) fn profiles_refresh(&self) {
        if self.profiles.is_busy() {
            return;
        }
        self.profiles.busy.set(ProfilesBusy::Refresh);
        self.profiles.set_error("");
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let rows = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                profile_rows(&hub)
            })
            .await;
            match rows {
                Ok(rows) => ui.profiles.rows.set(rows),
                Err(_) => ui.profiles.set_error("profiles.error_generic"),
            }
            ui.profiles.busy.set(ProfilesBusy::None);
        });
    }

    pub(crate) fn profiles_request_create(&self, snapshot: bool) {
        if self.profiles.is_busy() {
            return;
        }
        let name = self.profiles.name.get().trim().to_owned();
        self.profiles.set_error("");
        if name.is_empty() {
            self.profiles.set_error("profiles.error_empty_name");
            return;
        }
        self.profiles.create_confirm.set(String::new());
        self.profiles.busy.set(ProfilesBusy::Create);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let worker_name = name.clone();
            let outcome = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                create_profile(&hub, &worker_name, snapshot, false)
            })
            .await;
            ui.profiles.busy.set(ProfilesBusy::None);
            match outcome {
                Ok(CreateOutcome::Saved) => {
                    ui.profiles.name.set(String::new());
                    ui.profiles.create_confirm.set(String::new());
                    ui.profiles_refresh();
                }
                Ok(CreateOutcome::Exists) => {
                    ui.profiles.create_snapshot.set(snapshot);
                    ui.profiles.create_confirm.set(name);
                }
                Ok(CreateOutcome::InvalidName) => {
                    ui.profiles.set_error("profiles.error_invalid_name")
                }
                Ok(CreateOutcome::Failed(key)) => ui.profiles.set_error(key),
                Err(_) => ui.profiles.set_error("profiles.error_generic"),
            }
        });
    }

    pub(crate) fn profiles_confirm_create(&self) {
        if self.profiles.is_busy() {
            return;
        }
        let name = self.profiles.create_confirm.get();
        if name.is_empty() {
            return;
        }
        let snapshot = self.profiles.create_snapshot.get();
        self.profiles.set_error("");
        self.profiles.busy.set(ProfilesBusy::Create);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let worker_name = name.clone();
            let outcome = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                create_profile(&hub, &worker_name, snapshot, true)
            })
            .await;
            ui.profiles.busy.set(ProfilesBusy::None);
            match outcome {
                Ok(CreateOutcome::Saved) => {
                    ui.profiles.name.set(String::new());
                    ui.profiles.create_confirm.set(String::new());
                    ui.profiles_refresh();
                }
                Ok(CreateOutcome::InvalidName) => {
                    ui.profiles.create_confirm.set(String::new());
                    ui.profiles.set_error("profiles.error_invalid_name");
                }
                Ok(CreateOutcome::Failed(key)) => {
                    ui.profiles.create_confirm.set(String::new());
                    ui.profiles.set_error(key);
                }
                Ok(CreateOutcome::Exists) => ui.profiles.create_confirm.set(String::new()),
                Err(_) => ui.profiles.set_error("profiles.error_generic"),
            }
        });
    }

    pub(crate) fn profiles_cancel_create(&self) {
        self.profiles.create_confirm.set(String::new());
    }

    pub(crate) fn profiles_request_rename(&self, name: &str) {
        if self.profiles.is_busy() {
            return;
        }
        self.profiles.set_error("");
        self.profiles.rename_from.set(name.to_owned());
        self.profiles.rename_name.set(name.to_owned());
    }

    pub(crate) fn profiles_cancel_rename(&self) {
        self.profiles.rename_from.set(String::new());
        self.profiles.rename_name.set(String::new());
    }

    pub(crate) fn profiles_confirm_rename(&self) {
        if self.profiles.is_busy() {
            return;
        }
        let from = self.profiles.rename_from.get();
        if from.is_empty() {
            return;
        }
        let to = self.profiles.rename_name.get().trim().to_owned();
        self.profiles.set_error("");
        if to.is_empty() {
            self.profiles.set_error("profiles.error_empty_name");
            return;
        }
        if to == from {
            self.profiles_cancel_rename();
            return;
        }
        self.profiles.busy.set(ProfilesBusy::Rename);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let worker_from = from.clone();
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.profiles()
                    .rename(&worker_from, &to)
                    .map_err(|error| match error.kind() {
                        io::ErrorKind::InvalidInput => "profiles.error_invalid_name",
                        io::ErrorKind::AlreadyExists => "profiles.error_name_taken",
                        io::ErrorKind::NotFound => "profiles.error_not_found",
                        _ => "profiles.error_io",
                    })
            })
            .await;
            ui.profiles.busy.set(ProfilesBusy::None);
            match result {
                Ok(Ok(())) => {
                    ui.profiles.rename_from.set(String::new());
                    ui.profiles.rename_name.set(String::new());
                    ui.profiles_refresh();
                }
                Ok(Err(key)) => ui.profiles.set_error(key),
                Err(_) => ui.profiles.set_error("profiles.error_generic"),
            }
        });
    }

    pub(crate) fn profiles_request_apply(&self, name: &str) {
        if self.profiles.is_busy() {
            return;
        }
        let name = name.to_owned();
        self.profiles.set_error("");
        self.profiles.busy.set(ProfilesBusy::Plan);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let worker_name = name.clone();
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.plan_profile(&worker_name).map_err(profile_apply_error)
            })
            .await;
            ui.profiles.busy.set(ProfilesBusy::None);
            match result {
                Ok(Ok(ops)) => {
                    ui.profiles.apply_count.set(ops.len());
                    ui.profiles.apply_ops.set(ops);
                    ui.profiles.apply_name.set(name);
                }
                Ok(Err(error)) => {
                    ui.profiles.apply_name.set(String::new());
                    ui.profiles.error.set(error);
                }
                Err(_) => ui.profiles.set_error("profiles.error_generic"),
            }
        });
    }

    pub(crate) fn profiles_cancel_apply(&self) {
        self.profiles.apply_name.set(String::new());
    }

    pub(crate) fn profiles_confirm_apply(&self) {
        if self.profiles.is_busy() {
            return;
        }
        if self.pending_install.get().is_some() {
            self.profiles.set_error("notice.pending_first");
            return;
        }
        if self.install_preparing() || self.toggle_busy.get().is_some() {
            self.profiles.set_error("notice.busy");
            return;
        }
        let name = self.profiles.apply_name.get();
        if name.is_empty() {
            return;
        }
        self.profiles.set_error("");
        self.profiles.busy.set(ProfilesBusy::Apply);
        let confirmed = self.profiles.apply_ops.get();
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let worker_name = name.clone();
            let result = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.apply_confirmed_profile(&worker_name, &confirmed)
                    .map(|report| report.applied.len())
                    .map_err(profile_apply_error)
            })
            .await;
            ui.profiles.busy.set(ProfilesBusy::None);
            match result {
                Ok(Ok(applied)) => {
                    ui.profiles.apply_name.set(String::new());
                    ui.notice.set(t!("profiles.applied", count = applied));
                    ui.profiles_refresh();
                }
                Ok(Err(error)) => {
                    ui.profiles.apply_name.set(String::new());
                    ui.profiles.error.set(error);
                }
                Err(_) => ui.profiles.set_error("profiles.error_generic"),
            }
        });
    }

    pub(crate) fn profiles_request_remove(&self, name: &str) {
        if self.profiles.is_busy() {
            return;
        }
        self.profiles.set_error("");
        self.profiles.remove_name.set(name.to_owned());
    }

    pub(crate) fn profiles_cancel_remove(&self) {
        self.profiles.remove_name.set(String::new());
    }

    pub(crate) fn profiles_confirm_remove(&self) {
        if self.profiles.is_busy() {
            return;
        }
        let name = self.profiles.remove_name.get();
        if name.is_empty() {
            return;
        }
        self.profiles.set_error("");
        self.profiles.busy.set(ProfilesBusy::Remove);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let worker_name = name.clone();
            let result = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let active = hub.profiles().active();
                if active.as_deref() == Some(worker_name.as_str()) {
                    hub.set_active_profile(None)
                        .map_err(|error| error_key(&error))?;
                }
                match hub.profiles().remove(&worker_name) {
                    Ok(()) => Ok(profile_rows(&hub)),
                    Err(_) => Err("profiles.error_io"),
                }
            })
            .await;
            ui.profiles.busy.set(ProfilesBusy::None);
            match result {
                Ok(Ok(rows)) => {
                    ui.profiles.remove_name.set(String::new());
                    ui.profiles.rows.set(rows);
                }
                Ok(Err(key)) => {
                    ui.profiles.remove_name.set(String::new());
                    ui.profiles.set_error(key);
                }
                Err(_) => ui.profiles.set_error("profiles.error_generic"),
            }
        });
    }
}
