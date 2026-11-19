//! One accepted bridge connection: framing, heartbeat, request bookkeeping and events.

use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use crate::bridge::proto::{ErrorBody, MAX_FRAME, Message};
use crate::bridge::server::{BridgeError, BridgeEvent, ServerShared, close_session};
use crate::logs::Logs;

pub(crate) enum Frame {
    Message(Message),
    Ignored,
}

pub(crate) fn read_frame(
    stream: &mut TcpStream,
    buffer: &mut Vec<u8>,
) -> io::Result<Option<Frame>> {
    let mut header = [0u8; 4];
    if stream.read(&mut header[..1])? == 0 {
        return Ok(None);
    }
    stream.read_exact(&mut header[1..])?;
    let length = u32::from_le_bytes(header) as usize;
    if length > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds the 1 MiB limit",
        ));
    }
    buffer.resize(length, 0);
    stream.read_exact(buffer)?;
    match serde_json::from_slice::<Message>(buffer) {
        Ok(message) => Ok(Some(Frame::Message(message))),
        Err(_) => Ok(Some(Frame::Ignored)),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeModInfo {
    pub guid: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeSessionInfo {
    pub pid: u32,
    pub unity_version: Option<String>,
    pub bepinex_version: Option<String>,
    pub mods: Vec<BridgeModInfo>,
}

impl BridgeSessionInfo {
    pub fn mod_guids(&self) -> Vec<String> {
        self.mods.iter().map(|entry| entry.guid.clone()).collect()
    }
}

pub(crate) struct Session {
    pub info: BridgeSessionInfo,
    writer: Mutex<TcpStream>,
    pending: Mutex<HashMap<u64, Sender<Result<Value, ErrorBody>>>>,
    inflight: Mutex<HashSet<String>>,
    next_id: AtomicU64,
    next_nonce: AtomicU64,
    ping_id: Mutex<Option<u64>>,
    missed_pongs: AtomicU32,
    closed: AtomicBool,
}

impl Session {
    pub(crate) fn new(
        stream: &TcpStream,
        info: BridgeSessionInfo,
        _logs: Arc<Logs>,
    ) -> io::Result<Arc<Self>> {
        let writer = stream.try_clone()?;
        Ok(Arc::new(Self {
            info,
            writer: Mutex::new(writer),
            pending: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashSet::new()),
            next_id: AtomicU64::new(1),
            next_nonce: AtomicU64::new(1),
            ping_id: Mutex::new(None),
            missed_pongs: AtomicU32::new(0),
            closed: AtomicBool::new(false),
        }))
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    pub(crate) fn send_message(&self, message: &Message) -> io::Result<()> {
        let bytes = crate::bridge::proto::encode(message)?;
        let mut writer = lock(&self.writer);
        writer.write_all(&bytes)?;
        writer.flush()
    }

    pub(crate) fn respond(&self, id: u64, result: Value) {
        let _ = self.send_message(&Message::response(id, result));
    }

    pub(crate) fn resolve(
        &self,
        id: u64,
        ok: bool,
        result: Option<Value>,
        error: Option<ErrorBody>,
    ) {
        let sender = lock(&self.pending).remove(&id);
        if let Some(sender) = sender {
            let outcome = if ok {
                Ok(result.unwrap_or(Value::Null))
            } else {
                Err(error.unwrap_or_else(|| ErrorBody::new("unavailable", "bridge request failed")))
            };
            let _ = sender.send(outcome);
            return;
        }
        if *lock(&self.ping_id) == Some(id) {
            self.missed_pongs.store(0, Ordering::Relaxed);
        }
    }

    pub(crate) fn invoke(
        &self,
        command: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, BridgeError> {
        if self.is_closed() {
            return Err(BridgeError::NotConnected);
        }
        if !lock(&self.inflight).insert(command.to_owned()) {
            return Err(BridgeError::Busy(command.to_owned()));
        }
        let outcome = self.invoke_inner(command, params, timeout);
        lock(&self.inflight).remove(command);
        outcome
    }

    fn invoke_inner(
        &self,
        command: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, BridgeError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let nonce = self.next_nonce.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();
        lock(&self.pending).insert(id, sender);
        let request = Message::request(
            id,
            "cmd.invoke",
            json!({ "nonce": nonce.to_string(), "id": command, "params": params }),
        );
        if let Err(error) = self.send_message(&request) {
            lock(&self.pending).remove(&id);
            return Err(BridgeError::Io(error.to_string()));
        }
        match receiver.recv_timeout(timeout) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(body)) => Err(BridgeError::from_client(&body)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                lock(&self.pending).remove(&id);
                Err(BridgeError::Timeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(BridgeError::NotConnected),
        }
    }

    pub(crate) fn shutdown(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Ok(writer) = self.writer.lock() {
            let _ = writer.shutdown(Shutdown::Both);
        } else {
            let writer = self
                .writer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let _ = writer.shutdown(Shutdown::Both);
        }
        self.fail_pending("unavailable", "bridge session closed");
    }

    fn fail_pending(&self, code: &str, message: &str) {
        let pending = std::mem::take(&mut *lock(&self.pending));
        for sender in pending.into_values() {
            let _ = sender.send(Err(ErrorBody::new(code, message)));
        }
    }
}

pub(crate) fn read_loop(mut stream: TcpStream, session: Arc<Session>, shared: Arc<ServerShared>) {
    let mut buffer = Vec::new();
    let reason = loop {
        match read_frame(&mut stream, &mut buffer) {
            Ok(None) => break "client closed".to_owned(),
            Ok(Some(Frame::Ignored)) => continue,
            Ok(Some(Frame::Message(message))) => match message {
                Message::Req { id, method, .. } if method == "ping" => {
                    session.respond(id, json!({}));
                }
                Message::Req { .. } => {}
                Message::Res {
                    id,
                    ok,
                    result,
                    error,
                    ..
                } => {
                    session.resolve(id, ok, result, error);
                }
                Message::Evt { method, params, .. } => {
                    if matches!(
                        method.as_str(),
                        "game.state" | "mods.loaded" | "mods.changed"
                    ) {
                        let _ = shared.events.send(BridgeEvent::ClientEvent {
                            pid: session.info.pid,
                            method,
                            params,
                        });
                    }
                }
            },
            Err(error) => break format!("read failed: {error}"),
        }
    };
    close_session(&shared, &session, &reason);
}

pub(crate) fn heartbeat_loop(
    session: Arc<Session>,
    shared: Arc<ServerShared>,
    interval: Duration,
    limit: u32,
) {
    let mut waited = Duration::ZERO;
    loop {
        if session.is_closed() || shared.stop.load(Ordering::Relaxed) {
            break;
        }
        let slice = interval.min(Duration::from_millis(100));
        thread::sleep(slice);
        waited += slice;
        if waited < interval {
            continue;
        }
        waited = Duration::ZERO;
        if session.missed_pongs.load(Ordering::Relaxed) >= limit {
            close_session(&shared, &session, "heartbeat timeout");
            break;
        }
        let id = session.next_id.fetch_add(1, Ordering::Relaxed);
        *lock(&session.ping_id) = Some(id);
        session.missed_pongs.fetch_add(1, Ordering::Relaxed);
        if session
            .send_message(&Message::request(id, "ping", json!({})))
            .is_err()
        {
            close_session(&shared, &session, "heartbeat write failed");
            break;
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
