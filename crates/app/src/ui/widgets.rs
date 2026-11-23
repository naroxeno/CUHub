//! Pixel-scaled widgets and motion helpers shared by app views.

use nsh_core::element::FontAsset;
use nsh_core::prelude::*;

use super::fx::{PRESS_SHIFT, states};
use crate::palette::*;

pub const PIXEL_FONT: FontAsset = asset!("fonts/fusion-pixel-12px-proportional-zh_hans.ttf");

pub const NEXUS_PAGE_SIZE: usize = 50;

pub fn scaled(zoom: &Signal<u8>, base: f32) -> impl Fn() -> Points + Clone + use<> {
    let zoom = *zoom;
    move || px(base * zoom.get() as f32)
}

pub fn pix(s: impl IntoReactive<SharedString>, base: f32, color: u32, zoom: &Signal<u8>) -> Text {
    pix_c(s, base, rgb(color), zoom)
}

pub fn pix_c(
    s: impl IntoReactive<SharedString>,
    base: f32,
    color: impl IntoReactive<AdaptiveColor>,
    zoom: &Signal<u8>,
) -> Text {
    text(s)
        .text_size(scaled(zoom, base))
        .line_height(scaled(zoom, base * 4.0 / 3.0))
        .font_family(PIXEL_FONT.family)
        .text_color(color)
}

pub fn panel(
    zoom: &Signal<u8>,
    title: impl IntoReactive<SharedString>,
    body: impl IntoElement,
) -> View {
    let z = *zoom;
    view()
        .flex_col()
        .bg(rgb(SURFACE))
        .border_l(scaled(&z, 1.0))
        .border_r(scaled(&z, 1.0))
        .border_b(scaled(&z, 1.0))
        .border_color(rgb(BORDER))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_center()
                .h(scaled(&z, 1.0))
                .child(view().flex_1().h(scaled(&z, 1.0)).bg(rgb(BORDER)))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 5.0))
                        .px(scaled(&z, 8.0))
                        .bg(rgb(SURFACE))
                        .child(view().w(scaled(&z, 2.0)).h(scaled(&z, 9.0)).bg(rgb(ACCENT)))
                        .child(pix(title, 12.0, TEXT, &z)),
                )
                .child(view().flex_1().h(scaled(&z, 1.0)).bg(rgb(BORDER))),
        )
        .child(
            view()
                .flex_col()
                .flex_1()
                .pt(scaled(&z, 9.0))
                .px(scaled(&z, 2.0))
                .pb(scaled(&z, 2.0))
                .child(
                    view()
                        .flex_col()
                        .flex_1()
                        .p(scaled(&z, 8.0))
                        .border(scaled(&z, 1.0))
                        .border_color(rgb(BORDER))
                        .child(body),
                ),
        )
}

pub fn button(
    zoom: &Signal<u8>,
    label: impl IntoReactive<SharedString>,
    enabled: bool,
    on_press: impl Fn() + 'static,
) -> Text {
    let z = *zoom;
    let mut button = states(
        pix(label, 12.0, if enabled { TEXT } else { TEXT_DIM }, &z)
            .px(scaled(&z, 10.0))
            .py(scaled(&z, 4.0))
            .bg(rgb(SURFACE_RAISED))
            .border(scaled(&z, 1.0))
            .border_color(if enabled {
                rgb(BORDER)
            } else {
                rgb(BORDER_DIM)
            }),
    );
    if enabled {
        button = button
            .hover(|s| s.bg(rgb(TEXT)).text_color(rgb(BG)))
            .active(|s| {
                s.bg(rgb(ACCENT_PRESS))
                    .text_color(rgb(TEXT))
                    .translate_y(px(PRESS_SHIFT))
            })
            .cursor(Cursor::Pointer)
            .on_press(on_press);
    }
    button.flex_shrink_0()
}

pub fn danger_button(
    zoom: &Signal<u8>,
    label: impl IntoReactive<SharedString>,
    enabled: bool,
    on_press: impl Fn() + 'static,
) -> Text {
    let z = *zoom;
    let mut button = states(
        pix(label, 12.0, if enabled { DANGER } else { TEXT_DIM }, &z)
            .px(scaled(&z, 10.0))
            .py(scaled(&z, 4.0))
            .bg(rgb(SURFACE_RAISED))
            .border(scaled(&z, 1.0))
            .border_color(if enabled {
                rgb(DANGER)
            } else {
                rgb(BORDER_DIM)
            }),
    );
    if enabled {
        button = button
            .hover(|s| s.bg(rgb(DANGER)).text_color(rgb(BG)))
            .active(|s| {
                s.bg(rgb(DANGER))
                    .text_color(rgb(BG))
                    .translate_y(px(PRESS_SHIFT))
            })
            .cursor(Cursor::Pointer)
            .on_press(on_press);
    }
    button.flex_shrink_0()
}

pub fn rise_in() -> Motion {
    motion().entering(MotionPreset::FadeInUp)
}

pub fn fade_in() -> Motion {
    motion().entering(MotionPreset::FadeIn)
}

pub fn spinner(zoom: &Signal<u8>) -> View {
    let z = *zoom;
    let mut dots = view().flex_row().items_center().gap(scaled(&z, 2.0));
    for step in 0..4 {
        dots = dots.child(
            view()
                .w(scaled(&z, 3.0))
                .h(scaled(&z, 3.0))
                .bg(rgb(ACCENT))
                .animation(
                    KeyframeAnimation::new(Keyframes::Pulse)
                        .duration(640.0)
                        .delay(step as f32 * 120.0)
                        .frame_rate(20.0),
                ),
        );
    }
    dots
}
