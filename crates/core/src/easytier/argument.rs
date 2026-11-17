//! Ported from burningtnt/Terracotta@694980b `src/easytier/argument.rs`, plus the argument assembly in `src/easytier/executable_impl.rs`.
//! Local changes: assembly extracted into `command_arguments`; flag strings and order are unchanged.
use std::borrow::Cow;
use std::ffi::OsString;
use std::net::{Ipv4Addr, SocketAddr};

type CowString = Cow<'static, str>;

#[derive(Clone, Debug)]
pub struct PortForward {
    pub(crate) local: SocketAddr,
    pub(crate) remote: SocketAddr,
    pub(crate) proto: Proto,
}

#[derive(Clone, Debug)]
pub enum Proto {
    TCP,
    UDP,
}

impl Proto {
    pub fn name(&self) -> &'static str {
        match self {
            Proto::TCP => "tcp",
            Proto::UDP => "udp",
        }
    }
}

#[derive(Clone)]
pub enum Argument {
    NoTun,
    Compression(CowString),
    MultiThread,
    LatencyFirst,
    EnableKcpProxy,
    NetworkName(CowString),
    NetworkSecret(CowString),
    PublicServer(CowString),
    Listener { address: SocketAddr, proto: Proto },
    PortForward(PortForward),
    DHCP,
    HostName(CowString),
    IPv4(Ipv4Addr),
    TcpWhitelist(u16),
    UdpWhitelist(u16),
    P2POnly,
}

pub fn command_arguments(arguments: Vec<Argument>) -> Vec<OsString> {
    let mut built: Vec<OsString> = Vec::with_capacity(arguments.len() * 2);
    for argument in arguments {
        match argument {
            Argument::NoTun => built.push("--no-tun".into()),
            Argument::Compression(method) => built.push(format!("--compression={method}").into()),
            Argument::MultiThread => built.push("--multi-thread".into()),
            Argument::LatencyFirst => built.push("--latency-first".into()),
            Argument::EnableKcpProxy => built.push("--enable-kcp-proxy".into()),
            Argument::PublicServer(server) => built.extend(["-p".into(), server.as_ref().into()]),
            Argument::NetworkName(name) => {
                built.extend(["--network-name".into(), name.as_ref().into()])
            }
            Argument::NetworkSecret(secret) => {
                built.extend(["--network-secret".into(), secret.as_ref().into()])
            }
            Argument::Listener { address, proto } => built.extend([
                "-l".into(),
                format!("{}://{}", proto.name(), address).into(),
            ]),
            Argument::PortForward(PortForward {
                local,
                remote,
                proto,
            }) => {
                built.push(format!("--port-forward={}://{}/{}", proto.name(), local, remote).into())
            }
            Argument::DHCP => built.push("-d".into()),
            Argument::HostName(name) => built.extend(["--hostname".into(), name.as_ref().into()]),
            Argument::IPv4(address) => built.extend(["--ipv4".into(), address.to_string().into()]),
            Argument::TcpWhitelist(port) => built.push(format!("--tcp-whitelist={port}").into()),
            Argument::UdpWhitelist(port) => built.push(format!("--udp-whitelist={port}").into()),
            Argument::P2POnly => built.push("--p2p-only".into()),
        }
    }
    built
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembles_network_arguments() {
        let arguments = command_arguments(vec![
            Argument::NetworkName("room".into()),
            Argument::NetworkSecret("secret".into()),
            Argument::PublicServer("tcp://public.easytier.top:11010".into()),
            Argument::IPv4(Ipv4Addr::new(10, 0, 0, 1)),
        ]);
        assert_eq!(
            arguments,
            vec![
                OsString::from("--network-name"),
                OsString::from("room"),
                OsString::from("--network-secret"),
                OsString::from("secret"),
                OsString::from("-p"),
                OsString::from("tcp://public.easytier.top:11010"),
                OsString::from("--ipv4"),
                OsString::from("10.0.0.1"),
            ]
        );
    }

    #[test]
    fn assembles_listener_and_forward_arguments() {
        let arguments = command_arguments(vec![
            Argument::Listener {
                address: "127.0.0.1:11010".parse().unwrap(),
                proto: Proto::TCP,
            },
            Argument::PortForward(PortForward {
                local: "127.0.0.1:25565".parse().unwrap(),
                remote: "10.0.0.1:25565".parse().unwrap(),
                proto: Proto::UDP,
            }),
            Argument::TcpWhitelist(25565),
            Argument::UdpWhitelist(25565),
            Argument::NoTun,
            Argument::DHCP,
            Argument::P2POnly,
            Argument::MultiThread,
        ]);
        assert_eq!(
            arguments,
            vec![
                OsString::from("-l"),
                OsString::from("tcp://127.0.0.1:11010"),
                OsString::from("--port-forward=udp://127.0.0.1:25565/10.0.0.1:25565"),
                OsString::from("--tcp-whitelist=25565"),
                OsString::from("--udp-whitelist=25565"),
                OsString::from("--no-tun"),
                OsString::from("-d"),
                OsString::from("--p2p-only"),
                OsString::from("--multi-thread"),
            ]
        );
    }
}
