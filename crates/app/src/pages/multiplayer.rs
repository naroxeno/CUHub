//! The multiplayer page: connection status, hosting, joining and players.

use cuhub_core::easytier::EasyTierMember;
use cuhub_core::multiplayer::{RoomRole, TunnelState};
use nsh_base::input;
use nsh_core::prelude::*;

use crate::palette::*;
use crate::ui::{
    MultiplayerUi, PIXEL_FONT, Ui, busy_key, button, danger_button, panel, pix, pix_c, rise_in,
    scaled, spinner,
};

pub fn multiplayer(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let status_ui = ui.clone();
    let setup_ui = ui.clone();
    let forms_ui = ui.clone();
    let room_ui = ui.clone();
    view()
        .flex_col()
        .flex_1()
        .gap(scaled(&z, 12.0))
        .bind_motion(&rise_in())
        .child(status_card(&status_ui))
        .child(when(
            move || !setup_ui.mp.core_ready.get() && !setup_ui.mp.running(),
            move || setup_card(&setup_ui),
        ))
        .child(when(
            move || !forms_ui.mp.running(),
            move || forms(&forms_ui),
        ))
        .child(when(
            move || room_ui.mp.running(),
            move || room_card(&room_ui),
        ))
}

fn forms(ui: &Ui) -> View {
    let z = ui.zoom;
    let host_ui = ui.clone();
    let join_ui = ui.clone();
    view()
        .flex_row()
        .gap(scaled(&z, 12.0))
        .child(host_card(&host_ui).flex_1())
        .child(join_card(&join_ui).flex_1())
}

fn status_card(ui: &Ui) -> View {
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
                        move || rgb(state_color(mp.state.get())),
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
                            TunnelState::Connected { virtual_ip, port } => {
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

fn setup_card(ui: &Ui) -> View {
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

fn host_card(ui: &Ui) -> View {
    let z = ui.zoom;
    let mp = ui.mp;
    let host_ui = ui.clone();
    panel(
        &z,
        move || t!("multiplayer.host_panel"),
        view()
            .flex_col()
            .gap(scaled(&z, 8.0))
            .child(pix(
                move || t!("multiplayer.host_hint"),
                11.0,
                TEXT_MUTED,
                &z,
            ))
            .child(field(
                &z,
                move || t!("multiplayer.port"),
                port_input(&z, mp.port),
            ))
            .child(pix(move || t!("multiplayer.port_hint"), 10.0, TEXT_DIM, &z))
            .child(button(
                &z,
                move || t!("multiplayer.host"),
                !mp.is_busy(),
                move || host_ui.mp_host(),
            )),
    )
}

fn join_card(ui: &Ui) -> View {
    let z = ui.zoom;
    let mp = ui.mp;
    let join_ui = ui.clone();
    panel(
        &z,
        move || t!("multiplayer.join_panel"),
        view()
            .flex_col()
            .gap(scaled(&z, 8.0))
            .child(pix(
                move || t!("multiplayer.join_hint"),
                11.0,
                TEXT_MUTED,
                &z,
            ))
            .child(field(
                &z,
                move || t!("multiplayer.code"),
                code_input(&z, mp.code),
            ))
            .child(pix(move || t!("multiplayer.code_hint"), 10.0, TEXT_DIM, &z))
            .child(field(
                &z,
                move || t!("multiplayer.port"),
                port_input(&z, mp.port),
            ))
            .child(button(
                &z,
                move || t!("multiplayer.join"),
                !mp.is_busy(),
                move || join_ui.mp_join(),
            )),
    )
}

fn room_card(ui: &Ui) -> View {
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
    panel(
        &z,
        move || t!("multiplayer.room_panel"),
        view()
            .flex_col()
            .gap(scaled(&z, 10.0))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(scaled(&z, 8.0))
                    .child(pix(move || t!("multiplayer.role"), 10.0, TEXT_DIM, &z))
                    .child(pix(move || role_label(mp.role.get()), 12.0, TEXT, &z)),
            )
            .child(room_code_row(&z, mp, &copy_ui))
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
            )),
    )
}

fn room_code_row(z: &Signal<u8>, mp: MultiplayerUi, copy_ui: &Ui) -> View {
    let copy = copy_ui.clone();
    view()
        .flex_row()
        .items_center()
        .gap(scaled(z, 10.0))
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
    input()
        .value(draft)
        .placeholder("25565")
        .font_family(PIXEL_FONT.family)
        .text_size(scaled(z, 12.0))
        .h(scaled(z, 24.0))
        .w(scaled(z, 90.0))
        .px(scaled(z, 6.0))
        .bg(rgb(SURFACE_RAISED))
        .border(scaled(z, 1.0))
        .border_color(rgb(BORDER))
        .text_color(rgb(TEXT))
        .on_change(move |text: &str| draft.set(text.to_owned()))
}

fn code_input(z: &Signal<u8>, draft: Signal<String>) -> impl IntoElement {
    input()
        .value(draft)
        .placeholder(t!("multiplayer.code_placeholder"))
        .font_family(PIXEL_FONT.family)
        .text_size(scaled(z, 12.0))
        .h(scaled(z, 24.0))
        .flex_1()
        .px(scaled(z, 6.0))
        .bg(rgb(SURFACE_RAISED))
        .border(scaled(z, 1.0))
        .border_color(rgb(BORDER))
        .text_color(rgb(TEXT))
        .on_change(move |text: &str| draft.set(text.to_owned()))
}

fn core_path_input(z: &Signal<u8>, draft: Signal<String>) -> impl IntoElement {
    input()
        .value(draft)
        .placeholder(t!("multiplayer.core_path_placeholder"))
        .font_family(PIXEL_FONT.family)
        .text_size(scaled(z, 12.0))
        .h(scaled(z, 24.0))
        .flex_1()
        .px(scaled(z, 6.0))
        .bg(rgb(SURFACE_RAISED))
        .border(scaled(z, 1.0))
        .border_color(rgb(BORDER))
        .text_color(rgb(TEXT))
        .on_change(move |text: &str| draft.set(text.to_owned()))
}

fn state_label(state: TunnelState) -> String {
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
        TunnelState::Failed(_) => t!("multiplayer.state_failed_hint"),
    }
}

fn state_color(state: TunnelState) -> u32 {
    match state {
        TunnelState::Idle => TEXT_DIM,
        TunnelState::WaitingPeer => WARN,
        TunnelState::Connected { .. } => ACCENT,
        TunnelState::Failed(_) => DANGER,
    }
}

fn role_label(role: RoomRole) -> String {
    match role {
        RoomRole::Host => t!("multiplayer.role_host"),
        RoomRole::Guest => t!("multiplayer.role_guest"),
    }
}
