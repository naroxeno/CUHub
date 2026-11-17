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
    assert!(
        !without
            .iter()
            .any(|argument| argument.starts_with("--port-forward"))
    );
    let with = render(guest_arguments(
        &room(),
        &relays(),
        PORT,
        Some(HOST_ADDRESS),
    ));
    let forwards: Vec<String> = with
        .iter()
        .filter(|argument| argument.starts_with("--port-forward"))
        .cloned()
        .collect();
    assert_eq!(
        forwards,
        vec![
            "--port-forward=tcp://0.0.0.0:25565/10.144.144.1:25565".to_owned(),
            "--port-forward=udp://0.0.0.0:25565/10.144.144.1:25565".to_owned(),
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
