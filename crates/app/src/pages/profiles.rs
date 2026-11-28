//! Profiles page: saved deployment profiles plus migrating loose Plugins into the library.

mod migration;

use crate::app_state::{ProfileRow, Ui};
use crate::components::{button, danger_button, panel, pix, rise_in, scaled, text_input};
use crate::palette::*;
use crate::shell::nav;
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
        .child(migration::panel_view(&adopt_ui))
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
        .bg(theme_color(SURFACE_RAISED))
        .border(scaled(&z, 1.0))
        .border_color(theme_color(BORDER_DIM))
        .child(pix(move || t!("profiles.empty"), 11.0, TEXT_MUTED, &z))
}

fn profile_row(ui: &Ui, row: &ProfileRow) -> View {
    let z = ui.zoom;
    let name = row.name.clone();
    let active = row.active;
    let entries = row.entries;
    let orphans = row.orphans.join(", ");
    let show_orphans = !orphans.is_empty();
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
        .border_b_color(theme_color(BORDER_DIM))
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
            move || show_orphans,
            move || {
                let list = orphans.clone();
                pix(
                    move || t!("profiles.orphaned", list = list.clone()),
                    10.0,
                    WARN,
                    &z,
                )
            },
        ))
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
            text_input(&z)
                .value(draft)
                .on_change(move |text: &str| draft.set(text.to_owned()))
                .placeholder(move || t!("profiles.name_placeholder")),
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
            text_input(&z)
                .value(name)
                .on_change(move |text: &str| name_input.set(text.to_owned()))
                .placeholder(move || t!("profiles.name_placeholder")),
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
