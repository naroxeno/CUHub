//! Multiplayer state: drafts, the tunnel poller and the room workers.

use cuhub_core::easytier::{self, EasyTierMember};
use cuhub_core::hub::{Hub, HubError};
use cuhub_core::multiplayer::{RoomRole, TunnelState};
use nsh_core::prelude::*;
use std::path::PathBuf;
use std::time::Duration;

use super::Ui;

const POLL_MILLIS: u64 = 1500;
const DISCOVERY_TICKS: u32 = 40;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MpBusy {
    None,
    Host,
    Join,
    Leave,
    Core,
}

#[derive(Clone, Copy)]
pub struct MultiplayerUi {
    pub port: Signal<String>,
    pub code: Signal<String>,
    pub core_path: Signal<String>,
    pub core_ready: Signal<bool>,
    pub elevated: Signal<bool>,
    pub state: Signal<TunnelState>,
    pub peers: Signal<Vec<EasyTierMember>>,
    pub room: Signal<String>,
    pub role: Signal<RoomRole>,
    pub busy: Signal<MpBusy>,
    pub error: Signal<&'static str>,
    pub leave_confirm: Signal<bool>,
    pub polling: Signal<bool>,
}

pub fn busy_key(busy: MpBusy) -> &'static str {
    match busy {
        MpBusy::Host => "multiplayer.busy_host",
        MpBusy::Join => "multiplayer.busy_join",
        MpBusy::Leave => "multiplayer.busy_leave",
        MpBusy::Core => "multiplayer.busy_core",
        MpBusy::None => "",
    }
}

fn error_key(error: &HubError) -> &'static str {
    match error {
        HubError::Room(_) => "multiplayer.error_code",
        HubError::Elevation => "multiplayer.error_elevation",
        HubError::EasyTier(_) | HubError::Network(_) | HubError::NetworkUnavailable => {
            "multiplayer.error_download"
        }
        HubError::Io(_) | HubError::Settings(_) => "multiplayer.error_core_path",
        _ => "multiplayer.error_start",
    }
}

fn core_ready(hub: &Hub) -> bool {
    if hub.core_ready() == easytier::Status::Complete {
        return true;
    }
    hub.multiplayer_state()
        .core_path
        .as_deref()
        .is_some_and(|path| easytier::detect_custom(path).status == easytier::Status::Complete)
}

fn port_value(text: &str) -> Option<u16> {
    text.trim().parse::<u16>().ok().filter(|port| *port != 0)
}

impl MultiplayerUi {
    pub fn new(hub: &Hub) -> Self {
        let last = hub.multiplayer_state().last_room.clone();
        let core_path = hub
            .multiplayer_state()
            .core_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        Self {
            port: signal("25565".to_owned()),
            code: signal(last.map(|room| room.code).unwrap_or_default()),
            core_path: signal(core_path),
            core_ready: signal(core_ready(hub)),
            elevated: signal(hub.can_create_tunnel()),
            state: signal(TunnelState::Idle),
            peers: signal(Vec::new()),
            room: signal(hub.room_code().unwrap_or_default()),
            role: signal(RoomRole::Host),
            busy: signal(MpBusy::None),
            error: signal(""),
            leave_confirm: signal(false),
            polling: signal(false),
        }
    }

    pub fn running(&self) -> bool {
        matches!(
            self.state.get(),
            TunnelState::WaitingPeer | TunnelState::Connected { .. }
        )
    }

    pub fn is_busy(&self) -> bool {
        self.busy.get() != MpBusy::None
    }

    fn finish_start(&self, code: Option<String>, error: &'static str, role: RoomRole) -> bool {
        self.busy.set(MpBusy::None);
        let Some(code) = code else {
            self.error.set(error);
            return false;
        };
        self.room.set(code);
        self.role.set(role);
        self.state.set(TunnelState::WaitingPeer);
        self.core_ready.set(true);
        self.polling.set(false);
        true
    }
}

impl Ui {
    pub(crate) fn mp_host(&self) {
        if self.mp.is_busy() {
            return;
        }
        let Some(port) = port_value(&self.mp.port.get()) else {
            self.mp.error.set("multiplayer.error_port");
            return;
        };
        self.mp.error.set("");
        self.mp.busy.set(MpBusy::Host);
        self.mp.leave_confirm.set(false);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.start_host(port)
            })
            .await;
            let (code, error) = match result {
                Ok(Ok(code)) => (Some(code), ""),
                Ok(Err(error)) => (None, error_key(&error)),
                Err(_) => (None, "multiplayer.error_generic"),
            };
            if ui.mp.finish_start(code, error, RoomRole::Host) {
                ui.mp_poll();
            }
        });
    }

    pub(crate) fn mp_join(&self) {
        if self.mp.is_busy() {
            return;
        }
        let input = self.mp.code.get();
        if input.trim().is_empty() {
            self.mp.error.set("multiplayer.error_code");
            return;
        }
        let Some(port) = port_value(&self.mp.port.get()) else {
            self.mp.error.set("multiplayer.error_port");
            return;
        };
        self.mp.error.set("");
        self.mp.busy.set(MpBusy::Join);
        self.mp.leave_confirm.set(false);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.join_room(&input, port)
            })
            .await;
            let (code, error) = match result {
                Ok(Ok(code)) => (Some(code), ""),
                Ok(Err(error)) => (None, error_key(&error)),
                Err(_) => (None, "multiplayer.error_generic"),
            };
            if ui.mp.finish_start(code, error, RoomRole::Guest) {
                ui.mp_poll();
            }
        });
    }

    pub(crate) fn mp_request_leave(&self) {
        if self.mp.is_busy() {
            return;
        }
        self.mp.error.set("");
        self.mp.leave_confirm.set(true);
    }

    pub(crate) fn mp_cancel_leave(&self) {
        self.mp.leave_confirm.set(false);
    }

    pub(crate) fn mp_leave(&self) {
        if self.mp.is_busy() {
            return;
        }
        self.mp.busy.set(MpBusy::Leave);
        self.mp.error.set("");
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let left = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.leave_multiplayer();
            })
            .await;
            for _ in 0..20 {
                if !ui.mp.polling.get() {
                    break;
                }
                let _ = blocking(|| std::thread::sleep(Duration::from_millis(100))).await;
            }
            ui.mp.polling.set(false);
            ui.mp.state.set(TunnelState::Idle);
            ui.mp.room.set(String::new());
            ui.mp.peers.set(Vec::new());
            ui.mp.leave_confirm.set(false);
            ui.mp.busy.set(MpBusy::None);
            if left.is_err() {
                ui.mp.error.set("multiplayer.error_generic");
            }
        });
    }

    pub(crate) fn mp_poll(&self) {
        if self.mp.polling.get() {
            return;
        }
        self.mp.polling.set(true);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let mut ticks = 0u32;
            loop {
                let interval =
                    blocking(|| std::thread::sleep(Duration::from_millis(POLL_MILLIS))).await;
                if interval.is_err() {
                    break;
                }
                let probe = hub.clone();
                let snapshot = blocking(move || {
                    let mut hub = probe
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    (hub.tunnel_state(), hub.tunnel_peers().unwrap_or_default())
                })
                .await;
                let Ok((state, peers)) = snapshot else {
                    break;
                };
                let running = matches!(
                    state,
                    TunnelState::WaitingPeer | TunnelState::Connected { .. }
                );
                let idle = matches!(state, TunnelState::Idle | TunnelState::Failed(_));
                let waiting = matches!(state, TunnelState::WaitingPeer);
                ui.mp.peers.set(peers);
                ui.mp.state.set(state);
                ticks += 1;
                if waiting && ui.mp.role.get() == RoomRole::Guest && ticks >= DISCOVERY_TICKS {
                    ui.mp.error.set("multiplayer.error_not_found");
                    break;
                }
                if idle {
                    ui.mp.room.set(String::new());
                    ui.mp.leave_confirm.set(false);
                }
                if !running {
                    break;
                }
            }
            ui.mp.polling.set(false);
        });
    }

    pub(crate) fn mp_save_core(&self) {
        let text = self.mp.core_path.get().trim().to_owned();
        let path = if text.is_empty() {
            None
        } else {
            Some(PathBuf::from(text))
        };
        self.mp_store_core(path, false);
    }

    pub(crate) fn mp_clear_core(&self) {
        self.mp.core_path.set(String::new());
        self.mp_store_core(None, true);
    }

    fn mp_store_core(&self, path: Option<PathBuf>, cleared: bool) {
        if self.mp.is_busy() {
            return;
        }
        self.mp.error.set("");
        self.mp.busy.set(MpBusy::Core);
        let hub = self.hub.clone();
        let ready_hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let saved = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.set_multiplayer_core_path(path)
            })
            .await;
            let ready = blocking(move || {
                let hub = ready_hub
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                core_ready(&hub)
            })
            .await;
            ui.mp.busy.set(MpBusy::None);
            match saved {
                Ok(Ok(())) => {
                    let ready = ready.unwrap_or(false);
                    ui.mp.core_ready.set(ready);
                    if cleared {
                        ui.notice.set(t!("multiplayer.core_cleared"));
                    } else if ready {
                        ui.notice.set(t!("multiplayer.core_saved"));
                    } else {
                        ui.mp.error.set("multiplayer.core_missing");
                    }
                }
                Ok(Err(error)) => ui.mp.error.set(error_key(&error)),
                Err(_) => ui.mp.error.set("multiplayer.error_generic"),
            }
        });
    }
}
