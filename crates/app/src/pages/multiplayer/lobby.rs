//! EasyTier setup, room forms and port fields.

use cuprum_core::multiplayer::DEFAULT_GAME_PORT;
use nsh_core::prelude::*;

use crate::app_state::{Ui, host_port, join_port, port_mismatch, port_source_key};
use crate::components::{button, panel, pix, scaled, text_input};
use crate::palette::*;

pub(super) fn forms(ui: &Ui) -> View {
    let z = ui.zoom;
    view()
        .flex_row()
        .flex_wrap()
        .gap(scaled(&z, 12.0))
        .child(host_card(ui).min_w(scaled(&z, 220.0)))
        .child(join_card(ui).min_w(scaled(&z, 220.0)))
}

pub(super) fn setup_card(ui: &Ui) -> View {
    let z = ui.zoom;
    let mp = ui.mp;
    let save_ui = ui.clone();
    let clear_ui = ui.clone();
    panel(
        &z,
        move || t!("multiplayer.setup_panel"),
        view()
            .flex_col()
            .gap(scaled(&z, 8.0))
            .child(pix(move || t!("multiplayer.setup_note"), 11.0, TEXT, &z))
            .child(pix(
                move || t!("multiplayer.setup_av"),
                10.0,
                TEXT_MUTED,
                &z,
            ))
            .child(field(
                &z,
                move || t!("multiplayer.core_path"),
                core_path_input(&z, mp.core_path),
            ))
            .child(pix(
                move || t!("multiplayer.core_path_hint"),
                10.0,
                TEXT_DIM,
                &z,
            ))
            .child(
                view()
                    .flex_row()
                    .gap(scaled(&z, 8.0))
                    .child(button(
                        &z,
                        move || t!("multiplayer.core_save"),
                        !mp.is_busy(),
                        move || save_ui.mp_save_core(),
                    ))
                    .child(button(
                        &z,
                        move || t!("multiplayer.core_clear"),
                        !mp.is_busy(),
                        move || clear_ui.mp_clear_core(),
                    )),
            )
            .child(when(
                move || mp.is_busy(),
                move || pix(move || t!("multiplayer.busy_core"), 10.0, ACCENT, &z),
            )),
    )
}

pub(super) fn lobby_card(z: Signal<u8>, title: &'static str, accent: ColorRole) -> View {
    view()
        .flex_col()
        .flex_1()
        .min_w(px(0.0))
        .gap(scaled(&z, 9.0))
        .p(scaled(&z, 14.0))
        .bg(theme_color(SURFACE))
        .border(scaled(&z, 1.0))
        .border_color(theme_color(BORDER_DIM))
        .border_t(scaled(&z, 2.0))
        .border_t_color(theme_color(accent))
        .child(pix(move || t!(title), 16.0, TEXT, &z))
}

fn host_card(ui: &Ui) -> View {
    let z = ui.zoom;
    let mp = ui.mp;
    let bridge = ui.bridge;
    let action = ui.clone();
    lobby_card(z, "multiplayer.host_panel", ACCENT)
        .child(pix(
            move || t!("multiplayer.host_hint"),
            11.0,
            TEXT_MUTED,
            &z,
        ))
        .child(pix(
            move || {
                let choice = host_port(&bridge.get(), &mp.port.get());
                t!(
                    "multiplayer.port_source",
                    port = choice.port,
                    source = t!(port_source_key(choice.source))
                )
            },
            11.0,
            ACCENT,
            &z,
        ))
        .child(room_action(ui, "multiplayer.host", move || {
            action.mp_host()
        }))
        .child(port_options(ui, true))
}

fn join_card(ui: &Ui) -> View {
    let z = ui.zoom;
    let mp = ui.mp;
    let action = ui.clone();
    lobby_card(z, "multiplayer.join_panel", REQUEST)
        .child(field(
            &z,
            move || t!("multiplayer.code"),
            code_input(&z, mp.code),
        ))
        .child(when(
            move || mp.error.get() == "multiplayer.error_code",
            move || pix(move || t!("multiplayer.error_code"), 11.0, DANGER, &z),
        ))
        .child(room_action(ui, "multiplayer.join", move || {
            action.mp_join()
        }))
        .child(port_options(ui, false))
}

fn room_action(ui: &Ui, key: &'static str, action: impl Fn() + Clone + 'static) -> View {
    let z = ui.zoom;
    let mp = ui.mp;
    view().flex_row().child(
        when(
            move || !mp.is_busy() && mp.elevated.get(),
            move || {
                button(&z, move || t!(key), true, action.clone()).border_color(theme_color(ACCENT))
            },
        )
        .otherwise(move || button(&z, move || t!(key), false, || {})),
    )
}

fn port_options(ui: &Ui, host: bool) -> View {
    let z = ui.zoom;
    let open = signal(false);
    let ui = ui.clone();
    let port = ui.mp.guest_port;
    view()
        .flex_col()
        .gap(scaled(&z, 6.0))
        .child(view().flex_row().child(button(
            &z,
            move || {
                if open.get() {
                    t!("multiplayer.port_options_hide")
                } else if !host && let Some(port) = join_port(&port.get()) {
                    t!("multiplayer.guest_port_options", port = port)
                } else {
                    t!("multiplayer.port_options")
                }
            },
            true,
            move || open.set(!open.get()),
        )))
        .child(when(
            move || open.get(),
            move || {
                if host {
                    port_field(&ui)
                } else {
                    view()
                        .flex_col()
                        .gap(scaled(&z, 6.0))
                        .child(field(
                            &z,
                            move || t!("multiplayer.guest_port"),
                            port_input(&z, port),
                        ))
                        .child(pix(
                            move || t!("multiplayer.guest_port_hint"),
                            10.0,
                            TEXT_MUTED,
                            &z,
                        ))
                }
            },
        ))
}

fn field(
    z: &Signal<u8>,
    label: impl IntoReactive<SharedString>,
    control: impl IntoElement,
) -> View {
    view()
        .flex_col()
        .gap(scaled(z, 4.0))
        .child(pix(label, 10.0, TEXT_DIM, z))
        .child(control)
}

fn port_input(z: &Signal<u8>, draft: Signal<String>) -> impl IntoElement {
    text_input(z)
        .value(draft)
        .placeholder(DEFAULT_GAME_PORT.to_string())
        .w(scaled(z, 90.0))
        .on_change(move |text: &str| draft.set(text.to_owned()))
}

fn port_field(ui: &Ui) -> View {
    let z = ui.zoom;
    let mp = ui.mp;
    let bridge = ui.bridge;
    let redetect_ui = ui.clone();
    let adopt_ui = ui.clone();
    let issues = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 6.0))
        .child(field(
            &z,
            move || t!("multiplayer.port"),
            port_input(&z, mp.port),
        ))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(button(
                    &z,
                    move || t!("multiplayer.port_redetect"),
                    !mp.is_busy(),
                    move || redetect_ui.mp_redetect_port(),
                )),
        )
        .child(when(
            move || port_mismatch(&bridge.get(), &mp.port.get()).is_some(),
            move || {
                let adopt = adopt_ui.clone();
                view()
                    .flex_col()
                    .gap(scaled(&z, 6.0))
                    .child(pix(
                        move || {
                            let (manual, detected) =
                                port_mismatch(&bridge.get(), &mp.port.get()).unwrap_or_default();
                            t!(
                                "multiplayer.port_mismatch",
                                manual = manual,
                                detected = detected
                            )
                        },
                        10.0,
                        WARN,
                        &z,
                    ))
                    .child(button(
                        &z,
                        move || t!("multiplayer.port_adopt"),
                        !adopt.mp.is_busy(),
                        move || adopt.mp_adopt_detected_port(),
                    ))
            },
        ))
        .child(when(
            move || !mp.port_error.get().is_empty(),
            move || super::issue_button(&issues, "multiplayer-game"),
        ))
        .child(pix(
            move || t!("multiplayer.port_hint", port = DEFAULT_GAME_PORT),
            10.0,
            TEXT_DIM,
            &z,
        ))
}

fn code_input(z: &Signal<u8>, draft: Signal<String>) -> impl IntoElement {
    text_input(z)
        .value(draft)
        .placeholder(t!("multiplayer.code_placeholder"))
        .flex_1()
        .on_change(move |text: &str| draft.set(text.to_owned()))
}

fn core_path_input(z: &Signal<u8>, draft: Signal<String>) -> impl IntoElement {
    text_input(z)
        .value(draft)
        .placeholder(t!("multiplayer.core_path_placeholder"))
        .flex_1()
        .on_change(move |text: &str| draft.set(text.to_owned()))
}
