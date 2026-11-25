//! Multiplayer page layout and visibility rules.

mod connection;
mod game;
mod lobby;
mod room;

use cuhub_core::multiplayer::TunnelState;

use nsh_core::prelude::*;

use self::connection::status_card;
use self::game::bridge_bar;
use self::lobby::{forms, setup_card};
use self::room::room_card;
use crate::app_state::Ui;
use crate::components::{rise_in, scaled};

pub fn multiplayer(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let bridge_ui = ui.clone();
    let status_mp = ui.mp;
    let status_ui = ui.clone();
    let setup_ui = ui.clone();
    let forms_ui = ui.clone();
    let room_ui = ui.clone();
    view()
        .flex_col()
        .flex_1()
        .gap(scaled(&z, 12.0))
        .bind_motion(&rise_in())
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
        .child(when(
            move || {
                status_mp.is_busy()
                    || !status_mp.elevated.get()
                    || !status_mp.error.get().is_empty()
                    || matches!(status_mp.state.get(), TunnelState::Failed(_))
            },
            move || status_card(&status_ui),
        ))
        .child(bridge_bar(&bridge_ui))
}
