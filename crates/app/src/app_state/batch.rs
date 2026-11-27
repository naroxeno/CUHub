//! Batch update queue: row selection, dependency preflight and the sequential apply worker.

use crate::app_state::install::{DependencyPrompt, PendingInstall, RemoteAttempt, prepare_update};
use crate::app_state::state::Ui;
use cuprum_core::hub::{Hub, HubError};
use cuprum_core::mods::{DependencyIssue, InstalledMod};
use cuprum_core::sources::{Repo, SourceEntry, SourceKind};
use nsh_core::prelude::*;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BatchMode {
    Selected,
    All,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BatchPhase {
    Preflight,
    Applying,
}

#[derive(Clone, PartialEq, Eq)]
pub struct BatchProgress {
    pub phase: BatchPhase,
    pub index: usize,
    pub total: usize,
    pub name: String,
}

#[derive(Clone)]
struct BatchTarget {
    item: InstalledMod,
    name: String,
    pinned: Option<SourceKind>,
    effective: Option<SourceKind>,
    repo: Option<Repo>,
    asset: Option<String>,
    nexus_id: Option<String>,
    entry: Option<SourceEntry>,
}

enum Preflight {
    Ready(Option<PendingInstall>),
    Blocked(Vec<DependencyIssue>),
}

enum ItemOutcome {
    Applied,
    Blocked(Vec<DependencyIssue>),
    Failed,
}

#[derive(Default)]
struct BatchTally {
    applied: usize,
    failed: usize,
    late: Vec<DependencyIssue>,
}

impl BatchTally {
    fn record(&mut self, outcome: Result<ItemOutcome, ()>) {
        match outcome {
            Ok(ItemOutcome::Applied) => self.applied += 1,
            Ok(ItemOutcome::Blocked(issues)) => {
                self.failed += 1;
                self.late.extend(issues);
            }
            Ok(ItemOutcome::Failed) | Err(()) => self.failed += 1,
        }
    }
}

pub fn updatable_count(ui: &Ui) -> usize {
    ui.local_rows
        .get()
        .iter()
        .filter(|row| row.can_update())
        .count()
}

pub fn selected_count(ui: &Ui) -> usize {
    ui.batch_selection.get().len()
}

pub fn all_updatable_selected(ui: &Ui) -> bool {
    let rows = ui.local_rows.get();
    let selection = ui.batch_selection.get();
    let mut any = false;
    for row in rows.iter().filter(|row| row.can_update()) {
        any = true;
        if !selection.contains(&row.source_key) {
            return false;
        }
    }
    any
}

pub fn prune(selection: &BTreeSet<String>, valid: &BTreeSet<String>) -> BTreeSet<String> {
    selection.intersection(valid).cloned().collect()
}

fn entry_for(hub: &Hub, repo: &Repo) -> Option<SourceEntry> {
    let slug = repo.slug();
    hub.sources()?
        .entries
        .iter()
        .find(|entry| entry.repo.slug() == slug)
        .cloned()
}

fn dedupe_issues(issues: Vec<DependencyIssue>) -> Vec<DependencyIssue> {
    let mut unique: Vec<DependencyIssue> = Vec::new();
    for issue in issues {
        if !unique.contains(&issue) {
            unique.push(issue);
        }
    }
    unique
}

fn issues_summary(issues: &[DependencyIssue]) -> String {
    issues
        .iter()
        .map(|issue| issue.name.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

fn log_failure(hub: &Hub, name: &str, detail: &str) {
    hub.logs()
        .activity("Batch", &format!("{name} failed: {detail}"));
}

fn preflight_target(hub: &Hub, target: &BatchTarget) -> Result<Preflight, HubError> {
    if let Some(entry) = target.entry.as_ref() {
        let issues = hub.remote_install_issues(entry)?;
        return Ok(if issues.is_empty() {
            Preflight::Ready(None)
        } else {
            Preflight::Blocked(issues)
        });
    }
    let attempt = prepare_update(
        hub,
        &target.item,
        target.pinned,
        target.effective,
        target.repo.as_ref(),
        target.asset.as_deref(),
        target.nexus_id.as_deref(),
    )?;
    Ok(match attempt {
        RemoteAttempt::Ready(pending) => Preflight::Ready(Some(*pending)),
        RemoteAttempt::Blocked(issues) => Preflight::Blocked(issues),
    })
}

fn apply_target(
    hub: &mut Hub,
    target: &BatchTarget,
    pending: Option<PendingInstall>,
) -> ItemOutcome {
    let pending = match pending {
        Some(pending) => pending,
        None => {
            let attempt = match prepare_update(
                hub,
                &target.item,
                target.pinned,
                target.effective,
                target.repo.as_ref(),
                target.asset.as_deref(),
                target.nexus_id.as_deref(),
            ) {
                Ok(attempt) => attempt,
                Err(error) => {
                    log_failure(hub, &target.name, &error.to_string());
                    return ItemOutcome::Failed;
                }
            };
            match attempt {
                RemoteAttempt::Ready(pending) => *pending,
                RemoteAttempt::Blocked(issues) => {
                    log_failure(hub, &target.name, &issues_summary(&issues));
                    return ItemOutcome::Blocked(issues);
                }
            }
        }
    };
    if let Some(entry) = target.entry.as_ref() {
        match hub.remote_install_issues(entry) {
            Ok(issues) if !issues.is_empty() => {
                hub.discard_staging(&pending.staging);
                log_failure(hub, &target.name, &issues_summary(&issues));
                return ItemOutcome::Blocked(issues);
            }
            Ok(_) => {}
            Err(error) => {
                hub.discard_staging(&pending.staging);
                log_failure(hub, &target.name, &error.to_string());
                return ItemOutcome::Failed;
            }
        }
    }
    let applied = hub.apply_package(&pending.package);
    hub.discard_staging(&pending.staging);
    match applied {
        Ok(report) => {
            hub.logs().activity(
                "Batch",
                &format!("Updated {} ({} file(s)).", target.name, report.files),
            );
            ItemOutcome::Applied
        }
        Err(error) => {
            log_failure(hub, &target.name, &error.to_string());
            ItemOutcome::Failed
        }
    }
}

async fn discard_prepared(hub: &Arc<Mutex<Hub>>, prepared: Vec<Option<PendingInstall>>) {
    let pendings: Vec<PendingInstall> = prepared.into_iter().flatten().collect();
    if pendings.is_empty() {
        return;
    }
    let lock = hub.clone();
    let _ = blocking(move || {
        let hub = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for pending in &pendings {
            hub.discard_staging(&pending.staging);
        }
    })
    .await;
}

fn run_batch(ui: &Ui, hub: Arc<Mutex<Hub>>, targets: Vec<BatchTarget>) {
    let ui = ui.clone();
    ui.remote_busy.set(Some(String::from("batch")));
    let worker = ui.clone();
    worker.spawn_worker(async move {
        let total = targets.len();
        let mut prepared: Vec<Option<PendingInstall>> = (0..total).map(|_| None).collect();
        let mut blockers: Vec<DependencyIssue> = Vec::new();
        let mut abort: Option<String> = None;
        for (index, target) in targets.iter().enumerate() {
            ui.batch_progress.set(Some(BatchProgress {
                phase: BatchPhase::Preflight,
                index: index + 1,
                total,
                name: target.name.clone(),
            }));
            let lock = hub.clone();
            let checked = target.clone();
            let result = blocking(move || {
                let hub = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                preflight_target(&hub, &checked)
            })
            .await;
            match result {
                Ok(Ok(Preflight::Ready(pending))) => prepared[index] = pending,
                Ok(Ok(Preflight::Blocked(issues))) => blockers.extend(issues),
                Ok(Err(error)) => {
                    abort = Some(error.to_string());
                    break;
                }
                Err(error) => {
                    abort = Some(error.to_string());
                    break;
                }
            }
        }
        if abort.is_some() {
            discard_prepared(&hub, prepared).await;
            ui.batch_progress.set(None);
            ui.remote_busy.set(None);
            let error = abort.unwrap_or_default();
            ui.task_failed_with("task.batch_title", "task.reason_failed");
            ui.notice.set(t!("notice.batch_aborted", error = error));
            return;
        }
        if !blockers.is_empty() {
            discard_prepared(&hub, prepared).await;
            ui.batch_progress.set(None);
            ui.remote_busy.set(None);
            ui.dependency_prompt.set(Some(DependencyPrompt {
                target: None,
                issues: dedupe_issues(blockers),
            }));
            ui.notice.set(t!("dependency.blocked"));
            return;
        }
        let mut tally = BatchTally::default();
        for (index, target) in targets.into_iter().enumerate() {
            ui.batch_progress.set(Some(BatchProgress {
                phase: BatchPhase::Applying,
                index: index + 1,
                total,
                name: target.name.clone(),
            }));
            let pending = prepared.get_mut(index).and_then(Option::take);
            let lock = hub.clone();
            let outcome = blocking(move || {
                let mut hub = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                apply_target(&mut hub, &target, pending)
            })
            .await;
            tally.record(outcome.map_err(|_| ()));
        }
        ui.batch_progress.set(None);
        ui.remote_busy.set(None);
        ui.refresh_mods();
        ui.refresh_bridge_install();
        let late = std::mem::take(&mut tally.late);
        if !late.is_empty() {
            ui.dependency_prompt.set(Some(DependencyPrompt {
                target: None,
                issues: dedupe_issues(late),
            }));
        }
        if tally.failed == 0 {
            ui.clear_task_failure("task.batch_title");
        } else {
            ui.task_failed_with("task.batch_title", "task.reason_failed");
        }
        let summary = t!(
            "notice.batch_done",
            ok = tally.applied,
            failed = tally.failed
        );
        let lock = hub.clone();
        let line = summary.clone();
        let _ = blocking(move || {
            let hub = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            hub.logs().activity("Batch", &line);
        })
        .await;
        ui.notice.set(summary);
    });
}

impl Ui {
    pub fn toggle_batch_selection(&self, key: String) {
        let mut selection = self.batch_selection.get();
        if !selection.remove(&key) {
            selection.insert(key);
        }
        self.batch_selection.set(selection);
    }

    pub fn select_updatable(&self) {
        let selection: BTreeSet<String> = self
            .local_rows
            .get()
            .iter()
            .filter(|row| row.can_update())
            .map(|row| row.source_key.clone())
            .collect();
        self.batch_selection.set(selection);
    }

    pub fn clear_batch_selection(&self) {
        self.batch_selection.set(BTreeSet::new());
    }

    pub fn start_batch(&self, mode: BatchMode) {
        if self.batch_progress.get().is_some() {
            self.notice.set(t!("notice.busy"));
            return;
        }
        if self.pending_install.get().is_some() {
            self.notice.set(t!("notice.pending_first"));
            return;
        }
        if self.dependency_prompt.get().is_some() {
            self.notice.set(t!("dependency.blocked"));
            return;
        }
        if self.update_busy.get().is_some()
            || self.remote_busy.get().is_some()
            || self.apply_running.get()
            || self.profiles.is_busy()
        {
            self.notice.set(t!("notice.busy"));
            return;
        }
        let rows = self.local_rows.get();
        let selection = self.batch_selection.get();
        let mut targets = Vec::new();
        {
            let hub = self.hub();
            for row in rows.iter() {
                if !row.can_update() {
                    continue;
                }
                if mode == BatchMode::Selected && !selection.contains(&row.source_key) {
                    continue;
                }
                let repo = row.source.repo.clone();
                let entry = repo.as_ref().and_then(|repo| entry_for(&hub, repo));
                targets.push(BatchTarget {
                    item: row.item.clone(),
                    name: row.item.name.clone(),
                    pinned: row.source.pinned,
                    effective: row.source.effective,
                    repo,
                    asset: row.source.asset.clone(),
                    nexus_id: row.item.metadata_id.clone(),
                    entry,
                });
            }
        }
        if targets.is_empty() {
            self.notice.set(t!("notice.batch_nothing"));
            return;
        }
        let count = targets.len();
        self.batch_selection.set(BTreeSet::new());
        self.batch_progress.set(Some(BatchProgress {
            phase: BatchPhase::Preflight,
            index: 0,
            total: count,
            name: String::new(),
        }));
        self.notice.set(t!("notice.batch_started", count = count));
        run_batch(self, self.hub.clone(), targets);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cuprum_core::mods::IssueKind;

    fn issue(name: &str, kind: IssueKind) -> DependencyIssue {
        DependencyIssue {
            name: name.to_owned(),
            minimum_version: None,
            installed_version: None,
            kind,
            entry: None,
        }
    }

    #[test]
    fn tally_keeps_going_and_aggregates_blockers() {
        let mut tally = BatchTally::default();
        tally.record(Ok(ItemOutcome::Applied));
        tally.record(Ok(ItemOutcome::Failed));
        tally.record(Err(()));
        tally.record(Ok(ItemOutcome::Blocked(vec![
            issue("Alpha", IssueKind::Missing),
            issue("Alpha", IssueKind::Missing),
        ])));
        tally.record(Ok(ItemOutcome::Blocked(vec![issue(
            "Beta",
            IssueKind::Outdated,
        )])));
        tally.record(Ok(ItemOutcome::Applied));
        assert_eq!(tally.applied, 2);
        assert_eq!(tally.failed, 4);
        assert_eq!(dedupe_issues(tally.late).len(), 2);
    }

    #[test]
    fn dedupe_keeps_one_line_per_issue() {
        let issues = vec![
            issue("Alpha", IssueKind::Missing),
            issue("Alpha", IssueKind::Missing),
            issue("Beta", IssueKind::Outdated),
        ];
        assert_eq!(dedupe_issues(issues).len(), 2);
    }

    #[test]
    fn pruning_drops_stale_keys() {
        let selection: BTreeSet<String> =
            ["a", "b", "c"].iter().map(|key| key.to_string()).collect();
        let valid: BTreeSet<String> = ["b", "c", "d"].iter().map(|key| key.to_string()).collect();
        let pruned = prune(&selection, &valid);
        assert_eq!(pruned.len(), 2);
        assert!(pruned.contains("b"));
        assert!(pruned.contains("c"));
    }
}
