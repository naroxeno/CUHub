//! Transient feedback for shell commands.

use std::time::Duration;

use nsh_core::prelude::*;

use crate::app_state::Ui;
use crate::components::{fade_in, pix, scaled};
use crate::palette::*;

pub fn notice_toast(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let notice = ui.notice;
    let visible = signal(false);
    let revision = signal(0u64);
    effect(move || {
        if notice.get().is_empty() {
            visible.set(false);
        } else {
            let current = revision.get_untracked().wrapping_add(1);
            revision.set(current);
            visible.set(true);
            set_timeout(Duration::from_secs(5), move || {
                if revision.get_untracked() == current {
                    visible.set(false);
                }
            });
        }
    });
    when(
        move || visible.get(),
        move || {
            view()
                .absolute()
                .left(scaled(&z, 16.0))
                .bottom(scaled(&z, 14.0))
                .w(relative(0.56))
                .pointer_events(PointerEvents::None)
                .p(scaled(&z, 8.0))
                .bg(theme_color(SURFACE_RAISED))
                .border(scaled(&z, 1.0))
                .border_color(theme_color(BORDER_DIM))
                .bind_motion(&fade_in())
                .child(pix(move || notice.get(), 11.0, TEXT, &z))
        },
    )
}
