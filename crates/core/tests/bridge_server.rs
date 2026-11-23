//! Loopback bridge host tests with a fake guest: random loopback endpoint, handshake, framing, permissions and lifecycle.

use std::collections::BTreeMap;
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use cuhub_core::bridge::command::{BridgeSettings, Permission, Permissions};
use cuhub_core::bridge::handshake::{HandshakeError, TOKEN};
use cuhub_core::bridge::proto::{self, MAX_FRAME, Message, PROTO_MAJOR};
use cuhub_core::bridge::server::{BridgeError, BridgeEvent, BridgeOptions, BridgeServer, HOST};
use cuhub_core::logs::Logs;

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "cuhub-bridge-test-{tag}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    server: BridgeServer,
    logs: Arc<Logs>,
    app_data: TempDir,
    port: u16,
}

impl Fixture {
    fn new(tag: &str, options: BridgeOptions, settings: &BridgeSettings) -> Self {
        let app_data = TempDir::new(tag);
        let logs = Arc::new(Logs::start(app_data.path()));
        let server = BridgeServer::start_at(
            0,
            Permissions::from_settings(settings),
            options,
            logs.clone(),
        )
        .expect("start bridge");
        let endpoint: SocketAddr = server
            .status()
            .endpoint
            .as_deref()
            .expect("bridge endpoint")
            .parse()
            .expect("bridge endpoint address");
        Self {
            server,
            logs,
            app_data,
            port: endpoint.port(),
        }
    }
}

fn connect(port: u16) -> TcpStream {
    let stream = TcpStream::connect((HOST, port)).expect("connect bridge");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    stream
}

fn hello(token: &str, pid: u32, major: u16, guid: &str) -> Message {
    Message::request(
        1,
        "hello",
        json!({
            "token": token,
            "proto": { "major": major, "minor": 0 },
            "caps": [],
            "game": { "pid": pid, "unity_version": "2022.3.62f1", "bepinex_version": "5.4.23" },
            "mods": [{ "guid": guid, "version": "0.1.0" }],
        }),
    )
}

fn send_hello(stream: &mut TcpStream, token: &str, pid: u32, major: u16, guid: &str) -> Message {
    proto::write_message(stream, &hello(token, pid, major, guid)).expect("write hello");
    proto::read_message(stream)
        .expect("read hello reply")
        .expect("hello reply present")
}

fn hello_ok(reply: Message) -> Value {
    match reply {
        Message::Res {
            ok, result, error, ..
        } => {
            assert!(ok, "hello failed: {error:?}");
            result.expect("hello result")
        }
        other => panic!("unexpected hello reply: {other:?}"),
    }
}

fn spawn_responder(mut stream: TcpStream) -> (JoinHandle<()>, Receiver<(String, Value)>) {
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let mut reader = stream.try_clone().expect("clone client stream");
        loop {
            match proto::read_message(&mut reader) {
                Ok(Some(Message::Req {
                    id, method, params, ..
                })) => {
                    if method == "cmd.invoke" {
                        let _ = sender.send((method.clone(), params.clone()));
                    }
                    let result = if method == "ping" {
                        json!({})
                    } else {
                        json!({ "echo": params })
                    };
                    if proto::write_message(&mut stream, &Message::response(id, result)).is_err() {
                        break;
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }
    });
    (handle, receiver)
}

fn wait_until(mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if check() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    check()
}

fn drain_events(server: &BridgeServer) -> Vec<BridgeEvent> {
    let mut events = Vec::new();
    while let Some(event) = server.try_recv() {
        events.push(event);
    }
    events
}

#[test]
fn handshake_lists_commands_and_round_trips_a_command() {
    let fixture = Fixture::new(
        "roundtrip",
        BridgeOptions::default(),
        &BridgeSettings::default(),
    );
    assert!(fixture.server.status().running);
    let expected_endpoint = format!("{HOST}:{}", fixture.port);
    assert_eq!(
        fixture.server.status().endpoint.as_deref(),
        Some(expected_endpoint.as_str())
    );

    let mut stream = connect(fixture.port);
    let result = hello_ok(send_hello(
        &mut stream,
        TOKEN,
        4242,
        PROTO_MAJOR,
        "cuhub.test",
    ));
    assert_eq!(result["host_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(result["commands"].as_array().map(Vec::len), Some(15));
    assert_eq!(result["perms"]["mp.state"], "granted");
    assert_eq!(result["perms"]["mp.host"], "denied");
    assert_eq!(result["perms"]["save.save"], "denied");
    assert!(wait_until(|| fixture.server.status().session.is_some()));
    assert_eq!(fixture.server.status().last_error, None);

    let (_responder, commands) = spawn_responder(stream);
    let value = fixture
        .server
        .invoke("mp.state", json!({ "from": "test" }))
        .expect("invoke");
    assert_eq!(value["echo"]["id"], "mp.state");
    assert_eq!(value["echo"]["params"]["from"], "test");
    let (method, params) = commands
        .recv_timeout(Duration::from_secs(2))
        .expect("command frame");
    assert_eq!(method, "cmd.invoke");
    assert_eq!(params["nonce"], "1");
    assert!(params["nonce"].as_str().is_some());

    let status = fixture.server.status();
    assert_eq!(
        status.session.as_ref().map(|session| session.pid),
        Some(4242)
    );
    assert!(
        fixture
            .logs
            .entries()
            .iter()
            .any(|line| line.contains("mp.state"))
    );

    fixture.server.stop();
    assert!(!fixture.server.status().running);
}

#[test]
fn handshake_waits_for_the_guest_to_send_hello() {
    let fixture = Fixture::new(
        "delayed-hello",
        BridgeOptions::default(),
        &BridgeSettings::default(),
    );
    let mut stream = connect(fixture.port);
    thread::sleep(Duration::from_millis(100));

    hello_ok(send_hello(
        &mut stream,
        TOKEN,
        4243,
        PROTO_MAJOR,
        "cuhub.delayed",
    ));
    assert!(wait_until(|| fixture.server.status().session.is_some()));
}

#[test]
fn bad_token_is_rejected_with_a_failure_and_recorded_in_the_status() {
    let fixture = Fixture::new(
        "bad-token",
        BridgeOptions::default(),
        &BridgeSettings::default(),
    );
    let mut stream = connect(fixture.port);
    proto::write_message(
        &mut stream,
        &hello(&"a".repeat(64), 7, PROTO_MAJOR, "cuhub.test"),
    )
    .expect("write hello");
    match proto::read_message(&mut stream)
        .expect("read bad token reply")
        .expect("reply present")
    {
        Message::Res {
            ok,
            error: Some(error),
            ..
        } => {
            assert!(!ok);
            assert_eq!(error.code, "bad_token");
        }
        other => panic!("unexpected reply to bad token: {other:?}"),
    }
    assert!(wait_until(
        || fixture.server.status().last_error == Some(HandshakeError::BadToken)
    ));

    let mut stream = connect(fixture.port);
    hello_ok(send_hello(&mut stream, TOKEN, 7, PROTO_MAJOR, "cuhub.test"));
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

#[test]
fn proto_major_mismatch_reports_both_versions_and_recovers() {
    let fixture = Fixture::new(
        "proto",
        BridgeOptions::default(),
        &BridgeSettings::default(),
    );
    let mut stream = connect(fixture.port);
    let reply = send_hello(&mut stream, TOKEN, 9, PROTO_MAJOR + 1, "cuhub.test");
    match reply {
        Message::Res {
            ok,
            error: Some(error),
            ..
        } => {
            assert!(!ok);
            assert_eq!(error.code, "proto_mismatch");
            assert!(error.message.contains(&PROTO_MAJOR.to_string()));
            assert!(error.message.contains(&(PROTO_MAJOR + 1).to_string()));
        }
        other => panic!("unexpected proto mismatch reply: {other:?}"),
    }
    assert!(matches!(
        proto::read_message(&mut stream),
        Ok(None) | Err(_)
    ));
    assert!(wait_until(|| fixture.server.status().last_error
        == Some(HandshakeError::ProtocolMismatch {
            host: PROTO_MAJOR,
            guest: PROTO_MAJOR + 1,
        })));
    assert!(fixture.server.status().running);

    let mut stream = connect(fixture.port);
    hello_ok(send_hello(&mut stream, TOKEN, 9, PROTO_MAJOR, "cuhub.test"));
    assert!(wait_until(|| fixture.server.status().session.is_some()));
    assert_eq!(fixture.server.status().last_error, None);
}

#[test]
fn partial_frame_is_rejected_and_the_listener_recovers() {
    let fixture = Fixture::new(
        "partial",
        BridgeOptions::default(),
        &BridgeSettings::default(),
    );
    let mut stream = connect(fixture.port);
    stream.write_all(&10u32.to_le_bytes()).expect("header");
    stream.write_all(&[1, 2, 3]).expect("partial body");
    drop(stream);

    let mut stream = connect(fixture.port);
    hello_ok(send_hello(
        &mut stream,
        TOKEN,
        11,
        PROTO_MAJOR,
        "cuhub.test",
    ));
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

#[test]
fn oversized_frame_ends_the_session_and_allows_reconnect() {
    let fixture = Fixture::new(
        "oversized",
        BridgeOptions::default(),
        &BridgeSettings::default(),
    );
    let mut stream = connect(fixture.port);
    hello_ok(send_hello(
        &mut stream,
        TOKEN,
        12,
        PROTO_MAJOR,
        "cuhub.test",
    ));
    stream
        .write_all(&((MAX_FRAME as u32) + 1).to_le_bytes())
        .expect("oversized header");

    assert!(wait_until(|| fixture.server.status().session.is_none()));
    let events = drain_events(&fixture.server);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, BridgeEvent::SessionEnded { pid: 12, .. }))
    );

    let mut stream = connect(fixture.port);
    hello_ok(send_hello(
        &mut stream,
        TOKEN,
        13,
        PROTO_MAJOR,
        "cuhub.test",
    ));
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

#[test]
fn unknown_command_is_rejected_before_any_session() {
    let fixture = Fixture::new(
        "unknown",
        BridgeOptions::default(),
        &BridgeSettings::default(),
    );
    let error = fixture
        .server
        .invoke("mp.unknown", json!({}))
        .expect_err("unknown command");
    assert!(matches!(error, BridgeError::UnknownCommand(ref name) if name == "mp.unknown"));
    let error = fixture
        .server
        .invoke("save.save", json!({}))
        .expect_err("not connected");
    assert!(matches!(error, BridgeError::NotConnected));
}

#[test]
fn default_permissions_deny_control_and_mutate() {
    let fixture = Fixture::new(
        "denied",
        BridgeOptions::default(),
        &BridgeSettings::default(),
    );
    let mut stream = connect(fixture.port);
    hello_ok(send_hello(
        &mut stream,
        TOKEN,
        21,
        PROTO_MAJOR,
        "cuhub.test",
    ));
    assert!(wait_until(|| fixture.server.status().session.is_some()));
    let (_responder, _commands) = spawn_responder(stream);

    assert!(fixture.server.invoke("mp.state", json!({})).is_ok());
    assert!(matches!(
        fixture.server.invoke("mp.host", json!({})),
        Err(BridgeError::Denied(ref name)) if name == "mp.host"
    ));
    assert!(matches!(
        fixture.server.invoke("save.save", json!({})),
        Err(BridgeError::Denied(ref name)) if name == "save.save"
    ));
}

#[test]
fn setting_permissions_live_grants_control_in_the_running_session() {
    let fixture = Fixture::new("live", BridgeOptions::default(), &BridgeSettings::default());
    let mut stream = connect(fixture.port);
    hello_ok(send_hello(
        &mut stream,
        TOKEN,
        23,
        PROTO_MAJOR,
        "cuhub.test",
    ));
    assert!(wait_until(|| fixture.server.status().session.is_some()));
    let (_responder, _commands) = spawn_responder(stream);

    assert!(matches!(
        fixture.server.invoke("mp.host", json!({ "address": "127.0.0.1:7777" })),
        Err(BridgeError::Denied(ref name)) if name == "mp.host"
    ));

    let entries = BTreeMap::from([(
        "cuhub.test".to_owned(),
        BTreeMap::from([
            ("mp.host".to_owned(), Permission::Granted),
            ("mp.join".to_owned(), Permission::Granted),
        ]),
    )]);
    fixture.server.set_permissions(Permissions::new(entries));

    let value = fixture
        .server
        .invoke("mp.host", json!({ "address": "127.0.0.1:7777" }))
        .expect("live grant");
    assert_eq!(value["echo"]["id"], "mp.host");
}

#[test]
fn granted_settings_permission_allows_a_mutation() {
    let mut settings = BridgeSettings::default();
    settings.permissions.insert(
        "cuhub.test".to_owned(),
        BTreeMap::from([("save.save".to_owned(), Permission::Granted)]),
    );
    let fixture = Fixture::new("granted", BridgeOptions::default(), &settings);
    let mut stream = connect(fixture.port);
    let result = hello_ok(send_hello(
        &mut stream,
        TOKEN,
        22,
        PROTO_MAJOR,
        "cuhub.test",
    ));
    assert_eq!(result["perms"]["save.save"], "granted");
    assert!(wait_until(|| fixture.server.status().session.is_some()));
    let (_responder, _commands) = spawn_responder(stream);
    let value = fixture
        .server
        .invoke("save.save", json!({}))
        .expect("granted");
    assert_eq!(value["echo"]["id"], "save.save");
}

#[test]
fn command_times_out_without_a_reply() {
    let options = BridgeOptions {
        request_timeout: Duration::from_millis(200),
        ..BridgeOptions::default()
    };
    let fixture = Fixture::new("timeout", options, &BridgeSettings::default());
    let mut stream = connect(fixture.port);
    hello_ok(send_hello(
        &mut stream,
        TOKEN,
        31,
        PROTO_MAJOR,
        "cuhub.test",
    ));
    assert!(wait_until(|| fixture.server.status().session.is_some()));
    let error = fixture
        .server
        .invoke("mp.state", json!({}))
        .expect_err("timeout");
    assert!(
        matches!(error, BridgeError::Timeout),
        "unexpected error: {error:?}"
    );
}

#[test]
fn missed_heartbeats_close_the_session() {
    let options = BridgeOptions {
        heartbeat: Duration::from_millis(80),
        handshake_timeout: Duration::from_secs(1),
        missed_pongs: 2,
        ..BridgeOptions::default()
    };
    let fixture = Fixture::new("heartbeat", options, &BridgeSettings::default());
    let mut stream = connect(fixture.port);
    hello_ok(send_hello(
        &mut stream,
        TOKEN,
        41,
        PROTO_MAJOR,
        "cuhub.test",
    ));

    let mut ended = false;
    let closed = wait_until(|| {
        for event in drain_events(&fixture.server) {
            if let BridgeEvent::SessionEnded { pid: 41, reason } = event {
                ended = reason == "heartbeat timeout";
            }
        }
        fixture.server.status().session.is_none() && ended
    });
    assert!(closed && ended);
}

#[test]
fn disconnected_client_allows_a_new_session() {
    let fixture = Fixture::new(
        "reconnect",
        BridgeOptions::default(),
        &BridgeSettings::default(),
    );
    let mut stream = connect(fixture.port);
    hello_ok(send_hello(
        &mut stream,
        TOKEN,
        51,
        PROTO_MAJOR,
        "cuhub.test",
    ));
    assert!(wait_until(|| fixture.server.status().session.is_some()));
    stream.shutdown(std::net::Shutdown::Both).expect("shutdown");
    assert!(wait_until(|| fixture.server.status().session.is_none()));

    let mut stream = connect(fixture.port);
    hello_ok(send_hello(
        &mut stream,
        TOKEN,
        52,
        PROTO_MAJOR,
        "cuhub.test",
    ));
    assert!(wait_until(|| {
        fixture
            .server
            .status()
            .session
            .as_ref()
            .map(|session| session.pid)
            == Some(52)
    }));
}

#[test]
fn a_second_connection_is_rejected_while_busy() {
    let fixture = Fixture::new("busy", BridgeOptions::default(), &BridgeSettings::default());
    let mut first = connect(fixture.port);
    hello_ok(send_hello(&mut first, TOKEN, 61, PROTO_MAJOR, "cuhub.test"));

    let mut second = connect(fixture.port);
    match send_hello(&mut second, TOKEN, 62, PROTO_MAJOR, "cuhub.test") {
        Message::Res {
            ok,
            error: Some(error),
            ..
        } => {
            assert!(!ok);
            assert_eq!(error.code, "busy");
        }
        other => panic!("unexpected reply while busy: {other:?}"),
    }
    let mut duplicate = connect(fixture.port);
    match send_hello(&mut duplicate, TOKEN, 61, PROTO_MAJOR, "cuhub.test") {
        Message::Res {
            ok,
            error: Some(error),
            ..
        } => {
            assert!(!ok);
            assert_eq!(error.code, "duplicate_pid");
        }
        other => panic!("unexpected duplicate pid reply: {other:?}"),
    }
    assert_eq!(
        fixture
            .server
            .status()
            .session
            .as_ref()
            .map(|session| session.pid),
        Some(61)
    );
    let _ = first.shutdown(std::net::Shutdown::Both);
}

#[test]
fn a_second_host_reports_the_port_in_use() {
    let fixture = Fixture::new("port", BridgeOptions::default(), &BridgeSettings::default());
    let logs = Arc::new(Logs::start(fixture.app_data.path()));
    let error = match BridgeServer::start_at(
        fixture.port,
        Permissions::default(),
        BridgeOptions::default(),
        logs,
    ) {
        Ok(_) => panic!("second host must fail"),
        Err(error) => error,
    };
    assert_eq!(error, BridgeError::PortInUse(fixture.port));
    assert!(fixture.server.status().running);
}
