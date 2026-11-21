//! Local mods page: rows, bulk actions and empty state.

use super::bepinex::{bepinex_missing, bepinex_progress, start_bepinex};
use crate::i18n;
use crate::palette::*;
use crate::shell::nav;
use crate::ui::batch::batch_checkbox;
use crate::ui::{LocalModRow, Ui, button, panel, pix, rise_in, scaled};
use cuhub_core::mods::ModStatus;
use nsh_core::prelude::*;

mod actions;
mod bridge;
use actions::{bulk_menu, row_actions};
use bridge::{bridge_section, is_bridge_row};

pub fn local_mods(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let empty = ui.clone();
    let empty_ui = ui.clone();
    let empty_z = z;
    let refresh = ui.clone();
    let select_ui = ui.clone();
    let refresh_watch = ui.clone();
    let list_z = z;
    let rows_ui = ui.clone();
    let local_rows = ui.local_rows;
    let bridge_install = ui.bridge_install;
    let visible_rows = computed(move || {
        let status = bridge_install.get();
        local_rows
            .get()
            .into_iter()
            .filter(|row| !is_bridge_row(row, status.as_ref()))
            .collect::<Vec<_>>()
    });
    let rows = each(
        move || visible_rows.get(),
        |row| row.row_key(),
        move |row, _| {
            let row = row.get();
            mod_row(&list_z, &row, &rows_ui)
        },
    );
    view().flex_col().flex_1().bind_motion(&rise_in()).child(
        panel(
            &z,
            move || nav::title(nav::LOCAL),
            view()
                .flex_col()
                .gap(scaled(&z, 8.0))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .justify_between()
                        .gap(scaled(&z, 8.0))
                        .px(scaled(&z, 8.0))
                        .py(scaled(&z, 4.0))
                        .bg(rgb(SURFACE_RAISED))
                        .border_b(scaled(&z, 1.0))
                        .border_b_color(rgb(BORDER_DIM))
                        .child(pix(
                            move || t!("local.count", count = visible_rows.get().len()),
                            11.0,
                            TEXT_MUTED,
                            &z,
                        ))
                        .child(
                            view()
                                .flex_row()
                                .flex_wrap()
                                .items_center()
                                .gap(scaled(&z, 8.0))
                                .child(bulk_menu(&z, ui))
                                .child(batch_select_button(&z, &select_ui))
                                .child(
                                    when(
                                        move || refresh_watch.mod_files_busy(),
                                        move || {
                                            button(&z, move || t!("common.refresh"), false, || {})
                                        },
                                    )
                                    .otherwise({
                                        let refresh = refresh.clone();
                                        move || {
                                            let refresh = refresh.clone();
                                            button(
                                                &z,
                                                move || t!("common.refresh"),
                                                true,
                                                move || {
                                                    refresh.refresh_mods();
                                                },
                                            )
                                        }
                                    }),
                                ),
                        ),
                )
                .child(when(
                    move || empty.mods.get().is_empty(),
                    move || empty_state(&empty_z, &empty_ui),
                ))
                .child(rows)
                .child(bridge_section(&z, ui)),
        )
        .flex_1(),
    )
}

fn batch_select_button(z: &Signal<u8>, ui: &Ui) -> impl IntoElement {
    let z = *z;
    let available = ui.clone();
    let watch = ui.clone();
    let clear = ui.clone();
    let select = ui.clone();
    when(
        move || crate::ui::batch::updatable_count(&available) == 0,
        move || pix(move || t!("local.no_updates"), 10.0, TEXT_MUTED, &z),
    )
    .otherwise(move || {
        let clear = clear.clone();
        let select = select.clone();
        let watch = watch.clone();
        when(
            move || crate::ui::batch::all_updatable_selected(&watch),
            move || {
                let clear = clear.clone();
                button(
                    &z,
                    move || t!("local.batch_clear"),
                    true,
                    move || {
                        clear.clear_batch_selection();
                    },
                )
            },
        )
        .otherwise({
            let select = select.clone();
            move || {
                let select = select.clone();
                let label = select.clone();
                button(
                    &z,
                    move || {
                        t!(
                            "local.batch_select_all",
                            count = crate::ui::batch::updatable_count(&label)
                        )
                    },
                    true,
                    move || select.select_updatable(),
                )
            }
        })
    })
}

fn empty_state(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let detect = ui.clone();
    let installable = ui.clone();
    let start = ui.clone();
    let progress = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .py(scaled(&z, 12.0))
        .child(pix(move || t!("local.empty"), 12.0, TEXT_MUTED, &z))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(button(
                    &z,
                    move || t!("common.detect_game"),
                    true,
                    move || {
                        detect.detect_game();
                    },
                ))
                .child(when(
                    move || bepinex_missing(&installable),
                    move || {
                        let start = start.clone();
                        button(
                            &z,
                            move || t!("bepinex.install"),
                            true,
                            move || {
                                start_bepinex(&start);
                            },
                        )
                    },
                )),
        )
        .child(bepinex_progress(&z, &progress))
}

fn mod_row(z: &Signal<u8>, row: &LocalModRow, ui: &Ui) -> View {
    let details = signal(false);
    let zoom = *z;
    let item = &row.item;
    let update_ready = row.can_update();
    let (status_key, color) = update_status(row);
    let mut identity = view()
        .flex_1()
        .min_w(scaled(z, 120.0))
        .flex_row()
        .items_center()
        .gap(scaled(z, 6.0));
    if update_ready {
        identity = identity.child(batch_checkbox(z, ui, row.source_key.clone(), true));
    }
    let marker = if update_ready {
        WARN
    } else if item.disabled {
        TEXT_DIM
    } else {
        ACCENT
    };
    identity = identity.child(mod_marker(z, marker));
    let name_color = if item.disabled { TEXT_MUTED } else { TEXT };
    identity = identity.child(
        pix(item.name.clone(), 12.0, name_color, z)
            .min_w(px(0.0))
            .flex_1()
            .truncate(),
    );
    if row.loaded {
        identity = identity.child(pix(move || t!("local.loaded"), 10.0, ACCENT, z).flex_shrink_0());
    }
    let version = visible_version(row);
    let detail_row = row.clone();

    view()
        .flex_col()
        .border_b(scaled(z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .px(scaled(z, 6.0))
        .py(scaled(z, 5.0))
        .hover(|s| s.bg(rgb(SURFACE_RAISED)))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(scaled(z, 8.0))
                .child(identity)
                .child(pix(
                    move || version.clone().unwrap_or_else(|| "—".into()),
                    11.0,
                    TEXT_MUTED,
                    z,
                ))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap(scaled(z, 6.0))
                        .child(when(
                            move || status_key != "status.disabled",
                            move || pix(move || t!("local.enabled"), 10.0, TEXT_MUTED, &zoom),
                        ))
                        .child(pix(move || t!(status_key), 10.0, color, z)),
                )
                .child(row_actions(z, row, ui, details)),
        )
        .child(row_warnings(z, row))
        .child(when(
            move || details.get(),
            move || row_details(&zoom, &detail_row),
        ))
}

fn mod_marker(z: &Signal<u8>, color: u32) -> View {
    view()
        .w(scaled(z, 14.0))
        .h(scaled(z, 14.0))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .bg(rgb(SURFACE_RAISED))
        .border(scaled(z, 1.0))
        .border_color(rgb(BORDER_DIM))
        .child(
            view()
                .w(scaled(z, 6.0))
                .h(scaled(z, 6.0))
                .border(scaled(z, 2.0))
                .border_color(rgb(color)),
        )
}

fn visible_version(row: &LocalModRow) -> Option<String> {
    use cuhub_core::{sources::SourceKind, version::HubVersion};

    let installed = row.item.installed_version.as_ref()?;
    if row.can_update() {
        let candidate = match row.source.effective {
            Some(SourceKind::Github) => {
                row.source.release_version.as_ref().map(ToString::to_string)
            }
            _ => row.item.expected_version.clone(),
        };
        if let Some(candidate) = candidate
            && let Some(current) = HubVersion::parse_external(installed)
            && let Some(next) = HubVersion::parse_external(&candidate)
            && current < next
        {
            return Some(format!("{installed} → {candidate}"));
        }
    }
    Some(installed.clone())
}

fn update_status(row: &LocalModRow) -> (&'static str, u32) {
    use cuhub_core::sources::SourceKind;

    if row.item.disabled {
        ("status.disabled", TEXT_MUTED)
    } else if row.can_update() {
        ("status.update_available", WARN)
    } else if row.source.effective == Some(SourceKind::Github) {
        use cuhub_core::version::HubVersion;

        let installed = row
            .item
            .installed_version
            .as_deref()
            .and_then(HubVersion::parse_external);
        if matches!((installed.as_ref(), row.source.release_version.as_ref()), (Some(current), Some(latest)) if current >= latest)
        {
            ("local.no_newer_github", TEXT_MUTED)
        } else {
            ("status.update_unknown", TEXT_MUTED)
        }
    } else if row.item.status == ModStatus::UpToDate {
        ("local.no_newer_catalogue", TEXT_MUTED)
    } else {
        (
            i18n::status_key(row.item.status),
            status_color(row.item.status),
        )
    }
}

fn row_warnings(z: &Signal<u8>, row: &LocalModRow) -> View {
    let mut warnings = view().flex_col();
    let item = &row.item;
    if !item.missing_dependencies.is_empty() {
        let list = item.missing_dependencies.join(", ");
        warnings = warnings.child(pix(
            move || t!("status_detail.missing", list = list),
            10.0,
            WARN,
            z,
        ));
    }
    if !item.incompatible_with.is_empty() {
        let list = item.incompatible_with.join(", ");
        warnings = warnings.child(pix(
            move || t!("status_detail.conflicts", list = list),
            10.0,
            WARN,
            z,
        ));
    }
    warnings
}

fn row_details(z: &Signal<u8>, row: &LocalModRow) -> View {
    let detail_key = i18n::status_detail_key(row.item.status);
    let author = row
        .item
        .author
        .clone()
        .filter(|name| !name.trim().is_empty());
    let mut details = view()
        .flex_col()
        .mt(scaled(z, 5.0))
        .px(scaled(z, 8.0))
        .py(scaled(z, 5.0))
        .gap(scaled(z, 3.0))
        .bg(rgb(SURFACE_RAISED))
        .border_l(scaled(z, 2.0))
        .border_l_color(rgb(ACCENT))
        .child(pix(row.item.name.clone(), 11.0, TEXT, z))
        .child(pix(
            move || author.clone().unwrap_or_else(|| t!("nexus.unknown_author")),
            10.0,
            TEXT_MUTED,
            z,
        ))
        .child(pix(row.item.evidence_label(), 10.0, TEXT_MUTED, z))
        .child(pix(move || t!(detail_key), 10.0, TEXT_MUTED, z))
        .child(pix(
            row.source.text.clone(),
            10.0,
            if row.source.warning { WARN } else { TEXT_MUTED },
            z,
        ));
    if row.managed {
        details = details.child(pix(
            move || t!("local.managed_by_profile"),
            10.0,
            TEXT_MUTED,
            z,
        ));
    }
    details
}

pub(super) fn status_color(status: ModStatus) -> u32 {
    match status {
        ModStatus::Disabled => DANGER,
        ModStatus::OutOfDate => WARN,
        ModStatus::UpToDate => ACCENT,
        ModStatus::Unlisted | ModStatus::Unknown => TEXT_MUTED,
    }
}
