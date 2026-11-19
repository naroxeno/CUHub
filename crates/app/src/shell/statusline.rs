//! Status line: the current notice or page title, plus the game bridge state.

use cuhub_core::bridge::handshake::HandshakeError;
use cuhub_core::bridge::server::BridgeError;
use cuhub_core::hub::BridgeSnapshot;
use nsh_core::prelude::*;

use super::nav;
use crate::palette::*;
use crate::ui::{PIXEL_FONT, Ui, scaled};

pub fn statusline(ui: &Ui) -> impl IntoElement {
    let z = ui.zoom;
    let state = ui.clone();
    let watched = ui.clone();
    let bridge = ui.bridge;
    let toast = motion();
    effect(move || {
        if !watched.notice.get().is_empty() {
            toast.play(MotionPreset::FadeInLeft);
        }
    });
    view()
        .flex_row()
        .items_center()
        .h(scaled(&z, 24.0))
        .px(scaled(&z, 8.0))
        .bg(rgb(SURFACE))
        .border_t(scaled(&z, 1.0))
        .border_t_color(rgb(BORDER))
        .child(
            view().flex_1().min_w(px(0.0)).child(
                text(move || {
                    let notice = state.notice.get();
                    if notice.is_empty() {
                        t!(
                            "statusline.ready",
                            page = nav::title(state.page.get()),
                            zoom = state.zoom.get()
                        )
                    } else {
                        notice
                    }
                })
                .text_size(scaled(&z, 10.0))
                .font_family(PIXEL_FONT.family)
                .text_color(rgb(TEXT_MUTED))
                .bind_motion(&toast),
            ),
        )
        .child(
            text(move || bridge_text(bridge.get()))
                .text_size(scaled(&z, 10.0))
                .font_family(PIXEL_FONT.family)
                .text_color(rgb(bridge_color(&bridge.get()))),
        )
}

fn bridge_text(snapshot: BridgeSnapshot) -> String {
    if let Some(session) = snapshot.status.session {
        return t!("statusline.bridge_connected", pid = session.pid);
    }
    if !snapshot.game_running {
        return t!("statusline.bridge_game_off");
    }
    if let Some(error) = snapshot.status.last_error.as_ref() {
        return match error {
            HandshakeError::BadToken => t!("statusline.bridge_bad_token"),
            HandshakeError::ProtocolMismatch { host, guest } => {
                t!(
                    "statusline.bridge_proto_mismatch",
                    host = host,
                    guest = guest
                )
            }
            HandshakeError::Busy { same_pid: true } => t!("statusline.bridge_busy_same"),
            HandshakeError::Busy { same_pid: false } => t!("statusline.bridge_busy"),
            HandshakeError::Rejected(_) => t!("statusline.bridge_rejected"),
        };
    }
    if let Some(error) = snapshot.start_error.as_ref() {
        return match error {
            BridgeError::PortInUse(port) => t!("statusline.bridge_port_busy", port = port),
            other => t!("statusline.bridge_failed", error = other),
        };
    }
    t!("statusline.bridge_waiting")
}

fn bridge_color(snapshot: &BridgeSnapshot) -> u32 {
    if snapshot.status.session.is_some() {
        return ACCENT;
    }
    if snapshot.status.last_error.is_some() || snapshot.start_error.is_some() {
        return DANGER;
    }
    TEXT_MUTED
}
