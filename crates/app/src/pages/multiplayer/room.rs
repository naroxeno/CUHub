//! Active room summary, peers and leave confirmation.

use cuhub_core::easytier::EasyTierMember;
use cuhub_core::multiplayer::{DEFAULT_GAME_PORT, RoomRole, TunnelState};

use nsh_core::prelude::*;

use super::connection::{state_color, state_label};
use super::game::game_section;
use super::lobby::lobby_card;
use crate::app_state::{MultiplayerUi, Ui};
use crate::components::{button, danger_button, pix, pix_c, scaled};
use crate::palette::*;

pub(super) fn room_card(ui: &Ui) -> View {
    let z = ui.zoom;
    let mp = ui.mp;
    let copy_ui = ui.clone();
    let leave_ui = ui.clone();
    let confirm_ui = ui.clone();
    let cancel_ui = ui.clone();
    let list_ui = ui.clone();
    let peers = each(mp.peers, peer_key, move |member, _| {
        peer_row(&list_ui, &member.get())
    });
    lobby_card(z, "multiplayer.room_panel", ACCENT)
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(scaled(&z, 8.0))
                .child(pix(move || t!("multiplayer.role"), 10.0, TEXT_DIM, &z))
                .child(pix(move || role_label(mp.role.get()), 12.0, TEXT, &z)),
        )
        .child(when(
            move || mp.role.get() == RoomRole::Host,
            move || room_code_row(&z, mp, &copy_ui),
        ))
        .child(pix_c(
            move || match (mp.role.get(), mp.state.get()) {
                (RoomRole::Guest, TunnelState::WaitingPeer) => t!("multiplayer.finding_host"),
                (_, TunnelState::Connected { .. }) => t!("multiplayer.tunnel_ready"),
                (_, state) => state_label(state),
            },
            13.0,
            theme_color_with(move || state_color(mp.state.get())),
            &z,
        ))
        .child(when(
            move || mp.role.get() == RoomRole::Host && mp.host_room_port.get().is_some(),
            move || {
                pix(
                    move || {
                        t!(
                            "multiplayer.room_port",
                            port = mp.host_room_port.get().unwrap_or(DEFAULT_GAME_PORT)
                        )
                    },
                    11.0,
                    ACCENT,
                    &z,
                )
            },
        ))
        .child(game_section(ui))
        .child(
            view()
                .flex_col()
                .gap(scaled(&z, 6.0))
                .child(pix(move || t!("multiplayer.peers"), 10.0, TEXT_DIM, &z))
                .child(when(
                    move || mp.peers.get().is_empty(),
                    move || pix(move || t!("multiplayer.peer_empty"), 11.0, TEXT_DIM, &z),
                ))
                .child(peers),
        )
        .child(when(
            move || mp.leave_confirm.get(),
            move || leave_confirm(&confirm_ui, &cancel_ui),
        ))
        .child(when(
            move || !mp.leave_confirm.get(),
            move || {
                let leave = leave_ui.clone();
                danger_button(
                    &z,
                    move || t!("multiplayer.leave"),
                    !mp.is_busy(),
                    move || leave.mp_request_leave(),
                )
            },
        ))
}

fn room_code_row(z: &Signal<u8>, mp: MultiplayerUi, copy_ui: &Ui) -> View {
    let copy = copy_ui.clone();
    view()
        .flex_row()
        .items_center()
        .gap(scaled(z, 10.0))
        .flex_wrap()
        .child(pix(move || t!("multiplayer.code"), 10.0, TEXT_DIM, z))
        .child(pix(move || mp.room.get(), 22.0, REQUEST, z))
        .child(button(
            z,
            move || t!("multiplayer.copy"),
            true,
            move || {
                let code = copy.mp.room.get();
                if !code.is_empty() {
                    set_clipboard(code);
                    copy.notice.set(t!("multiplayer.copied"));
                }
            },
        ))
}

fn leave_confirm(confirm_ui: &Ui, cancel_ui: &Ui) -> View {
    let z = confirm_ui.zoom;
    let confirm = confirm_ui.clone();
    let cancel = cancel_ui.clone();
    view()
        .flex_col()
        .gap(scaled(&z, 8.0))
        .child(pix(move || t!("multiplayer.leave_warning"), 11.0, WARN, &z))
        .child(
            view()
                .flex_row()
                .gap(scaled(&z, 8.0))
                .child(danger_button(
                    &z,
                    move || t!("multiplayer.leave_confirm"),
                    true,
                    move || confirm.mp_leave(),
                ))
                .child(button(
                    &z,
                    move || t!("common.cancel"),
                    true,
                    move || cancel.mp_cancel_leave(),
                )),
        )
}

fn peer_row(ui: &Ui, member: &EasyTierMember) -> View {
    let z = ui.zoom;
    let name = member.hostname.clone();
    let address = member
        .address
        .map(|address| address.to_string())
        .unwrap_or_default();
    let local = member.is_local;
    view()
        .flex_row()
        .items_center()
        .gap(scaled(&z, 8.0))
        .child(pix(name, 11.0, TEXT, &z))
        .child(pix(address, 11.0, TEXT_MUTED, &z))
        .child(when(local, move || {
            pix(move || t!("multiplayer.peer_local"), 10.0, ACCENT, &z)
        }))
}

fn peer_key(member: &EasyTierMember) -> String {
    format!(
        "{}|{}|{}",
        member.hostname,
        member
            .address
            .map(|address| address.to_string())
            .unwrap_or_default(),
        member.is_local
    )
}

fn role_label(role: RoomRole) -> String {
    match role {
        RoomRole::Host => t!("multiplayer.role_host"),
        RoomRole::Guest => t!("multiplayer.role_guest"),
    }
}
