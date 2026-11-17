//! Title bar: product name and version, centered above the shell.

use nsh_core::prelude::*;

use crate::palette::*;
use crate::ui::{Ui, pix, scaled};

pub fn title_bar(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    view()
        .flex_row()
        .items_center()
        .justify_center()
        .h(scaled(&z, 32.0))
        .px(scaled(&z, 10.0))
        .gap(scaled(&z, 8.0))
        .bg(rgb(SURFACE))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER))
        .child(pix(move || t!("app.name"), 14.0, TEXT, &z))
        .child(pix(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            10.0,
            TEXT_DIM,
            &z,
        ))
}
