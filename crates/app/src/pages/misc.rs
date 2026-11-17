//! Smaller pages: multiplayer and skins.

use crate::palette::*;
use crate::ui::{Ui, button, panel, pix, rise_in, scaled};
use nsh_core::prelude::*;

pub fn multiplayer(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    view()
        .flex_col()
        .flex_1()
        .bind_motion(&rise_in())
        .child(panel(
            &z,
            move || t!("panel.multiplayer"),
            view()
                .flex_col()
                .gap(scaled(&z, 10.0))
                .child(pix(move || t!("multiplayer.intro"), 12.0, TEXT_MUTED, &z))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 10.0))
                        .child(pix(
                            move || t!("multiplayer.share_code"),
                            11.0,
                            TEXT_DIM,
                            &z,
                        ))
                        .child(pix("CUHB-0000-0000", 14.0, REQUEST, &z)),
                )
                .child(
                    view()
                        .flex_row()
                        .gap(scaled(&z, 8.0))
                        .child(button(&z, move || t!("multiplayer.host"), false, || {}))
                        .child(button(&z, move || t!("multiplayer.join"), false, || {})),
                )
                .child(pix(move || t!("multiplayer.not_wired"), 11.0, TEXT_DIM, &z)),
        ))
}

pub fn skins_backups(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    view()
        .flex_row()
        .flex_1()
        .bind_motion(&rise_in())
        .gap(scaled(&z, 12.0))
        .child(
            panel(
                &z,
                move || t!("panel.skins"),
                view()
                    .flex_col()
                    .gap(scaled(&z, 10.0))
                    .child(pix(move || t!("skins.intro"), 12.0, TEXT_MUTED, &z))
                    .child(button(&z, move || t!("skins.import"), false, || {})),
            )
            .flex_1(),
        )
        .child(
            panel(
                &z,
                move || t!("panel.backups"),
                view()
                    .flex_col()
                    .gap(scaled(&z, 10.0))
                    .child(pix(move || t!("backups.intro"), 12.0, TEXT_MUTED, &z))
                    .child(button(&z, move || t!("backups.create"), false, || {})),
            )
            .flex_1(),
        )
}
