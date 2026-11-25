use cuhub_core::nexus_auth::{GithubTokenStore, NexusKeyStore};
use nsh_core::prelude::*;

use super::{actions, section, settings_input};
use crate::app_state::Ui;
use crate::components::{button, danger_button, panel, pix, pix_c, scaled};
use crate::palette::*;

pub(super) fn services(ui: &Ui) -> View {
    view()
        .flex_col()
        .gap(scaled(&ui.zoom, 16.0))
        .child(nexus(ui))
        .child(github(ui))
}

fn nexus(ui: &Ui) -> View {
    let z = ui.zoom;
    let save = ui.clone();
    let test = ui.clone();
    let clear = ui.clone();
    let check = ui.nexus_key_check;
    let saved = ui.nexus_key_saved;
    panel(
        &z,
        move || t!("settings.nexus_key"),
        section(&z)
            .child(saved_status(&z, saved))
            .child(
                settings_input(&z, ui.nexus_key_draft, true)
                    .label(move || t!("settings.nexus_key"))
                    .placeholder(move || t!("settings.nexus_key_placeholder")),
            )
            .child(
                actions(&z)
                    .child(button(
                        &z,
                        move || t!("settings.save_key"),
                        true,
                        move || {
                            save.save_nexus_key();
                        },
                    ))
                    .child(button(
                        &z,
                        move || t!("settings.test_key"),
                        true,
                        move || test.validate_nexus_key(),
                    ))
                    .child(danger_button(
                        &z,
                        move || t!("settings.clear_key"),
                        true,
                        move || clear.clear_nexus_key(),
                    )),
            )
            .child(pix(move || check.get(), 11.0, TEXT, &z))
            .child(pix(
                move || {
                    t!(
                        "settings.nexus_key_note",
                        note = NexusKeyStore::protection_note()
                    )
                },
                11.0,
                TEXT_MUTED,
                &z,
            )),
    )
}

fn github(ui: &Ui) -> View {
    let z = ui.zoom;
    let save = ui.clone();
    let test = ui.clone();
    let clear = ui.clone();
    let check = ui.github_token_check;
    let saved = ui.github_token_saved;
    panel(
        &z,
        move || t!("settings.github_key"),
        section(&z)
            .child(saved_status(&z, saved))
            .child(
                settings_input(&z, ui.github_token_draft, true)
                    .label(move || t!("settings.github_key"))
                    .placeholder(move || t!("settings.github_placeholder")),
            )
            .child(
                actions(&z)
                    .child(button(
                        &z,
                        move || t!("settings.save_token"),
                        true,
                        move || {
                            save.save_github_token();
                        },
                    ))
                    .child(button(
                        &z,
                        move || t!("settings.test_token"),
                        true,
                        move || test.validate_github_token(),
                    ))
                    .child(danger_button(
                        &z,
                        move || t!("settings.clear_token"),
                        true,
                        move || clear.clear_github_token(),
                    )),
            )
            .child(pix(move || check.get(), 11.0, TEXT, &z))
            .child(pix(
                move || {
                    t!(
                        "settings.github_key_note",
                        note = GithubTokenStore::protection_note()
                    )
                },
                11.0,
                TEXT_MUTED,
                &z,
            )),
    )
}

fn saved_status(z: &Signal<u8>, saved: Signal<bool>) -> Text {
    pix_c(
        move || {
            if saved.get() {
                t!("settings.credential_saved")
            } else {
                t!("common.not_set")
            }
        },
        11.0,
        theme_color_with(move || if saved.get() { ACCENT } else { TEXT_MUTED }),
        z,
    )
}
