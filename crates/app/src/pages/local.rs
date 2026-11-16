//! Local mods page: rows, bulk actions and empty state.

use super::bepinex::{bepinex_missing, bepinex_pending_block, bepinex_progress, start_bepinex};
use super::shared::pending_panel;
use crate::i18n;
use crate::palette::*;
use crate::ui::{LocalModRow, Ui, button, danger_button, panel, pix, rise_in, scaled, spinner};
use cuhub_core::mods::ModStatus;
use nsh_core::prelude::*;

pub fn local_mods(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let summary = ui.clone();
    let empty = ui.clone();
    let empty_ui = ui.clone();
    let empty_z = z;
    let pending = ui.pending_install;
    let pending_z = z;
    let pending_ui = ui.clone();
    let refresh = ui.clone();
    let bulk_state = ui.clone();
    let bulk_locked = ui.clone();
    let bulk_free = ui.clone();
    let bulk_z = z;
    let refresh_watch = ui.clone();
    let list_z = z;
    let rows_ui = ui.clone();
    let rows = each(
        ui.local_rows,
        |row| row.row_key(),
        move |row, _| {
            let row = row.get();
            mod_row(&list_z, &row, &rows_ui)
        },
    );
    view().flex_col().flex_1().bind_motion(&rise_in()).child(
        panel(
            &z,
            move || i18n::page_label(i18n::LOCAL_PAGE),
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
                        .child(pix(
                            move || t!("local.count", count = summary.mods.get().len()),
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
                                .child(
                                    when(
                                        move || bulk_state.mod_files_busy(),
                                        move || mod_bulk_buttons(&bulk_z, &bulk_locked, false),
                                    )
                                    .otherwise(move || mod_bulk_buttons(&bulk_z, &bulk_free, true)),
                                )
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
                .child(when(
                    move || pending.get().is_some(),
                    move || pending_panel(&pending_z, &pending_ui),
                ))
                .child(table_header(&z))
                .child(rows),
        )
        .flex_1(),
    )
}

fn mod_bulk_buttons(z: &Signal<u8>, ui: &Ui, enabled: bool) -> View {
    let z = *z;
    let disable_ui = ui.clone();
    let enable_ui = ui.clone();
    let delete_ui = ui.clone();
    view()
        .flex_row()
        .items_center()
        .gap(scaled(&z, 8.0))
        .child(button(
            &z,
            move || t!("local.disable_all"),
            enabled,
            move || {
                disable_ui.set_all_mods_enabled(false);
            },
        ))
        .child(button(
            &z,
            move || t!("local.enable_all"),
            enabled,
            move || {
                enable_ui.set_all_mods_enabled(true);
            },
        ))
        .child(danger_button(
            &z,
            move || t!("common.delete_all"),
            enabled,
            move || {
                delete_ui.request_delete_all();
            },
        ))
}

fn empty_state(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let detect = ui.clone();
    let installable = ui.clone();
    let start = ui.clone();
    let progress = ui.clone();
    let pending = ui.clone();
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
        .child(when(
            move || pending.bepinex_pending.get().is_some(),
            move || {
                let block = pending.clone();
                bepinex_pending_block(&z, &block)
            },
        ))
}

fn mod_row(z: &Signal<u8>, row: &LocalModRow, ui: &Ui) -> View {
    let z = *z;
    let item = &row.item;
    let disabled = item.disabled;
    let name_color = if disabled { DANGER } else { TEXT };
    let meta_color = if disabled { DANGER } else { TEXT_MUTED };
    let detail_color = if disabled { DANGER } else { TEXT_DIM };
    let update_ready = row.can_update();
    let status_key = if update_ready && item.status != ModStatus::OutOfDate {
        "status.update_available"
    } else {
        i18n::status_key(item.status)
    };
    let color = if update_ready && item.status != ModStatus::OutOfDate {
        WARN
    } else {
        status_color(item.status)
    };
    let mut state = pix(move || t!(status_key), 11.0, color, &z);
    if update_ready {
        state = state.animation(
            KeyframeAnimation::new(Keyframes::Pulse)
                .duration(1800.0)
                .frame_rate(10.0),
        );
    }
    let detail_key = i18n::status_detail_key(item.status);
    let missing = item.missing_dependencies.join(", ");
    let conflicts = item.incompatible_with.join(", ");
    let has_missing = !item.missing_dependencies.is_empty();
    let has_conflicts = !item.incompatible_with.is_empty();
    view()
        .flex_col()
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .border_l(scaled(&z, 2.0))
        .border_l_color(rgb(color))
        .hover(|s| s.bg(rgb(SURFACE_RAISED)).border_b_color(rgb(BORDER)))
        .child(
            view()
                .flex_row()
                .items_center()
                .h(scaled(&z, 24.0))
                .px(scaled(&z, 6.0))
                .child(view().w(percentage(0.38)).child(pix(
                    item.name.clone(),
                    12.0,
                    name_color,
                    &z,
                )))
                .child(view().w(percentage(0.18)).child(pix(
                    item.version_label(),
                    11.0,
                    meta_color,
                    &z,
                )))
                .child(view().w(percentage(0.24)).child(pix(
                    item.author_label(),
                    11.0,
                    meta_color,
                    &z,
                )))
                .child(view().w(percentage(0.20)).child(state)),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 12.0))
                .px(scaled(&z, 6.0))
                .pb(scaled(&z, 3.0))
                .child(pix(item.evidence_label(), 10.0, detail_color, &z).flex_shrink_0())
                .child(
                    view().flex_1().min_w(px(0.0)).child(
                        pix(
                            move || {
                                let mut text = t!(detail_key);
                                if has_missing {
                                    text.push_str(&t!("status_detail.missing", list = missing));
                                }
                                if has_conflicts {
                                    text.push_str(&t!("status_detail.conflicts", list = conflicts));
                                }
                                text
                            },
                            10.0,
                            detail_color,
                            &z,
                        )
                        .truncate(),
                    ),
                ),
        )
        .child(source_line(&z, row, ui))
}

fn source_line(z: &Signal<u8>, row: &LocalModRow, ui: &Ui) -> View {
    let mut line = view()
        .flex_row()
        .items_center()
        .gap(scaled(z, 8.0))
        .px(scaled(z, 6.0))
        .pb(scaled(z, 3.0));
    if !row.source.text.is_empty() {
        let color = if row.source.warning {
            WARN
        } else if row.item.disabled {
            DANGER
        } else {
            TEXT_MUTED
        };
        line = line.child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .child(pix(row.source.text.clone(), 10.0, color, z).truncate()),
        );
    }
    if !row.source.can_check && row.source.repo.is_some() {
        line = line.child(spinner(z));
    }
    if row.source.can_pin() {
        let pin_ui = ui.clone();
        let key = row.source_key.clone();
        let options = row.source.options.clone();
        let pin_key = row.source.pin_key();
        line = line.child(button(
            z,
            move || t!(pin_key),
            true,
            move || {
                pin_ui.cycle_source_pin(key.clone(), options.clone());
            },
        ));
    }
    if row.source.can_check
        && let Some(repo) = row.source.repo.clone()
    {
        let check_ui = ui.clone();
        let key = row.source_key.clone();
        line = line.child(button(
            z,
            move || t!("common.check"),
            true,
            move || {
                check_ui.check_source(key.clone(), repo.clone());
            },
        ));
    }
    if row.can_update() {
        let update_ui = ui.clone();
        let key = row.source_key.clone();
        line = line.child(button(
            z,
            move || t!("common.update"),
            true,
            move || {
                update_ui.update_source(key.clone());
            },
        ));
    }
    let toggle_ui = ui.clone();
    let toggle_key = row.source_key.clone();
    let enable = row.item.disabled;
    let toggle_label_key = if enable {
        "common.enable"
    } else {
        "common.disable"
    };
    let toggle_z = *z;
    let toggle_watch = ui.clone();
    line = line.child(
        when(
            move || toggle_watch.mod_files_busy(),
            move || button(&toggle_z, move || t!(toggle_label_key), false, || {}),
        )
        .otherwise({
            let toggle_ui = toggle_ui.clone();
            move || {
                let toggle_ui = toggle_ui.clone();
                let toggle_key = toggle_key.clone();
                button(
                    &toggle_z,
                    move || t!(toggle_label_key),
                    true,
                    move || {
                        toggle_ui.set_mod_enabled(toggle_key.clone(), enable);
                    },
                )
            }
        }),
    );
    let delete_watch = ui.clone();
    let delete_z = *z;
    line = line.child(
        when(
            move || delete_watch.mod_files_busy(),
            move || danger_button(&delete_z, move || t!("common.delete"), false, || {}),
        )
        .otherwise({
            let delete_ui = ui.clone();
            let delete_key = row.source_key.clone();
            move || {
                let delete_ui = delete_ui.clone();
                let delete_key = delete_key.clone();
                danger_button(
                    &delete_z,
                    move || t!("common.delete"),
                    true,
                    move || {
                        delete_ui.request_delete_mod(delete_key.clone());
                    },
                )
            }
        }),
    );
    line
}

pub(super) fn status_color(status: ModStatus) -> u32 {
    match status {
        ModStatus::Disabled => DANGER,
        ModStatus::OutOfDate => WARN,
        ModStatus::UpToDate => ACCENT,
        ModStatus::Unlisted | ModStatus::Unknown => TEXT_MUTED,
    }
}

fn table_header(z: &Signal<u8>) -> impl IntoElement {
    let z = *z;
    view()
        .flex_row()
        .items_center()
        .h(scaled(&z, 20.0))
        .px(scaled(&z, 6.0))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER))
        .child(view().w(percentage(0.38)).child(pix(
            move || t!("local.table.name"),
            10.0,
            TEXT_MUTED,
            &z,
        )))
        .child(view().w(percentage(0.18)).child(pix(
            move || t!("local.table.version"),
            10.0,
            TEXT_MUTED,
            &z,
        )))
        .child(view().w(percentage(0.24)).child(pix(
            move || t!("local.table.author"),
            10.0,
            TEXT_MUTED,
            &z,
        )))
        .child(view().w(percentage(0.20)).child(pix(
            move || t!("local.table.state"),
            10.0,
            TEXT_MUTED,
            &z,
        )))
}
