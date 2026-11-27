//! Ported from burningtnt/Terracotta@694980b `src/ports.rs`.
//! Local changes: file header only; port ids and the fallback offset are unchanged.
use std::io;
use std::net::{Ipv4Addr, TcpListener};

#[repr(u8)]
pub enum PortRequest {
    EasyTierRPC,
    Scaffolding,
    Minecraft,
}

impl PortRequest {
    pub fn request_specific(port: u16) -> io::Result<u16> {
        TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .and_then(|socket| socket.local_addr())
            .map(|address| address.port())
    }

    pub fn request(self) -> u16 {
        TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .and_then(|socket| socket.local_addr())
            .map(|address| address.port())
            .unwrap_or(self as u8 as u16 + 35780)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discriminants_match_the_port_table() {
        assert_eq!(PortRequest::EasyTierRPC as u8, 0);
        assert_eq!(PortRequest::Scaffolding as u8, 1);
        assert_eq!(PortRequest::Minecraft as u8, 2);
        assert_eq!(PortRequest::Minecraft as u16 + 35780, 35782);
    }

    #[test]
    fn requests_bind_loopback_ports() {
        assert_ne!(PortRequest::EasyTierRPC.request(), 0);
        assert_ne!(PortRequest::request_specific(0).unwrap(), 0);
    }
}
