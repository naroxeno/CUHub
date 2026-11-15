//! Length-prefixed JSON frames for the game bridge. Contract: `docs/bridge/protocol.md`.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTO_MAJOR: u16 = 1;
pub const PROTO_MINOR: u16 = 0;
pub const MAX_FRAME: usize = 1024 * 1024;

pub mod code {
    pub const UNKNOWN_COMMAND: &str = "unknown_command";
    pub const DENIED: &str = "denied";
    pub const INVALID_PARAMS: &str = "invalid_params";
    pub const BUSY: &str = "busy";
    pub const UNAVAILABLE: &str = "unavailable";
    pub const TIMEOUT: &str = "timeout";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

impl ErrorBody {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Message {
    Req {
        v: u16,
        id: u64,
        method: String,
        #[serde(default)]
        params: Value,
    },
    Res {
        v: u16,
        id: u64,
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<ErrorBody>,
    },
    Evt {
        v: u16,
        method: String,
        #[serde(default)]
        params: Value,
    },
}

impl Message {
    pub fn request(id: u64, method: &str, params: Value) -> Self {
        Self::Req {
            v: PROTO_MAJOR,
            id,
            method: method.to_owned(),
            params,
        }
    }

    pub fn response(id: u64, result: Value) -> Self {
        Self::Res {
            v: PROTO_MAJOR,
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(id: u64, error: ErrorBody) -> Self {
        Self::Res {
            v: PROTO_MAJOR,
            id,
            ok: false,
            result: None,
            error: Some(error),
        }
    }

    pub fn event(method: &str, params: Value) -> Self {
        Self::Evt {
            v: PROTO_MAJOR,
            method: method.to_owned(),
            params,
        }
    }
}

pub fn encode(message: &Message) -> io::Result<Vec<u8>> {
    let body = serde_json::to_vec(message).map_err(io::Error::other)?;
    if body.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame is {} bytes, limit is {MAX_FRAME}", body.len()),
        ));
    }
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

pub fn decode(body: &[u8]) -> io::Result<Message> {
    serde_json::from_slice(body).map_err(io::Error::other)
}

pub fn write_message<W: Write>(writer: &mut W, message: &Message) -> io::Result<()> {
    writer.write_all(&encode(message)?)
}

pub fn read_message<R: Read>(reader: &mut R) -> io::Result<Option<Message>> {
    let mut header = [0u8; 4];
    let mut filled = 0;
    while filled < header.len() {
        match reader.read(&mut header[filled..])? {
            0 if filled == 0 => return Ok(None),
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "connection closed inside a frame header",
                ));
            }
            count => filled += count,
        }
    }
    let len = u32::from_le_bytes(header) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame header announces {len} bytes, limit is {MAX_FRAME}"),
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    decode(&body).map(Some)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn request_round_trip() {
        let message = Message::request(7, "game.state", json!({"screen": "menu"}));
        let bytes = encode(&message).expect("encode");
        let decoded = decode(&bytes[4..]).expect("decode");
        assert_eq!(decoded, message);
    }

    #[test]
    fn response_and_event_round_trip() {
        let mut buffer = Vec::new();
        let response = Message::response(1, json!({"ok": true}));
        let failure = Message::failure(2, ErrorBody::new(code::DENIED, "not allowed"));
        let event = Message::event("mods.loaded", json!({"count": 3}));
        for message in [&response, &failure, &event] {
            write_message(&mut buffer, message).expect("write");
        }
        let mut reader = buffer.as_slice();
        for message in [&response, &failure, &event] {
            let decoded = read_message(&mut reader).expect("read");
            assert_eq!(decoded.as_ref(), Some(message));
        }
        assert!(read_message(&mut reader).expect("read").is_none());
    }

    #[test]
    fn clean_close_reports_no_message() {
        let mut empty: &[u8] = &[];
        assert!(read_message(&mut empty).expect("read").is_none());
    }

    #[test]
    fn truncated_header_and_body_are_errors() {
        let mut short_header: &[u8] = &[1, 0];
        assert!(read_message(&mut short_header).is_err());

        let body = serde_json::to_vec(&Message::event("x", json!({}))).expect("json");
        let mut frame = (body.len() as u32).to_le_bytes().to_vec();
        frame.extend_from_slice(&body[..body.len() - 1]);
        assert!(read_message(&mut frame.as_slice()).is_err());
    }

    #[test]
    fn oversized_frames_are_rejected_both_ways() {
        let huge = json!({ "blob": "x".repeat(MAX_FRAME) });
        let message = Message::event("x", huge);
        assert!(encode(&message).is_err());

        let mut frame = u32::try_from(MAX_FRAME + 1).unwrap().to_le_bytes().to_vec();
        frame.extend_from_slice(b"{}");
        assert!(read_message(&mut frame.as_slice()).is_err());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let body = br#"{"kind":"evt","v":1,"method":"x","params":{},"future":"ignored"}"#;
        let message = decode(body).expect("decode");
        assert_eq!(message, Message::event("x", json!({})));
    }

    #[test]
    fn missing_params_defaults_to_null() {
        let body = br#"{"kind":"req","v":1,"id":9,"method":"ping"}"#;
        let message = decode(body).expect("decode");
        assert_eq!(message, Message::request(9, "ping", Value::Null));
    }
}
