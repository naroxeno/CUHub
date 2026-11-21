//! Status line: the current notice or page title, plus the game bridge state.

use nsh_core::prelude::*;

use super::nav;
use crate::palette::*;
use crate::ui::{PIXEL_FONT, Ui, bridge_color, bridge_text, scaled};

pub fn statusline(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let state = ui.clone();
    let watched = ui.clone();
    let bridge = ui.bridge;
    let toast = motion();
    effect(move || {
        if !watched.notice.get().is_empty() {
            toast.play(MotionPreset::FadeInLeft);
        }
    });
    view()
        .flex_row()
        .items_center()
        .h(scaled(&z, 24.0))
        .px(scaled(&z, 8.0))
        .bg(rgb(SURFACE))
        .border_t(scaled(&z, 1.0))
        .border_t_color(rgb(BORDER))
        .child(
            view().flex_1().min_w(px(0.0)).child(
                text(move || {
                    let notice = state.notice.get();
                    if notice.is_empty() {
                        t!(
                            "statusline.ready",
                            page = nav::title(state.page.get()),
                            zoom = state.zoom.get()
                        )
                    } else {
                        notice
                    }
                })
                .text_size(scaled(&z, 10.0))
                .font_family(PIXEL_FONT.family)
                .text_color(rgb(TEXT_MUTED))
                .bind_motion(&toast),
            ),
        )
        .child(
            text(move || bridge_text(bridge.get()))
                .text_size(scaled(&z, 10.0))
                .font_family(PIXEL_FONT.family)
                .text_color(rgb(bridge_color(&bridge.get()))),
        )
}
