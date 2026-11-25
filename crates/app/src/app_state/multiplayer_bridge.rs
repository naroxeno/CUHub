//! Game bridge commands and automatic multiplayer handoff.

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use cuhub_core::bridge::server::BridgeError;
use cuhub_core::hub::HubError;
use cuhub_core::multiplayer::{PortError, RoomRole, TunnelState};
use nsh_core::prelude::*;

use super::multiplayer_rules::{
    GameHandoff, GamePhase, HostDrop, PortFollow, detected_port, game_address, game_phase,
    host_drop, port_follow, session_handoff,
};
use super::{MpBusy, Ui};

const POLL_MILLIS: u64 = 2_000;
const RETRY_SECONDS: [u64; 3] = [0, 3, 8];

fn game_error_text(error: &HubError) -> String {
    match error {
        HubError::Bridge(BridgeError::Denied(_)) => t!("multiplayer.game_error_denied"),
        HubError::Bridge(BridgeError::Unavailable)
        | HubError::Bridge(BridgeError::NotConnected) => {
            t!("multiplayer.game_error_session")
        }
        HubError::Bridge(BridgeError::Failed { code, message }) if code == "invalid_params" => {
            t!("multiplayer.game_error_params", error = message)
        }
        HubError::Bridge(BridgeError::Timeout) => t!("multiplayer.game_error_timeout"),
        HubError::GamePort { expected, reported } => match reported {
            Some(reported) => t!(
                "multiplayer.game_error_game_port",
                expected = expected,
                reported = reported
            ),
            None => t!("multiplayer.game_error_game_port_none", expected = expected),
        },
        HubError::NoRoom => t!("multiplayer.game_error_no_room"),
        HubError::Port(PortError::InUse(port)) => {
            t!("multiplayer.error_port_in_use", port = port)
        }
        _ => t!("multiplayer.game_error_generic", error = error.to_string()),
    }
}

fn stops_auto(error: &HubError) -> bool {
    matches!(error, HubError::Bridge(BridgeError::Denied(_)))
        || matches!(
            error,
            HubError::Bridge(BridgeError::Failed { code, .. }) if code == "invalid_params"
        )
}

struct Gate {
    pid: Option<u32>,
    done: bool,
    attempts: u8,
    next_at: Instant,
    tunnel: Option<(Ipv4Addr, u16)>,
    host_missing: bool,
}

impl Gate {
    fn at(pid: Option<u32>) -> Self {
        Self {
            pid,
            done: false,
            attempts: 0,
            next_at: Instant::now(),
            tunnel: None,
            host_missing: false,
        }
    }
}

impl Ui {
    pub fn mp_game_phase(&self) -> GamePhase {
        game_phase(&self.bridge.get(), self.mp.role.get(), &self.mp.state.get())
    }

    pub fn mp_game_address(&self) -> Option<String> {
        game_address(
            &self.bridge.get(),
            self.mp.role.get(),
            &self.mp.state.get(),
            &self.mp.port.get(),
        )
    }

    pub(crate) fn mp_toggle_bridge_control(&self) {
        if self.mp.is_busy() {
            return;
        }
        let enabled = !self.bridge.get().control;
        self.mp.busy.set(MpBusy::Game);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let saved = blocking(move || {
                let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                hub.set_bridge_control(enabled)
                    .map(|()| hub.settings().clone())
            })
            .await;
            match saved {
                Ok(Ok(settings)) => {
                    ui.settings.set(settings);
                    let mut snapshot = ui.bridge.get();
                    snapshot.control = enabled;
                    ui.bridge.set(snapshot);
                    ui.notice.set(t!(if enabled {
                        "multiplayer.bridge_control_on"
                    } else {
                        "multiplayer.bridge_control_off"
                    }));
                }
                Ok(Err(error)) => ui
                    .notice
                    .set(t!("notice.save_settings_failed", error = error.to_string())),
                Err(_) => ui.notice.set(t!("multiplayer.error_generic")),
            }
            ui.mp.busy.set(MpBusy::None);
        });
    }

    pub(crate) fn mp_request_game_control(&self, host: bool) {
        if self.mp.is_busy() || self.mp_game_phase() != GamePhase::Ready {
            return;
        }
        if self.mp_game_address().is_none() {
            self.mp.game_error.set(t!("multiplayer.game_error_address"));
            return;
        }
        self.mp.game_error.set(String::new());
        self.mp.game_confirm.set(Some(host));
    }

    pub(crate) fn mp_cancel_game_control(&self) {
        self.mp.game_confirm.set(None);
    }

    pub(crate) fn mp_confirm_game_control(&self) {
        let Some(host) = self.mp.game_confirm.get() else {
            return;
        };
        self.mp.game_confirm.set(None);
        self.mp_run_game_control(host);
    }

    pub(crate) fn mp_retry_game_control(&self) {
        if self.mp.is_busy() {
            return;
        }
        self.mp.game_error.set(String::new());
        self.mp.port_error.set(String::new());
        self.mp.gate_epoch.set(self.mp.gate_epoch.get() + 1);
        self.mp_run_game_control(self.mp.role.get() == RoomRole::Host);
    }

    fn mp_run_game_control(&self, host: bool) {
        if self.mp.is_busy() {
            return;
        }
        if self.mp_game_phase() != GamePhase::Ready {
            self.mp.game_error.set(t!("multiplayer.game_error_ready"));
            return;
        }
        let Some(address) = self.mp_game_address() else {
            self.mp.game_error.set(t!("multiplayer.game_error_address"));
            return;
        };
        self.mp.game_error.set(String::new());
        self.mp.busy.set(MpBusy::Game);
        let hub = self.hub.clone();
        let ui = self.clone();
        self.spawn_worker(async move {
            let result = blocking(move || {
                let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                if host {
                    hub.start_game_host().map(|_| ())
                } else {
                    hub.game_join(&address).map(|_| ())
                }
            })
            .await;
            ui.mp.busy.set(MpBusy::None);
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => ui.mp.game_error.set(game_error_text(&error)),
                Err(_) => ui.mp.game_error.set(t!("multiplayer.game_error_worker")),
            }
        });
    }

    pub(crate) fn watch_mp_control(&self) {
        let ui = self.clone();
        self.spawn_worker(async move {
            let mut gate = Gate::at(None);
            let mut epoch = ui.mp.gate_epoch.get();
            loop {
                let slept =
                    blocking(|| std::thread::sleep(Duration::from_millis(POLL_MILLIS))).await;
                if slept.is_err() {
                    break;
                }
                let current_epoch = ui.mp.gate_epoch.get();
                if current_epoch != epoch {
                    epoch = current_epoch;
                    gate = Gate::at(gate.pid);
                }
                let snapshot = ui.bridge.get();
                let pid = snapshot.status.session.as_ref().map(|session| session.pid);
                match session_handoff(gate.pid, pid) {
                    GameHandoff::Same => {}
                    GameHandoff::Entered => {
                        gate = Gate::at(pid);
                        ui.mp.game_error.set(String::new());
                    }
                    GameHandoff::Left => {
                        gate = Gate::at(None);
                        ui.mp.game_error.set(String::new());
                        ui.mp.detected.set(None);
                        ui.mp.port_error.set(String::new());
                    }
                    GameHandoff::Restarted => {
                        gate = Gate::at(pid);
                        ui.mp.game_error.set(String::new());
                        ui.mp.detected.set(None);
                    }
                }
                if gate.pid != pid {
                    gate = Gate::at(pid);
                }
                let detected = detected_port(&snapshot);
                if ui.mp.detected.get() != detected {
                    ui.mp.detected.set(detected);
                }
                if ui.mp.role.get() == RoomRole::Host
                    && ui.mp.running()
                    && !ui.mp.is_busy()
                    && ui.mp.game_confirm.get().is_none()
                {
                    let hub = ui.hub.clone();
                    let update = blocking(move || {
                        let mut hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        hub.reconcile_room_port()
                    })
                    .await;
                    if let Ok(update) = update {
                        match port_follow(update) {
                            PortFollow::Clear => ui.mp.port_error.set(String::new()),
                            PortFollow::Ignore => {}
                            PortFollow::Moved(port) => {
                                ui.mp.host_room_port.set(Some(port));
                                ui.mp.port_error.set(String::new());
                                ui.mp.detected.set(Some(port));
                                ui.notice.set(t!("multiplayer.port_moved", port = port));
                            }
                            PortFollow::Failed { port, message } => ui.mp.port_error.set(t!(
                                "multiplayer.port_failed",
                                port = port,
                                error = message
                            )),
                        }
                    }
                }
                let role = ui.mp.role.get();
                let tunnel = ui.mp.state.get();
                if role == RoomRole::Guest {
                    let current = match tunnel {
                        TunnelState::Connected {
                            virtual_ip, port, ..
                        } => Some((virtual_ip, port)),
                        _ => None,
                    };
                    let drop = host_drop(role, gate.tunnel, &tunnel);
                    let first_connection = current.is_some() && gate.tunnel.is_none();
                    gate.tunnel = current.or(gate.tunnel);
                    if first_connection {
                        gate.attempts = 0;
                        gate.done = false;
                        gate.next_at = Instant::now();
                    }
                    match drop {
                        HostDrop::None if gate.host_missing && current.is_some() => {
                            ui.mp.game_error.set(t!("multiplayer.game_host_returned"));
                            gate.host_missing = false;
                        }
                        HostDrop::None => {}
                        HostDrop::Left if !gate.host_missing => {
                            gate.host_missing = true;
                            ui.mp.game_error.set(t!("multiplayer.game_host_left"));
                            gate.done = true;
                        }
                        HostDrop::Left => {}
                        HostDrop::Moved(_) => {
                            gate.host_missing = false;
                            ui.mp.game_error.set(t!("multiplayer.game_host_moved"));
                            gate.done = true;
                        }
                    }
                }
                if gate.done
                    || !snapshot.control
                    || ui.mp.game_confirm.get().is_some()
                    || ui.mp.is_busy()
                {
                    continue;
                }
                if game_phase(&snapshot, role, &tunnel) != GamePhase::Ready {
                    continue;
                }
                let host = role == RoomRole::Host;
                let address = game_address(&snapshot, role, &tunnel, &ui.mp.port.get());
                if !host && address.is_none() {
                    continue;
                }
                let address = address.unwrap_or_default();
                if !host {
                    if gate.attempts as usize >= RETRY_SECONDS.len() {
                        gate.done = true;
                        continue;
                    }
                    if Instant::now() < gate.next_at {
                        continue;
                    }
                }
                gate.attempts += 1;
                if host {
                    gate.done = true;
                }
                let command_ui = ui.clone();
                let result = blocking(move || {
                    let hub = command_ui.hub.clone();
                    let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    if host {
                        hub.start_game_host().map(|_| ())
                    } else {
                        hub.game_join(&address).map(|_| ())
                    }
                })
                .await;
                match result {
                    Ok(Ok(())) => {
                        ui.mp.game_error.set(String::new());
                        if !host {
                            gate.done = true;
                        }
                    }
                    Ok(Err(error)) => {
                        ui.mp.game_error.set(game_error_text(&error));
                        if host
                            || stops_auto(&error)
                            || gate.attempts as usize >= RETRY_SECONDS.len()
                        {
                            gate.done = true;
                        } else {
                            gate.next_at = Instant::now()
                                + Duration::from_secs(RETRY_SECONDS[gate.attempts as usize]);
                        }
                    }
                    Err(_) => {
                        ui.mp.game_error.set(t!("multiplayer.game_error_worker"));
                        gate.done = true;
                    }
                }
            }
        });
    }
}
