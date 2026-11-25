//! The skins and backups page.

use crate::app_state::Ui;
use crate::components::{button, panel, pix, rise_in, scaled};
use crate::palette::*;
use nsh_core::prelude::*;

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
