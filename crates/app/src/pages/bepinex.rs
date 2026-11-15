//! BepInEx install and status overlay with its actions.

use crate::palette::*;
use crate::ui::{Ui, button, fade_in, panel, pix, pix_c, rise_in, scaled, spinner};
use cuhub_core::install::{Flavor, Mode};
use nsh_core::prelude::*;

pub fn bepinex_overlay(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let shown = ui.bepinex_confirm;
    let cancel = ui.clone();
    let repair = ui.clone();
    when(
        move || shown.get(),
        move || {
            let veil = fade_in();
            let card = rise_in();
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
                        .child(button(
                            &z,
                            move || t!("common.cancel"),
                            true,
                            move || cancel.bepinex_confirm.set(false),
                        ))
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
            view()
                .absolute()
                .inset(px(0.0))
                .w_full()
                .h_full()
                .bg(rgba(0x000000CC))
                .bind_motion(&veil)
                .items_center()
                .justify_center()
                .child(
                    panel(&z, move || t!("panel.bepinex_repair"), body)
                        .w(scaled(&z, 460.0))
                        .max_w(percentage(0.9))
                        .bind_motion(&card),
                )
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

fn bepinex_installed_label(status: &cuhub_core::install::Status) -> String {
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
        move || {
            let Some(status) = tone.bepinex.get() else {
                return rgb(TEXT_DIM);
            };
            if status.il2cpp {
                rgb(DANGER)
            } else if status.complete {
                rgb(ACCENT)
            } else if status.installed {
                rgb(WARN)
            } else {
                rgb(TEXT_MUTED)
            }
        },
        z,
    )
}

pub(super) fn bepinex_flavor_button(ui: &Ui, flavor: Flavor, z: &Signal<u8>) -> Text {
    let colored = ui.clone();
    let bordered = ui.clone();
    let pressed = ui.clone();
    pix_c(
        move || match flavor {
            Flavor::Lts => t!("bepinex.flavor_lts"),
            Flavor::Experimental => t!("bepinex.flavor_experimental"),
        },
        12.0,
        move || {
            if colored.bepinex_flavor.get() == flavor {
                rgb(ACCENT)
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
        if bordered.bepinex_flavor.get() == flavor {
            rgb(ACCENT)
        } else {
            rgb(BORDER_DIM)
        }
    })
    .on_press(move || pressed.set_bepinex_flavor(flavor))
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

pub(super) fn bepinex_pending_block(z: &Signal<u8>, ui: &Ui) -> View {
    let z = *z;
    let Some(install) = ui.bepinex_pending.get() else {
        return view();
    };
    let replace = install
        .plan
        .files
        .iter()
        .any(|file| file.action == cuhub_core::install::FileAction::Replace);
    let blocked = !install.plan.conflicts.is_empty();
    let version = install.release.version;
    let mut block = view()
        .flex_col()
        .gap(scaled(&z, 4.0))
        .p(scaled(&z, 8.0))
        .border(scaled(&z, 1.0))
        .border_color(rgb(BORDER_DIM))
        .bg(rgb(SURFACE))
        .child(pix(
            t!("bepinex.plan_summary", version = version),
            11.0,
            TEXT,
            &z,
        ));
    if blocked {
        block = block.child(pix(t!("bepinex.plan_conflicts"), 10.0, DANGER, &z));
    }
    block.child(bepinex_pending_actions(&z, ui, blocked, replace))
}

fn bepinex_pending_actions(z: &Signal<u8>, ui: &Ui, blocked: bool, replace: bool) -> View {
    let z = *z;
    let gate = ui.clone();
    let live_apply = ui.clone();
    let live_cancel = ui.clone();
    view().flex_row().items_center().gap(scaled(&z, 8.0)).child(
        when(
            move || !gate.bepinex_busy.get(),
            move || {
                let apply_ui = live_apply.clone();
                let cancel_ui = live_cancel.clone();
                view()
                    .flex_row()
                    .items_center()
                    .gap(scaled(&z, 8.0))
                    .child(button(
                        &z,
                        move || {
                            if replace {
                                t!("bepinex.replace")
                            } else {
                                t!("bepinex.install")
                            }
                        },
                        !blocked,
                        move || apply_ui.apply_bepinex(),
                    ))
                    .child(button(
                        &z,
                        move || t!("common.cancel"),
                        true,
                        move || cancel_ui.cancel_bepinex(),
                    ))
            },
        )
        .otherwise(move || {
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(button(&z, move || t!("common.apply"), false, || {}))
                .child(button(&z, move || t!("common.cancel"), false, || {}))
        }),
    )
}

pub(super) fn start_bepinex(ui: &Ui) {
    if !bepinex_has_folder(ui) {
        ui.notice.set(t!("notice.bepinex_no_game_folder"));
        ui.page.set(6);
        return;
    }
    if bepinex_installed(ui) {
        ui.bepinex_confirm.set(true);
        return;
    }
    ui.prepare_bepinex(Mode::Fresh);
}
