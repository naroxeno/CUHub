// Multiplayer session persistence: the last room, a custom core path, relay overrides.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunnelState {
    Idle,
    WaitingPeer,
    Connected { virtual_ip: Ipv4Addr, port: u16 },
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoomRole {
    Host,
    Guest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastRoom {
    pub role: RoomRole,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiplayerState {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub last_room: Option<LastRoom>,
    #[serde(default)]
    pub core_path: Option<PathBuf>,
    #[serde(default)]
    pub relay_override: Option<Vec<String>>,
}

fn schema_version() -> u32 {
    1
}

impl Default for MultiplayerState {
    fn default() -> Self {
        Self {
            schema_version: schema_version(),
            last_room: None,
            core_path: None,
            relay_override: None,
        }
    }
}

impl MultiplayerState {
    pub fn path(app_data: &Path) -> PathBuf {
        app_data.join("multiplayer.json")
    }

    pub fn load(app_data: &Path) -> Self {
        let path = Self::path(app_data);
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self, app_data: &Path) -> io::Result<()> {
        let path = Self::path(app_data);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, text)?;
        restrict(&tmp)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn set_last_room(&mut self, role: RoomRole, code: &str) {
        self.last_room = Some(LastRoom {
            role,
            code: code.to_owned(),
        });
    }
}

#[cfg(unix)]
fn restrict(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn restrict(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cuhub-mp-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_is_default() {
        let dir = temp_dir("missing");
        let state = MultiplayerState::load(&dir);
        assert_eq!(state, MultiplayerState::default());
        assert_eq!(state.schema_version, 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn roundtrip_keeps_fields() {
        let dir = temp_dir("roundtrip");
        let mut state = MultiplayerState {
            schema_version: 1,
            last_room: None,
            core_path: Some(PathBuf::from("/opt/easytier")),
            relay_override: Some(vec!["tcp://relay.example:11010".into()]),
        };
        state.set_last_room(RoomRole::Guest, "C1-7K3M-9Q2X-4H8P-V6RW");
        state.save(&dir).unwrap();
        let loaded = MultiplayerState::load(&dir);
        assert_eq!(loaded, state);
        assert_eq!(loaded.last_room.unwrap().role, RoomRole::Guest);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_file_falls_back_to_default() {
        let dir = temp_dir("corrupt");
        fs::write(MultiplayerState::path(&dir), "{not json").unwrap();
        assert_eq!(MultiplayerState::load(&dir), MultiplayerState::default());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let dir = temp_dir("unknown");
        fs::write(
            MultiplayerState::path(&dir),
            r#"{"schemaVersion":1,"lastRoom":{"role":"host","code":"C1-7K3M-9Q2X-4H8P-V6RW"},"future":true}"#,
        )
        .unwrap();
        let state = MultiplayerState::load(&dir);
        assert_eq!(state.last_room.unwrap().code, "C1-7K3M-9Q2X-4H8P-V6RW");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn saved_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("mode");
        MultiplayerState::default().save(&dir).unwrap();
        let mode = fs::metadata(MultiplayerState::path(&dir))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(!dir.join("multiplayer.json.tmp").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
