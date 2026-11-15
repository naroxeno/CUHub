//! Settings page: folders, tokens and zoom.

use super::bepinex::{
    bepinex_actions, bepinex_flavor_button, bepinex_flavor_conflict, bepinex_installed_flavor,
    bepinex_pending_block, bepinex_progress, bepinex_status_line,
};
use crate::i18n;
use crate::palette::*;
use crate::ui::{Ui, button, panel, pix, pix_c, rise_in, scaled};
use cuhub_core::install::Flavor;
use cuhub_core::nexus_auth::{GithubTokenStore, NexusKeyStore};
use nsh_base::input;
use nsh_core::prelude::*;

pub(super) fn game_folder_path(ui: &Ui) -> Option<String> {
    let path = ui.settings.get().game_path;
    if path.trim().is_empty() {
        None
    } else {
        Some(path)
    }
}

pub(super) fn row(
    z: &Signal<u8>,
    label_key: &'static str,
    value: impl IntoReactive<SharedString>,
    color: u32,
) -> impl IntoElement {
    let z = *z;
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .h(scaled(&z, 24.0))
        .px(scaled(&z, 6.0))
        .border_b(scaled(&z, 1.0))
        .border_b_color(rgb(BORDER_DIM))
        .child(pix(move || t!(label_key), 12.0, TEXT_MUTED, &z))
        .child(pix(value, 12.0, color, &z))
}

pub(super) fn setting_row(
    z: &Signal<u8>,
    label_key: &'static str,
    value: impl IntoReactive<SharedString>,
) -> impl IntoElement {
    row(z, label_key, value, TEXT)
}

pub fn settings(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let folder = ui.clone();
    let detect = ui.clone();
    let rescan = ui.clone();
    let path = ui.clone();
    let key_status = ui.clone();
    let save_key = ui.clone();
    let clear_key = ui.clone();
    let check_key = ui.clone();
    let draft = ui.nexus_key_draft;
    let token_status = ui.clone();
    let save_token = ui.clone();
    let clear_token = ui.clone();
    let check_token = ui.clone();
    let token_draft = ui.github_token_draft;
    let bepinex_watch = ui.clone();
    let bepinex_ui = ui.clone();
    let bepinex_conflict = ui.clone();
    let bepinex_conflict_text = ui.clone();
    view()
        .flex_col()
        .flex_1()
        .bind_motion(&rise_in())
        .child(panel(
            &z,
            move || t!("panel.settings"),
            view()
                .flex_col()
                .gap(scaled(&z, 10.0))
                .child(pix(
                    move || t!("settings.game_folder"),
                    12.0,
                    TEXT_MUTED,
                    &z,
                ))
                .child(setting_row(&z, "settings.detected_folder", move || {
                    game_folder_path(&folder).unwrap_or_else(|| t!("common.not_set"))
                }))
                .child(
                    view()
                        .flex_row()
                        .gap(scaled(&z, 8.0))
                        .child(button(
                            &z,
                            move || t!("common.detect_game"),
                            true,
                            move || {
                                detect.detect_game();
                            },
                        ))
                        .child(button(
                            &z,
                            move || t!("common.rescan_mods"),
                            true,
                            move || {
                                rescan.refresh_mods();
                            },
                        )),
                )
                .child(pix(move || t!("settings.detect_note"), 11.0, TEXT_DIM, &z))
                .child(pix(move || t!("settings.bepinex"), 12.0, TEXT_MUTED, &z))
                .child(bepinex_status_line(&z, ui))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap(scaled(&z, 8.0))
                        .child(bepinex_flavor_button(ui, Flavor::Lts, &z))
                        .child(bepinex_flavor_button(ui, Flavor::Experimental, &z)),
                )
                .child(when(
                    move || bepinex_flavor_conflict(&bepinex_conflict),
                    move || {
                        let conflict = bepinex_conflict_text.clone();
                        pix(
                            move || {
                                t!(
                                    "bepinex.flavor_conflict",
                                    installed = bepinex_installed_flavor(&conflict),
                                    selected = conflict.bepinex_flavor.get().label()
                                )
                            },
                            10.0,
                            WARN,
                            &z,
                        )
                    },
                ))
                .child(bepinex_actions(&z, ui))
                .child(bepinex_progress(&z, ui))
                .child(when(
                    move || bepinex_watch.bepinex_pending.get().is_some(),
                    move || bepinex_pending_block(&bepinex_ui.zoom, &bepinex_ui),
                ))
                .child(pix(
                    move || t!("settings.interface_scale"),
                    12.0,
                    TEXT_MUTED,
                    &z,
                ))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap(scaled(&z, 8.0))
                        .child(zoom_button(ui, 1))
                        .child(zoom_button(ui, 2))
                        .child(zoom_button(ui, 3)),
                )
                .child(pix(move || t!("settings.scale_note"), 11.0, TEXT_DIM, &z))
                .child(pix(move || t!("settings.theme"), 12.0, TEXT_MUTED, &z))
                .child(pix(move || t!("settings.theme_pixel"), 12.0, TEXT, &z))
                .child(pix(move || t!("settings.language"), 12.0, TEXT_MUTED, &z))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap(scaled(&z, 8.0))
                        .child(language_button(ui, 0, &z))
                        .child(language_button(ui, 1, &z))
                        .child(language_button(ui, 2, &z))
                        .child(language_button(ui, 3, &z)),
                )
                .child(pix(
                    move || t!("settings.language_note"),
                    11.0,
                    TEXT_DIM,
                    &z,
                ))
                .child(pix(move || t!("settings.storage"), 12.0, TEXT_MUTED, &z))
                .child(setting_row(&z, "settings.data_folder", move || {
                    path.app_data.clone()
                }))
                .child(pix(move || t!("settings.nexus_key"), 12.0, TEXT_MUTED, &z))
                .child(pix(
                    move || {
                        t!(
                            "settings.nexus_key_note",
                            note = NexusKeyStore::protection_note()
                        )
                    },
                    11.0,
                    TEXT_DIM,
                    &z,
                ))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 8.0))
                        .child(
                            input()
                                .value(ui.nexus_key_draft)
                                .masked(true)
                                .placeholder(move || t!("settings.nexus_key_placeholder"))
                                .font_family(crate::ui::PIXEL_FONT.family)
                                .text_size(scaled(&z, 12.0))
                                .h(scaled(&z, 24.0))
                                .flex_1()
                                .px(scaled(&z, 6.0))
                                .bg(rgb(SURFACE_RAISED))
                                .border(scaled(&z, 1.0))
                                .border_color(rgb(BORDER))
                                .text_color(rgb(TEXT))
                                .on_change(move |text: &str| draft.set(text.to_owned())),
                        )
                        .child(button(
                            &z,
                            move || t!("settings.save_key"),
                            true,
                            move || {
                                save_key.save_nexus_key();
                            },
                        ))
                        .child(button(
                            &z,
                            move || t!("settings.test_key"),
                            true,
                            move || {
                                check_key.validate_nexus_key();
                            },
                        ))
                        .child(button(
                            &z,
                            move || t!("settings.clear_key"),
                            true,
                            move || {
                                clear_key.clear_nexus_key();
                            },
                        )),
                )
                .child(pix(
                    move || key_status.nexus_key_check.get(),
                    11.0,
                    TEXT_DIM,
                    &z,
                ))
                .child(pix(move || t!("settings.github_key"), 12.0, TEXT_MUTED, &z))
                .child(pix(
                    move || {
                        t!(
                            "settings.github_key_note",
                            note = GithubTokenStore::protection_note()
                        )
                    },
                    11.0,
                    TEXT_DIM,
                    &z,
                ))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 8.0))
                        .child(
                            input()
                                .value(token_draft)
                                .masked(true)
                                .placeholder(move || t!("settings.github_placeholder"))
                                .font_family(crate::ui::PIXEL_FONT.family)
                                .text_size(scaled(&z, 12.0))
                                .h(scaled(&z, 24.0))
                                .flex_1()
                                .px(scaled(&z, 6.0))
                                .bg(rgb(SURFACE_RAISED))
                                .border(scaled(&z, 1.0))
                                .border_color(rgb(BORDER))
                                .text_color(rgb(TEXT))
                                .on_change(move |text: &str| token_draft.set(text.to_owned())),
                        )
                        .child(button(
                            &z,
                            move || t!("settings.save_token"),
                            true,
                            move || {
                                save_token.save_github_token();
                            },
                        ))
                        .child(button(
                            &z,
                            move || t!("settings.test_token"),
                            true,
                            move || {
                                check_token.validate_github_token();
                            },
                        ))
                        .child(button(
                            &z,
                            move || t!("settings.clear_token"),
                            true,
                            move || {
                                clear_token.clear_github_token();
                            },
                        )),
                )
                .child(pix(
                    move || token_status.github_token_check.get(),
                    11.0,
                    TEXT_DIM,
                    &z,
                )),
        ))
}

fn zoom_button(ui: &Ui, level: u8) -> Text {
    let z = ui.zoom;
    let label = match level {
        1 => "1x",
        2 => "2x",
        _ => "3x",
    };
    let colored = ui.clone();
    let bordered = ui.clone();
    let pressed = ui.clone();
    pix_c(
        label,
        12.0,
        move || {
            if colored.zoom.get() == level {
                rgb(ACCENT)
            } else {
                rgb(TEXT_MUTED)
            }
        },
        &z,
    )
    .px(scaled(&z, 10.0))
    .py(scaled(&z, 4.0))
    .bg(rgb(SURFACE_RAISED))
    .border(scaled(&z, 1.0))
    .border_color(move || {
        if bordered.zoom.get() == level {
            rgb(ACCENT)
        } else {
            rgb(BORDER_DIM)
        }
    })
    .on_press(move || pressed.set_ui_scale(level))
}

fn language_button(ui: &Ui, index: usize, z: &Signal<u8>) -> Text {
    let (code, _) = i18n::LANGUAGES[index];
    let colored = ui.clone();
    let bordered = ui.clone();
    let pressed = ui.clone();
    pix_c(
        move || i18n::language_label(code),
        12.0,
        move || {
            if colored.settings.get().language == code {
                rgb(REQUEST)
            } else {
                rgb(TEXT_MUTED)
            }
        },
        z,
    )
    .px(scaled(z, 10.0))
    .py(scaled(z, 4.0))
    .bg(rgb(SURFACE_RAISED))
    .border(scaled(z, 1.0))
    .border_color(move || {
        if bordered.settings.get().language == code {
            rgb(ACCENT)
        } else {
            rgb(BORDER_DIM)
        }
    })
    .on_press(move || pressed.set_language(code))
}
