use nsh_core::prelude::*;

use super::section;
use crate::app_state::Ui;
use crate::components::{choice_button, panel, pix, scaled};
use crate::i18n;
use crate::palette::*;

pub(super) fn appearance(ui: &Ui) -> View {
    let z = ui.zoom;
    let mut languages = view().flex_row().flex_wrap().gap(scaled(&z, 6.0));
    for index in 0..i18n::LANGUAGES.len() {
        languages = languages.child(language_button(ui, index, &z));
    }
    view()
        .flex_col()
        .gap(scaled(&z, 16.0))
        .child(panel(&z, move || t!("settings.theme"), theme_choices(ui)))
        .child(panel(
            &z,
            move || t!("settings.interface_scale"),
            section(&z)
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap(scaled(&z, 6.0))
                        .child(zoom_button(ui, 1))
                        .child(zoom_button(ui, 2))
                        .child(zoom_button(ui, 3)),
                )
                .child(pix(move || t!("settings.scale_note"), 11.0, TEXT_MUTED, &z)),
        ))
        .child(panel(
            &z,
            move || t!("settings.language"),
            section(&z).child(languages).child(pix(
                move || t!("settings.language_note"),
                11.0,
                TEXT_MUTED,
                &z,
            )),
        ))
}

fn theme_choices(ui: &Ui) -> View {
    let z = ui.zoom;
    let settings = ui.settings;
    let custom = ui.custom_theme;
    let mut choices = view().flex_row().flex_wrap().gap(scaled(&z, 6.0));
    for theme in Theme::available(custom) {
        let pressed = ui.clone();
        choices = choices.child(choice_button(
            &z,
            move || theme.label(),
            move || Theme::selected(&settings.get().theme, custom) == theme,
            move || pressed.set_theme(theme),
        ));
    }
    section(&z).child(choices)
}

pub(crate) fn zoom_button(ui: &Ui, level: u8) -> Text {
    let z = ui.zoom;
    let label = match level {
        1 => "1x",
        2 => "2x",
        _ => "3x",
    };
    let pressed = ui.clone();
    choice_button(
        &z,
        label,
        move || z.get() == level,
        move || pressed.set_ui_scale(level),
    )
}

pub(crate) fn language_button(ui: &Ui, index: usize, z: &Signal<u8>) -> Text {
    let (code, _) = i18n::LANGUAGES[index];
    let settings = ui.settings;
    let pressed = ui.clone();
    choice_button(
        z,
        move || i18n::language_label(code),
        move || settings.get().language == code,
        move || pressed.set_language(code),
    )
}
