//! Top bar: product name, version and the quick actions on the right.

use nsh_core::prelude::*;

use crate::palette::*;
use crate::ui::{Ui, button, pix, scaled};

pub fn top_bar(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let launch = ui.clone();
    let report = ui.clone();
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .h(scaled(&z, 32.0))
        .px(scaled(&z, 10.0))
        .bg(rgb(SURFACE))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(pix("CUHUB", 14.0, TEXT, &z))
                .child(pix(env!("CARGO_PKG_VERSION"), 10.0, TEXT_DIM, &z)),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(button(
                    &z,
                    move || t!("topbar.launch"),
                    true,
                    move || {
                        launch.launch_game();
                    },
                ))
                .child(button(
                    &z,
                    move || t!("topbar.report_issues"),
                    false,
                    move || {
                        let _ = &report;
                    },
                )),
        )
}
