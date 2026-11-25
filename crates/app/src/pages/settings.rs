//! Settings navigation and shared form layout.

mod appearance;
mod game;
mod services;
mod storage;

pub(crate) use appearance::{language_button, zoom_button};

use nsh_base::{Input, tooltip};
use nsh_core::EllipsizeMode;
use nsh_core::prelude::*;

use crate::app_state::Ui;
use crate::components::{choice_button, pix, rise_in, scaled, text_input};
use crate::palette::*;

type Category = (&'static str, fn(&Ui) -> View);

const CATEGORIES: [Category; 4] = [
    ("settings.category_game", game::game),
    ("settings.category_appearance", appearance::appearance),
    ("settings.storage", storage::storage),
    ("settings.category_services", services::services),
];

pub fn settings(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let selected = signal(0);
    let mut tabs = actions(&z);
    let mut content = view().flex_col().min_w(px(0.0));
    for (index, (label, render)) in CATEGORIES.into_iter().enumerate() {
        tabs = tabs.child(
            choice_button(
                &z,
                move || t!(label),
                move || selected.get() == index,
                move || selected.set(index),
            )
            .bg(theme_color_with(move || {
                if selected.get() == index {
                    ACCENT_SURFACE
                } else {
                    SURFACE
                }
            })),
        );
        let ui = ui.clone();
        content = content.child(when(move || selected.get() == index, move || render(&ui)));
    }
    view()
        .flex_col()
        .flex_1()
        .gap(scaled(&z, 18.0))
        .bind_motion(&rise_in())
        .child(pix(move || t!("panel.settings"), 18.0, TEXT, &z))
        .child(tabs)
        .child(content)
}

pub(super) fn game_folder_path(ui: &Ui) -> Option<String> {
    let path = ui.settings.get().game_path;
    (!path.trim().is_empty()).then_some(path)
}

fn path_row(z: &Signal<u8>, path: impl Fn() -> String + Clone + 'static) -> Text {
    let full_path = path.clone();
    pix(path, 12.0, TEXT, z)
        .number_of_lines(1)
        .text_overflow(EllipsizeMode::Middle)
        .tooltip(tooltip().child(pix(full_path, 11.0, TEXT, z)))
}

fn section(z: &Signal<u8>) -> View {
    view().flex_col().gap(scaled(z, 10.0))
}

fn actions(z: &Signal<u8>) -> View {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(scaled(z, 8.0))
}

fn settings_input(z: &Signal<u8>, draft: Signal<String>, masked: bool) -> Input {
    text_input(z)
        .value(draft)
        .masked(masked)
        .h(scaled(z, 26.0))
        .min_w(px(0.0))
        .on_change(move |text: &str| draft.set(text.to_owned()))
}
