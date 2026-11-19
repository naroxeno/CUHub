//! Game-side multiplayer control: the bridge permission switch, manual room commands and the auto gate.

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use cuhub_core::bridge::server::BridgeError;
use cuhub_core::hub::{BridgeSnapshot, HubError};
use cuhub_core::multiplayer::{RoomRole, TunnelState};
use nsh_core::prelude::*;

use super::multiplayer::port_value;
use super::{MpBusy, Ui};

const POLL_MILLIS: u64 = 2_000;
const HOST_ADDRESS: &str = "10.144.144.1";
const RETRY_SECONDS: [u64; 3] = [0, 3, 8];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GamePhase {
    NoSession,
    Unavailable,
    NoControl,
    WaitingTunnel,
    Ready,
    Hosting,
    Joined,
}

pub fn game_phase(snapshot: &BridgeSnapshot, role: RoomRole, tunnel: &TunnelState) -> GamePhase {
    if snapshot.status.session.is_none() {
        return GamePhase::NoSession;
    }
    let Some(state) = snapshot.mp.as_ref() else {
        return GamePhase::Unavailable;
    };
    if !state.available {
        return GamePhase::Unavailable;
    }
    if !snapshot.control {
        return GamePhase::NoControl;
    }
    if state.running {
        return if state.host {
            GamePhase::Hosting
        } else {
            GamePhase::Joined
        };
    }
    match role {
        RoomRole::Host => GamePhase::Ready,
        RoomRole::Guest => {
            if matches!(tunnel, TunnelState::Connected { .. }) {
                GamePhase::Ready
            } else {
                GamePhase::WaitingTunnel
            }
        }
    }
}

pub fn game_status_key(phase: GamePhase, role: RoomRole) -> &'static str {
    match phase {
        GamePhase::NoSession => "multiplayer.game_status_session",
        GamePhase::Unavailable => "multiplayer.game_status_unavailable",
        GamePhase::NoControl => "multiplayer.game_status_no_control",
        GamePhase::WaitingTunnel => "multiplayer.game_status_tunnel",
        GamePhase::Ready => match role {
            RoomRole::Host => "multiplayer.game_status_ready_host",
            RoomRole::Guest => "multiplayer.game_status_ready_guest",
        },
        GamePhase::Hosting => "multiplayer.game_status_hosting",
        GamePhase::Joined => "multiplayer.game_status_joined",
    }
}

pub fn game_address(role: RoomRole, tunnel: &TunnelState, port: &str) -> Option<String> {
    match role {
        RoomRole::Host => Some(format!("{HOST_ADDRESS}:{}", port_value(port)?)),
        RoomRole::Guest => match tunnel {
            TunnelState::Connected { virtual_ip, port } => Some(format!("{virtual_ip}:{port}")),
            _ => None,
        },
    }
}

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
}

impl Gate {
    fn at(pid: Option<u32>) -> Self {
        Self {
            pid,
            done: false,
            attempts: 0,
            next_at: Instant::now(),
            tunnel: None,
        }
    }
}

impl Ui {
    pub fn mp_game_phase(&self) -> GamePhase {
        game_phase(&self.bridge.get(), self.mp.role.get(), &self.mp.state.get())
    }

    pub fn mp_game_address(&self) -> Option<String> {
        game_address(
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
                    hub.game_host(&address)
                } else {
                    hub.game_join(&address)
                }
            })
            .await;
            ui.mp.busy.set(MpBusy::None);
            match result {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => ui.mp.game_error.set(game_error_text(&error)),
                Err(_) => ui.mp.game_error.set(t!("multiplayer.game_error_worker")),
            }
        });
    }

    pub(crate) fn watch_mp_control(&self) {
        let ui = self.clone();
        self.spawn_worker(async move {
            let mut gate = Gate::at(None);
            loop {
                let slept =
                    blocking(|| std::thread::sleep(Duration::from_millis(POLL_MILLIS))).await;
                if slept.is_err() {
                    break;
                }
                let snapshot = ui.bridge.get();
                let pid = snapshot.status.session.as_ref().map(|session| session.pid);
                if pid != gate.pid {
                    gate = Gate::at(pid);
                }
                let role = ui.mp.role.get();
                let tunnel = ui.mp.state.get();
                if role == RoomRole::Guest {
                    let key = match tunnel {
                        TunnelState::Connected { virtual_ip, port } => Some((virtual_ip, port)),
                        _ => None,
                    };
                    if key.is_some() && key != gate.tunnel {
                        gate.attempts = 0;
                        gate.done = false;
                        gate.next_at = Instant::now();
                    }
                    gate.tunnel = key;
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
                let Some(address) = game_address(role, &tunnel, &ui.mp.port.get()) else {
                    continue;
                };
                if role == RoomRole::Guest {
                    if gate.attempts as usize >= RETRY_SECONDS.len() {
                        gate.done = true;
                        continue;
                    }
                    if Instant::now() < gate.next_at {
                        continue;
                    }
                }
                gate.attempts += 1;
                let host = role == RoomRole::Host;
                if host {
                    gate.done = true;
                }
                let command_ui = ui.clone();
                let result = blocking(move || {
                    let hub = command_ui.hub.clone();
                    let hub = hub.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    if host {
                        hub.game_host(&address)
                    } else {
                        hub.game_join(&address)
                    }
                })
                .await;
                match result {
                    Ok(Ok(_)) => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use cuhub_core::bridge::mp::MpState;
    use cuhub_core::bridge::server::BridgeStatus;
    use cuhub_core::bridge::session::BridgeSessionInfo;

    fn snapshot(control: bool, mp: Option<MpState>) -> BridgeSnapshot {
        BridgeSnapshot {
            game_running: true,
            status: BridgeStatus {
                running: true,
                endpoint: None,
                session: Some(BridgeSessionInfo {
                    pid: 42,
                    unity_version: None,
                    bepinex_version: None,
                    mods: Vec::new(),
                }),
                last_error: None,
            },
            mp,
            control,
            start_error: None,
        }
    }

    fn state(available: bool, running: bool, host: bool) -> MpState {
        MpState {
            available,
            running,
            client: !host,
            server: host,
            host,
            krokmp_version: None,
        }
    }

    fn tunnel(connected: bool) -> TunnelState {
        if connected {
            TunnelState::Connected {
                virtual_ip: Ipv4Addr::new(10, 144, 144, 2),
                port: 7777,
            }
        } else {
            TunnelState::WaitingPeer
        }
    }

    #[test]
    fn waits_for_the_game_session_first() {
        let mut snapshot = snapshot(true, Some(state(true, false, true)));
        snapshot.status.session = None;
        assert_eq!(
            game_phase(&snapshot, RoomRole::Host, &tunnel(false)),
            GamePhase::NoSession
        );
    }

    #[test]
    fn reports_unavailable_without_a_usable_game_state() {
        assert_eq!(
            game_phase(&snapshot(true, None), RoomRole::Host, &tunnel(false)),
            GamePhase::Unavailable
        );
        assert_eq!(
            game_phase(
                &snapshot(true, Some(state(false, false, true))),
                RoomRole::Host,
                &tunnel(false)
            ),
            GamePhase::Unavailable
        );
    }

    #[test]
    fn control_off_comes_before_room_readiness() {
        assert_eq!(
            game_phase(
                &snapshot(false, Some(state(true, false, true))),
                RoomRole::Host,
                &tunnel(false)
            ),
            GamePhase::NoControl
        );
    }

    #[test]
    fn host_reads_ready_then_hosting() {
        assert_eq!(
            game_phase(
                &snapshot(true, Some(state(true, false, true))),
                RoomRole::Host,
                &tunnel(false)
            ),
            GamePhase::Ready
        );
        assert_eq!(
            game_phase(
                &snapshot(true, Some(state(true, true, true))),
                RoomRole::Host,
                &tunnel(false)
            ),
            GamePhase::Hosting
        );
    }

    #[test]
    fn guest_waits_for_the_tunnel_then_joins() {
        assert_eq!(
            game_phase(
                &snapshot(true, Some(state(true, false, false))),
                RoomRole::Guest,
                &tunnel(false)
            ),
            GamePhase::WaitingTunnel
        );
        assert_eq!(
            game_phase(
                &snapshot(true, Some(state(true, false, false))),
                RoomRole::Guest,
                &tunnel(true)
            ),
            GamePhase::Ready
        );
        assert_eq!(
            game_phase(
                &snapshot(true, Some(state(true, true, false))),
                RoomRole::Guest,
                &tunnel(true)
            ),
            GamePhase::Joined
        );
    }

    #[test]
    fn host_address_uses_the_fixed_host_endpoint() {
        assert_eq!(
            game_address(RoomRole::Host, &tunnel(false), "25565").as_deref(),
            Some("10.144.144.1:25565")
        );
        assert_eq!(
            game_address(RoomRole::Host, &tunnel(false), " 5123 ").as_deref(),
            Some("10.144.144.1:5123")
        );
    }

    #[test]
    fn host_address_rejects_a_bad_port() {
        assert_eq!(game_address(RoomRole::Host, &tunnel(false), ""), None);
        assert_eq!(game_address(RoomRole::Host, &tunnel(false), "0"), None);
        assert_eq!(game_address(RoomRole::Host, &tunnel(false), "70000"), None);
    }

    #[test]
    fn guest_address_uses_the_connected_tunnel() {
        assert_eq!(
            game_address(RoomRole::Guest, &tunnel(true), "25565").as_deref(),
            Some("10.144.144.2:7777")
        );
        assert_eq!(game_address(RoomRole::Guest, &tunnel(false), "25565"), None);
    }

    #[test]
    fn status_keys_match_each_phase() {
        assert_eq!(
            game_status_key(GamePhase::NoSession, RoomRole::Host),
            "multiplayer.game_status_session"
        );
        assert_eq!(
            game_status_key(GamePhase::Ready, RoomRole::Host),
            "multiplayer.game_status_ready_host"
        );
        assert_eq!(
            game_status_key(GamePhase::Ready, RoomRole::Guest),
            "multiplayer.game_status_ready_guest"
        );
        assert_eq!(
            game_status_key(GamePhase::Hosting, RoomRole::Guest),
            "multiplayer.game_status_hosting"
        );
        assert_eq!(
            game_status_key(GamePhase::Joined, RoomRole::Host),
            "multiplayer.game_status_joined"
        );
    }
}
