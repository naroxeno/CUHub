//! Title bar: the frameless window's own HUD strip, with its window controls.

use nsh_core::desktop::WindowControl;
use nsh_core::prelude::*;

use crate::palette::*;
use crate::ui::{PRESS_SHIFT, STATE_MS, Ui, pix, scaled};

const BAR_BASE: f32 = 40.0;
const BUTTON_W: f32 = 34.0;
const BUTTON_H: f32 = 26.0;
const GLYPH_BASE: f32 = 12.0;

pub fn title_bar(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    view()
        .flex_row()
        .items_center()
        .h(scaled(&z, BAR_BASE))
        .pl(scaled(&z, 10.0))
        .pr(scaled(&z, 10.0))
        .gap(scaled(&z, 8.0))
        .bg(rgb(SURFACE))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER))
        .window_drag(true)
        .child(marker(&z))
        .child(pix(move || t!("app.name"), 14.0, TEXT, &z))
        .child(pix(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            10.0,
            TEXT_DIM,
            &z,
        ))
        .child(
            view()
                .w(scaled(&z, 1.0))
                .h(scaled(&z, 14.0))
                .bg(rgb(BORDER_DIM)),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .flex_1()
                .gap(scaled(&z, 8.0))
                .mx(scaled(&z, 4.0))
                .child(view().flex_1().h(scaled(&z, 1.0)).bg(rgb(BORDER_DIM)))
                .child(view().w(scaled(&z, 4.0)).h(scaled(&z, 4.0)).bg(rgb(ACCENT)))
                .child(pix("///", 10.0, TEXT_DIM, &z)),
        )
        .child(window_button(&z, WindowControl::Minimize, "—", false))
        .child(window_button(&z, WindowControl::Maximize, "□", false))
        .child(window_button(&z, WindowControl::Close, "×", true))
}

fn marker(z: &Signal<u8>) -> View {
    view()
        .flex_row()
        .items_center()
        .justify_center()
        .w(scaled(z, 8.0))
        .h(scaled(z, 8.0))
        .bg(rgb(ACCENT))
        .child(view().w(scaled(z, 3.0)).h(scaled(z, 3.0)).bg(rgb(SURFACE)))
}

fn window_button(
    z: &Signal<u8>,
    control: WindowControl,
    glyph: &'static str,
    danger: bool,
) -> View {
    view()
        .flex_row()
        .items_center()
        .justify_center()
        .w(scaled(z, BUTTON_W))
        .h(scaled(z, BUTTON_H))
        .bg(rgb(SURFACE))
        .border(scaled(z, 1.0))
        .border_color(rgb(BORDER_DIM))
        .hover(move |s| {
            s.bg(rgb(SURFACE_RAISED))
                .border_color(rgb(if danger { DANGER } else { ACCENT }))
        })
        .active(|s| s.bg(rgb(BORDER_DIM)).translate_y(px(PRESS_SHIFT)))
        .transition()
        .duration(STATE_MS)
        .cursor(Cursor::Pointer)
        .window_control(control)
        .window_drag(false)
        .child(pix(glyph, GLYPH_BASE, TEXT_MUTED, z))
}
