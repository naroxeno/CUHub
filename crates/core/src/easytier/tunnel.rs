//! EasyTier tunnel facade. Guests replace the core when the forwarding destination changes, because this crate has no EasyTier gRPC linkage.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;

use crate::easytier::argument::{Argument, PortForward, Proto};
use crate::easytier::process::{EasyTierProcess, LaunchConfig, ProcessError, spawn};
use crate::easytier::status::{EasyTierMember, parse_peers};
use crate::logs::Logs;
use crate::room::{HostCandidate, RoomCode, discover_host, host_hostname};

pub const DEFAULT_RELAYS: [&str; 4] = [
    "tcp://public.easytier.top:11010",
    "tcp://public2.easytier.cn:54321",
    "https://etnode.zkitefly.eu.org/node1",
    "https://etnode.zkitefly.eu.org/node2",
];

const HOST_ADDRESS: Ipv4Addr = Ipv4Addr::new(10, 144, 144, 1);
const STATUS_MESSAGE: &str = "cannot parse the EasyTier peer list";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Host,
    Guest,
}

pub struct TunnelConfig {
    pub core: PathBuf,
    pub cli: PathBuf,
    pub room: RoomCode,
    pub role: Role,
    pub port: u16,
    pub relays: Vec<String>,
}

#[derive(Debug)]
pub enum TunnelError {
    Process(ProcessError),
    Status(String),
    Io { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for TunnelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Process(error) => write!(formatter, "{error}"),
            Self::Status(message) => formatter.write_str(message),
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for TunnelError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Process(error) => Some(error),
            Self::Status(_) => None,
            Self::Io { source, .. } => Some(source),
        }
    }
}

impl From<ProcessError> for TunnelError {
    fn from(error: ProcessError) -> Self {
        Self::Process(error)
    }
}

pub(crate) fn host_arguments(room: &RoomCode, relays: &[String], port: u16) -> Vec<Argument> {
    let mut arguments = base_arguments(room, relays);
    arguments.push(Argument::HostName(host_hostname(port).into()));
    arguments.push(Argument::IPv4(HOST_ADDRESS));
    arguments.push(Argument::TcpWhitelist(port));
    arguments.push(Argument::UdpWhitelist(port));
    arguments
}

pub(crate) fn guest_arguments(
    room: &RoomCode,
    relays: &[String],
    port: u16,
    forward: Option<SocketAddr>,
) -> Vec<Argument> {
    let mut arguments = base_arguments(room, relays);
    arguments.push(Argument::DHCP);
    arguments.push(Argument::NoTun);
    if let Some(host) = forward {
        for proto in [Proto::TCP, Proto::UDP] {
            arguments.push(Argument::PortForward(PortForward {
                local: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
                remote: host,
                proto,
            }));
        }
    }
    arguments
}

fn local_forward_port() -> io::Result<u16> {
    for _ in 0..16 {
        let tcp = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = tcp.local_addr()?.port();
        match UdpSocket::bind((Ipv4Addr::LOCALHOST, port)) {
            Ok(_) => return Ok(port),
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AddrInUse,
        "no local TCP/UDP forwarding port",
    ))
}

fn base_arguments(room: &RoomCode, relays: &[String]) -> Vec<Argument> {
    let mut arguments = Vec::with_capacity(relays.len() + 8);
    for relay in relays {
        arguments.push(Argument::PublicServer(relay.clone().into()));
    }
    arguments.push(Argument::NetworkName(room.network_name().into()));
    arguments.push(Argument::NetworkSecret(room.network_secret().into()));
    arguments.push(Argument::MultiThread);
    arguments.push(Argument::LatencyFirst);
    arguments
}

pub struct Tunnel {
    process: EasyTierProcess,
    role: Role,
    port: u16,
    room: RoomCode,
    core: PathBuf,
    cli: PathBuf,
    relays: Vec<String>,
    host: Option<HostCandidate>,
    local_port: Option<u16>,
    missed_host: u8,
    missed_rpc: u8,
    last_remote: Option<SocketAddr>,
    logs: Arc<Logs>,
}

impl Tunnel {
    pub fn start(config: TunnelConfig, logs: Arc<Logs>) -> Result<Self, TunnelError> {
        let TunnelConfig {
            core,
            cli,
            room,
            role,
            port,
            relays,
        } = config;
        let arguments = match role {
            Role::Host => host_arguments(&room, &relays, port),
            Role::Guest => guest_arguments(&room, &relays, port, None),
        };
        let process = spawn(
            LaunchConfig {
                core: core.clone(),
                cli: cli.clone(),
                arguments,
            },
            Arc::clone(&logs),
        )?;
        Ok(Self {
            process,
            role,
            port,
            room,
            core,
            cli,
            relays,
            host: None,
            local_port: None,
            missed_host: 0,
            missed_rpc: 0,
            last_remote: None,
            logs,
        })
    }

    pub fn restart(&mut self, port: u16) -> Result<(), TunnelError> {
        let arguments = match self.role {
            Role::Host => host_arguments(&self.room, &self.relays, port),
            Role::Guest => guest_arguments(&self.room, &self.relays, port, None),
        };
        self.replace_process(arguments)?;
        self.port = port;
        self.host = None;
        self.local_port = None;
        self.missed_host = 0;
        self.last_remote = None;
        Ok(())
    }

    fn replace_process(&mut self, arguments: Vec<Argument>) -> Result<(), TunnelError> {
        self.process.kill();
        self.missed_rpc = 0;
        self.process = spawn(
            LaunchConfig {
                core: self.core.clone(),
                cli: self.cli.clone(),
                arguments,
            },
            Arc::clone(&self.logs),
        )?;
        Ok(())
    }

    pub fn poll(&mut self) -> Result<Option<HostCandidate>, TunnelError> {
        if self.role == Role::Host {
            return Ok(None);
        }
        let peers = match self.members() {
            Ok(peers) => {
                self.missed_rpc = 0;
                peers
            }
            Err(_) if self.missed_rpc < 2 => {
                self.missed_rpc += 1;
                return Ok(self.host.clone());
            }
            Err(error) => return Err(error),
        };
        let port_override = (self.port != 0).then_some(self.port);
        let discovered = match discover_host(&peers, port_override) {
            Ok(discovered) => discovered,
            Err(()) => {
                self.process.kill();
                self.host = None;
                return Err(TunnelError::Status(
                    "Multiple hosts found. Use the host-port option to disambiguate.".into(),
                ));
            }
        };
        let Some((host, remote_port)) = discovered else {
            self.missed_host = self.missed_host.saturating_add(1);
            if self.missed_host >= 3 && self.last_remote.take().is_some() {
                self.host = None;
                self.replace_process(guest_arguments(&self.room, &self.relays, 0, None))?;
            }
            return Ok(self.host.clone());
        };
        self.missed_host = 0;
        let remote = SocketAddr::from((host.address, remote_port));
        let unchanged = self.last_remote == Some(remote);
        if !unchanged {
            let local_port = match self.local_port {
                Some(port) => port,
                None => {
                    local_forward_port().map_err(|error| TunnelError::Status(error.to_string()))?
                }
            };
            let arguments = guest_arguments(&self.room, &self.relays, local_port, Some(remote));
            self.replace_process(arguments)?;
            self.local_port = Some(local_port);
            self.last_remote = Some(remote);
            self.host = None;
            return Ok(None);
        }
        self.host = Some(host.clone());
        Ok(Some(host))
    }

    pub fn forwarded_ports(&self) -> Option<(u16, u16)> {
        Some((self.local_port?, self.last_remote?.port()))
    }

    pub fn peers(&self) -> Result<Vec<EasyTierMember>, TunnelError> {
        self.members()
    }

    pub fn host(&self) -> Option<&HostCandidate> {
        self.host.as_ref()
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn is_alive(&self) -> bool {
        self.process.is_alive()
    }

    pub fn stop(&mut self) {
        self.process.kill();
    }

    fn members(&self) -> Result<Vec<EasyTierMember>, TunnelError> {
        let json = self
            .process
            .peers_json()
            .map_err(|source| TunnelError::Io {
                path: self.cli.clone(),
                source,
            })?;
        parse_peers(&json).ok_or_else(|| TunnelError::Status(STATUS_MESSAGE.to_owned()))
    }
}

#[cfg(test)]
mod tests;
