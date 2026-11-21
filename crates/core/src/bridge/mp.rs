//! Typed view of the game-side `mp.state` reply exposed by the C# plugin.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MpState {
    pub available: bool,
    pub running: bool,
    pub client: bool,
    pub server: bool,
    pub host: bool,
    #[serde(default, deserialize_with = "lenient_port")]
    pub port: Option<u16>,
    #[serde(default)]
    pub krokmp_version: Option<String>,
}

impl MpState {
    pub fn from_value(value: &Value) -> Option<Self> {
        serde_json::from_value(value.clone()).ok()
    }

    pub fn room_port(&self) -> Option<u16> {
        if !self.running || !self.server {
            return None;
        }
        self.port
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GameStart {
    pub started: bool,
    pub host: bool,
    pub address: String,
    pub port: u16,
}

impl GameStart {
    pub fn from_value(value: &Value) -> Option<Self> {
        serde_json::from_value(value.clone()).ok()
    }
}

fn lenient_port<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value
        .and_then(|value| value.as_u64())
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port != 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(port: Value) -> MpState {
        MpState::from_value(&json!({
            "available": true,
            "running": true,
            "client": false,
            "server": true,
            "host": true,
            "port": port,
        }))
        .expect("parse state")
    }

    #[test]
    fn parses_the_game_side_reply() {
        let state = MpState::from_value(&json!({
            "available": true,
            "running": true,
            "client": false,
            "server": true,
            "host": true,
            "bridge_receivers_installed": true,
            "krokmp_version": "1.4.2.0",
        }))
        .expect("parse state");
        assert!(state.available);
        assert!(state.running);
        assert!(!state.client);
        assert!(state.server);
        assert!(state.host);
        assert_eq!(state.port, None);
        assert_eq!(state.room_port(), None);
        assert_eq!(state.krokmp_version.as_deref(), Some("1.4.2.0"));
    }

    #[test]
    fn reads_the_listening_port_of_a_host() {
        let state = parse(json!(7790));
        assert_eq!(state.port, Some(7790));
        assert_eq!(state.room_port(), Some(7790));
    }

    #[test]
    fn a_missing_port_reads_as_absent() {
        let state = MpState::from_value(&json!({
            "available": true,
            "running": true,
            "client": false,
            "server": true,
            "host": true,
            "krokmp_version": "1.4.2.0",
        }))
        .expect("parse state");
        assert_eq!(state.port, None);
    }

    #[test]
    fn a_null_port_reads_as_absent() {
        let state = parse(Value::Null);
        assert_eq!(state.port, None);
    }

    #[test]
    fn an_invalid_port_reads_as_absent() {
        for port in [
            json!(70000),
            json!(-1),
            json!("7790"),
            json!(0),
            json!(7790.5),
        ] {
            let state = parse(port.clone());
            assert_eq!(state.port, None, "port {port} should be ignored");
        }
    }

    #[test]
    fn a_guest_port_is_not_a_room_port() {
        let state = MpState::from_value(&json!({
            "available": true,
            "running": true,
            "client": true,
            "server": false,
            "host": false,
            "port": 7790,
        }))
        .expect("parse state");
        assert_eq!(state.port, Some(7790));
        assert_eq!(state.room_port(), None);
    }

    #[test]
    fn a_stopped_transport_has_no_room_port() {
        let state = MpState::from_value(&json!({
            "available": true,
            "running": false,
            "client": false,
            "server": true,
            "host": true,
            "port": 7790,
        }))
        .expect("parse state");
        assert_eq!(state.room_port(), None);
    }

    #[test]
    fn a_null_version_reads_as_absent() {
        let state = MpState::from_value(&json!({
            "available": false,
            "running": false,
            "client": false,
            "server": false,
            "host": false,
            "krokmp_version": null,
        }))
        .expect("parse state");
        assert_eq!(state.krokmp_version, None);
    }

    #[test]
    fn a_reply_without_the_flags_is_not_a_state() {
        assert_eq!(MpState::from_value(&json!({ "running": true })), None);
    }

    #[test]
    fn parses_a_game_start_reply() {
        let start = GameStart::from_value(&json!({
            "started": true,
            "host": true,
            "address": "10.144.144.1:7790",
            "port": 7790,
        }))
        .expect("parse reply");
        assert!(start.started);
        assert!(start.host);
        assert_eq!(start.address, "10.144.144.1:7790");
        assert_eq!(start.port, 7790);
    }

    #[test]
    fn a_start_reply_without_a_port_is_not_a_reply() {
        assert_eq!(
            GameStart::from_value(&json!({
                "started": true,
                "host": true,
                "address": "10.144.144.1:7790",
            })),
            None
        );
        assert_eq!(
            GameStart::from_value(&json!({
                "started": true,
                "host": true,
                "address": "10.144.144.1:7790",
                "port": "7790",
            })),
            None
        );
    }
}
