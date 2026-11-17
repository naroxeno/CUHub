//! EasyTier tunnel facade. Guests restart the core once with baked --port-forward flags after the host is discovered, because this crate has no EasyTier gRPC linkage.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use crate::easytier::argument::{Argument, PortForward, Proto};
use crate::easytier::process::{EasyTierProcess, LaunchConfig, ProcessError, spawn};
use crate::easytier::status::{EasyTierMember, parse_peers};
use crate::logs::Logs;
use crate::room::{HostCandidate, RoomCode, find_host, host_hostname};

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
    forward: Option<Ipv4Addr>,
) -> Vec<Argument> {
    let mut arguments = base_arguments(room, relays);
    if let Some(host) = forward {
        for proto in [Proto::TCP, Proto::UDP] {
            arguments.push(Argument::PortForward(PortForward {
                local: SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)),
                remote: SocketAddr::from((host, port)),
                proto,
            }));
        }
    }
    arguments
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
    forwarded: bool,
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
            forwarded: false,
            logs,
        })
    }

    pub fn poll(&mut self) -> Result<Option<HostCandidate>, TunnelError> {
        if self.role == Role::Host {
            return Ok(None);
        }
        if let Some(host) = &self.host {
            return Ok(Some(host.clone()));
        }
        let peers = self.members()?;
        let Some(host) = find_host(&peers, self.port) else {
            return Ok(None);
        };
        if !self.forwarded {
            self.process.kill();
            let arguments =
                guest_arguments(&self.room, &self.relays, self.port, Some(host.address));
            self.process = spawn(
                LaunchConfig {
                    core: self.core.clone(),
                    cli: self.cli.clone(),
                    arguments,
                },
                Arc::clone(&self.logs),
            )?;
            self.forwarded = true;
        }
        self.host = Some(host.clone());
        Ok(Some(host))
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
