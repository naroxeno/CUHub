//! Pending task cards, live download progress and the poller behind the task drawer.

use super::batch::{BatchPhase, selected_count, updatable_count};
use super::install::{DependencyPrompt, PendingInstall};
use super::state::Ui;
use crate::palette::*;
use cuhub_core::hub::BepInExInstall;
use cuhub_core::install::FileAction;
use cuhub_core::mods::IssueKind;
use cuhub_core::network::DownloadProgress;
use nsh_core::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

const POLL_MS: u64 = 120;
const BYTE_UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Install,
    Dependency,
    BepInEx,
    Batch,
    Download,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TaskAction {
    Apply,
    Discard,
    InstallDependencies,
    DismissDependencies,
    ApplyBepInEx,
    CancelBepInEx,
    UpdateSelected,
    UpdateAll,
}

#[derive(Clone, PartialEq, Eq)]
pub struct TaskButton {
    pub action: TaskAction,
    pub label: String,
    pub enabled: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TaskProgress {
    pub written: u64,
    pub total: Option<u64>,
}

impl TaskProgress {
    pub fn fraction(&self) -> f32 {
        match self.total {
            Some(total) if total > 0 => (self.written as f32 / total as f32).clamp(0.02, 1.0),
            _ => 1.0,
        }
    }

    pub fn percent(&self) -> Option<u32> {
        match self.total {
            Some(total) if total > 0 => {
                let percent = self.written.saturating_mul(100) / total;
                Some(percent.min(100) as u32)
            }
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct TaskCard {
    pub key: String,
    pub kind: TaskKind,
    pub title: String,
    pub subtitle: String,
    pub lines: Vec<(String, u32)>,
    pub progress: Option<TaskProgress>,
    pub status: String,
    pub buttons: Vec<TaskButton>,
    pub busy: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct DownloadTrack {
    pub id: u64,
    pub label: String,
    pub written: u64,
    pub total: Option<u64>,
}

static DOWNLOADS: OnceLock<Mutex<Vec<DownloadTrack>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn downloads() -> MutexGuard<'static, Vec<DownloadTrack>> {
    DOWNLOADS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn download_snapshot() -> Vec<DownloadTrack> {
    downloads().clone()
}

pub struct DownloadTracker {
    id: u64,
}

impl DownloadTracker {
    pub fn start(label: impl Into<String>) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        downloads().push(DownloadTrack {
            id,
            label: label.into(),
            written: 0,
            total: None,
        });
        Self { id }
    }

    pub fn report(&mut self, progress: DownloadProgress) {
        if let Some(track) = downloads().iter_mut().find(|item| item.id == self.id) {
            track.written = progress.written;
            track.total = progress.total;
        }
    }
}

impl Drop for DownloadTracker {
    fn drop(&mut self) {
        downloads().retain(|item| item.id != self.id);
    }
}

pub fn bytes_label(bytes: u64) -> String {
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < BYTE_UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, BYTE_UNITS[unit])
    } else {
        format!("{:.1} {}", value, BYTE_UNITS[unit])
    }
}

pub fn pending_count(ui: &Ui) -> usize {
    let mut count = ui.downloads.get().len();
    if ui.dependency_prompt.get().is_some() {
        count += 1;
    }
    if ui.pending_install.get().is_some() {
        count += 1;
    }
    if ui.bepinex_pending.get().is_some() {
        count += 1;
    }
    if ui.batch_progress.get().is_some() {
        count += 1;
    }
    count
}

pub fn cards(ui: &Ui) -> Vec<TaskCard> {
    let mut cards = Vec::new();
    if let Some(prompt) = ui.dependency_prompt.get() {
        cards.push(dependency_card(ui, &prompt));
    }
    if let Some(pending) = ui.pending_install.get() {
        cards.push(install_card(ui, &pending));
    }
    if let Some(install) = ui.bepinex_pending.get() {
        cards.push(bepinex_card(ui, &install));
    }
    if let Some(card) = batch_card(ui) {
        cards.push(card);
    }
    for track in ui.downloads.get() {
        cards.push(download_card(&track));
    }
    cards
}

fn file_summary(replaced: usize, created: usize) -> String {
    let files = replaced + created;
    t!(
        "task.files_summary",
        files = files,
        replaced = replaced,
        created = created
    )
}

fn dependency_card(ui: &Ui, prompt: &DependencyPrompt) -> TaskCard {
    let mut lines = Vec::new();
    for issue in &prompt.issues {
        let line = match issue.kind {
            IssueKind::Missing => match issue.minimum_version.as_deref() {
                Some(version) => (
                    t!(
                        "dependency.missing_version",
                        name = issue.name.clone(),
                        version = version.to_owned()
                    ),
                    DANGER,
                ),
                None => (t!("dependency.missing", name = issue.name.clone()), DANGER),
            },
            IssueKind::Outdated => (
                t!(
                    "dependency.outdated",
                    name = issue.name.clone(),
                    required = issue.minimum_version.clone().unwrap_or_default(),
                    installed = issue
                        .installed_version
                        .clone()
                        .unwrap_or_else(|| t!("common.none"))
                ),
                WARN,
            ),
        };
        lines.push(line);
    }
    if prompt.has_manual() {
        lines.push((t!("dependency.manual"), TEXT_MUTED));
    }
    let busy = ui.remote_busy.get().is_some();
    TaskCard {
        key: String::from("dependency"),
        kind: TaskKind::Dependency,
        title: t!("task.dependency_title"),
        subtitle: t!("task.dependency_meta", count = prompt.issues.len()),
        lines,
        progress: None,
        status: if busy {
            t!("dependency.installing")
        } else {
            String::new()
        },
        buttons: vec![
            TaskButton {
                action: TaskAction::InstallDependencies,
                label: t!("dependency.install"),
                enabled: prompt.can_install() && !busy,
            },
            TaskButton {
                action: TaskAction::DismissDependencies,
                label: t!("common.cancel"),
                enabled: !busy,
            },
        ],
        busy,
    }
}

fn install_card(ui: &Ui, pending: &PendingInstall) -> TaskCard {
    let replaced = pending
        .plan
        .files
        .iter()
        .filter(|file| file.action == FileAction::Replace)
        .count();
    let created = pending.plan.files.len().saturating_sub(replaced);
    let files = file_summary(replaced, created);
    let mut lines = vec![
        (
            t!(
                "pending.from",
                source = pending.package.source.as_str(),
                target = pending.package.target.display().to_string()
            ),
            TEXT_MUTED,
        ),
        (files, TEXT_MUTED),
    ];
    let mut notes = pending
        .notes
        .iter()
        .map(|note| note.text())
        .collect::<Vec<_>>();
    notes.extend(pending.plan.warnings.clone());
    if !notes.is_empty() {
        lines.push((notes.join(" "), WARN));
    }
    if !pending.plan.conflicts.is_empty() {
        let list = pending
            .plan
            .conflicts
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push((t!("pending.conflicts", list = list), DANGER));
    }
    if pending.game_running {
        lines.push((t!("task.game_running"), WARN));
    }
    let running = ui.apply_running.get();
    TaskCard {
        key: String::from("install"),
        kind: TaskKind::Install,
        title: pending.mod_name.clone(),
        subtitle: t!("task.version", version = pending.package.version.clone()),
        lines,
        progress: None,
        status: ui.apply_status.get(),
        buttons: vec![
            TaskButton {
                action: TaskAction::Apply,
                label: t!("common.apply"),
                enabled: !running && pending.plan.can_apply(),
            },
            TaskButton {
                action: TaskAction::Discard,
                label: t!("common.cancel"),
                enabled: true,
            },
        ],
        busy: running,
    }
}

fn bepinex_card(ui: &Ui, install: &BepInExInstall) -> TaskCard {
    let blocked = !install.plan.conflicts.is_empty();
    let replace = install
        .plan
        .files
        .iter()
        .any(|file| file.action == FileAction::Replace);
    let replaced = install
        .plan
        .files
        .iter()
        .filter(|file| file.action == FileAction::Replace)
        .count();
    let created = install.plan.files.len().saturating_sub(replaced);
    let files = file_summary(replaced, created);
    let mut lines = vec![(files, TEXT_MUTED)];
    if blocked {
        lines.push((t!("bepinex.plan_conflicts"), DANGER));
    }
    let busy = ui.bepinex_busy.get();
    TaskCard {
        key: String::from("bepinex"),
        kind: TaskKind::BepInEx,
        title: t!("task.bepinex_title"),
        subtitle: t!("bepinex.plan_summary", version = install.release.version),
        lines,
        progress: None,
        status: ui.bepinex_status_text.get(),
        buttons: vec![
            TaskButton {
                action: TaskAction::ApplyBepInEx,
                label: if replace {
                    t!("bepinex.replace")
                } else {
                    t!("bepinex.install")
                },
                enabled: !blocked && !busy,
            },
            TaskButton {
                action: TaskAction::CancelBepInEx,
                label: t!("common.cancel"),
                enabled: !busy,
            },
        ],
        busy,
    }
}

fn batch_card(ui: &Ui) -> Option<TaskCard> {
    let progress = ui.batch_progress.get();
    let updatable = updatable_count(ui);
    let selected = selected_count(ui);
    if progress.is_none() && updatable == 0 && selected == 0 {
        return None;
    }
    let busy = progress.is_some();
    let mut status = String::new();
    if let Some(progress) = progress.as_ref() {
        status = match progress.phase {
            BatchPhase::Preflight => t!(
                "task.batch_preflight",
                index = progress.index,
                total = progress.total
            ),
            BatchPhase::Applying => t!(
                "task.batch_current",
                index = progress.index,
                total = progress.total,
                name = progress.name.clone()
            ),
        };
    }
    Some(TaskCard {
        key: String::from("batch"),
        kind: TaskKind::Batch,
        title: t!("task.batch_title"),
        subtitle: t!(
            "task.batch_meta",
            selected = selected,
            updatable = updatable
        ),
        lines: Vec::new(),
        progress: None,
        status,
        buttons: vec![
            TaskButton {
                action: TaskAction::UpdateSelected,
                label: t!("local.batch_selected", count = selected),
                enabled: selected > 0 && !busy,
            },
            TaskButton {
                action: TaskAction::UpdateAll,
                label: t!("local.batch_all", count = updatable),
                enabled: updatable > 0 && !busy,
            },
        ],
        busy,
    })
}

fn download_card(track: &DownloadTrack) -> TaskCard {
    TaskCard {
        key: format!("download-{}", track.id),
        kind: TaskKind::Download,
        title: track.label.clone(),
        subtitle: t!("task.downloading"),
        lines: Vec::new(),
        progress: Some(TaskProgress {
            written: track.written,
            total: track.total,
        }),
        status: String::new(),
        buttons: Vec::new(),
        busy: false,
    }
}

impl Ui {
    pub(crate) fn watch_downloads(&self) {
        let ui = self.clone();
        self.spawn_worker(async move {
            let mut last = Vec::new();
            loop {
                let tracks = download_snapshot();
                if tracks != last {
                    last = tracks.clone();
                    ui.downloads.set(tracks);
                }
                let _ = blocking(|| std::thread::sleep(Duration::from_millis(POLL_MS))).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_total_never_invents_a_percentage() {
        let unknown = TaskProgress {
            written: 1024,
            total: None,
        };
        assert_eq!(unknown.percent(), None);
        assert_eq!(unknown.fraction(), 1.0);
    }

    #[test]
    fn percent_follows_written_bytes() {
        let half = TaskProgress {
            written: 512,
            total: Some(1024),
        };
        assert_eq!(half.percent(), Some(50));
        assert!((half.fraction() - 0.5).abs() < 0.001);
        let over = TaskProgress {
            written: 4096,
            total: Some(1024),
        };
        assert_eq!(over.percent(), Some(100));
        assert_eq!(over.fraction(), 1.0);
    }

    #[test]
    fn byte_labels_switch_units() {
        assert_eq!(bytes_label(0), "0 B");
        assert_eq!(bytes_label(999), "999 B");
        assert_eq!(bytes_label(1024), "1.0 KiB");
        assert_eq!(bytes_label(1024 * 1024 * 3 / 2), "1.5 MiB");
    }
}
