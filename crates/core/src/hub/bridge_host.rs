//! Bridge host lifecycle on the Hub and the snapshot the UI polls.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use crate::bridge::BRIDGE_GUID;
use crate::bridge::command::{BridgeSettings, Permission, Permissions};
use crate::bridge::install as package;
use crate::bridge::mp::{GameStart, MpState};
use crate::bridge::server::{BridgeError, BridgeEvent, BridgeOptions, BridgeServer, BridgeStatus};
use crate::easytier::{Role, Tunnel};
use crate::hub::{Hub, HubError};
use crate::install;
use crate::mods;
use crate::settings::Settings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeInstallStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub target: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BridgeInstallReport {
    pub version: Option<String>,
    pub target: PathBuf,
    pub files: usize,
    pub backup: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum HubEvent {
    Bridge(BridgeEvent),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BridgeSnapshot {
    pub game_running: bool,
    pub status: BridgeStatus,
    pub mp: Option<MpState>,
    pub control: bool,
    pub start_error: Option<BridgeError>,
}

impl Hub {
    pub fn start_bridge(&mut self) -> Result<BridgeStatus, HubError> {
        if let Some(server) = &self.bridge {
            return Ok(server.status());
        }
        let permissions = Permissions::from_settings(&self.settings.bridge);
        let server = BridgeServer::start(permissions, BridgeOptions::default(), self.logs.clone())?;
        let status = server.status();
        self.bridge = Some(server);
        Ok(status)
    }

    pub fn stop_bridge(&mut self) -> Result<(), HubError> {
        if let Some(server) = self.bridge.take() {
            server.stop();
        }
        Ok(())
    }

    pub fn bridge_control_enabled(&self) -> bool {
        control_enabled(&self.settings.bridge)
    }

    pub fn set_bridge_control(&mut self, enabled: bool) -> Result<(), HubError> {
        if control_enabled(&self.settings.bridge) == enabled {
            return Ok(());
        }
        let settings = with_control(&self.settings, enabled);
        self.save_settings(&settings).map_err(HubError::Io)?;
        if let Some(server) = &self.bridge {
            server.set_permissions(Permissions::from_settings(&self.settings.bridge));
        }
        self.logs.activity(
            "Bridge",
            &format!(
                "Multiplayer control {} for {BRIDGE_GUID}.",
                if enabled { "enabled" } else { "disabled" }
            ),
        );
        Ok(())
    }

    pub fn bridge_status(&self) -> BridgeStatus {
        self.bridge
            .as_ref()
            .map(BridgeServer::status)
            .unwrap_or_default()
    }

    pub fn bridge_snapshot(&mut self) -> BridgeSnapshot {
        // No consumer yet; drain so the channel cannot grow without bound.
        let _ = self.take_events();
        let game_running = self.game_running().unwrap_or(false);
        let mut status = self.bridge_status();
        let mut start_error = None;
        if game_running {
            self.bridge_seen_game = true;
            if !status.running {
                match self.start_bridge() {
                    Ok(started) => status = started,
                    Err(HubError::Bridge(error)) => start_error = Some(error),
                    Err(_) => {}
                }
            }
        } else if status.running && self.bridge_seen_game {
            let _ = self.stop_bridge();
            self.bridge_seen_game = false;
            status = self.bridge_status();
        }
        let mp = if status.running && status.session.is_some() {
            self.poll_mp_state()
        } else {
            None
        };
        BridgeSnapshot {
            game_running,
            status,
            mp,
            control: control_enabled(&self.settings.bridge),
            start_error,
        }
    }

    fn poll_mp_state(&self) -> Option<MpState> {
        let server = self.bridge.as_ref()?;
        let value = server.invoke_silent("mp.state", json!({})).ok()?;
        MpState::from_value(&value)
    }

    pub fn game_room_port(&self) -> Option<u16> {
        let game_running = self.game_running().unwrap_or(false);
        let status = self.bridge_status();
        let state = if game_running && status.running && status.session.is_some() {
            self.poll_mp_state()
        } else {
            None
        };
        room_port(game_running, &status, state)
    }

    pub fn invoke_bridge(&self, command: &str, params: Value) -> Result<Value, HubError> {
        let Some(server) = &self.bridge else {
            return Err(HubError::Bridge(BridgeError::Unavailable));
        };
        server.invoke(command, params).map_err(HubError::Bridge)
    }

    pub fn game_host(&self, address: &str) -> Result<Value, HubError> {
        self.invoke_bridge("mp.host", json!({ "address": address }))
    }

    pub fn game_join(&self, address: &str) -> Result<Value, HubError> {
        self.invoke_bridge("mp.join", json!({ "address": address }))
    }

    pub fn start_game_host(&self) -> Result<GameStart, HubError> {
        let port = self
            .tunnel
            .as_ref()
            .filter(|tunnel| tunnel.role() == Role::Host)
            .map(Tunnel::port)
            .ok_or(HubError::NoRoom)?;
        let address = match self.tunnel_virtual_ip() {
            Some(ip) => format!("{ip}:{port}"),
            None => return Err(HubError::NoRoom),
        };
        let started =
            GameStart::from_value(&self.game_host(&address)?).ok_or(HubError::UnexpectedContent)?;
        let reported = self.await_room_port();
        if started.port != port || reported != Some(port) {
            return Err(HubError::GamePort {
                expected: port,
                reported: reported.or(Some(started.port)),
            });
        }
        self.logs.activity(
            "Multiplayer",
            &format!("The game is hosting the room on port {port}."),
        );
        Ok(started)
    }

    fn await_room_port(&self) -> Option<u16> {
        for attempt in 0..10 {
            if let Some(port) = self.game_room_port() {
                return Some(port);
            }
            if attempt < 9 {
                thread::sleep(Duration::from_millis(100));
            }
        }
        None
    }

    pub fn take_events(&mut self) -> Vec<HubEvent> {
        let Some(server) = self.bridge.as_ref() else {
            return Vec::new();
        };
        let mut events = Vec::new();
        while let Some(event) = server.try_recv() {
            events.push(HubEvent::Bridge(event));
        }
        events
    }

    pub fn bridge_install_status(&self) -> Result<BridgeInstallStatus, HubError> {
        let plugins = self.plugins_dir()?;
        let target = package::target_dir(&plugins);
        let plugin = package::installed_plugin(&target);
        let version = plugin.as_deref().and_then(mods::read_mod_version);
        Ok(BridgeInstallStatus {
            installed: plugin.is_some(),
            version,
            target,
        })
    }

    pub fn install_bridge(&mut self, archive: &Path) -> Result<BridgeInstallReport, HubError> {
        if !archive.is_file() {
            return Err(HubError::Install(format!(
                "The bridge archive does not exist: {}",
                archive.display()
            )));
        }
        self.ensure_game_closed()?;
        let plugins = self.plugins_dir()?;
        let target = package::target_dir(&plugins);
        if self.links.get(&target).is_some() {
            return Err(HubError::Install(format!(
                "{} is deployed by a profile. Switch profiles instead of installing the bridge here.",
                target.display()
            )));
        }
        let staging = self.staging_root();
        let staged = match install::stage_archive(archive, &staging) {
            Ok(staged) => staged,
            Err(error) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(HubError::Install(error.to_string()));
            }
        };
        let staged_root = staged.staging_root.clone();
        let payload = match install::plan_package(&staged) {
            Ok(payload) => payload,
            Err(error) => {
                let _ = fs::remove_dir_all(&staged_root);
                return Err(HubError::Plan(error.to_string()));
            }
        };
        let checked = match package::validate_payload(&payload) {
            Ok(checked) => checked,
            Err(error) => {
                let _ = fs::remove_dir_all(&staged_root);
                return Err(HubError::Plan(error));
            }
        };
        let identity = mods::read_identity(&checked.plugin);
        let guid = identity.as_ref().and_then(|(_, guid)| guid.as_deref());
        if guid != Some(crate::bridge::BRIDGE_GUID) {
            let _ = fs::remove_dir_all(&staged_root);
            let found = match guid {
                Some(guid) => format!("declares GUID {guid}"),
                None => String::from("has no BepInPlugin GUID"),
            };
            return Err(HubError::Plan(format!(
                "{} {found}; expected {}.",
                crate::bridge::install::PLUGIN_FILE,
                crate::bridge::BRIDGE_GUID
            )));
        }
        let version = mods::read_mod_version(&checked.plugin);
        let plan = match install::plan_payload(
            &staged,
            install::PlanTarget {
                root: &target,
                plugins: &plugins,
                owned_root: &target,
                owned_files: &[],
            },
        ) {
            Ok(plan) => plan,
            Err(error) => {
                let _ = fs::remove_dir_all(&staged_root);
                return Err(HubError::Plan(error.to_string()));
            }
        };
        let report = match install::apply_upgrade(&plan, &self.backup_root()) {
            Ok(report) => report,
            Err(error) => {
                let _ = fs::remove_dir_all(&staged_root);
                return Err(HubError::Apply(error.to_string()));
            }
        };
        let _ = fs::remove_dir_all(&staged_root);
        let files = report.replaced.len() + report.created.len();
        self.logs.activity(
            "Bridge",
            &format!(
                "Installed bridge {} into {} ({} file(s)).",
                version.as_deref().unwrap_or("unknown"),
                target.display(),
                files
            ),
        );
        Ok(BridgeInstallReport {
            version,
            target,
            files,
            backup: report.backup,
        })
    }
}

fn room_port(game_running: bool, status: &BridgeStatus, state: Option<MpState>) -> Option<u16> {
    if !game_running || !status.running || status.session.is_none() {
        return None;
    }
    state.and_then(|state| state.room_port())
}

const CONTROL_COMMANDS: [&str; 2] = ["mp.host", "mp.join"];

fn control_enabled(settings: &BridgeSettings) -> bool {
    CONTROL_COMMANDS.iter().all(|command| {
        settings
            .permissions
            .get(BRIDGE_GUID)
            .and_then(|entries| entries.get(*command))
            == Some(&Permission::Granted)
    })
}

fn with_control(settings: &Settings, enabled: bool) -> Settings {
    let mut next = settings.clone();
    let entries = next
        .bridge
        .permissions
        .entry(BRIDGE_GUID.to_owned())
        .or_default();
    for command in CONTROL_COMMANDS {
        entries.insert(
            command.to_owned(),
            if enabled {
                Permission::Granted
            } else {
                Permission::Denied
            },
        );
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::session::BridgeSessionInfo;
    use serde_json::json;

    fn state(value: serde_json::Value) -> MpState {
        MpState::from_value(&value).expect("parse state")
    }

    fn host(port: Option<u16>) -> MpState {
        state(json!({
            "available": true,
            "running": true,
            "client": false,
            "server": true,
            "host": true,
            "port": port,
        }))
    }

    fn guest() -> MpState {
        state(json!({
            "available": true,
            "running": true,
            "client": true,
            "server": false,
            "host": false,
            "port": 7790,
        }))
    }

    fn connected() -> BridgeStatus {
        BridgeStatus {
            running: true,
            endpoint: Some("127.0.0.1:41297".to_owned()),
            session: Some(BridgeSessionInfo {
                pid: 4242,
                unity_version: None,
                bepinex_version: None,
                mods: Vec::new(),
            }),
            last_error: None,
        }
    }

    #[test]
    fn a_room_port_needs_a_running_game_and_an_attached_bridge() {
        assert_eq!(room_port(false, &connected(), Some(host(Some(7790)))), None);
        assert_eq!(
            room_port(true, &BridgeStatus::default(), Some(host(Some(7790)))),
            None
        );
        let stopped = BridgeStatus {
            running: true,
            ..BridgeStatus::default()
        };
        assert_eq!(room_port(true, &stopped, Some(host(Some(7790)))), None);
    }

    #[test]
    fn an_attached_bridge_reports_the_game_room_port() {
        assert_eq!(
            room_port(true, &connected(), Some(host(Some(7790)))),
            Some(7790)
        );
    }

    #[test]
    fn a_missing_or_guest_state_has_no_room_port() {
        assert_eq!(room_port(true, &connected(), None), None);
        assert_eq!(room_port(true, &connected(), Some(guest())), None);
    }

    #[test]
    fn control_grants_only_the_two_multiplayer_commands() {
        let mut settings = Settings::default();
        settings
            .bridge
            .permissions
            .entry("other.mod".to_owned())
            .or_default()
            .insert("mp.host".to_owned(), Permission::Granted);

        assert!(!control_enabled(&settings.bridge));

        let enabled = with_control(&settings, true);
        assert!(control_enabled(&enabled.bridge));
        let bridge = &enabled.bridge.permissions[BRIDGE_GUID];
        assert_eq!(bridge.len(), 2);
        assert_eq!(bridge["mp.host"], Permission::Granted);
        assert_eq!(bridge["mp.join"], Permission::Granted);
        assert_eq!(
            enabled.bridge.permissions["other.mod"]["mp.host"],
            Permission::Granted
        );

        let disabled = with_control(&enabled, false);
        assert!(!control_enabled(&disabled.bridge));
        let bridge = &disabled.bridge.permissions[BRIDGE_GUID];
        assert_eq!(bridge["mp.host"], Permission::Denied);
        assert_eq!(bridge["mp.join"], Permission::Denied);
    }

    #[test]
    fn control_enabled_requires_both_commands_granted() {
        let mut settings = Settings::default();
        settings
            .bridge
            .permissions
            .entry(BRIDGE_GUID.to_owned())
            .or_default()
            .insert("mp.host".to_owned(), Permission::Granted);
        assert!(!control_enabled(&settings.bridge));
    }
}
