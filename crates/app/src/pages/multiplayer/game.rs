//! Game bridge controls and room handoff UI.

use cuprum_core::multiplayer::RoomRole;

use nsh_core::prelude::*;

use crate::app_state::{
    GamePhase, HOST_ADDRESS, MpBusy, MultiplayerUi, Ui, bridge_color, bridge_text, detected_port,
    game_address, game_status_key,
};
use crate::components::{button, copy_button, pix, pix_c, scaled};
use crate::palette::*;

pub(super) fn bridge_bar(ui: &Ui) -> View {
    let z = ui.zoom;
    let bridge = ui.bridge;
    view()
        .flex_col()
        .gap(scaled(&z, 5.0))
        .p(scaled(&z, 10.0))
        .bg(theme_color(SURFACE))
        .border_t(scaled(&z, 1.0))
        .border_t_color(theme_color(BORDER_DIM))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(scaled(&z, 12.0))
                .child(bridge_control(ui))
                .child(pix_c(
                    move || bridge_text(bridge.get()),
                    10.0,
                    theme_color_with(move || bridge_color(&bridge.get())),
                    &z,
                )),
        )
        .child(when(
            move || !bridge.get().game_running,
            move || {
                pix(
                    move || t!("multiplayer.bridge_wait_game"),
                    10.0,
                    TEXT_MUTED,
                    &z,
                )
            },
        ))
}

fn bridge_control(ui: &Ui) -> View {
    let z = ui.zoom;
    let bridge = ui.bridge;
    let toggle_ui = ui.clone();
    let mp = ui.mp;
    view()
        .flex_row()
        .items_center()
        .flex_wrap()
        .gap(scaled(&z, 8.0))
        .child(pix(
            move || t!("multiplayer.bridge_control"),
            10.0,
            TEXT_MUTED,
            &z,
        ))
        .child(pix(
            move || {
                if bridge.get().control {
                    t!("multiplayer.bridge_control_on")
                } else {
                    t!("multiplayer.bridge_control_off")
                }
            },
            11.0,
            TEXT,
            &z,
        ))
        .child(
            when(
                move || !mp.is_busy(),
                move || {
                    let toggle = toggle_ui.clone();
                    button(&z, bridge_control_label(bridge), true, move || {
                        toggle.mp_toggle_bridge_control()
                    })
                },
            )
            .otherwise(move || button(&z, bridge_control_label(bridge), false, || {})),
        )
}

fn bridge_control_label(bridge: Signal<cuprum_core::hub::BridgeSnapshot>) -> impl Fn() -> String {
    move || {
        if bridge.get().control {
            t!("multiplayer.bridge_control_disable")
        } else {
            t!("multiplayer.bridge_control_enable")
        }
    }
}

pub(super) fn game_section(ui: &Ui) -> View {
    let z = ui.zoom;
    let mp = ui.mp;
    let bridge = ui.bridge;
    let address_ui = ui.clone();
    let actions_ui = ui.clone();
    let confirm_ui = ui.clone();
    let issues = ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .p(scaled(&z, 10.0))
        .bg(theme_color(SURFACE_RAISED))
        .child(pix(
            move || t!("multiplayer.game_panel"),
            10.0,
            TEXT_DIM,
            &z,
        ))
        .child(pix(
            move || t!(game_status_key(mp.game_phase(&bridge.get()), mp.role.get())),
            11.0,
            TEXT,
            &z,
        ))
        .child(when(
            move || {
                mp.game_phase(&bridge.get()) == GamePhase::Hosting
                    && detected_port(&bridge.get()).is_some()
            },
            move || {
                pix(
                    move || {
                        let port = detected_port(&bridge.get()).unwrap_or_default();
                        t!("multiplayer.game_hosting_detail", port = port)
                    },
                    10.0,
                    ACCENT,
                    &z,
                )
            },
        ))
        .child(when(
            move || !bridge.get().control,
            move || {
                pix(
                    move || t!("multiplayer.bridge_control_hint"),
                    10.0,
                    TEXT_MUTED,
                    &z,
                )
            },
        ))
        .child(when(
            move || {
                game_address(
                    &bridge.get(),
                    mp.role.get(),
                    &mp.state.get(),
                    &mp.port.get(),
                )
                .is_some()
            },
            move || game_address_row(&address_ui),
        ))
        .child(when(
            move || mp.game_confirm.get().is_none(),
            move || game_actions(&actions_ui),
        ))
        .child(when(
            move || mp.game_confirm.get().is_some(),
            move || game_confirm(&confirm_ui),
        ))
        .child(when(
            move || !mp.game_error.get().is_empty(),
            move || super::issue_button(&issues, "multiplayer-game"),
        ))
        .child(when(
            move || mp.busy.get() == MpBusy::Game,
            move || pix(move || t!("multiplayer.busy_game"), 10.0, ACCENT, &z),
        ))
        .child(when(
            move || mp.busy.get() == MpBusy::Port,
            move || pix(move || t!("multiplayer.busy_port"), 10.0, ACCENT, &z),
        ))
}

fn room_game_address(
    mp: MultiplayerUi,
    bridge: &cuprum_core::hub::BridgeSnapshot,
) -> Option<String> {
    let port = if mp.role.get() == RoomRole::Host {
        mp.host_room_port.get().map(|port| port.to_string())
    } else {
        None
    };
    if let Some(port) = port {
        return Some(format!("{HOST_ADDRESS}:{port}"));
    }
    game_address(bridge, mp.role.get(), &mp.state.get(), &mp.port.get())
}

fn game_address_row(ui: &Ui) -> View {
    let z = ui.zoom;
    let mp = ui.mp;
    let bridge = ui.bridge;
    view()
        .flex_row()
        .items_center()
        .gap(scaled(&z, 8.0))
        .child(pix(
            move || t!("multiplayer.game_address"),
            10.0,
            TEXT_DIM,
            &z,
        ))
        .child(pix(
            move || room_game_address(mp, &bridge.get()).unwrap_or_default(),
            13.0,
            ACCENT,
            &z,
        ))
        .child(copy_button(&z, "multiplayer.copy", move || {
            room_game_address(mp, &bridge.get())
        }))
}

fn game_actions(ui: &Ui) -> View {
    let z = ui.zoom;
    let mp = ui.mp;
    let bridge = ui.bridge;
    let action_ui = ui.clone();
    let retry_ui = ui.clone();
    let role = mp.role.get();
    let label_key = match role {
        RoomRole::Host => "multiplayer.game_host",
        RoomRole::Guest => "multiplayer.game_join",
    };
    view()
        .flex_row()
        .flex_wrap()
        .gap(scaled(&z, 8.0))
        .child(when(
            move || {
                mp.game_error.get().is_empty()
                    && mp.game_phase(&bridge.get()) == GamePhase::Ready
                    && !mp.is_busy()
            },
            move || {
                let action = action_ui.clone();
                button(
                    &z,
                    move || t!(label_key),
                    true,
                    move || action.mp_request_game_control(role == RoomRole::Host),
                )
            },
        ))
        .child(when(
            move || {
                !mp.game_error.get().is_empty()
                    && mp.game_phase(&bridge.get()) == GamePhase::Ready
                    && !mp.is_busy()
            },
            move || {
                let retry = retry_ui.clone();
                button(
                    &z,
                    move || t!("multiplayer.game_retry"),
                    true,
                    move || retry.mp_retry_game_control(),
                )
            },
        ))
}

fn game_confirm(ui: &Ui) -> View {
    let z = ui.zoom;
    let confirm_ui = ui.clone();
    let cancel_ui = ui.clone();
    let address = game_address(
        &ui.bridge.get(),
        ui.mp.role.get(),
        &ui.mp.state.get(),
        &ui.mp.port.get(),
    )
    .unwrap_or_default();
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .child(pix(
            move || {
                let address = address.clone();
                t!("multiplayer.game_confirm_warning", address = address)
            },
            10.0,
            WARN,
            &z,
        ))
        .child(
            view()
                .flex_row()
                .gap(scaled(&z, 8.0))
                .child(button(
                    &z,
                    move || t!("multiplayer.game_confirm"),
                    true,
                    move || confirm_ui.mp_confirm_game_control(),
                ))
                .child(button(
                    &z,
                    move || t!("common.cancel"),
                    true,
                    move || cancel_ui.mp_cancel_game_control(),
                )),
        )
}
