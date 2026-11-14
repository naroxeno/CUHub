//! The six placeholder pages behind the shell navigation.

use nsh_core::prelude::*;

use crate::palette::*;
use crate::ui::{button, panel, pix, pix_c, scaled, Ui};

pub fn nexus(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom.clone();
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
                        .child(stat(&z, "Tracked mods", "0", TEXT))
                        .child(stat(&z, "Updates", "0", WARN))
                        .child(stat(&z, "Last sync", "Never", TEXT_MUTED)),
                )
                .child(pix(
                    "Announcements and trending lists load from the hub config.",
                    12.0,
                    TEXT_MUTED,
                    &z,
                )),
        ))
}

fn stat(z: &Signal<u8>, label: &'static str, value: &'static str, color: u32) -> impl IntoElement {
    let z = z.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 2.0))
        .child(pix(label, 10.0, TEXT_DIM, &z))
        .child(pix(value, 24.0, color, &z))
}

const MODS: [(&str, &str, &str, &str, u32); 5] = [
    ("CasualtiesLib", "0.4.2", "Naro", "Enabled", ACCENT),
    ("Better Loot", "1.2.0", "kestrel", "Update", WARN),
    ("Hardcore Nights", "0.9.5", "kestrel", "Enabled", ACCENT),
    ("Old Textures", "0.1.0", "moth", "Disabled", TEXT_DIM),
    ("Broken Pack", "2.0.0", "moth", "Missing", DANGER),
];

pub fn local_mods(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom.clone();
    let mut rows = view().flex_col().child(table_header(&z));
    for (name, version, author, state, color) in MODS {
        rows = rows.child(
            view()
                .flex_row()
                .items_center()
                .h(scaled(&z, 24.0))
                .px(scaled(&z, 6.0))
                .border_b(scaled(&z, 1.0))
                .border_b_color(rgb(BORDER_DIM))
                .hover(|s| s.bg(rgb(SURFACE_RAISED)))
                .child(view().w(percentage(0.40)).child(pix(name, 12.0, TEXT, &z)))
                .child(view().w(percentage(0.15)).child(pix(version, 11.0, TEXT_MUTED, &z)))
                .child(view().w(percentage(0.25)).child(pix(author, 11.0, TEXT_MUTED, &z)))
                .child(view().w(percentage(0.20)).child(pix(state, 11.0, color, &z))),
        );
    }
    view()
        .flex_col()
        .flex_1()
        .child(panel(&z, "LOCAL MODS", rows).flex_1())
}

fn table_header(z: &Signal<u8>) -> impl IntoElement {
    let z = z.clone();
    view()
        .flex_row()
        .items_center()
        .h(scaled(&z, 20.0))
        .px(scaled(&z, 6.0))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER))
        .child(view().w(percentage(0.40)).child(pix("NAME", 10.0, TEXT_MUTED, &z)))
        .child(view().w(percentage(0.15)).child(pix("VERSION", 10.0, TEXT_MUTED, &z)))
        .child(view().w(percentage(0.25)).child(pix("AUTHOR", 10.0, TEXT_MUTED, &z)))
        .child(view().w(percentage(0.20)).child(pix("STATE", 10.0, TEXT_MUTED, &z)))
}

pub fn multiplayer(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom.clone();
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
    let z = ui.zoom.clone();
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
                    .child(pix("Character skin previews and merge output.", 12.0, TEXT_MUTED, &z))
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
                    .child(pix("Save slot backups and restore points.", 12.0, TEXT_MUTED, &z))
                    .child(button(&z, "Create backup", false, || {})),
            )
            .flex_1(),
        )
}

pub fn hub_home(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom.clone();
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
            .child(row(&z, "Local data folder", "appdata", TEXT_MUTED)),
    ))
}

fn row(z: &Signal<u8>, label: &'static str, value: &'static str, color: u32) -> impl IntoElement {
    let z = z.clone();
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

pub fn settings(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom.clone();
    view().flex_col().flex_1().child(panel(
        &z,
        "SETTINGS",
        view()
            .flex_col()
            .gap(scaled(&z, 10.0))
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
                "Integer steps only, applied immediately. Prototype keeps it in memory.",
                11.0,
                TEXT_DIM,
                &z,
            ))
            .child(pix("Theme", 12.0, TEXT_MUTED, &z))
            .child(pix("Pixel (fixed)", 12.0, TEXT, &z)),
    ))
}

fn zoom_button(ui: &Ui, level: u8) -> Text {
    let z = ui.zoom.clone();
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
    .on_press(move || pressed.zoom.set(level))
}
