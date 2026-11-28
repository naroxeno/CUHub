use super::{TaskAction, TaskButton, TaskCard, TaskKind};
use crate::palette::{ColorRole, DANGER, TEXT_MUTED};
use cuprum_core::hub::{DependencyPlan, DependencyState, PlannedDependency};
use nsh_core::prelude::*;

pub(in crate::app_state) fn thunderstore_dependency_line(
    entry: &PlannedDependency,
) -> (String, ColorRole) {
    let package = &entry.package;
    let version = &entry.version;
    let (text, color) = match &entry.state {
        DependencyState::Satisfied => (t!("dependency.thunderstore_satisfied"), TEXT_MUTED),
        DependencyState::ReuseLocal(path) => (
            t!(
                "dependency.thunderstore_reuse_local",
                path = path.display().to_string()
            ),
            TEXT_MUTED,
        ),
        DependencyState::LocalCandidate(local) => (
            t!(
                "dependency.thunderstore_local_candidate",
                version = local.version.clone(),
                path = local.path.display().to_string()
            ),
            TEXT_MUTED,
        ),
        DependencyState::VerifiedLocal(local) => (
            t!(
                "dependency.thunderstore_verified_local",
                path = local.path.display().to_string()
            ),
            TEXT_MUTED,
        ),
        DependencyState::LocalConflict(details) => (
            t!(
                "dependency.thunderstore_local_conflict",
                details = details.clone()
            ),
            DANGER,
        ),
        DependencyState::Install => (t!("dependency.thunderstore_install"), TEXT_MUTED),
        DependencyState::Unavailable => (t!("dependency.thunderstore_unavailable"), DANGER),
        DependencyState::Cycle => (t!("dependency.thunderstore_cycle"), DANGER),
        DependencyState::VersionConflict(other) => (
            t!("dependency.thunderstore_conflict", other = other.clone()),
            DANGER,
        ),
        DependencyState::InstalledVersion(installed) => (
            t!(
                "dependency.thunderstore_installed_version",
                installed = installed.clone()
            ),
            DANGER,
        ),
        DependencyState::DeploymentUnavailable => (t!("dependency.thunderstore_inactive"), DANGER),
        DependencyState::RuntimeRequired => (t!("dependency.thunderstore_runtime"), DANGER),
    };
    (format!("{package} {version} · {text}"), color)
}

pub(super) fn card(plan: DependencyPlan, busy: bool, recovery: bool) -> TaskCard {
    let lines: Vec<_> = plan
        .entries
        .iter()
        .map(thunderstore_dependency_line)
        .collect();
    let copy_details = lines
        .iter()
        .map(|(line, _)| line.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut buttons = Vec::new();
    if plan.installable() {
        buttons.push(TaskButton {
            action: TaskAction::ApplyThunderstoreDependencies {
                confirm_local: plan.requires_local_confirmation(),
            },
            label: if plan.requires_local_confirmation() {
                t!("dependency.thunderstore_confirm_local")
            } else if recovery {
                t!("dependency.thunderstore_resume")
            } else {
                t!("dependency.thunderstore_apply")
            },
            enabled: !busy,
        });
    }
    if !recovery {
        buttons.push(TaskButton {
            action: TaskAction::DismissThunderstoreDependencies,
            label: t!("common.close"),
            enabled: !busy,
        });
    }
    let status = if busy {
        t!("dependency.thunderstore_working")
    } else if plan.installable() {
        t!(
            "dependency.thunderstore_destination",
            path = plan.plugins.display().to_string()
        )
    } else {
        t!("dependency.thunderstore_read_only")
    };
    TaskCard {
        key: String::from("thunderstore-dependencies"),
        kind: TaskKind::Dependency,
        title: t!("dependency.thunderstore_preview"),
        subtitle: t!("dependency.thunderstore_profile", profile = plan.profile),
        lines,
        progress: None,
        status,
        buttons,
        busy,
        copy_details: Some(copy_details),
    }
}
