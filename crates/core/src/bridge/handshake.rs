//! Handshake of one bridge connection: shared token, protocol major and admission.

use std::net::TcpStream;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::json;

use crate::bridge::command::COMMANDS;
use crate::bridge::proto::{self, ErrorBody, Message, PROTO_MAJOR};
use crate::bridge::server::{ServerShared, lock, read};
use crate::bridge::session::{self, BridgeModInfo, BridgeSessionInfo};

// Shared with the C# plugin; it only keeps accidental cross-version connections
// out, the loopback listener is the actual boundary.
pub const TOKEN: &str = "c0ffee5e1d1a7a9b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d5e6f70819a2b";

const MAX_MODS: usize = 256;
const MAX_FIELD: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeError {
    BadToken,
    ProtocolMismatch { host: u16, guest: u16 },
    Busy { same_pid: bool },
    Rejected(String),
}

impl std::fmt::Display for HandshakeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadToken => formatter.write_str("bad token"),
            Self::ProtocolMismatch { host, guest } => {
                write!(
                    formatter,
                    "protocol major {guest} does not match host major {host}"
                )
            }
            Self::Busy { same_pid: true } => {
                formatter.write_str("this game process already has a session")
            }
            Self::Busy { same_pid: false } => {
                formatter.write_str("the bridge already has a session")
            }
            Self::Rejected(reason) => formatter.write_str(reason),
        }
    }
}

pub(crate) fn accept(
    stream: &mut TcpStream,
    shared: &Arc<ServerShared>,
) -> Result<BridgeSessionInfo, HandshakeError> {
    stream
        .set_read_timeout(Some(shared.options.handshake_timeout))
        .map_err(|error| HandshakeError::Rejected(format!("timeout setup failed: {error}")))?;
    let mut buffer = Vec::new();
    let message = match session::read_frame(stream, &mut buffer) {
        Ok(Some(session::Frame::Message(message))) => message,
        Ok(Some(session::Frame::Ignored)) => {
            return Err(HandshakeError::Rejected(
                "unreadable hello frame".to_owned(),
            ));
        }
        Ok(None) => {
            return Err(HandshakeError::Rejected("closed before hello".to_owned()));
        }
        Err(error) => {
            return Err(HandshakeError::Rejected(format!(
                "hello read failed: {error}"
            )));
        }
    };
    let Message::Req {
        id, method, params, ..
    } = message
    else {
        return Err(HandshakeError::Rejected(
            "first frame is not a request".to_owned(),
        ));
    };
    if method != "hello" {
        return Err(HandshakeError::Rejected(format!(
            "first request is {method}"
        )));
    }
    let hello: HelloParams = serde_json::from_value(params)
        .map_err(|_| HandshakeError::Rejected("invalid hello".to_owned()))?;
    if !tokens_match(&hello.token, TOKEN) {
        let response = Message::failure(id, ErrorBody::new("bad_token", "token rejected"));
        let _ = proto::write_message(stream, &response);
        return Err(HandshakeError::BadToken);
    }
    if hello.proto.major != PROTO_MAJOR {
        let response = Message::failure(
            id,
            ErrorBody::new(
                "proto_mismatch",
                format!(
                    "host protocol {}, guest protocol {}",
                    PROTO_MAJOR, hello.proto.major
                ),
            ),
        );
        let _ = proto::write_message(stream, &response);
        return Err(HandshakeError::ProtocolMismatch {
            host: PROTO_MAJOR,
            guest: hello.proto.major,
        });
    }
    if hello.mods.len() > MAX_MODS {
        return Err(HandshakeError::Rejected("too many mods".to_owned()));
    }
    let mut mods = Vec::with_capacity(hello.mods.len());
    for entry in hello.mods {
        if entry.guid.is_empty() || entry.guid.len() > MAX_FIELD || entry.version.len() > MAX_FIELD
        {
            return Err(HandshakeError::Rejected("invalid mod entry".to_owned()));
        }
        mods.push(BridgeModInfo {
            guid: entry.guid,
            version: entry.version,
        });
    }
    let pid = hello.game.pid.unwrap_or_default();
    if let Some(current) = lock(&shared.session).as_ref() {
        let same_pid = current.info.pid == pid;
        let (code, message) = if same_pid {
            ("duplicate_pid", "this game process already has a session")
        } else {
            ("busy", "the bridge already has a session")
        };
        let response = Message::failure(id, ErrorBody::new(code, message));
        let _ = proto::write_message(stream, &response);
        return Err(HandshakeError::Busy { same_pid });
    }
    let info = BridgeSessionInfo {
        pid,
        unity_version: hello.game.unity_version,
        bepinex_version: hello.game.bepinex_version,
        mods,
    };
    let guids = info.mod_guids();
    let commands: Vec<serde_json::Value> = COMMANDS
        .iter()
        .map(|command| json!({ "id": command.id, "policy": command.policy }))
        .collect();
    let response = Message::response(
        id,
        json!({
            "host_version": env!("CARGO_PKG_VERSION"),
            "caps": [],
            "commands": commands,
            "perms": read(&shared.permissions).snapshot(&guids),
        }),
    );
    proto::write_message(stream, &response)
        .map_err(|error| HandshakeError::Rejected(format!("hello reply failed: {error}")))?;
    stream
        .set_read_timeout(None)
        .map_err(|error| HandshakeError::Rejected(format!("timeout reset failed: {error}")))?;
    Ok(info)
}

fn tokens_match(candidate: &str, expected: &str) -> bool {
    if candidate.len() != expected.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in candidate.bytes().zip(expected.bytes()) {
        difference |= left ^ right;
    }
    difference == 0
}

#[derive(Deserialize)]
struct HelloParams {
    token: String,
    proto: ProtoVersion,
    #[serde(default)]
    game: HelloGame,
    #[serde(default)]
    mods: Vec<HelloMod>,
}

#[derive(Deserialize)]
struct ProtoVersion {
    major: u16,
}

#[derive(Deserialize, Default)]
struct HelloGame {
    #[serde(default)]
    pid: Option<u32>,
    #[serde(default)]
    unity_version: Option<String>,
    #[serde(default)]
    bepinex_version: Option<String>,
}

#[derive(Deserialize)]
struct HelloMod {
    #[serde(default)]
    guid: String,
    #[serde(default)]
    version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_token_is_64_lowercase_hex_characters() {
        assert_eq!(TOKEN.len(), 64);
        assert!(
            TOKEN
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );
    }

    #[test]
    fn tokens_match_only_on_the_exact_shared_value() {
        assert!(tokens_match(TOKEN, TOKEN));
        assert!(!tokens_match(&TOKEN[1..], TOKEN));
        assert!(!tokens_match(&TOKEN.to_ascii_uppercase(), TOKEN));
        assert!(!tokens_match("", TOKEN));
    }

    #[test]
    fn a_protocol_mismatch_names_both_majors() {
        let error = HandshakeError::ProtocolMismatch { host: 1, guest: 2 };
        let text = error.to_string();
        assert!(text.contains('1'));
        assert!(text.contains('2'));
    }
}
