//! The six pages behind the shell navigation; data comes from the Hub facade.

use cuhub_core::mods::{InstalledMod, ModStatus};
use nsh_core::prelude::*;

use crate::palette::*;
use crate::ui::{Ui, button, panel, pix, pix_c, scaled};

pub fn nexus(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let tracked = ui.clone();
    let updates = ui.clone();
    let synced = ui.clone();
    let rescan = ui.clone();
    view()
        .flex_col()
        .flex_1()
        .gap(scaled(&z, 12.0))
        .child(panel(
            &z,
            "NEXUS DASHBOARD",
            view()
                .flex_col()
                .gap(scaled(&z, 12.0))
                .child(
                    view()
                        .flex_row()
                        .gap(scaled(&z, 32.0))
                        .child(stat(
                            &z,
                            "Tracked mods",
                            move || tracked.metadata_count.get().to_string(),
                            TEXT,
                        ))
                        .child(stat(
                            &z,
                            "Updates",
                            move || {
                                updates
                                    .mods
                                    .get()
                                    .iter()
                                    .filter(|item| item.status == ModStatus::OutOfDate)
                                    .count()
                                    .to_string()
                            },
                            WARN,
                        ))
                        .child(stat(
                            &z,
                            "Last sync",
                            move || synced.metadata_synced.get(),
                            TEXT_MUTED,
                        )),
                )
                .child(pix(
                    "Community catalogue entries come from the cached metadata; announcements wait for the Hub config worker.",
                    12.0,
                    TEXT_MUTED,
                    &z,
                ))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 8.0))
                        .child(button(&z, "Rescan mods", true, move || {
                            rescan.refresh_mods();
                        })),
                ),
        ))
}

fn stat(
    z: &Signal<u8>,
    label: &'static str,
    value: impl IntoReactive<SharedString>,
    color: u32,
) -> impl IntoElement {
    let z = *z;
    view()
        .flex_col()
        .gap(scaled(&z, 2.0))
        .child(pix(label, 10.0, TEXT_DIM, &z))
        .child(pix(value, 24.0, color, &z))
}

pub fn local_mods(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let summary = ui.clone();
    let empty = ui.clone();
    let empty_ui = ui.clone();
    let empty_z = z;
    let refresh = ui.clone();
    let list_z = z;
    let rows = each(
        ui.mods,
        |item| item.row_key(),
        move |item, _| {
            let item = item.get();
            mod_row(&list_z, &item)
        },
    );
    view().flex_col().flex_1().child(
        panel(
            &z,
            "LOCAL MODS",
            view()
                .flex_col()
                .gap(scaled(&z, 8.0))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(pix(
                            move || format!("{} mod(s) in Plugins", summary.mods.get().len()),
                            11.0,
                            TEXT_MUTED,
                            &z,
                        ))
                        .child(button(&z, "Refresh", true, move || {
                            refresh.refresh_mods();
                        })),
                )
                .child(when(
                    move || empty.mods.get().is_empty(),
                    move || empty_state(&empty_z, &empty_ui),
                ))
                .child(table_header(&z))
                .child(rows),
        )
        .flex_1(),
    )
}

fn empty_state(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let ui = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .py(scaled(&z, 12.0))
        .child(pix(
            "No mods were found in the selected game folder.",
            12.0,
            TEXT_MUTED,
            &z,
        ))
        .child(button(&z, "Detect game", true, move || {
            ui.detect_game();
        }))
}

fn mod_row(z: &Signal<u8>, item: &InstalledMod) -> View {
    let z = *z;
    let color = status_color(item.status);
    let mut detail = item.status_detail();
    if !item.missing_dependencies.is_empty() {
        detail.push_str(" Missing: ");
        detail.push_str(&item.missing_dependencies.join(", "));
    }
    if !item.incompatible_with.is_empty() {
        detail.push_str(" Conflicts: ");
        detail.push_str(&item.incompatible_with.join(", "));
    }
    view()
        .flex_col()
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .border_l(scaled(&z, 2.0))
        .border_l_color(rgb(color))
        .hover(|s| s.bg(rgb(SURFACE_RAISED)))
        .child(
            view()
                .flex_row()
                .items_center()
                .h(scaled(&z, 24.0))
                .px(scaled(&z, 6.0))
                .child(
                    view()
                        .w(percentage(0.38))
                        .child(pix(item.name.clone(), 12.0, TEXT, &z)),
                )
                .child(view().w(percentage(0.18)).child(pix(
                    item.version_label(),
                    11.0,
                    TEXT_MUTED,
                    &z,
                )))
                .child(view().w(percentage(0.24)).child(pix(
                    item.author_label(),
                    11.0,
                    TEXT_MUTED,
                    &z,
                )))
                .child(
                    view()
                        .w(percentage(0.20))
                        .child(pix(item.status_label(), 11.0, color, &z)),
                ),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 12.0))
                .px(scaled(&z, 6.0))
                .pb(scaled(&z, 3.0))
                .child(pix(item.evidence_label(), 10.0, TEXT_DIM, &z))
                .child(pix(detail, 10.0, TEXT_DIM, &z)),
        )
}

fn status_color(status: ModStatus) -> u32 {
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
        .child(
            view()
                .w(percentage(0.38))
                .child(pix("NAME", 10.0, TEXT_MUTED, &z)),
        )
        .child(
            view()
                .w(percentage(0.18))
                .child(pix("VERSION", 10.0, TEXT_MUTED, &z)),
        )
        .child(
            view()
                .w(percentage(0.24))
                .child(pix("AUTHOR", 10.0, TEXT_MUTED, &z)),
        )
        .child(
            view()
                .w(percentage(0.20))
                .child(pix("STATE", 10.0, TEXT_MUTED, &z)),
        )
}

pub fn multiplayer(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    view().flex_col().flex_1().child(panel(
        &z,
        "MULTIPLAYER",
        view()
            .flex_col()
            .gap(scaled(&z, 10.0))
            .child(pix(
                "Session list and share codes land here at runtime.",
                12.0,
                TEXT_MUTED,
                &z,
            ))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(scaled(&z, 10.0))
                    .child(pix("Share code", 11.0, TEXT_DIM, &z))
                    .child(pix("CUHB-0000-0000", 14.0, REQUEST, &z)),
            )
            .child(
                view()
                    .flex_row()
                    .gap(scaled(&z, 8.0))
                    .child(button(&z, "Host session", false, || {}))
                    .child(button(&z, "Join session", false, || {})),
            )
            .child(pix("Not wired in the prototype.", 11.0, TEXT_DIM, &z)),
    ))
}

pub fn skins_backups(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    view()
        .flex_row()
        .flex_1()
        .gap(scaled(&z, 12.0))
        .child(
            panel(
                &z,
                "SKINS",
                view()
                    .flex_col()
                    .gap(scaled(&z, 10.0))
                    .child(pix(
                        "Character skin previews and merge output.",
                        12.0,
                        TEXT_MUTED,
                        &z,
                    ))
                    .child(button(&z, "Import skin", false, || {})),
            )
            .flex_1(),
        )
        .child(
            panel(
                &z,
                "BACKUPS",
                view()
                    .flex_col()
                    .gap(scaled(&z, 10.0))
                    .child(pix(
                        "Save slot backups and restore points.",
                        12.0,
                        TEXT_MUTED,
                        &z,
                    ))
                    .child(button(&z, "Create backup", false, || {})),
            )
            .flex_1(),
        )
}

pub fn hub_home(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let folder = ui.clone();
    let data = ui.clone();
    let mods = ui.clone();
    let catalogue = ui.clone();
    view().flex_col().flex_1().child(panel(
        &z,
        "HUB HOME",
        view()
            .flex_col()
            .gap(scaled(&z, 10.0))
            .child(pix(
                "Curated collections from the community hub.",
                12.0,
                TEXT_MUTED,
                &z,
            ))
            .child(row(&z, "Featured collection", "Not loaded", TEXT_DIM))
            .child(setting_row(&z, "Game folder", move || {
                game_folder_label(&folder)
            }))
            .child(setting_row(&z, "Installed mods", move || {
                mods.mods.get().len().to_string()
            }))
            .child(setting_row(&z, "Catalogue entries", move || {
                catalogue.metadata_count.get().to_string()
            }))
            .child(setting_row(&z, "Local data folder", move || {
                data.app_data.clone()
            })),
    ))
}

fn game_folder_label(ui: &Ui) -> String {
    let path = ui.settings.get().game_path;
    if path.trim().is_empty() {
        String::from("Not set")
    } else {
        path
    }
}

fn row(
    z: &Signal<u8>,
    label: &'static str,
    value: impl IntoReactive<SharedString>,
    color: u32,
) -> impl IntoElement {
    let z = *z;
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .h(scaled(&z, 24.0))
        .px(scaled(&z, 6.0))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .child(pix(label, 12.0, TEXT_MUTED, &z))
        .child(pix(value, 12.0, color, &z))
}

fn setting_row(
    z: &Signal<u8>,
    label: &'static str,
    value: impl IntoReactive<SharedString>,
) -> impl IntoElement {
    row(z, label, value, TEXT)
}

pub fn settings(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let folder = ui.clone();
    let detect = ui.clone();
    let rescan = ui.clone();
    let path = ui.clone();
    view().flex_col().flex_1().child(panel(
        &z,
        "SETTINGS",
        view()
            .flex_col()
            .gap(scaled(&z, 10.0))
            .child(pix("Game folder", 12.0, TEXT_MUTED, &z))
            .child(setting_row(&z, "Detected folder", move || {
                game_folder_label(&folder)
            }))
            .child(
                view()
                    .flex_row()
                    .gap(scaled(&z, 8.0))
                    .child(button(&z, "Detect game", true, move || {
                        detect.detect_game();
                    }))
                    .child(button(&z, "Rescan mods", true, move || {
                        rescan.refresh_mods();
                    })),
            )
            .child(pix(
                "Detection reads the Steam libraries; BepInEx must already be installed.",
                11.0,
                TEXT_DIM,
                &z,
            ))
            .child(pix("Interface scale", 12.0, TEXT_MUTED, &z))
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap(scaled(&z, 8.0))
                    .child(zoom_button(ui, 1))
                    .child(zoom_button(ui, 2))
                    .child(zoom_button(ui, 3)),
            )
            .child(pix(
                "Integer steps only, saved to Settings.json.",
                11.0,
                TEXT_DIM,
                &z,
            ))
            .child(pix("Theme", 12.0, TEXT_MUTED, &z))
            .child(pix("Pixel (fixed)", 12.0, TEXT, &z))
            .child(pix("Storage", 12.0, TEXT_MUTED, &z))
            .child(setting_row(&z, "Data folder", move || {
                path.app_data.clone()
            })),
    ))
}

fn zoom_button(ui: &Ui, level: u8) -> Text {
    let z = ui.zoom;
    let label = match level {
        1 => "1x",
        2 => "2x",
        _ => "3x",
    };
    let colored = ui.clone();
    let bordered = ui.clone();
    let pressed = ui.clone();
    pix_c(
        label,
        12.0,
        move || {
            if colored.zoom.get() == level {
                rgb(ACCENT)
            } else {
                rgb(TEXT_MUTED)
            }
        },
        &z,
    )
    .px(scaled(&z, 10.0))
    .py(scaled(&z, 4.0))
    .bg(rgb(SURFACE_RAISED))
    .border(scaled(&z, 1.0))
    .border_color(move || {
        if bordered.zoom.get() == level {
            rgb(ACCENT)
        } else {
            rgb(BORDER_DIM)
        }
    })
    .on_press(move || pressed.set_ui_scale(level))
}
