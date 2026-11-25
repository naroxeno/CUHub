//! Title bar: the frameless window's own HUD strip, with its window controls.

use nsh_core::desktop::WindowControl;
use nsh_core::prelude::*;

use super::icon_masks;
use crate::app_state::Ui;
use crate::components::{PRESS_SHIFT, STATE_MS, pix, pixel_icon, scaled};
use crate::palette::*;

const BAR_BASE: f32 = 40.0;
const BUTTON_W: f32 = 34.0;
const BUTTON_H: f32 = 26.0;
const APP_ICON: ImageAsset = asset!("branding/cuprum.png");

pub fn title_bar(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    view()
        .flex_row()
        .items_center()
        .h(scaled(&z, BAR_BASE))
        .pl(scaled(&z, 10.0))
        .pr(scaled(&z, 10.0))
        .gap(scaled(&z, 8.0))
        .bg(theme_color(SURFACE))
        .border_b(scaled(&z, 1.0))
        .border_b_color(theme_color(BORDER_DIM))
        .window_drag(true)
        .child(
            image(APP_ICON)
                .w(scaled(&z, 16.0))
                .h(scaled(&z, 16.0))
                .flex_shrink_0(),
        )
        .child(pix(move || t!("app.name"), 14.0, TEXT, &z))
        .child(pix(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            10.0,
            TEXT_DIM,
            &z,
        ))
        .child(
            view()
                .w(scaled(&z, 16.0))
                .h(scaled(&z, 2.0))
                .bg(theme_color(WARN)),
        )
        .child(view().flex_1())
        .child(window_button(
            &z,
            WindowControl::Minimize,
            icon_masks::MINIMIZE,
            false,
        ))
        .child(window_button(
            &z,
            WindowControl::Maximize,
            icon_masks::MAXIMIZE,
            false,
        ))
        .child(window_button(
            &z,
            WindowControl::Close,
            icon_masks::CLOSE,
            true,
        ))
}

fn window_button(z: &Signal<u8>, control: WindowControl, icon: [u8; 8], danger: bool) -> View {
    view()
        .flex_row()
        .items_center()
        .justify_center()
        .w(scaled(z, BUTTON_W))
        .h(scaled(z, BUTTON_H))
        .bg(theme_color(SURFACE))
        .border(scaled(z, 1.0))
        .border_color(theme_color(BORDER_DIM))
        .hover(move |s| {
            s.bg(theme_color(SURFACE_RAISED))
                .border_color(theme_color(if danger { DANGER } else { ACCENT }))
        })
        .active(|s| s.bg(theme_color(BORDER_DIM)).translate_y(px(PRESS_SHIFT)))
        .transition()
        .duration(STATE_MS)
        .cursor(Cursor::Pointer)
        .window_control(control)
        .window_drag(false)
        .child(pixel_icon(icon, theme_color(TEXT_MUTED), z))
}
