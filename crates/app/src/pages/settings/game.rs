use cuprum_core::install::Flavor;
use nsh_core::prelude::*;

use super::{game_folder_path, path_row, section};
use crate::app_state::Ui;
use crate::components::{button, panel, pix, scaled};
use crate::pages::bepinex::{
    bepinex_actions, bepinex_flavor_button, bepinex_flavor_conflict, bepinex_installed_flavor,
    bepinex_progress, bepinex_status_line,
};
use crate::palette::*;

pub(super) fn game(ui: &Ui) -> View {
    let z = ui.zoom;
    let folder = ui.clone();
    let detect = ui.clone();
    let rescan = ui.clone();
    let wizard = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 16.0))
        .child(panel(
            &z,
            move || t!("settings.game_folder"),
            section(&z)
                .child(path_row(&z, move || {
                    game_folder_path(&folder).unwrap_or_else(|| t!("common.not_set"))
                }))
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
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
                .child(pix(move || t!("settings.detect_note"), 11.0, TEXT_DIM, &z)),
        ))
        .child(bepinex(ui))
        .child(view().flex_row().child(button(
            &z,
            move || t!("settings.run_wizard"),
            true,
            move || crate::setup::open(&wizard),
        )))
}

fn bepinex(ui: &Ui) -> View {
    let z = ui.zoom;
    let bepinex_conflict = ui.clone();
    let bepinex_conflict_text = ui.clone();
    panel(
        &z,
        move || t!("settings.bepinex"),
        section(&z)
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
            .child(bepinex_progress(&z, ui)),
    )
}
