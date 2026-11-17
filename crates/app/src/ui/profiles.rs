//! Profile and content-library state: rows, drafts and the workers behind them.

use cuhub_core::adopt::{AdoptError, AdoptPlan, AdoptSkip};
use cuhub_core::hub::{Hub, HubError};
use cuhub_core::profiles::{Profile, ProfileEntry};
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
    Scan,
    Adopt,
}

#[derive(Clone, PartialEq)]
pub struct ProfileRow {
    pub name: String,
    pub active: bool,
    pub entries: usize,
}

#[derive(Clone, PartialEq)]
pub struct AdoptReason {
    pub key: &'static str,
    pub count: usize,
}

#[derive(Clone, Copy)]
pub struct ProfilesUi {
    pub rows: Signal<Vec<ProfileRow>>,
    pub name: Signal<String>,
    pub busy: Signal<ProfilesBusy>,
    pub error: Signal<&'static str>,
    pub create_confirm: Signal<String>,
    pub create_snapshot: Signal<bool>,
    pub apply_name: Signal<String>,
    pub apply_count: Signal<usize>,
    pub remove_name: Signal<String>,
    pub adoptable: Signal<usize>,
    pub adopt_scanned: Signal<bool>,
    pub adopt_skipped: Signal<usize>,
    pub adopt_reasons: Signal<Vec<AdoptReason>>,
    pub adopt_done: Signal<Option<(usize, String)>>,
    pub adopt_error: Signal<&'static str>,
}

impl ProfilesUi {
    pub fn new() -> Self {
        Self {
            rows: signal(Vec::new()),
            name: signal(String::new()),
            busy: signal(ProfilesBusy::None),
            error: signal(""),
            create_confirm: signal(String::new()),
            create_snapshot: signal(false),
            apply_name: signal(String::new()),
            apply_count: signal(0usize),
            remove_name: signal(String::new()),
            adoptable: signal(0usize),
            adopt_scanned: signal(false),
            adopt_skipped: signal(0usize),
            adopt_reasons: signal(Vec::new()),
            adopt_done: signal(None::<(usize, String)>),
            adopt_error: signal(""),
        }
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

enum AdoptOutcome {
    Done { adopted: usize, backup: String },
    Partial { adopted: usize, backup: String },
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

fn scan_summary(plan: &AdoptPlan) -> (usize, usize, Vec<AdoptReason>) {
    const KEYS: [&str; 7] = [
        "profiles.adopt_skip_disabled",
        "profiles.adopt_skip_managed",
        "profiles.adopt_skip_outside",
        "profiles.adopt_skip_invalid",
        "profiles.adopt_skip_missing",
        "profiles.adopt_skip_store",
        "profiles.adopt_skip_duplicate",
    ];
    let mut counts = [0usize; 7];
    for (_, skip) in &plan.skipped {
        let index = match skip {
            AdoptSkip::Disabled => 0,
            AdoptSkip::AlreadyManaged => 1,
            AdoptSkip::OutsidePlugins => 2,
            AdoptSkip::InvalidName => 3,
            AdoptSkip::MissingVersion => 4,
            AdoptSkip::StoreHasVersion => 5,
            AdoptSkip::DuplicateName => 6,
        };
        counts[index] += 1;
    }
    let reasons = KEYS
        .iter()
        .zip(counts)
        .filter(|(_, count)| *count > 0)
        .map(|(key, count)| AdoptReason { key, count })
        .collect();
    (plan.items.len(), plan.skipped.len(), reasons)
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

impl Ui {
    pub(crate) fn profiles_refresh(&self) {
        if self.profiles.is_busy() {
            return;
        }
        self.profiles.busy.set(ProfilesBusy::Refresh);
        self.profiles.error.set("");
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
                Err(_) => ui.profiles.error.set("profiles.error_generic"),
            }
            ui.profiles.busy.set(ProfilesBusy::None);
        });
    }

    pub(crate) fn profiles_request_create(&self, snapshot: bool) {
        if self.profiles.is_busy() {
            return;
        }
        let name = self.profiles.name.get().trim().to_owned();
        self.profiles.error.set("");
        if name.is_empty() {
            self.profiles.error.set("profiles.error_empty_name");
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
                    ui.profiles.error.set("profiles.error_invalid_name")
                }
                Ok(CreateOutcome::Failed(key)) => ui.profiles.error.set(key),
                Err(_) => ui.profiles.error.set("profiles.error_generic"),
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
        self.profiles.error.set("");
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
                    ui.profiles.error.set("profiles.error_invalid_name");
                }
                Ok(CreateOutcome::Failed(key)) => {
                    ui.profiles.create_confirm.set(String::new());
                    ui.profiles.error.set(key);
                }
                Ok(CreateOutcome::Exists) => ui.profiles.create_confirm.set(String::new()),
                Err(_) => ui.profiles.error.set("profiles.error_generic"),
            }
        });
    }

    pub(crate) fn profiles_cancel_create(&self) {
        self.profiles.create_confirm.set(String::new());
    }

    pub(crate) fn profiles_request_apply(&self, name: &str) {
        if self.profiles.is_busy() {
            return;
        }
        let name = name.to_owned();
        self.profiles.error.set("");
        self.profiles.busy.set(ProfilesBusy::Plan);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let worker_name = name.clone();
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.plan_profile(&worker_name)
                    .map(|ops| ops.len())
                    .map_err(|error| error_key(&error))
            })
            .await;
            ui.profiles.busy.set(ProfilesBusy::None);
            match result {
                Ok(Ok(count)) => {
                    ui.profiles.apply_count.set(count);
                    ui.profiles.apply_name.set(name);
                }
                Ok(Err(key)) => {
                    ui.profiles.apply_name.set(String::new());
                    ui.profiles.error.set(key);
                }
                Err(_) => ui.profiles.error.set("profiles.error_generic"),
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
        let name = self.profiles.apply_name.get();
        if name.is_empty() {
            return;
        }
        self.profiles.error.set("");
        self.profiles.busy.set(ProfilesBusy::Apply);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let worker_name = name.clone();
            let result = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                match hub.apply_profile(&worker_name) {
                    Ok(report) => {
                        let applied = report.applied.len();
                        match hub.set_active_profile(Some(&worker_name)) {
                            Ok(()) => Ok((applied, None)),
                            Err(error) => Ok((applied, Some(error_key(&error)))),
                        }
                    }
                    Err(error) => Err(error_key(&error)),
                }
            })
            .await;
            ui.profiles.busy.set(ProfilesBusy::None);
            match result {
                Ok(Ok((applied, active_error))) => {
                    ui.profiles.apply_name.set(String::new());
                    ui.profiles.error.set(active_error.unwrap_or(""));
                    ui.notice.set(t!("profiles.applied", count = applied));
                    ui.profiles_refresh();
                }
                Ok(Err(key)) => {
                    ui.profiles.apply_name.set(String::new());
                    ui.profiles.error.set(key);
                }
                Err(_) => ui.profiles.error.set("profiles.error_generic"),
            }
        });
    }

    pub(crate) fn profiles_request_remove(&self, name: &str) {
        if self.profiles.is_busy() {
            return;
        }
        self.profiles.error.set("");
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
        self.profiles.error.set("");
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
                    ui.profiles.error.set(key);
                }
                Err(_) => ui.profiles.error.set("profiles.error_generic"),
            }
        });
    }

    pub(crate) fn adopt_scan(&self) {
        if self.profiles.is_busy() {
            return;
        }
        self.profiles.adopt_error.set("");
        self.profiles.adopt_done.set(None);
        self.profiles.adopt_reasons.set(Vec::new());
        self.profiles.busy.set(ProfilesBusy::Scan);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                match hub.plan_adopt() {
                    Ok(plan) => Ok(scan_summary(&plan)),
                    Err(error) => Err(error_key(&error)),
                }
            })
            .await;
            ui.profiles.busy.set(ProfilesBusy::None);
            match result {
                Ok(Ok((adoptable, skipped, reasons))) => {
                    ui.profiles.adopt_scanned.set(true);
                    ui.profiles.adoptable.set(adoptable);
                    ui.profiles.adopt_skipped.set(skipped);
                    ui.profiles.adopt_reasons.set(reasons);
                }
                Ok(Err(key)) => {
                    ui.profiles.adopt_scanned.set(false);
                    ui.profiles.adoptable.set(0);
                    ui.profiles.adopt_skipped.set(0);
                    ui.profiles.adopt_error.set(key);
                }
                Err(_) => ui.profiles.error.set("profiles.error_generic"),
            }
        });
    }

    pub(crate) fn adopt_confirm(&self) {
        if self.profiles.is_busy() {
            return;
        }
        if self.profiles.adoptable.get() == 0 {
            return;
        }
        self.profiles.adopt_error.set("");
        self.profiles.adopt_done.set(None);
        self.profiles.busy.set(ProfilesBusy::Adopt);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                let plan = match hub.plan_adopt() {
                    Ok(plan) => plan,
                    Err(error) => return AdoptOutcome::Failed(error_key(&error)),
                };
                if plan.items.is_empty() {
                    return AdoptOutcome::Failed("profiles.adopt_none");
                }
                match hub.apply_adopt(&plan) {
                    Ok(report) => AdoptOutcome::Done {
                        adopted: report.adopted.len(),
                        backup: report.backup_dir.display().to_string(),
                    },
                    Err(HubError::Adopt(AdoptError::Adopt { report, .. })) => {
                        AdoptOutcome::Partial {
                            adopted: report.adopted.len(),
                            backup: report.backup_dir.display().to_string(),
                        }
                    }
                    Err(error) => AdoptOutcome::Failed(error_key(&error)),
                }
            })
            .await;
            ui.profiles.busy.set(ProfilesBusy::None);
            match result {
                Ok(AdoptOutcome::Done { adopted, backup }) => {
                    ui.profiles.adopt_done.set(Some((adopted, backup)));
                    ui.profiles.adopt_scanned.set(false);
                    ui.profiles.adoptable.set(0);
                    ui.profiles.adopt_skipped.set(0);
                    ui.profiles.adopt_reasons.set(Vec::new());
                }
                Ok(AdoptOutcome::Partial { adopted, backup }) => {
                    ui.profiles.adopt_done.set(Some((adopted, backup)));
                    ui.profiles.adopt_scanned.set(false);
                    ui.profiles.adoptable.set(0);
                    ui.profiles.adopt_skipped.set(0);
                    ui.profiles.adopt_reasons.set(Vec::new());
                    ui.profiles.adopt_error.set("profiles.adopt_partial");
                }
                Ok(AdoptOutcome::Failed(key)) => {
                    ui.profiles.adopt_scanned.set(false);
                    ui.profiles.adoptable.set(0);
                    ui.profiles.adopt_skipped.set(0);
                    ui.profiles.adopt_error.set(key);
                }
                Err(_) => ui.profiles.error.set("profiles.error_generic"),
            }
        });
    }
}
