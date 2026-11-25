//! Bridge polling worker: the host follows the game process and the UI reads its snapshot.

use std::collections::BTreeSet;
use std::time::Duration;

use cuhub_core::bridge::handshake::HandshakeError;
use cuhub_core::bridge::server::BridgeError;
use cuhub_core::hub::BridgeSnapshot;
use nsh_core::prelude::*;

use super::state::Ui;
use crate::palette::{ACCENT, DANGER, TEXT_MUTED, ColorRole};

const POLL_MILLIS: u64 = 2_000;

impl Ui {
    pub(crate) fn watch_bridge(&self) {
        let ui = self.clone();
        let hub = self.hub.clone();
        self.spawn_worker(async move {
            let mut last = BridgeSnapshot::default();
            let mut loaded = BTreeSet::new();
            loop {
                let probe = hub.clone();
                let snapshot = blocking(move || {
                    let mut hub = probe
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    hub.bridge_snapshot()
                })
                .await;
                if let Ok(snapshot) = snapshot {
                    let current: BTreeSet<String> = snapshot
                        .status
                        .session
                        .as_ref()
                        .map(|session| {
                            session
                                .mods
                                .iter()
                                .map(|entry| entry.guid.clone())
                                .collect()
                        })
                        .unwrap_or_default();
                    let session_changed = current != loaded;
                    if snapshot != last {
                        ui.bridge.set(snapshot.clone());
                        last = snapshot;
                    }
                    if session_changed {
                        loaded = current;
                        ui.rebuild_local_rows();
                    }
                }
                let _ = blocking(|| std::thread::sleep(Duration::from_millis(POLL_MILLIS))).await;
            }
        });
    }
}

pub(crate) fn bridge_text(snapshot: BridgeSnapshot) -> String {
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

pub(crate) fn bridge_color(snapshot: &BridgeSnapshot) -> ColorRole {
    if snapshot.status.session.is_some() {
        return ACCENT;
    }
    if snapshot.game_running
        && (snapshot.status.last_error.is_some() || snapshot.start_error.is_some())
    {
        return DANGER;
    }
    TEXT_MUTED
}
