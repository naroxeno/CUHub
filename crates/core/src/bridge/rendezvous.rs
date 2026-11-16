//! Discovery file the host writes so the game process can find the bridge.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::proto::PROTO_MAJOR;

pub const DIR: &str = "CasualtiesHub";
pub const FILE: &str = "bridge.json";
pub const LOOPBACK: &str = "127.0.0.1";
const TOKEN_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeFile {
    pub proto: u16,
    pub host: HostInfo,
    pub endpoint: Endpoint,
    pub token: String,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostInfo {
    pub pid: u32,
    pub version: String,
    pub os: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

impl BridgeFile {
    pub fn path_in(program_data: &Path) -> PathBuf {
        program_data.join(DIR).join(FILE)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.proto != PROTO_MAJOR {
            return Err("protocol major mismatch");
        }
        if self.endpoint.host != LOOPBACK {
            return Err("endpoint is not loopback");
        }
        if self.endpoint.port == 0 {
            return Err("endpoint port is zero");
        }
        let token = self.token.as_bytes();
        if token.len() != TOKEN_LEN
            || !token
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
        {
            return Err("token is not 64 lowercase hex characters");
        }
        Ok(())
    }

    pub fn write(&self, program_data: &Path) -> io::Result<PathBuf> {
        let path = Self::path_in(program_data);
        let body = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        crate::atomic::replace_private(&path, &body)?;
        Ok(path)
    }

    pub fn read(program_data: &Path) -> io::Result<Option<Self>> {
        let path = Self::path_in(program_data);
        let body = match fs::read(&path) {
            Ok(body) => body,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        // A stale or unreadable file must read as "no host": the game may never fail over discovery.
        let Ok(file) = serde_json::from_slice::<Self>(&body) else {
            return Ok(None);
        };
        if file.validate().is_err() {
            return Ok(None);
        }
        Ok(Some(file))
    }

    pub fn remove(program_data: &Path) -> io::Result<()> {
        match fs::remove_file(Self::path_in(program_data)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::temp_dir;

    use super::*;

    fn sample() -> BridgeFile {
        BridgeFile {
            proto: PROTO_MAJOR,
            host: HostInfo {
                pid: 42,
                version: "0.1.0".to_owned(),
                os: "linux".to_owned(),
            },
            endpoint: Endpoint {
                host: LOOPBACK.to_owned(),
                port: 47821,
            },
            token: "a".repeat(TOKEN_LEN),
            started_at: "2026-11-15T09:00:00Z".to_owned(),
        }
    }

    #[test]
    fn write_read_remove_round_trip() {
        let dir = temp_dir("bridge-rendezvous");
        let file = sample();
        let path = file.write(&dir).expect("write");
        assert!(path.ends_with(Path::new(DIR).join(FILE)));
        assert_eq!(BridgeFile::read(&dir).expect("read"), Some(file));
        BridgeFile::remove(&dir).expect("remove");
        assert_eq!(BridgeFile::read(&dir).expect("read"), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_reads_as_none() {
        let dir = temp_dir("bridge-missing");
        assert_eq!(BridgeFile::read(&dir).expect("read"), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupted_file_reads_as_none() {
        let dir = temp_dir("bridge-corrupt");
        let path = BridgeFile::path_in(&dir);
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(&path, b"{ not json").expect("write");
        assert_eq!(BridgeFile::read(&dir).expect("read"), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validation_rejects_wrong_proto_endpoint_and_token() {
        let mut file = sample();
        file.proto = PROTO_MAJOR + 1;
        assert!(file.validate().is_err());

        let mut file = sample();
        file.endpoint.host = "0.0.0.0".to_owned();
        assert!(file.validate().is_err());

        let mut file = sample();
        file.endpoint.port = 0;
        assert!(file.validate().is_err());

        let mut file = sample();
        file.token = "A".repeat(TOKEN_LEN);
        assert!(file.validate().is_err());

        let mut file = sample();
        file.token.truncate(TOKEN_LEN - 1);
        assert!(file.validate().is_err());

        assert!(sample().validate().is_ok());
    }
}
