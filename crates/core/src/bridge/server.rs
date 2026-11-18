//! Loopback bridge host: lock file, rendezvous endpoint, handshake and session lifecycle.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::bridge::command::{self, Permissions};
use crate::bridge::handshake;
use crate::bridge::proto::{ErrorBody, PROTO_MAJOR};
use crate::bridge::rendezvous::{self, BridgeFile, Endpoint, HostInfo};
use crate::bridge::session::{self, BridgeSessionInfo, Session};
use crate::logs::Logs;

const LOCK_FILE: &str = "bridge.lock";
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

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub app_data: PathBuf,
    pub program_data: PathBuf,
    pub permissions: Permissions,
    pub options: BridgeOptions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BridgeStatus {
    pub running: bool,
    pub endpoint: Option<String>,
    pub program_data: Option<PathBuf>,
    pub session: Option<BridgeSessionInfo>,
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

#[derive(Debug)]
pub enum BridgeError {
    Locked(String),
    Io(io::Error),
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
            Self::Locked(path) => {
                write!(formatter, "Another Cuhub bridge host already holds {path}.")
            }
            Self::Io(error) => write!(formatter, "Bridge I/O error: {error}"),
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
        Self::Io(error)
    }
}

pub(crate) struct ServerShared {
    pub program_data: PathBuf,
    pub token: String,
    pub local_addr: SocketAddr,
    pub logs: Arc<Logs>,
    pub events: Sender<BridgeEvent>,
    pub session: Mutex<Option<Arc<Session>>>,
    pub permissions: Permissions,
    pub options: BridgeOptions,
    pub stop: AtomicBool,
    pub lock_file: Mutex<Option<File>>,
    pub listener_thread: Mutex<Option<JoinHandle<()>>>,
    pub workers: Mutex<Vec<JoinHandle<()>>>,
}

pub struct BridgeServer {
    shared: Arc<ServerShared>,
    events: Receiver<BridgeEvent>,
}

impl BridgeServer {
    pub fn start(config: BridgeConfig, logs: Arc<Logs>) -> Result<Self, BridgeError> {
        fs::create_dir_all(&config.app_data)?;
        let lock_path = config.app_data.join(LOCK_FILE);
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        match lock_file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(BridgeError::Locked(lock_path.display().to_string()));
            }
            Err(TryLockError::Error(error)) => return Err(BridgeError::Io(error)),
        }

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let local_addr = listener.local_addr()?;
        let token = generate_token();
        let bridge_file = BridgeFile {
            proto: PROTO_MAJOR,
            host: HostInfo {
                pid: std::process::id(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                os: std::env::consts::OS.to_owned(),
            },
            endpoint: Endpoint {
                host: rendezvous::LOOPBACK.to_owned(),
                port: local_addr.port(),
            },
            token: token.clone(),
            started_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Err(error) = bridge_file.write(&config.program_data) {
            drop(lock_file);
            return Err(BridgeError::Io(error));
        }

        let (sender, events) = mpsc::channel();
        let shared = Arc::new(ServerShared {
            program_data: config.program_data,
            token,
            local_addr,
            logs: logs.clone(),
            events: sender,
            session: Mutex::new(None),
            permissions: config.permissions,
            options: config.options,
            stop: AtomicBool::new(false),
            lock_file: Mutex::new(Some(lock_file)),
            listener_thread: Mutex::new(None),
            workers: Mutex::new(Vec::new()),
        });
        let accept_shared = shared.clone();
        let listener_thread = thread::spawn(move || {
            accept_loop(listener, accept_shared);
        });
        *lock(&shared.listener_thread) = Some(listener_thread);
        logs.activity("Bridge", &format!("Listening on {local_addr}."));
        Ok(Self { shared, events })
    }

    pub fn status(&self) -> BridgeStatus {
        let session = lock(&self.shared.session)
            .as_ref()
            .map(|session| session.info.clone());
        BridgeStatus {
            running: !self.shared.stop.load(Ordering::Relaxed),
            endpoint: Some(self.shared.local_addr.to_string()),
            program_data: Some(self.shared.program_data.clone()),
            session,
        }
    }

    pub fn invoke(&self, command: &str, params: Value) -> Result<Value, BridgeError> {
        if self.shared.stop.load(Ordering::Relaxed) {
            return Err(BridgeError::Unavailable);
        }
        let spec = command::find(command)
            .ok_or_else(|| BridgeError::UnknownCommand(command.to_owned()))?;
        let session = lock(&self.shared.session)
            .clone()
            .ok_or(BridgeError::NotConnected)?;
        let mods = session.info.mod_guids();
        if !self.shared.permissions.allows(spec, &mods) {
            return Err(BridgeError::Denied(command.to_owned()));
        }
        let started = Instant::now();
        let outcome = session.invoke(spec.id, params, self.shared.options.request_timeout);
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
        if let Err(error) = BridgeFile::remove(&self.shared.program_data) {
            self.shared.logs.error("Bridge rendezvous", &error);
        }
        if let Some(thread) = lock(&self.shared.listener_thread).take() {
            let _ = thread.join();
        }
        let workers: Vec<JoinHandle<()>> = lock(&self.shared.workers).drain(..).collect();
        for worker in workers {
            let _ = worker.join();
        }
        let lock_file = lock(&self.shared.lock_file).take();
        drop(lock_file);
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
                let _ = stream.set_nodelay(true);
                match handshake::accept(&mut stream, &shared) {
                    Ok(info) => start_session(stream, peer, info, &shared),
                    Err(reason) => {
                        shared
                            .logs
                            .activity("Bridge", &format!("Rejected {peer}: {reason}."));
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
    session.shutdown();
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
}

fn generate_token() -> String {
    use std::fmt::Write as _;

    use aes_gcm::aead::OsRng;
    use aes_gcm::aead::rand_core::RngCore;

    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let mut token = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(token, "{byte:02x}");
    }
    token
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
