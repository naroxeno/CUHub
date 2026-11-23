//! Content library migration scan, confirmations and per-mod results.

use cuhub_core::adopt::AdoptSkip;
use cuhub_core::hub::{MigrationPreview, MigrationRow};
use cuhub_core::sources::MatchKind;
use nsh_core::prelude::*;

use crate::palette::*;
use crate::ui::{
    MigrationResult, ProfilesBusy, Ui, button, danger_button, panel, pix, scaled, spinner,
};

pub(super) fn panel_view(ui: &Ui) -> View {
    let z = ui.zoom;
    let migration = ui.profiles.migration;
    let busy = ui.profiles.busy;
    let scan = ui.clone();
    let rows = ui.clone();
    let local = ui.clone();
    panel(
        &z,
        move || t!("profiles.adopt_title"),
        view()
            .flex_col()
            .gap(scaled(&z, 8.0))
            .child(pix(move || t!("profiles.adopt_hint"), 11.0, TEXT_MUTED, &z))
            .child(
                view()
                    .flex_row()
                    .gap(scaled(&z, 8.0))
                    .items_center()
                    .child(button(
                        &z,
                        move || t!("profiles.adopt_scan"),
                        true,
                        move || scan.adopt_scan(),
                    ))
                    .child(when(
                        move || matches!(busy.get(), ProfilesBusy::Scan | ProfilesBusy::Adopt),
                        move || spinner(&z),
                    )),
            )
            .child(when(
                move || !migration.error.get().is_empty(),
                move || pix(move || migration.error.get(), 11.0, DANGER, &z),
            ))
            .child(when(
                move || migration.session.get().is_some() && migration.rows.get().is_empty(),
                move || pix(move || t!("profiles.adopt_none"), 11.0, TEXT_MUTED, &z),
            ))
            .child(each(
                migration.rows,
                |row| row.path.clone(),
                move |row, _| migration_row(&rows, &row.get()),
            ))
            .child(when(
                move || migration.preview.get().is_none() && !migration.remaining().is_empty(),
                move || local_actions(&local),
            )),
    )
}

fn migration_row(ui: &Ui, row: &MigrationRow) -> View {
    let z = ui.zoom;
    let migration = ui.profiles.migration;
    let path = row.path.clone();
    let status_path = path.clone();
    let preview_path = path.clone();
    let sources = ui.clone();
    let preview = ui.clone();
    let row_actions = row.clone();
    let name = row.name.clone();
    let version = row.version.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 4.0))
        .py(scaled(&z, 6.0))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .child(pix(
            move || {
                t!(
                    "profiles.migration_item",
                    name = name.clone(),
                    version = version
                        .clone()
                        .unwrap_or_else(|| t!("profiles.migration_unknown").to_string())
                )
            },
            12.0,
            TEXT,
            &z,
        ))
        .child(pix(row.path.display().to_string(), 10.0, TEXT_MUTED, &z))
        .child(when(
            move || {
                !migration.can_act(&status_path)
                    || migration.results.get().contains_key(&status_path)
            },
            {
                let path = path.clone();
                move || {
                    let path = path.clone();
                    pix(
                        move || result_text(migration.results.get().get(&path)),
                        11.0,
                        WARN,
                        &z,
                    )
                }
            },
        ))
        .child(when(
            move || migration.can_act(&path),
            move || row_actions_view(&sources, &row_actions),
        ))
        .child(when(
            move || {
                migration
                    .preview
                    .get()
                    .is_some_and(|pending| pending.row().path == preview_path)
            },
            move || match migration.preview.get() {
                Some(pending) => github_confirm(&preview, &pending),
                None => view(),
            },
        ))
}

fn row_actions_view(ui: &Ui, row: &MigrationRow) -> View {
    let z = ui.zoom;
    let busy = ui.profiles.busy;
    let preview = ui.profiles.migration.preview;
    let mut actions = view().flex_col().gap(scaled(&z, 4.0));
    if let Err(reason) = row.local.as_ref() {
        let reason = skip_key(*reason);
        actions = actions.child(pix(
            move || t!("profiles.migration_skipped", reason = t!(reason)),
            11.0,
            TEXT_MUTED,
            &z,
        ));
    } else {
        actions = actions.child(pix(
            move || t!("profiles.migration_default"),
            11.0,
            TEXT_MUTED,
            &z,
        ));
    }
    for (kind, entry) in &row.sources {
        let kind = match kind {
            MatchKind::Guid => "profiles.migration_guid",
            MatchKind::Dll => "profiles.migration_dll",
            MatchKind::Name => "profiles.migration_name",
        };
        let repo = entry.repo.slug();
        let asset = entry
            .asset
            .as_ref()
            .map(|asset| format!(" · {asset}"))
            .unwrap_or_default();
        let available = row.local.is_ok();
        let row = row.clone();
        let entry = entry.clone();
        let ui = ui.clone();
        actions = actions.child(
            view()
                .flex_col()
                .items_start()
                .gap(scaled(&z, 4.0))
                .child(pix(
                    move || format!("GitHub · {repo}{asset} · {}", t!(kind)),
                    11.0,
                    TEXT_MUTED,
                    &z,
                ))
                .child(when(move || available, {
                    let row = row.clone();
                    move || {
                        let (ui, row, entry) = (ui.clone(), row.clone(), entry.clone());
                        button(
                            &z,
                            move || t!("profiles.migration_preview"),
                            true,
                            move || {
                                if busy.get() == ProfilesBusy::None && preview.get().is_none() {
                                    ui.adopt_preview_github(row.clone(), entry.clone());
                                }
                            },
                        )
                    }
                })),
        );
    }
    if row.sources.is_empty() {
        actions = actions.child(pix(
            move || t!("profiles.migration_no_source"),
            10.0,
            TEXT_MUTED,
            &z,
        ));
    }
    actions
}

fn github_confirm(ui: &Ui, preview: &MigrationPreview) -> View {
    let z = ui.zoom;
    let confirm = ui.clone();
    let cancel = ui.clone();
    let package = preview.package();
    let local = preview.local_version.clone();
    let incoming = package.version.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 4.0))
        .child(pix(
            move || {
                t!(
                    "profiles.migration_versions",
                    local = local.clone(),
                    incoming = incoming.clone()
                )
            },
            11.0,
            WARN,
            &z,
        ))
        .child(scope(
            ui,
            preview.row().path.display().to_string(),
            package.target.display().to_string(),
        ))
        .child(backup_root(ui, preview.backup_root.display().to_string()))
        .child(
            view()
                .flex_row()
                .gap(scaled(&z, 8.0))
                .child(danger_button(
                    &z,
                    move || t!("profiles.migration_reinstall"),
                    true,
                    move || confirm.adopt_confirm_github(),
                ))
                .child(button(
                    &z,
                    move || t!("common.cancel"),
                    true,
                    move || cancel.adopt_cancel_github(),
                )),
        )
}

fn local_actions(ui: &Ui) -> View {
    let z = ui.zoom;
    let migration = ui.profiles.migration;
    let request = ui.clone();
    let confirm = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 6.0))
        .items_start()
        .child(when(
            move || !migration.local_confirm.get(),
            move || {
                let request = request.clone();
                button(
                    &z,
                    move || t!("profiles.migration_import"),
                    true,
                    move || {
                        if !request.profiles.is_busy() {
                            migration.local_confirm.set(true);
                        }
                    },
                )
            },
        ))
        .child(when(
            move || migration.local_confirm.get(),
            move || {
                let confirm = confirm.clone();
                let mut details = view().flex_col().gap(scaled(&z, 4.0));
                for row in migration.remaining() {
                    if let Ok(item) = row.local {
                        details = details.child(scope(
                            &confirm,
                            item.source.display().to_string(),
                            item.target.display().to_string(),
                        ));
                    }
                }
                if let Some(session) = migration.session.get() {
                    details = details.child(backup_root(
                        &confirm,
                        session.backup_root.display().to_string(),
                    ));
                }
                details.child(
                    view()
                        .flex_row()
                        .gap(scaled(&z, 8.0))
                        .child(danger_button(
                            &z,
                            move || t!("profiles.adopt_confirm"),
                            true,
                            move || confirm.adopt_confirm_local(),
                        ))
                        .child(button(
                            &z,
                            move || t!("common.cancel"),
                            true,
                            move || migration.local_confirm.set(false),
                        )),
                )
            },
        ))
}

fn scope(ui: &Ui, old: String, target: String) -> View {
    let z = ui.zoom;
    view()
        .flex_col()
        .child(pix(
            move || t!("profiles.migration_old_path", path = old.clone()),
            11.0,
            WARN,
            &z,
        ))
        .child(pix(
            move || t!("profiles.migration_target", path = target.clone()),
            11.0,
            TEXT,
            &z,
        ))
}

fn backup_root(ui: &Ui, root: String) -> Text {
    pix(
        move || t!("profiles.migration_backup_root", path = root.clone()),
        11.0,
        WARN,
        &ui.zoom,
    )
}

fn result_text(result: Option<&MigrationResult>) -> String {
    match result {
        Some(MigrationResult::Done(path)) => {
            t!("profiles.migration_done", path = path.display().to_string())
        }
        Some(MigrationResult::Failed(error)) => {
            t!("profiles.migration_failed", error = error.clone())
        }
        Some(MigrationResult::ProfileFailed(error)) => {
            t!("profiles.migration_profile_failed", error = error.clone())
        }
        None => "".into(),
    }
}

fn skip_key(reason: AdoptSkip) -> &'static str {
    match reason {
        AdoptSkip::Disabled => "profiles.adopt_skip_disabled",
        AdoptSkip::AlreadyManaged => "profiles.adopt_skip_managed",
        AdoptSkip::OutsidePlugins => "profiles.adopt_skip_outside",
        AdoptSkip::InvalidName => "profiles.adopt_skip_invalid",
        AdoptSkip::MissingVersion => "profiles.adopt_skip_missing",
        AdoptSkip::StoreHasVersion => "profiles.adopt_skip_store",
        AdoptSkip::DuplicateName => "profiles.adopt_skip_duplicate",
        AdoptSkip::DuplicateTarget => "profiles.adopt_skip_target",
        AdoptSkip::BuiltIn => "profiles.adopt_skip_builtin",
        AdoptSkip::UnsafeFiles => "profiles.migration_unsafe",
    }
}
