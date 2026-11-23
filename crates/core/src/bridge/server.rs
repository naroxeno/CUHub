//! Loopback bridge host: fixed endpoint, handshake admission and session lifecycle.

use std::io;
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::bridge::command::{self, Permissions};
use crate::bridge::handshake::{self, HandshakeError};
use crate::bridge::proto::ErrorBody;
use crate::bridge::session::{self, BridgeSessionInfo, Session};
use crate::logs::Logs;

pub const HOST: Ipv4Addr = Ipv4Addr::LOCALHOST;
pub const PORT: u16 = 41297;

const ACCEPT_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BridgeOptions {
    pub heartbeat: Duration,
    pub handshake_timeout: Duration,
    pub request_timeout: Duration,
    pub missed_pongs: u32,
}

impl Default for BridgeOptions {
    fn default() -> Self {
        Self {
            heartbeat: Duration::from_secs(10),
            handshake_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(2),
            missed_pongs: 3,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BridgeStatus {
    pub running: bool,
    pub endpoint: Option<String>,
    pub session: Option<BridgeSessionInfo>,
    pub last_error: Option<HandshakeError>,
}

#[derive(Debug, Clone)]
pub enum BridgeEvent {
    SessionStarted(BridgeSessionInfo),
    SessionEnded {
        pid: u32,
        reason: String,
    },
    ClientEvent {
        pid: u32,
        method: String,
        params: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    PortInUse(u16),
    Io(String),
    NotConnected,
    UnknownCommand(String),
    Denied(String),
    Busy(String),
    Timeout,
    Unavailable,
    Failed { code: String, message: String },
}

impl BridgeError {
    pub(crate) fn from_client(body: &ErrorBody) -> Self {
        match body.code.as_str() {
            "unknown_command" => Self::UnknownCommand(body.message.clone()),
            "denied" => Self::Denied(body.message.clone()),
            "busy" => Self::Busy(body.message.clone()),
            "timeout" => Self::Timeout,
            "unavailable" => Self::Unavailable,
            _ => Self::Failed {
                code: body.code.clone(),
                message: body.message.clone(),
            },
        }
    }
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PortInUse(port) => {
                write!(formatter, "The bridge port {port} is already in use.")
            }
            Self::Io(message) => write!(formatter, "Bridge I/O error: {message}"),
            Self::NotConnected => formatter.write_str("The game is not connected to the bridge."),
            Self::UnknownCommand(command) => {
                write!(formatter, "Unknown bridge command: {command}.")
            }
            Self::Denied(command) => {
                write!(formatter, "The bridge command {command} is not allowed.")
            }
            Self::Busy(command) => {
                write!(
                    formatter,
                    "The bridge command {command} is already running."
                )
            }
            Self::Timeout => {
                formatter.write_str("The game did not answer the bridge command in time.")
            }
            Self::Unavailable => formatter.write_str("The bridge session is not available."),
            Self::Failed { code, message } => {
                write!(
                    formatter,
                    "The bridge command failed with {code}: {message}"
                )
            }
        }
    }
}

impl std::error::Error for BridgeError {}

impl From<io::Error> for BridgeError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

pub(crate) struct ServerShared {
    pub endpoint: SocketAddr,
    pub logs: Arc<Logs>,
    pub events: Sender<BridgeEvent>,
    pub session: Mutex<Option<Arc<Session>>>,
    pub last_error: Mutex<Option<HandshakeError>>,
    pub permissions: RwLock<Permissions>,
    pub options: BridgeOptions,
    pub stop: AtomicBool,
    pub listener_thread: Mutex<Option<JoinHandle<()>>>,
    pub workers: Mutex<Vec<JoinHandle<()>>>,
}

pub struct BridgeServer {
    shared: Arc<ServerShared>,
    events: Receiver<BridgeEvent>,
}

impl BridgeServer {
    pub fn start(
        permissions: Permissions,
        options: BridgeOptions,
        logs: Arc<Logs>,
    ) -> Result<Self, BridgeError> {
        Self::start_at(PORT, permissions, options, logs)
    }

    pub fn start_at(
        port: u16,
        permissions: Permissions,
        options: BridgeOptions,
        logs: Arc<Logs>,
    ) -> Result<Self, BridgeError> {
        let requested = SocketAddr::from((HOST, port));
        let listener = match TcpListener::bind(requested) {
            Ok(listener) => listener,
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                return Err(BridgeError::PortInUse(port));
            }
            Err(error) => return Err(BridgeError::Io(error.to_string())),
        };
        let endpoint = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let (sender, events) = mpsc::channel();
        let shared = Arc::new(ServerShared {
            endpoint,
            logs: logs.clone(),
            events: sender,
            session: Mutex::new(None),
            last_error: Mutex::new(None),
            permissions: RwLock::new(permissions),
            options,
            stop: AtomicBool::new(false),
            listener_thread: Mutex::new(None),
            workers: Mutex::new(Vec::new()),
        });
        let accept_shared = shared.clone();
        let listener_thread = thread::spawn(move || {
            accept_loop(listener, accept_shared);
        });
        *lock(&shared.listener_thread) = Some(listener_thread);
        logs.activity("Bridge", &format!("Listening on {endpoint}."));
        Ok(Self { shared, events })
    }

    pub fn status(&self) -> BridgeStatus {
        let session = lock(&self.shared.session)
            .as_ref()
            .map(|session| session.info.clone());
        BridgeStatus {
            running: !self.shared.stop.load(Ordering::Relaxed),
            endpoint: Some(self.shared.endpoint.to_string()),
            session,
            last_error: lock(&self.shared.last_error).clone(),
        }
    }

    pub fn set_permissions(&self, permissions: Permissions) {
        *write(&self.shared.permissions) = permissions;
    }

    pub fn invoke(&self, command: &str, params: Value) -> Result<Value, BridgeError> {
        self.invoke_with(command, params, true)
    }

    pub(crate) fn invoke_silent(&self, command: &str, params: Value) -> Result<Value, BridgeError> {
        self.invoke_with(command, params, false)
    }

    fn invoke_with(&self, command: &str, params: Value, log: bool) -> Result<Value, BridgeError> {
        if self.shared.stop.load(Ordering::Relaxed) {
            return Err(BridgeError::Unavailable);
        }
        let spec = command::find(command)
            .ok_or_else(|| BridgeError::UnknownCommand(command.to_owned()))?;
        let session = lock(&self.shared.session)
            .clone()
            .ok_or(BridgeError::NotConnected)?;
        let mods = session.info.mod_guids();
        if !read(&self.shared.permissions).allows(spec, &mods) {
            return Err(BridgeError::Denied(command.to_owned()));
        }
        let started = Instant::now();
        let outcome = session.invoke(spec.id, params, self.shared.options.request_timeout);
        if log {
            match &outcome {
                Ok(_) => self.shared.logs.activity(
                    "Bridge",
                    &format!("Command {} completed in {:?}.", spec.id, started.elapsed()),
                ),
                Err(error) => self
                    .shared
                    .logs
                    .activity("Bridge", &format!("Command {} failed: {error}", spec.id)),
            }
        }
        outcome
    }

    pub fn try_recv(&self) -> Option<BridgeEvent> {
        self.events.try_recv().ok()
    }

    pub fn stop(&self) {
        if self.shared.stop.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(session) = lock(&self.shared.session).take() {
            session.shutdown();
        }
        if let Some(thread) = lock(&self.shared.listener_thread).take() {
            let _ = thread.join();
        }
        let workers: Vec<JoinHandle<()>> = lock(&self.shared.workers).drain(..).collect();
        for worker in workers {
            let _ = worker.join();
        }
        self.shared.logs.activity("Bridge", "Stopped.");
    }
}

impl Drop for BridgeServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn accept_loop(listener: TcpListener, shared: Arc<ServerShared>) {
    while !shared.stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((mut stream, peer)) => {
                if shared.stop.load(Ordering::Relaxed) {
                    let _ = stream.shutdown(Shutdown::Both);
                    break;
                }
                if let Err(error) = stream.set_nonblocking(false) {
                    shared.logs.error("Bridge connection", &error);
                    let _ = stream.shutdown(Shutdown::Both);
                    continue;
                }
                let _ = stream.set_nodelay(true);
                match handshake::accept(&mut stream, &shared) {
                    Ok(info) => start_session(stream, peer, info, &shared),
                    Err(error) => {
                        *lock(&shared.last_error) = Some(error.clone());
                        shared
                            .logs
                            .activity("Bridge", &format!("Rejected {peer}: {error}."));
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(ACCEPT_INTERVAL);
            }
            Err(error) => {
                shared.logs.error("Bridge listener", &error);
                thread::sleep(ACCEPT_INTERVAL);
            }
        }
    }
}

fn start_session(
    stream: TcpStream,
    peer: SocketAddr,
    info: BridgeSessionInfo,
    shared: &Arc<ServerShared>,
) {
    if shared.stop.load(Ordering::Relaxed) {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    }
    let session = match Session::new(&stream, info.clone(), shared.logs.clone()) {
        Ok(session) => session,
        Err(error) => {
            shared.logs.error("Bridge session", &error);
            let _ = stream.shutdown(Shutdown::Both);
            return;
        }
    };
    *lock(&shared.session) = Some(session.clone());
    *lock(&shared.last_error) = None;
    shared.logs.activity(
        "Bridge",
        &format!("Session started: pid {} from {peer}.", info.pid),
    );
    let _ = shared.events.send(BridgeEvent::SessionStarted(info));

    let reader_session = session.clone();
    let reader_shared = shared.clone();
    let reader = thread::spawn(move || session::read_loop(stream, reader_session, reader_shared));
    let heartbeat_session = session;
    let heartbeat_shared = shared.clone();
    let heartbeat_interval = shared.options.heartbeat;
    let missed_pongs = shared.options.missed_pongs;
    let heartbeat = thread::spawn(move || {
        session::heartbeat_loop(
            heartbeat_session,
            heartbeat_shared,
            heartbeat_interval,
            missed_pongs,
        );
    });
    lock(&shared.workers).push(reader);
    lock(&shared.workers).push(heartbeat);
}

pub(crate) fn close_session(shared: &Arc<ServerShared>, session: &Arc<Session>, reason: &str) {
    let removed = {
        let mut slot = lock(&shared.session);
        match slot.as_ref() {
            Some(current) if Arc::ptr_eq(current, session) => {
                *slot = None;
                true
            }
            _ => false,
        }
    };
    if removed {
        shared
            .logs
            .activity("Bridge", &format!("Session closed: {reason}."));
        let _ = shared.events.send(BridgeEvent::SessionEnded {
            pid: session.info.pid,
            reason: reason.to_owned(),
        });
    }
    // shutdown wakes the blocked reader; the slot is resolved first so the reader cannot replace this reason.
    session.shutdown();
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixed_endpoint_uses_the_shared_loopback_port() {
        assert_eq!(PORT, 41297);
        assert!(HOST.is_loopback());
    }

    #[test]
    fn start_refuses_the_fixed_port_when_it_is_taken() {
        let holder = TcpListener::bind(SocketAddr::from((HOST, PORT)));
        match holder {
            Ok(listener) => {
                let error = start_fixed();
                assert_eq!(error, BridgeError::PortInUse(PORT));
                drop(listener);
            }
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                let error = start_fixed();
                assert_eq!(error, BridgeError::PortInUse(PORT));
            }
            Err(error) => panic!("cannot probe the bridge port: {error}"),
        }
    }

    fn start_fixed() -> BridgeError {
        let app_data = crate::test_support::temp_dir("bridge-port");
        let logs = Arc::new(Logs::start(&app_data));
        match BridgeServer::start(Permissions::default(), BridgeOptions::default(), logs) {
            Ok(server) => {
                drop(server);
                panic!("the fixed port must be taken");
            }
            Err(error) => error,
        }
    }
}
