//! Experimental bridge host: framing, handshake, command policy and the fixed loopback endpoint.

pub const BRIDGE_GUID: &str = "dev.cuprum.bridge";

pub mod command;
pub mod handshake;
pub mod install;
pub mod mp;
pub mod proto;
pub mod server;
pub mod session;
