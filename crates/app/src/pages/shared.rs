//! Panels shared by more than one page.

use crate::palette::*;
use crate::ui::{Ui, button, panel, pix, rise_in, scaled};
use nsh_core::prelude::*;

pub(super) fn dependency_panel(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let Some(prompt) = ui.dependency_prompt.get() else {
        return view();
    };
    let mut body = view().flex_col().gap(scaled(&z, 4.0));
    for issue in &prompt.issues {
        let line = match issue.kind {
            cuhub_core::mods::IssueKind::Missing => match issue.minimum_version.as_deref() {
                Some(version) => t!(
                    "dependency.missing_version",
                    name = issue.name.clone(),
                    version = version.to_owned()
                ),
                None => t!("dependency.missing", name = issue.name.clone()),
            },
            cuhub_core::mods::IssueKind::Outdated => t!(
                "dependency.outdated",
                name = issue.name.clone(),
                required = issue.minimum_version.clone().unwrap_or_default(),
                installed = issue
                    .installed_version
                    .clone()
                    .unwrap_or_else(|| t!("common.none"))
            ),
        };
        let color = if issue.kind == cuhub_core::mods::IssueKind::Missing {
            DANGER
        } else {
            WARN
        };
        body = body.child(pix(line, 10.0, color, &z));
    }
    if prompt.has_manual() {
        body = body.child(pix(move || t!("dependency.manual"), 10.0, TEXT_MUTED, &z));
    }
    let install_ui = ui.clone();
    let cancel_ui = ui.clone();
    let can_install = prompt.can_install() && ui.remote_busy.get().is_none();
    body = body.child(
        view()
            .flex_row()
            .gap(scaled(&z, 8.0))
            .child(button(
                &z,
                move || t!("dependency.install"),
                can_install,
                move || install_ui.install_dependencies(),
            ))
            .child(button(
                &z,
                move || t!("common.cancel"),
                true,
                move || cancel_ui.dismiss_dependency_prompt(),
            )),
    );
    panel(&z, move || t!("panel.dependencies"), body).bind_motion(&rise_in())
}

pub(super) fn pending_panel(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let Some(pending) = ui.pending_install.get() else {
        return view();
    };
    let replaced = pending
        .plan
        .files
        .iter()
        .filter(|file| file.action == cuhub_core::install::FileAction::Replace)
        .count();
    let created = pending.plan.files.len().saturating_sub(replaced);
    let mod_name = pending.mod_name.clone();
    let file_count = pending.plan.files.len();
    let from_source = match pending.source {
        cuhub_core::sources::SourceKind::Github => "GitHub",
        cuhub_core::sources::SourceKind::Nexus => "Nexus",
    };
    let target_root = pending.plan.target_root.display().to_string();
    let mut notes = pending
        .notes
        .iter()
        .map(|note| note.text())
        .collect::<Vec<_>>();
    notes.extend(pending.plan.warnings.clone());
    let mut body = view()
        .flex_col()
        .gap(scaled(&z, 4.0))
        .child(pix(
            move || {
                t!(
                    "pending.summary",
                    name = mod_name,
                    files = file_count,
                    replaced = replaced,
                    created = created
                )
            },
            12.0,
            TEXT,
            &z,
        ))
        .child(pix(
            move || t!("pending.from", source = from_source, target = target_root),
            10.0,
            TEXT_MUTED,
            &z,
        ));
    if !notes.is_empty() {
        body = body.child(pix(notes.join(" "), 10.0, WARN, &z));
    }
    if !pending.plan.conflicts.is_empty() {
        let list = pending
            .plan
            .conflicts
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        body = body.child(pix(
            move || t!("pending.conflicts", list = list),
            10.0,
            DANGER,
            &z,
        ));
    }
    if pending.game_running {
        body = body.child(pix(move || t!("pending.game_running"), 10.0, WARN, &z));
    }
    let status_ui = ui.clone();
    body = body.child(pix(
        move || status_ui.apply_status.get(),
        10.0,
        TEXT_MUTED,
        &z,
    ));
    let apply_ui = ui.clone();
    let cancel_ui = ui.clone();
    let can_apply = pending.plan.can_apply();
    body = body.child(
        view()
            .flex_row()
            .gap(scaled(&z, 8.0))
            .child(button(
                &z,
                move || t!("common.apply"),
                can_apply,
                move || {
                    apply_ui.apply_pending();
                },
            ))
            .child(button(
                &z,
                move || t!("common.cancel"),
                true,
                move || {
                    cancel_ui.discard_pending();
                },
            )),
    );
    panel(&z, move || t!("panel.pending_update"), body).bind_motion(&rise_in())
}
