//! Tunnel status and connection feedback.

use cuhub_core::multiplayer::TunnelState;

use nsh_core::prelude::*;

use crate::app_state::{Ui, busy_key};
use crate::components::{panel, pix, pix_c, scaled, spinner};
use crate::palette::*;

pub(super) fn status_card(ui: &Ui) -> View {
    let z = ui.zoom;
    let mp = ui.mp;
    panel(
        &z,
        move || t!("multiplayer.status"),
        view()
            .flex_col()
            .gap(scaled(&z, 8.0))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(scaled(&z, 8.0))
                    .child(pix_c(
                        move || state_label(mp.state.get()),
                        16.0,
                        theme_color_with(move || state_color(mp.state.get())),
                        &z,
                    ))
                    .child(when(move || mp.is_busy(), move || spinner(&z))),
            )
            .child(pix(
                move || state_hint(mp.state.get()),
                11.0,
                TEXT_MUTED,
                &z,
            ))
            .child(when(
                move || matches!(mp.state.get(), TunnelState::Connected { .. }),
                move || {
                    pix(
                        move || match mp.state.get() {
                            TunnelState::Connected {
                                virtual_ip, port, ..
                            } => {
                                t!("multiplayer.connected_detail", ip = virtual_ip, port = port)
                            }
                            _ => String::new(),
                        },
                        11.0,
                        ACCENT,
                        &z,
                    )
                },
            ))
            .child(when(
                move || !mp.elevated.get(),
                move || {
                    view()
                        .flex_col()
                        .gap(scaled(&z, 4.0))
                        .child(pix(
                            move || t!("multiplayer.state_elevation"),
                            11.0,
                            DANGER,
                            &z,
                        ))
                        .child(pix(
                            move || t!("multiplayer.state_elevation_hint"),
                            10.0,
                            TEXT_MUTED,
                            &z,
                        ))
                },
            ))
            .child(when(
                move || mp.is_busy(),
                move || pix(move || t!(busy_key(mp.busy.get())), 11.0, ACCENT, &z),
            ))
            .child(when(
                move || !mp.error.get().is_empty(),
                move || pix(move || t!(mp.error.get()), 11.0, DANGER, &z),
            )),
    )
}

pub(super) fn state_label(state: TunnelState) -> String {
    match state {
        TunnelState::Idle => t!("multiplayer.state_idle"),
        TunnelState::WaitingPeer => t!("multiplayer.state_waiting"),
        TunnelState::Connected { .. } => t!("multiplayer.state_connected"),
        TunnelState::Failed(_) => t!("multiplayer.state_failed"),
    }
}

fn state_hint(state: TunnelState) -> String {
    match state {
        TunnelState::Idle => t!("multiplayer.state_idle_hint"),
        TunnelState::WaitingPeer => t!("multiplayer.state_waiting_hint"),
        TunnelState::Connected { .. } => t!("multiplayer.state_connected_hint"),
        TunnelState::Failed(reason) => reason,
    }
}

pub(super) fn state_color(state: TunnelState) -> ColorRole {
    match state {
        TunnelState::Idle => TEXT_DIM,
        TunnelState::WaitingPeer => WARN,
        TunnelState::Connected { .. } => ACCENT,
        TunnelState::Failed(_) => DANGER,
    }
}
