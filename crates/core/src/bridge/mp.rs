//! Typed view of the game-side `mp.state` reply exposed by the C# plugin.

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MpState {
    pub available: bool,
    pub running: bool,
    pub client: bool,
    pub server: bool,
    pub host: bool,
    #[serde(default)]
    pub krokmp_version: Option<String>,
}

impl MpState {
    pub fn from_value(value: &Value) -> Option<Self> {
        serde_json::from_value(value.clone()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        assert_eq!(state.krokmp_version.as_deref(), Some("1.4.2.0"));
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
}
