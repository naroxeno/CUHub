//! Developer console overlay, opened by tapping the version row five times.

use nsh_core::prelude::*;

use super::nav;
use crate::palette::*;
use crate::ui::{Ui, button, fade_in, panel, pix, rise_in, scaled};

pub fn dev_overlay(ui: &Ui) -> impl IntoElement {
    let show = ui.clone();
    let close = ui.clone();
    let done = ui.clone();
    when(
        move || show.dev.get(),
        move || {
            let z = close.zoom;
            let page = close.clone();
            let zoom = close.zoom.get();
            let veil = fade_in();
            let card = rise_in();
            view()
                .absolute()
                .inset(px(0.0))
                .w_full()
                .h_full()
                .bg(rgba(0x000000CC))
                .bind_motion(&veil)
                .items_center()
                .justify_center()
                .child(
                    panel(
                        &z,
                        move || t!("panel.developer_console"),
                        view()
                            .flex_col()
                            .gap(scaled(&z, 8.0))
                            .child(pix("nsh 0.5.1 · prototype shell", 12.0, TEXT_MUTED, &z))
                            .child(pix(
                                move || t!("dev.page", page = nav::title(page.page.get())),
                                12.0,
                                TEXT,
                                &z,
                            ))
                            .child(pix(move || t!("dev.zoom", zoom = zoom), 12.0, TEXT, &z))
                            .child(button(
                                &z,
                                move || t!("common.close"),
                                true,
                                move || done.dev.set(false),
                            )),
                    )
                    .w(scaled(&z, 320.0))
                    .bind_motion(&card),
                )
        },
    )
}
