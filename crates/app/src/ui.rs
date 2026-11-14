//! Zoom-aware pixel primitives: text, panels and buttons.

use std::time::Instant;

use nsh_core::element::FontAsset;
use nsh_core::prelude::*;

use crate::palette::*;

pub const PIXEL_FONT: FontAsset = asset!("fonts/fusion-pixel-12px-proportional-zh_hans.otf");

pub const PAGES: [&str; 6] = [
    "Nexus Dashboard",
    "Local Mods",
    "Multiplayer",
    "Skins & Backups",
    "Hub Home",
    "Settings",
];

#[derive(Clone)]
pub struct Ui {
    pub page: Signal<usize>,
    pub zoom: Signal<u8>,
    pub dev: Signal<bool>,
    pub taps: Signal<u32>,
    pub last_tap: Signal<Option<Instant>>,
    pub notice: Signal<String>,
}

impl Ui {
    pub fn new() -> Self {
        Self {
            page: signal(0usize),
            zoom: signal(2u8),
            dev: signal(false),
            taps: signal(0u32),
            last_tap: signal(None),
            notice: signal(String::new()),
        }
    }
}

pub fn scaled(zoom: &Signal<u8>, base: f32) -> impl Fn() -> Points + Clone + use<> {
    let zoom = zoom.clone();
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

pub fn panel(zoom: &Signal<u8>, title: &'static str, body: impl IntoElement) -> View {
    let z = zoom.clone();
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
                        .px(scaled(&z, 8.0))
                        .bg(rgb(SURFACE))
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
    label: &'static str,
    enabled: bool,
    on_press: impl Fn() + 'static,
) -> Text {
    let z = zoom.clone();
    let mut button = pix(label, 12.0, if enabled { TEXT } else { TEXT_DIM }, &z)
        .px(scaled(&z, 10.0))
        .py(scaled(&z, 4.0))
        .bg(rgb(SURFACE_RAISED))
        .border(scaled(&z, 1.0))
        .border_color(if enabled { rgb(BORDER) } else { rgb(BORDER_DIM) });
    if enabled {
        button = button
            .hover(|s| s.bg(rgb(TEXT)).text_color(rgb(BG)))
            .active(|s| s.bg(rgb(ACCENT_PRESS)).text_color(rgb(TEXT)))
            .on_press(on_press);
    }
    button
}
