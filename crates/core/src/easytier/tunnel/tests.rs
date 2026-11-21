use std::net::Ipv4Addr;

use super::*;
use crate::easytier::argument::command_arguments;
use crate::easytier::status::parse_peers;
use crate::room::{RoomCode, find_host};

const SAMPLE: &str = "C1-7K3M-9Q2X-4H8P-V6RW";
const PORT: u16 = 25565;
const HOST_ADDRESS: Ipv4Addr = Ipv4Addr::new(10, 144, 144, 1);
const PEERS: &str = r#"[
    {"route":[{"hostname":"guest-laptop","ipv4":"10.144.144.23","cost":"Local","nat_type":"Unknown"}]},
    {"route":[{"hostname":"cuhub-host-25566","ipv4":"10.144.144.5","cost":"9.2 ms","nat_type":"FullCone"}]},
    {"route":[{"hostname":"cuhub-host-25565","ipv4":"10.144.144.1","cost":"3.1 ms","nat_type":"FullCone"}]}
]"#;

fn room() -> RoomCode {
    RoomCode::parse(SAMPLE).unwrap()
}

fn relays() -> Vec<String> {
    DEFAULT_RELAYS
        .iter()
        .map(|relay| (*relay).to_owned())
        .collect()
}

fn render(arguments: Vec<Argument>) -> Vec<String> {
    command_arguments(arguments)
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect()
}

fn has_pair(arguments: &[String], flag: &str, value: &str) -> bool {
    arguments
        .windows(2)
        .any(|pair| pair[0] == flag && pair[1] == value)
}

#[test]
fn host_arguments_whitelist_and_identity() {
    let arguments = render(host_arguments(&room(), &relays(), PORT));
    assert!(has_pair(&arguments, "--hostname", "cuhub-host-25565"));
    assert!(has_pair(&arguments, "--ipv4", "10.144.144.1"));
    assert!(has_pair(&arguments, "--network-name", "cuhub-7K3M9Q2X"));
    assert!(has_pair(&arguments, "--network-secret", "4H8PV6R"));
    assert!(arguments.contains(&"--tcp-whitelist=25565".to_owned()));
    assert!(arguments.contains(&"--udp-whitelist=25565".to_owned()));
    let published: Vec<String> = arguments
        .windows(2)
        .filter(|pair| pair[0] == "-p")
        .map(|pair| pair[1].clone())
        .collect();
    assert_eq!(published, relays());
}

#[test]
fn guest_arguments_add_no_forward_until_host_known() {
    let without = render(guest_arguments(&room(), &relays(), PORT, None));
    assert!(without.iter().any(|arg| arg == "-d"));
    assert!(without.iter().any(|arg| arg == "--no-tun"));
    assert!(
        !without
            .iter()
            .any(|argument| argument.starts_with("--port-forward"))
    );
    let with = render(guest_arguments(
        &room(),
        &relays(),
        25003,
        Some(SocketAddr::from((HOST_ADDRESS, PORT))),
    ));
    let forwards: Vec<String> = with
        .iter()
        .filter(|argument| argument.starts_with("--port-forward"))
        .cloned()
        .collect();
    assert_eq!(
        forwards,
        vec![
            "--port-forward=tcp://127.0.0.1:25003/10.144.144.1:25565".to_owned(),
            "--port-forward=udp://127.0.0.1:25003/10.144.144.1:25565".to_owned(),
        ]
    );
    assert_eq!(with.len(), without.len() + 2);
}

#[test]
fn relay_defaults_are_present() {
    assert_eq!(DEFAULT_RELAYS.len(), 4);
    for relay in DEFAULT_RELAYS {
        assert!(!relay.is_empty());
        assert!(
            relay.starts_with("tcp://") || relay.starts_with("https://"),
            "{relay}"
        );
    }
}

#[test]
fn discovery_is_left_to_room() {
    let peers = parse_peers(PEERS).unwrap();
    assert_eq!(peers.len(), 3);
    assert_eq!(peers[0].hostname, "guest-laptop");
    assert_eq!(peers[0].address, Some(Ipv4Addr::new(10, 144, 144, 23)));
    assert!(peers[0].is_local);
    assert!(!peers[2].is_local);
    let host = find_host(&peers, PORT).unwrap();
    assert_eq!(host.hostname, "cuhub-host-25565");
    assert_eq!(host.address, HOST_ADDRESS);
}

#[cfg(unix)]
fn restart_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("cuhub-tunnel-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(unix)]
fn fake_core(
    dir: &std::path::Path,
    args: &std::path::Path,
    executable: bool,
) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let core = dir.join("fake-core.sh");
    let script = format!(
        r#"#!/bin/sh
printf '%s\n' "$@" > '{}'
sleep 30
"#,
        args.display()
    );
    std::fs::write(&core, script).unwrap();
    let mode = if executable { 0o755 } else { 0o644 };
    std::fs::set_permissions(&core, std::fs::Permissions::from_mode(mode)).unwrap();
    core
}

#[cfg(unix)]
fn wait_for_arguments(path: &std::path::Path, needle: &str) -> Vec<String> {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(text) = std::fs::read_to_string(path) {
            let lines: Vec<String> = text.lines().map(str::to_owned).collect();
            if lines.iter().any(|line| line == needle) {
                return lines;
            }
        }
        assert!(
            Instant::now() < deadline,
            "the fake core never recorded {needle}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn host_tunnel(dir: &std::path::Path, core: &std::path::Path) -> Tunnel {
    let logs = std::sync::Arc::new(crate::logs::Logs::start(dir));
    Tunnel::start(
        TunnelConfig {
            core: core.to_path_buf(),
            cli: core.to_path_buf(),
            room: room(),
            role: Role::Host,
            port: PORT,
            relays: relays(),
        },
        logs,
    )
    .unwrap()
}

#[cfg(unix)]
#[test]
fn restart_moves_the_port_and_keeps_the_room() {
    let dir = restart_dir("restart-move");
    let args = dir.join("args.txt");
    let core = fake_core(&dir, &args, true);
    let mut tunnel = host_tunnel(&dir, &core);
    let before = wait_for_arguments(&args, "cuhub-host-25565");
    assert!(has_pair(&before, "--network-name", "cuhub-7K3M9Q2X"));
    tunnel.restart(PORT + 1).unwrap();
    let after = wait_for_arguments(&args, "cuhub-host-25566");
    assert!(has_pair(&after, "--network-name", "cuhub-7K3M9Q2X"));
    assert!(has_pair(&after, "--network-secret", "4H8PV6R"));
    assert!(after.contains(&"--tcp-whitelist=25566".to_owned()));
    assert!(after.contains(&"--udp-whitelist=25566".to_owned()));
    assert!(!after.contains(&"--tcp-whitelist=25565".to_owned()));
    assert_eq!(tunnel.port(), PORT + 1);
    tunnel.stop();
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn restart_failure_keeps_the_old_port() {
    let dir = restart_dir("restart-failure");
    let args = dir.join("args.txt");
    let core = fake_core(&dir, &args, true);
    let mut tunnel = host_tunnel(&dir, &core);
    wait_for_arguments(&args, "cuhub-host-25565");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&core, std::fs::Permissions::from_mode(0o644)).unwrap();
    let error = tunnel.restart(PORT + 1).unwrap_err();
    assert!(matches!(error, TunnelError::Process(_)));
    assert_eq!(tunnel.port(), PORT);
    tunnel.stop();
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[test]
fn guest_discovers_and_retargets_without_reserving_the_hosts_port() {
    use std::os::unix::fs::PermissionsExt;

    let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let host_port = occupied.local_addr().unwrap().port();
    let dir = restart_dir("guest-retarget");
    let args = dir.join("args.txt");
    let core = fake_core(&dir, &args, true);
    let peers = dir.join("peers.json");
    let cli = dir.join("fake-cli.sh");
    std::fs::write(&cli, format!("#!/bin/sh\ncat '{}'\n", peers.display())).unwrap();
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
    let advertise = |port| {
        std::fs::write(&peers, format!(
            r#"[{{"route":[{{"hostname":"cuhub-host-{port}","ipv4":"10.42.0.7","cost":"1 ms","nat_type":"Unknown"}}]}}]"#
        )).unwrap();
    };
    advertise(host_port);
    let logs = Arc::new(Logs::start(&dir));
    let mut tunnel = Tunnel::start(
        TunnelConfig {
            core,
            cli,
            room: room(),
            role: Role::Guest,
            port: 0,
            relays: relays(),
        },
        logs,
    )
    .unwrap();
    assert!(tunnel.poll().unwrap().is_none());
    let (local, remote) = tunnel.forwarded_ports().unwrap();
    assert_eq!(remote, host_port);
    assert_ne!(local, host_port);
    wait_for_arguments(
        &args,
        &format!("--port-forward=udp://127.0.0.1:{local}/10.42.0.7:{host_port}"),
    );
    assert!(tunnel.poll().unwrap().is_some());

    let next = if host_port == 24444 { 24445 } else { 24444 };
    advertise(next);
    assert!(tunnel.poll().unwrap().is_none());
    wait_for_arguments(
        &args,
        &format!("--port-forward=udp://127.0.0.1:{local}/10.42.0.7:{next}"),
    );
    assert!(tunnel.poll().unwrap().is_some());
    assert_eq!(tunnel.forwarded_ports(), Some((local, next)));

    std::fs::write(&peers, "[]").unwrap();
    assert!(tunnel.poll().unwrap().is_some());
    assert!(tunnel.poll().unwrap().is_some());
    assert!(tunnel.poll().unwrap().is_none());
    assert_eq!(tunnel.forwarded_ports(), None);
    tunnel.stop();
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn automatic_discovery_rejects_ambiguous_hosts() {
    let peers = parse_peers(PEERS).unwrap();
    assert!(discover_host(&peers, None).is_err());
    let (host, port) = discover_host(&peers, Some(25566)).unwrap().unwrap();
    assert_eq!(host.address, Ipv4Addr::new(10, 144, 144, 5));
    assert_eq!(port, 25566);
}
