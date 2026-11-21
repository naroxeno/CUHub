//! Game room port policy: which port to host on, whether it is free, and when a reported listening port is stable enough to follow.

use std::io;
use std::net::{Ipv4Addr, TcpListener, UdpSocket};
use std::time::{Duration, Instant};

pub const DEFAULT_GAME_PORT: u16 = 7790;
pub const SAMPLE_INTERVAL: Duration = Duration::from_millis(350);
pub const CONFIRM_SAMPLES: u8 = 2;
pub const CHANGE_WINDOW: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortSource {
    Reported,
    Manual,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostPort {
    pub port: u16,
    pub source: PortSource,
}

pub fn choose_host_port(reported: Option<u16>, manual: Option<u16>) -> HostPort {
    if let Some(port) = reported.filter(|port| *port != 0) {
        return HostPort {
            port,
            source: PortSource::Reported,
        };
    }
    if let Some(port) = manual.filter(|port| *port != 0) {
        return HostPort {
            port,
            source: PortSource::Manual,
        };
    }
    HostPort {
        port: DEFAULT_GAME_PORT,
        source: PortSource::Default,
    }
}

#[derive(Debug)]
pub enum PortError {
    Invalid,
    InUse(u16),
    Io { port: u16, source: io::Error },
}

impl std::fmt::Display for PortError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid => formatter.write_str("The port must be between 1 and 65535."),
            Self::InUse(port) => write!(formatter, "Port {port} is already in use."),
            Self::Io { port, source } => write!(formatter, "Cannot check port {port}: {source}"),
        }
    }
}

impl std::error::Error for PortError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Invalid | Self::InUse(_) => None,
            Self::Io { source, .. } => Some(source),
        }
    }
}

pub fn ensure_port_available(port: u16) -> Result<(), PortError> {
    if port == 0 {
        return Err(PortError::Invalid);
    }
    // Holds both sockets until the check finishes so neither protocol can be claimed in between.
    let _tcp = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))
        .map_err(|source| bind_error(port, source))?;
    let _udp = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))
        .map_err(|source| bind_error(port, source))?;
    Ok(())
}

fn bind_error(port: u16, source: io::Error) -> PortError {
    if source.kind() == io::ErrorKind::AddrInUse {
        PortError::InUse(port)
    } else {
        PortError::Io { port, source }
    }
}

#[derive(Debug, Default)]
pub struct PortWatch {
    seen: Option<Watch>,
}

#[derive(Debug, Clone, Copy)]
struct Watch {
    port: u16,
    samples: u8,
    started: Instant,
    last: Instant,
}

impl Watch {
    fn new(port: u16, now: Instant) -> Self {
        Self {
            port,
            samples: 1,
            started: now,
            last: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDecision {
    Idle,
    Settled,
    Pending,
    Confirmed(u16),
    TimedOut(u16),
}

impl PortWatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.seen = None;
    }

    pub fn observe(&mut self, current: u16, sample: Option<u16>, now: Instant) -> PortDecision {
        let Some(port) = sample else {
            self.seen = None;
            return PortDecision::Idle;
        };
        if port == current {
            self.seen = None;
            return PortDecision::Settled;
        }
        let Some(watch) = &mut self.seen else {
            self.seen = Some(Watch::new(port, now));
            return PortDecision::Pending;
        };
        if now.saturating_duration_since(watch.started) >= CHANGE_WINDOW {
            self.seen = None;
            return PortDecision::TimedOut(port);
        }
        if watch.port != port {
            *watch = Watch {
                port,
                samples: 1,
                started: watch.started,
                last: now,
            };
            return PortDecision::Pending;
        }
        if now.saturating_duration_since(watch.last) >= SAMPLE_INTERVAL {
            watch.samples = watch.samples.saturating_add(1);
            watch.last = now;
            if watch.samples >= CONFIRM_SAMPLES {
                self.seen = None;
                return PortDecision::Confirmed(port);
            }
        }
        PortDecision::Pending
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortUpdate {
    Idle,
    Settled,
    Restarted { port: u16 },
    Failed { port: u16, message: String },
    Unsettled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reported_port_wins_over_manual() {
        let choice = choose_host_port(Some(24444), Some(25565));
        assert_eq!(choice.port, 24444);
        assert_eq!(choice.source, PortSource::Reported);
    }

    #[test]
    fn manual_port_is_the_fallback() {
        let choice = choose_host_port(None, Some(25566));
        assert_eq!(choice.port, 25566);
        assert_eq!(choice.source, PortSource::Manual);
    }

    #[test]
    fn default_is_the_last_resort() {
        let choice = choose_host_port(None, None);
        assert_eq!(choice.port, DEFAULT_GAME_PORT);
        assert_eq!(choice.source, PortSource::Default);
    }

    #[test]
    fn zero_is_not_a_choice() {
        assert_eq!(
            choose_host_port(Some(0), Some(25566)),
            HostPort {
                port: 25566,
                source: PortSource::Manual
            }
        );
        assert_eq!(
            choose_host_port(Some(0), Some(0)),
            HostPort {
                port: DEFAULT_GAME_PORT,
                source: PortSource::Default
            }
        );
    }

    #[test]
    fn rejects_port_zero() {
        assert!(matches!(ensure_port_available(0), Err(PortError::Invalid)));
    }

    #[test]
    fn finds_a_free_port() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(ensure_port_available(port).is_ok());
    }

    #[test]
    fn reports_a_tcp_port_in_use() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(matches!(
            ensure_port_available(port),
            Err(PortError::InUse(busy)) if busy == port
        ));
    }

    #[test]
    fn reports_a_udp_port_in_use() {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let port = socket.local_addr().unwrap().port();
        assert!(matches!(
            ensure_port_available(port),
            Err(PortError::InUse(busy)) if busy == port
        ));
    }

    #[test]
    fn steady_port_is_settled() {
        let mut watch = PortWatch::new();
        let now = Instant::now();
        assert_eq!(
            watch.observe(25565, Some(25565), now),
            PortDecision::Settled
        );
        assert_eq!(
            watch.observe(25565, Some(25565), now + Duration::from_secs(1)),
            PortDecision::Settled
        );
    }

    #[test]
    fn unreadable_port_is_idle() {
        let mut watch = PortWatch::new();
        assert_eq!(
            watch.observe(25565, None, Instant::now()),
            PortDecision::Idle
        );
    }

    #[test]
    fn changed_port_needs_two_spaced_samples() {
        let mut watch = PortWatch::new();
        let now = Instant::now();
        assert_eq!(
            watch.observe(25565, Some(24444), now),
            PortDecision::Pending
        );
        assert_eq!(
            watch.observe(25565, Some(24444), now + Duration::from_millis(100)),
            PortDecision::Pending
        );
        assert_eq!(
            watch.observe(25565, Some(24444), now + SAMPLE_INTERVAL),
            PortDecision::Confirmed(24444)
        );
    }

    #[test]
    fn a_moving_port_restarts_confirmation() {
        let mut watch = PortWatch::new();
        let now = Instant::now();
        assert_eq!(
            watch.observe(25565, Some(24444), now),
            PortDecision::Pending
        );
        assert_eq!(
            watch.observe(25565, Some(24445), now + SAMPLE_INTERVAL),
            PortDecision::Pending
        );
        assert_eq!(
            watch.observe(25565, Some(24445), now + SAMPLE_INTERVAL * 2),
            PortDecision::Confirmed(24445)
        );
    }

    #[test]
    fn port_returning_to_the_current_one_settles() {
        let mut watch = PortWatch::new();
        let now = Instant::now();
        assert_eq!(
            watch.observe(25565, Some(24444), now),
            PortDecision::Pending
        );
        assert_eq!(
            watch.observe(25565, Some(25565), now + SAMPLE_INTERVAL),
            PortDecision::Settled
        );
        assert_eq!(
            watch.observe(25565, Some(24444), now + SAMPLE_INTERVAL * 2),
            PortDecision::Pending
        );
        assert_eq!(
            watch.observe(25565, Some(24444), now + SAMPLE_INTERVAL * 3),
            PortDecision::Confirmed(24444)
        );
    }

    #[test]
    fn changing_port_forever_times_out() {
        let mut watch = PortWatch::new();
        let now = Instant::now();
        assert_eq!(
            watch.observe(25565, Some(24444), now),
            PortDecision::Pending
        );
        let late = now + CHANGE_WINDOW;
        assert_eq!(
            watch.observe(25565, Some(24445), late),
            PortDecision::TimedOut(24445)
        );
        assert_eq!(
            watch.observe(25565, Some(24446), late + SAMPLE_INTERVAL),
            PortDecision::Pending
        );
    }
}
