//! Pixel-scaled widgets and motion helpers shared by app views.

use nsh_base::{Input, input};
use nsh_core::element::FontAsset;
use nsh_core::prelude::*;

use super::fx::{PRESS_SHIFT, states};
use crate::palette::*;

pub const PIXEL_FONT: FontAsset = asset!("fonts/fusion-pixel-12px-proportional-zh_hans.ttf");

pub fn scaled(zoom: &Signal<u8>, base: f32) -> impl Fn() -> Points + Clone + use<> {
    let zoom = *zoom;
    move || px(base * zoom.get() as f32)
}

pub fn pix(
    s: impl IntoReactive<SharedString>,
    base: f32,
    color: ColorRole,
    zoom: &Signal<u8>,
) -> Text {
    pix_c(s, base, theme_color(color), zoom)
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

pub fn pixel_icon(
    mask: [u8; 8],
    color: impl IntoReactive<AdaptiveColor> + Clone,
    zoom: &Signal<u8>,
) -> View {
    let mut icon = view()
        .w(scaled(zoom, 12.0))
        .h(scaled(zoom, 12.0))
        .flex_shrink_0();
    for (y, row) in mask.into_iter().enumerate() {
        let mut x = 0;
        while x < 8 {
            if row & (0x80 >> x) == 0 {
                x += 1;
                continue;
            }
            let start = x;
            while x < 8 && row & (0x80 >> x) != 0 {
                x += 1;
            }
            icon = icon.child(
                view()
                    .absolute()
                    .left(scaled(zoom, 2.0 + start as f32))
                    .top(scaled(zoom, 2.0 + y as f32))
                    .w(scaled(zoom, (x - start) as f32))
                    .h(scaled(zoom, 1.0))
                    .bg(color.clone()),
            );
        }
    }
    icon
}

pub fn panel(
    zoom: &Signal<u8>,
    title: impl IntoReactive<SharedString>,
    body: impl IntoElement,
) -> View {
    let z = *zoom;
    view()
        .flex_col()
        .bg(theme_color(SURFACE))
        .border_l(scaled(&z, 1.0))
        .border_r(scaled(&z, 1.0))
        .border_b(scaled(&z, 1.0))
        .border_color(theme_color(BORDER_DIM))
        .child(
            view()
                .flex_row()
                .items_center()
                .justify_start()
                .h(scaled(&z, 1.0))
                .child(
                    view()
                        .w(scaled(&z, 12.0))
                        .h(scaled(&z, 1.0))
                        .bg(theme_color(BORDER_DIM)),
                )
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .px(scaled(&z, 8.0))
                        .bg(theme_color(SURFACE))
                        .child(pix(title, 12.0, TEXT, &z)),
                )
                .child(
                    view()
                        .flex_1()
                        .h(scaled(&z, 1.0))
                        .bg(theme_color(BORDER_DIM)),
                ),
        )
        .child(
            view()
                .flex_col()
                .flex_1()
                .pt(scaled(&z, 16.0))
                .px(scaled(&z, 12.0))
                .pb(scaled(&z, 12.0))
                .child(body),
        )
}

pub fn text_input(zoom: &Signal<u8>) -> Input {
    input()
        .font_family(PIXEL_FONT.family)
        .text_size(scaled(zoom, 12.0))
        .h(scaled(zoom, 24.0))
        .px(scaled(zoom, 6.0))
        .bg(theme_color(SURFACE_RAISED))
        .border(scaled(zoom, 1.0))
        .border_color(theme_color(BORDER))
        .text_color(theme_color(TEXT))
}

fn button_frame(z: &Signal<u8>, label: Text) -> Text {
    label
        .px(scaled(z, 10.0))
        .py(scaled(z, 4.0))
        .bg(theme_color(SURFACE_RAISED))
        .border(scaled(z, 1.0))
        .flex_shrink_0()
}

pub fn choice_button(
    z: &Signal<u8>,
    label: impl IntoReactive<SharedString>,
    selected: impl Fn() -> bool + Clone + 'static,
    on_press: impl Fn() + 'static,
) -> Text {
    let color = selected.clone();
    let border = selected.clone();
    let label = pix_c(
        label,
        12.0,
        theme_color_with(move || if color() { ACCENT } else { TEXT_MUTED }),
        z,
    );
    states(button_frame(z, label))
        .border_color(theme_color_with(
            move || if border() { ACCENT } else { BORDER_DIM },
        ))
        .hover(|s| s.bg(theme_color(ACCENT_SURFACE)))
        .focus_visible(|s| s.border_color(theme_color(ACCENT)))
        .active(|s| s.translate_y(px(PRESS_SHIFT)))
        .cursor(Cursor::Pointer)
        .accessibility_state(move || AccessibilityState {
            selected: selected(),
            ..AccessibilityState::default()
        })
        .on_press(on_press)
}

pub fn button(
    zoom: &Signal<u8>,
    label: impl IntoReactive<SharedString>,
    enabled: bool,
    on_press: impl Fn() + 'static,
) -> Text {
    let z = *zoom;
    let mut button = states(
        button_frame(
            &z,
            pix(label, 12.0, if enabled { TEXT } else { TEXT_DIM }, &z),
        )
        .border_color(if enabled {
            theme_color(BORDER)
        } else {
            theme_color(BORDER_DIM)
        }),
    );
    if enabled {
        button = button
            .hover(|s| {
                s.bg(theme_color(ACCENT_SURFACE))
                    .border_color(theme_color(ACCENT))
            })
            .focus_visible(|s| s.border_color(theme_color(ACCENT)))
            .active(|s| {
                s.bg(theme_color(ACCENT_PRESS))
                    .text_color(theme_color(TEXT))
                    .translate_y(px(PRESS_SHIFT))
            })
            .cursor(Cursor::Pointer)
            .on_press(on_press);
    }
    button
}

pub fn danger_button(
    zoom: &Signal<u8>,
    label: impl IntoReactive<SharedString>,
    enabled: bool,
    on_press: impl Fn() + 'static,
) -> Text {
    let z = *zoom;
    let mut button = states(
        button_frame(
            &z,
            pix(label, 12.0, if enabled { DANGER } else { TEXT_DIM }, &z),
        )
        .border_color(if enabled {
            theme_color(DANGER)
        } else {
            theme_color(BORDER_DIM)
        }),
    );
    if enabled {
        button = button
            .hover(|s| s.bg(theme_color(DANGER)).text_color(theme_color(BG)))
            .active(|s| {
                s.bg(theme_color(DANGER))
                    .text_color(theme_color(BG))
                    .translate_y(px(PRESS_SHIFT))
            })
            .cursor(Cursor::Pointer)
            .on_press(on_press);
    }
    button
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
                .bg(theme_color(ACCENT))
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
