//! Profiles page: saved deployment profiles plus migrating loose Plugins into the library.

use crate::palette::*;
use crate::shell::nav;
use crate::ui::{
    ProfileRow, ProfilesBusy, Ui, button, danger_button, panel, pix, rise_in, scaled, spinner,
};
use nsh_base::input;
use nsh_core::prelude::*;

pub fn profiles(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let list_ui = ui.clone();
    let adopt_ui = ui.clone();
    view()
        .flex_col()
        .flex_1()
        .gap(scaled(&z, 12.0))
        .bind_motion(&rise_in())
        .child(profile_panel(&list_ui))
        .child(adopt_panel(&adopt_ui))
}

fn profile_panel(ui: &Ui) -> View {
    let z = ui.zoom;
    let rows = ui.profiles.rows;
    let list_ui = ui.clone();
    let empty_watch = ui.clone();
    let empty_ui = ui.clone();
    let error_watch = ui.clone();
    let error_ui = ui.clone();
    let refresh_ui = ui.clone();
    let create_ui = ui.clone();
    panel(
        &z,
        move || nav::title(nav::PROFILES),
        view()
            .flex_col()
            .gap(scaled(&z, 10.0))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(pix(
                        move || t!("profiles.count", count = rows.get().len()),
                        11.0,
                        TEXT_MUTED,
                        &z,
                    ))
                    .child(button(
                        &z,
                        move || t!("common.refresh"),
                        true,
                        move || refresh_ui.profiles_refresh(),
                    )),
            )
            .child(when(
                move || empty_watch.profiles.rows.get().is_empty(),
                move || empty_profile(&empty_ui),
            ))
            .child(each(
                rows,
                |row| row.name.clone(),
                move |row, _| {
                    let row = row.get();
                    profile_row(&list_ui, &row)
                },
            ))
            .child(when(
                move || {
                    let (key, detail) = error_watch.profiles.error.get();
                    !key.is_empty() || !detail.is_empty()
                },
                move || {
                    let (key, detail) = error_ui.profiles.error.get();
                    let text = match (key.is_empty(), detail.is_empty()) {
                        (true, _) => detail,
                        (false, true) => t!(key),
                        (false, false) => format!("{}\n{detail}", t!(key)),
                    };
                    pix(text, 11.0, DANGER, &z)
                },
            ))
            .child(create_block(&create_ui)),
    )
}

fn empty_profile(ui: &Ui) -> View {
    let z = ui.zoom;
    view()
        .flex_col()
        .px(scaled(&z, 8.0))
        .py(scaled(&z, 8.0))
        .bg(rgb(SURFACE_RAISED))
        .border(scaled(&z, 1.0))
        .border_color(rgb(BORDER_DIM))
        .child(pix(move || t!("profiles.empty"), 11.0, TEXT_MUTED, &z))
}

fn profile_row(ui: &Ui, row: &ProfileRow) -> View {
    let z = ui.zoom;
    let name = row.name.clone();
    let active = row.active;
    let entries = row.entries;
    let rename_ui = ui.clone();
    let apply_ui = ui.clone();
    let remove_ui = ui.clone();
    let rename_name = name.clone();
    let apply_name = name.clone();
    let remove_name = name.clone();
    let rename_watch = ui.clone();
    let rename_watch_name = name.clone();
    let rename_confirm_ui = ui.clone();
    let rename_confirm_name = name.clone();
    let apply_watch = ui.clone();
    let apply_watch_name = name.clone();
    let remove_watch = ui.clone();
    let remove_watch_name = name.clone();
    let apply_confirm_ui = ui.clone();
    let apply_confirm_name = name.clone();
    let remove_confirm_ui = ui.clone();
    let remove_confirm_name = name.clone();
    let label_z = z;
    let count_z = z;
    view()
        .flex_col()
        .gap(scaled(&z, 6.0))
        .py(scaled(&z, 6.0))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_between()
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 8.0))
                        .child(pix(name, 12.0, TEXT, &label_z))
                        .child(when(
                            move || active,
                            move || pix(move || t!("profiles.active"), 10.0, ACCENT, &z),
                        ))
                        .child(pix(
                            move || t!("profiles.entries", count = entries),
                            10.0,
                            TEXT_MUTED,
                            &count_z,
                        )),
                )
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 8.0))
                        .child(button(
                            &z,
                            move || t!("profiles.rename"),
                            true,
                            move || rename_ui.profiles_request_rename(&rename_name),
                        ))
                        .child(button(
                            &z,
                            move || t!("profiles.apply"),
                            true,
                            move || apply_ui.profiles_request_apply(&apply_name),
                        ))
                        .child(danger_button(
                            &z,
                            move || t!("common.delete"),
                            true,
                            move || remove_ui.profiles_request_remove(&remove_name),
                        )),
                ),
        )
        .child(when(
            move || apply_watch.profiles.apply_name.get() == apply_watch_name,
            move || apply_confirm_block(&apply_confirm_ui, &apply_confirm_name),
        ))
        .child(when(
            move || remove_watch.profiles.remove_name.get() == remove_watch_name,
            move || remove_confirm_block(&remove_confirm_ui, &remove_confirm_name),
        ))
        .child(when(
            move || rename_watch.profiles.rename_from.get() == rename_watch_name,
            move || rename_block(&rename_confirm_ui, &rename_confirm_name),
        ))
}

fn apply_confirm_block(ui: &Ui, name: &str) -> View {
    let z = ui.zoom;
    let name = name.to_owned();
    let count = ui.profiles.apply_count;
    let confirm_ui = ui.clone();
    let cancel_ui = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 6.0))
        .child(pix(
            move || {
                t!(
                    "profiles.apply_warning",
                    name = name.clone(),
                    count = count.get()
                )
            },
            11.0,
            WARN,
            &z,
        ))
        .child(
            view()
                .flex_row()
                .gap(scaled(&z, 8.0))
                .child(danger_button(
                    &z,
                    move || t!("profiles.apply"),
                    true,
                    move || confirm_ui.profiles_confirm_apply(),
                ))
                .child(button(
                    &z,
                    move || t!("common.cancel"),
                    true,
                    move || cancel_ui.profiles_cancel_apply(),
                )),
        )
}

fn remove_confirm_block(ui: &Ui, name: &str) -> View {
    let z = ui.zoom;
    let name = name.to_owned();
    let confirm_ui = ui.clone();
    let cancel_ui = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 6.0))
        .child(pix(
            move || t!("profiles.remove_warning", name = name.clone()),
            11.0,
            WARN,
            &z,
        ))
        .child(
            view()
                .flex_row()
                .gap(scaled(&z, 8.0))
                .child(danger_button(
                    &z,
                    move || t!("common.delete"),
                    true,
                    move || confirm_ui.profiles_confirm_remove(),
                ))
                .child(button(
                    &z,
                    move || t!("common.cancel"),
                    true,
                    move || cancel_ui.profiles_cancel_remove(),
                )),
        )
}

fn rename_block(ui: &Ui, name: &str) -> View {
    let z = ui.zoom;
    let name = name.to_owned();
    let draft = ui.profiles.rename_name;
    let confirm_ui = ui.clone();
    let cancel_ui = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 6.0))
        .child(pix(
            move || t!("profiles.rename_hint", name = name.clone()),
            11.0,
            TEXT_MUTED,
            &z,
        ))
        .child(
            input()
                .value(draft)
                .on_change(move |text: &str| draft.set(text.to_owned()))
                .placeholder(move || t!("profiles.name_placeholder"))
                .font_family(crate::ui::PIXEL_FONT.family)
                .text_size(scaled(&z, 12.0))
                .h(scaled(&z, 24.0))
                .px(scaled(&z, 6.0))
                .bg(rgb(SURFACE_RAISED))
                .border(scaled(&z, 1.0))
                .border_color(rgb(BORDER)),
        )
        .child(
            view()
                .flex_row()
                .gap(scaled(&z, 8.0))
                .child(button(
                    &z,
                    move || t!("profiles.rename"),
                    true,
                    move || confirm_ui.profiles_confirm_rename(),
                ))
                .child(button(
                    &z,
                    move || t!("common.cancel"),
                    true,
                    move || cancel_ui.profiles_cancel_rename(),
                )),
        )
}

fn create_block(ui: &Ui) -> View {
    let z = ui.zoom;
    let name = ui.profiles.name;
    let name_input = ui.profiles.name;
    let snapshot_ui = ui.clone();
    let blank_ui = ui.clone();
    let confirm_watch = ui.clone();
    let confirm_ui = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .child(
            input()
                .value(name)
                .on_change(move |text: &str| name_input.set(text.to_owned()))
                .placeholder(move || t!("profiles.name_placeholder"))
                .font_family(crate::ui::PIXEL_FONT.family)
                .text_size(scaled(&z, 12.0))
                .text_color(rgb(TEXT))
                .h(scaled(&z, 24.0))
                .px(scaled(&z, 6.0))
                .bg(rgb(SURFACE_RAISED))
                .border(scaled(&z, 1.0))
                .border_color(rgb(BORDER)),
        )
        .child(
            view()
                .flex_row()
                .gap(scaled(&z, 8.0))
                .child(button(
                    &z,
                    move || t!("profiles.create_snapshot"),
                    true,
                    move || snapshot_ui.profiles_request_create(true),
                ))
                .child(button(
                    &z,
                    move || t!("profiles.create_blank"),
                    true,
                    move || blank_ui.profiles_request_create(false),
                )),
        )
        .child(when(
            move || !confirm_watch.profiles.create_confirm.get().is_empty(),
            move || create_confirm_block(&confirm_ui),
        ))
}

fn create_confirm_block(ui: &Ui) -> View {
    let z = ui.zoom;
    let name = ui.profiles.create_confirm;
    let confirm_ui = ui.clone();
    let cancel_ui = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 6.0))
        .child(pix(
            move || t!("profiles.overwrite_warning", name = name.get()),
            11.0,
            WARN,
            &z,
        ))
        .child(
            view()
                .flex_row()
                .gap(scaled(&z, 8.0))
                .child(danger_button(
                    &z,
                    move || t!("profiles.overwrite"),
                    true,
                    move || confirm_ui.profiles_confirm_create(),
                ))
                .child(button(
                    &z,
                    move || t!("common.cancel"),
                    true,
                    move || cancel_ui.profiles_cancel_create(),
                )),
        )
}

fn adopt_panel(ui: &Ui) -> View {
    let z = ui.zoom;
    let scan_ui = ui.clone();
    let busy_watch = ui.clone();
    let stats_watch = ui.clone();
    let stats_ui = ui.clone();
    let reasons = ui.profiles.adopt_reasons;
    let names = ui.profiles.adopt_names;
    let warning_watch = ui.clone();
    let warning_ui = ui.clone();
    let done_watch = ui.clone();
    let done_ui = ui.clone();
    let none_watch = ui.clone();
    let adopt_error_watch = ui.clone();
    let adopt_error_ui = ui.clone();
    panel(
        &z,
        move || t!("profiles.adopt_title"),
        view()
            .flex_col()
            .gap(scaled(&z, 10.0))
            .child(pix(move || t!("profiles.adopt_hint"), 11.0, TEXT_MUTED, &z))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(scaled(&z, 8.0))
                    .child(button(
                        &z,
                        move || t!("profiles.adopt_scan"),
                        true,
                        move || scan_ui.adopt_scan(),
                    ))
                    .child(when(
                        move || {
                            matches!(
                                busy_watch.profiles.busy.get(),
                                ProfilesBusy::Scan | ProfilesBusy::Adopt
                            )
                        },
                        move || {
                            view()
                                .flex_row()
                                .items_center()
                                .gap(scaled(&z, 6.0))
                                .child(spinner(&z))
                                .child(pix(
                                    move || t!("profiles.adopt_scanning"),
                                    10.0,
                                    TEXT_MUTED,
                                    &z,
                                ))
                        },
                    )),
            )
            .child(when(
                move || !adopt_error_watch.profiles.adopt_error.get().is_empty(),
                move || {
                    let key = adopt_error_ui.profiles.adopt_error.get();
                    pix(t!(key), 11.0, DANGER, &z)
                },
            ))
            .child(when(
                move || stats_watch.profiles.adopt_scanned.get(),
                move || {
                    let adoptable = stats_ui.profiles.adoptable.get();
                    let skipped = stats_ui.profiles.adopt_skipped.get();
                    view()
                        .flex_col()
                        .gap(scaled(&z, 4.0))
                        .child(when(
                            move || adoptable > 0,
                            move || {
                                pix(
                                    t!("profiles.adopt_adoptable", count = adoptable),
                                    11.0,
                                    TEXT,
                                    &z,
                                )
                            },
                        ))
                        .child(when(
                            move || skipped > 0,
                            move || {
                                pix(
                                    t!("profiles.adopt_skipped", count = skipped),
                                    10.0,
                                    TEXT_MUTED,
                                    &z,
                                )
                            },
                        ))
                        .child(each(
                            reasons,
                            |reason| reason.key,
                            move |reason, _| {
                                let reason = reason.get();
                                pix(
                                    t!(
                                        "profiles.adopt_reason",
                                        reason = t!(reason.key),
                                        count = reason.count
                                    ),
                                    10.0,
                                    TEXT_MUTED,
                                    &z,
                                )
                            },
                        ))
                        .child(each(
                            names,
                            |name| name.clone(),
                            move |name, _| {
                                pix(
                                    t!("profiles.adopt_name", name = name.get()),
                                    10.0,
                                    TEXT_MUTED,
                                    &z,
                                )
                            },
                        ))
                },
            ))
            .child(when(
                move || warning_watch.profiles.adoptable.get() > 0,
                move || adopt_confirm_block(&warning_ui),
            ))
            .child(when(
                move || done_watch.profiles.adopt_done.get().is_some(),
                move || {
                    let (count, dir) = done_ui.profiles.adopt_done.get().unwrap_or_default();
                    pix(
                        t!("profiles.adopt_done", count = count, dir = dir),
                        11.0,
                        ACCENT,
                        &z,
                    )
                },
            ))
            .child(when(
                move || {
                    none_watch.profiles.adopt_scanned.get()
                        && none_watch.profiles.adoptable.get() == 0
                        && none_watch.profiles.adopt_done.get().is_none()
                },
                move || pix(t!("profiles.adopt_none"), 11.0, TEXT_MUTED, &z),
            )),
    )
}

fn adopt_confirm_block(ui: &Ui) -> View {
    let z = ui.zoom;
    let adoptable = ui.profiles.adoptable;
    let confirm_ui = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 6.0))
        .child(pix(
            move || t!("profiles.adopt_warning", count = adoptable.get()),
            11.0,
            WARN,
            &z,
        ))
        .child(danger_button(
            &z,
            move || t!("profiles.adopt_confirm"),
            true,
            move || confirm_ui.adopt_confirm(),
        ))
}
