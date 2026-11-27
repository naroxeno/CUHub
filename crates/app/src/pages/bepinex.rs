//! BepInEx install and status overlay with its actions.

use crate::app_state::Ui;
use crate::components::{button, choice_button, confirmation_dialog, pix, pix_c, scaled, spinner};
use crate::palette::*;
use crate::shell::nav;
use cuprum_core::install::{Flavor, Mode};
use nsh_core::prelude::*;

pub fn bepinex_overlay(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let shown = ui.bepinex_confirm;
    let cancel = ui.clone();
    let repair = ui.clone();
    when(
        move || shown.get(),
        move || {
            let cancel_ref = element_ref();
            let cancel = cancel.clone();
            let repair = repair.clone();
            let body = view()
                .flex_col()
                .gap(scaled(&z, 8.0))
                .child(pix(
                    move || t!("bepinex.repair_scope"),
                    11.0,
                    TEXT_MUTED,
                    &z,
                ))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(scaled(&z, 8.0))
                        .child(
                            button(
                                &z,
                                move || t!("common.cancel"),
                                true,
                                move || cancel.bepinex_confirm.set(false),
                            )
                            .element_ref(&cancel_ref),
                        )
                        .child(button(
                            &z,
                            move || t!("bepinex.repair"),
                            true,
                            move || {
                                repair.bepinex_confirm.set(false);
                                repair.prepare_bepinex(Mode::Repair);
                            },
                        )),
                );
            confirmation_dialog(&z, move || t!("panel.bepinex_repair"), body)
                .on_open(move || cancel_ref.focus())
                .on_cancel(move || shown.set(false))
        },
    )
}

fn bepinex_has_folder(ui: &Ui) -> bool {
    !ui.settings.get().game_path.trim().is_empty()
}

fn bepinex_installed(ui: &Ui) -> bool {
    ui.bepinex
        .get()
        .map(|status| status.installed)
        .unwrap_or(false)
}

fn bepinex_il2cpp(ui: &Ui) -> bool {
    ui.bepinex
        .get()
        .map(|status| status.il2cpp)
        .unwrap_or(false)
}

pub(super) fn bepinex_missing(ui: &Ui) -> bool {
    ui.bepinex
        .get()
        .map(|status| !status.complete && !status.il2cpp)
        .unwrap_or(true)
}

fn bepinex_can_act(ui: &Ui) -> bool {
    bepinex_has_folder(ui)
        && !bepinex_il2cpp(ui)
        && !bepinex_flavor_conflict(ui)
        && !ui.bepinex_busy.get()
        && ui.bepinex_pending.get().is_none()
}

pub(super) fn bepinex_flavor_conflict(ui: &Ui) -> bool {
    match ui.bepinex.get().map(|status| status.major) {
        Some(Some(major)) => major != ui.bepinex_flavor.get().major(),
        _ => false,
    }
}

pub(super) fn bepinex_installed_flavor(ui: &Ui) -> u8 {
    ui.bepinex
        .get()
        .and_then(|status| status.major)
        .unwrap_or(5)
}

fn bepinex_action_label(ui: &Ui) -> String {
    if bepinex_installed(ui) {
        t!("bepinex.repair")
    } else {
        t!("bepinex.install")
    }
}

fn bepinex_installed_label(status: &cuprum_core::install::Status) -> String {
    match (&status.version, status.major) {
        (Some(version), _) => t!("bepinex.installed", version = version),
        (None, Some(major)) => t!("bepinex.installed_no_version", major = major),
        (None, None) => t!("bepinex.not_installed"),
    }
}

pub(super) fn bepinex_status_line(z: &Signal<u8>, ui: &Ui) -> Text {
    let label = ui.clone();
    let tone = ui.clone();
    pix_c(
        move || {
            let Some(status) = label.bepinex.get() else {
                return t!("bepinex.no_game_folder");
            };
            if status.il2cpp {
                return t!("bepinex.il2cpp");
            }
            if status.complete {
                return bepinex_installed_label(&status);
            }
            if status.installed {
                return t!("bepinex.incomplete");
            }
            t!("bepinex.not_installed")
        },
        12.0,
        theme_color_with(move || {
            let Some(status) = tone.bepinex.get() else {
                return TEXT_DIM;
            };
            if status.il2cpp {
                DANGER
            } else if status.complete {
                ACCENT
            } else if status.installed {
                WARN
            } else {
                TEXT_MUTED
            }
        }),
        z,
    )
}

pub(super) fn bepinex_flavor_button(ui: &Ui, flavor: Flavor, z: &Signal<u8>) -> Text {
    let selected = ui.bepinex_flavor;
    let pressed = ui.clone();
    choice_button(
        z,
        move || match flavor {
            Flavor::Lts => t!("bepinex.flavor_lts"),
            Flavor::Experimental => t!("bepinex.flavor_experimental"),
        },
        move || selected.get() == flavor,
        move || pressed.set_bepinex_flavor(flavor),
    )
}

pub(super) fn bepinex_actions(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let gate = ui.clone();
    let live_label = ui.clone();
    let live_press = ui.clone();
    let idle_label = ui.clone();
    view().flex_row().items_center().gap(scaled(&z, 8.0)).child(
        when(
            move || bepinex_can_act(&gate),
            move || {
                let label = live_label.clone();
                let press = live_press.clone();
                button(
                    &z,
                    move || bepinex_action_label(&label),
                    true,
                    move || {
                        start_bepinex(&press);
                    },
                )
            },
        )
        .otherwise(move || {
            let label = idle_label.clone();
            button(&z, move || bepinex_action_label(&label), false, || {})
        }),
    )
}

pub(super) fn bepinex_progress(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let busy = ui.bepinex_busy;
    let text = ui.bepinex_status_text;
    let z_spinner = z;
    let z_text = z;
    view()
        .flex_row()
        .items_center()
        .gap(scaled(&z, 6.0))
        .child(when(move || busy.get(), move || spinner(&z_spinner)))
        .child(when(
            move || !text.get().is_empty(),
            move || pix(move || text.get(), 10.0, WARN, &z_text),
        ))
}

pub(super) fn start_bepinex(ui: &Ui) {
    if !bepinex_has_folder(ui) {
        ui.notice.set(t!("notice.bepinex_no_game_folder"));
        ui.page.set(nav::SETTINGS);
        return;
    }
    if bepinex_installed(ui) {
        ui.bepinex_confirm.set(true);
        return;
    }
    ui.prepare_bepinex(Mode::Fresh);
}
