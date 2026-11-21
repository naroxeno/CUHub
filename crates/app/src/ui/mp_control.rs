//! Game-side multiplayer control: the bridge permission switch, manual room commands and the auto gate.

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use cuhub_core::bridge::server::BridgeError;
use cuhub_core::hub::{BridgeSnapshot, HubError};
use cuhub_core::multiplayer::{
    HostPort, PortError, PortSource, PortUpdate, RoomRole, TunnelState, choose_host_port,
};
use nsh_core::prelude::*;

use super::multiplayer::manual_port;
use super::{MpBusy, Ui};

const POLL_MILLIS: u64 = 2_000;
pub(crate) const HOST_ADDRESS: &str = "10.144.144.1";
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameHandoff {
    Same,
    Entered,
    Left,
    Restarted,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HostDrop {
    None,
    Left,
    Moved(u16),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PortFollow {
    Clear,
    Ignore,
    Moved(u16),
    Failed { port: u16, message: String },
}

pub fn detected_port(snapshot: &BridgeSnapshot) -> Option<u16> {
    snapshot.mp.as_ref().and_then(|state| state.room_port())
}

pub fn host_port(snapshot: &BridgeSnapshot, manual: &str) -> HostPort {
    choose_host_port(detected_port(snapshot), manual_port(manual))
}

pub fn join_port(manual: &str) -> Option<u16> {
    manual_port(manual)
}

pub fn port_mismatch(snapshot: &BridgeSnapshot, manual: &str) -> Option<(u16, u16)> {
    let manual = manual_port(manual)?;
    let reported = detected_port(snapshot)?;
    (manual != reported).then_some((manual, reported))
}

pub fn port_source_key(source: PortSource) -> &'static str {
    match source {
        PortSource::Reported => "multiplayer.port_source_reported",
        PortSource::Manual => "multiplayer.port_source_manual",
        PortSource::Default => "multiplayer.port_source_default",
    }
}

pub fn port_follow(update: PortUpdate) -> PortFollow {
    match update {
        PortUpdate::Idle | PortUpdate::Settled => PortFollow::Clear,
        PortUpdate::Unsettled => PortFollow::Ignore,
        PortUpdate::Restarted { port } => PortFollow::Moved(port),
        PortUpdate::Failed { port, message } => PortFollow::Failed { port, message },
    }
}

pub fn session_handoff(previous: Option<u32>, current: Option<u32>) -> GameHandoff {
    match (previous, current) {
        (None, None) => GameHandoff::Same,
        (Some(before), Some(after)) if before == after => GameHandoff::Same,
        (None, Some(_)) => GameHandoff::Entered,
        (Some(_), None) => GameHandoff::Left,
        (Some(_), Some(_)) => GameHandoff::Restarted,
    }
}

pub fn host_drop(role: RoomRole, before: Option<(Ipv4Addr, u16)>, after: &TunnelState) -> HostDrop {
    if role != RoomRole::Guest {
        return HostDrop::None;
    }
    let Some((previous_address, previous_port)) = before else {
        return HostDrop::None;
    };
    match after {
        TunnelState::Connected {
            virtual_ip, port, ..
        } if *port != previous_port || *virtual_ip != previous_address => HostDrop::Moved(*port),
        TunnelState::Connected { .. } => HostDrop::None,
        _ => HostDrop::Left,
    }
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
    if state.running || state.server {
        return if state.server || state.host {
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

pub fn game_address(
    snapshot: &BridgeSnapshot,
    role: RoomRole,
    tunnel: &TunnelState,
    manual: &str,
) -> Option<String> {
    match role {
        RoomRole::Host => {
            let port = host_port(snapshot, manual).port;
            Some(format!("{HOST_ADDRESS}:{port}"))
        }
        RoomRole::Guest => match tunnel {
            TunnelState::Connected {
                local_port: Some(port),
                ..
            } => Some(format!("127.0.0.1:{port}")),
            TunnelState::Connected {
                virtual_ip, port, ..
            } => Some(format!("{virtual_ip}:{port}")),
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
            port: None,
            krokmp_version: None,
        }
    }

    fn hosting(port: u16) -> MpState {
        MpState {
            available: true,
            running: true,
            client: false,
            server: true,
            host: true,
            port: Some(port),
            krokmp_version: None,
        }
    }

    fn tunnel(connected: bool) -> TunnelState {
        if connected {
            TunnelState::Connected {
                virtual_ip: Ipv4Addr::new(10, 144, 144, 2),
                port: 7777,
                local_port: None,
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
        let idle = MpState {
            available: true,
            running: false,
            client: false,
            server: false,
            host: true,
            port: None,
            krokmp_version: None,
        };
        assert_eq!(
            game_phase(&snapshot(true, Some(idle)), RoomRole::Host, &tunnel(false)),
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
    fn a_game_side_room_is_hosting_without_a_cuhub_command() {
        assert_eq!(
            game_phase(
                &snapshot(true, Some(state(true, false, true))),
                RoomRole::Guest,
                &tunnel(true)
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
    fn the_reported_port_beats_the_manual_field() {
        let snapshot = snapshot(true, Some(hosting(24444)));
        assert_eq!(
            game_address(&snapshot, RoomRole::Host, &tunnel(false), "25565").as_deref(),
            Some("10.144.144.1:24444")
        );
        assert_eq!(port_mismatch(&snapshot, "25565"), Some((25565, 24444)));
        assert_eq!(port_mismatch(&snapshot, "24444"), None);
    }

    #[test]
    fn the_manual_field_is_the_fallback_for_hosting() {
        let snapshot = snapshot(true, None);
        assert_eq!(
            game_address(&snapshot, RoomRole::Host, &tunnel(false), " 5123 ").as_deref(),
            Some("10.144.144.1:5123")
        );
        assert_eq!(
            game_address(&snapshot, RoomRole::Host, &tunnel(false), "").as_deref(),
            Some("10.144.144.1:7790")
        );
    }

    #[test]
    fn guest_address_uses_the_connected_tunnel() {
        let snapshot = snapshot(true, None);
        assert_eq!(
            game_address(&snapshot, RoomRole::Guest, &tunnel(true), "25565").as_deref(),
            Some("10.144.144.2:7777")
        );
        let forwarded = TunnelState::Connected {
            virtual_ip: Ipv4Addr::new(10, 42, 0, 7),
            port: 24444,
            local_port: Some(25003),
        };
        assert_eq!(
            game_address(&snapshot, RoomRole::Guest, &forwarded, "").as_deref(),
            Some("127.0.0.1:25003")
        );
        assert_eq!(
            game_address(&snapshot, RoomRole::Guest, &tunnel(false), "25565"),
            None
        );
    }

    #[test]
    fn joining_never_follows_a_reported_game_port() {
        assert_eq!(join_port(""), None);
        assert_eq!(join_port("25565"), Some(25565));
        let snapshot = snapshot(true, Some(hosting(24444)));
        assert_eq!(host_port(&snapshot, "").port, 24444);
        assert_eq!(join_port(""), None);
    }

    #[test]
    fn port_sources_map_to_copy_keys() {
        assert_eq!(
            port_source_key(PortSource::Reported),
            "multiplayer.port_source_reported"
        );
        assert_eq!(
            port_source_key(PortSource::Manual),
            "multiplayer.port_source_manual"
        );
        assert_eq!(
            port_source_key(PortSource::Default),
            "multiplayer.port_source_default"
        );
    }

    #[test]
    fn port_updates_map_to_follow_actions() {
        assert_eq!(port_follow(PortUpdate::Idle), PortFollow::Clear);
        assert_eq!(port_follow(PortUpdate::Settled), PortFollow::Clear);
        assert_eq!(port_follow(PortUpdate::Unsettled), PortFollow::Ignore);
        assert_eq!(
            port_follow(PortUpdate::Restarted { port: 24444 }),
            PortFollow::Moved(24444)
        );
        assert_eq!(
            port_follow(PortUpdate::Failed {
                port: 24444,
                message: "busy".to_owned()
            }),
            PortFollow::Failed {
                port: 24444,
                message: "busy".to_owned()
            }
        );
    }

    #[test]
    fn sessions_hand_over_between_room_switches_and_exits() {
        assert_eq!(session_handoff(None, None), GameHandoff::Same);
        assert_eq!(session_handoff(Some(7), Some(7)), GameHandoff::Same);
        assert_eq!(session_handoff(None, Some(7)), GameHandoff::Entered);
        assert_eq!(session_handoff(Some(7), None), GameHandoff::Left);
        assert_eq!(session_handoff(Some(7), Some(8)), GameHandoff::Restarted);
    }

    #[test]
    fn the_guest_sees_the_host_leave_or_move() {
        let connected = (Ipv4Addr::new(10, 144, 144, 2), 7777);
        assert_eq!(
            host_drop(RoomRole::Guest, Some(connected), &TunnelState::WaitingPeer),
            HostDrop::Left
        );
        assert_eq!(
            host_drop(RoomRole::Guest, Some(connected), &tunnel(true)),
            HostDrop::None
        );
        assert_eq!(
            host_drop(
                RoomRole::Guest,
                Some(connected),
                &TunnelState::Connected {
                    virtual_ip: Ipv4Addr::new(10, 144, 144, 2),
                    port: 8888,
                    local_port: None,
                }
            ),
            HostDrop::Moved(8888)
        );
        assert_eq!(
            host_drop(RoomRole::Host, Some(connected), &TunnelState::WaitingPeer),
            HostDrop::None
        );
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
