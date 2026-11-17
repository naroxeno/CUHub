//! EasyTier release pinning, cached downloads and on-disk detection. No processes, no UI.

mod argument;
mod detect;
mod ports;
mod process;
mod release;
mod status;
mod tunnel;

pub use argument::{Argument, PortForward, Proto, command_arguments};
pub use detect::{Detection, Status, detect_custom, detect_managed};
pub use ports::PortRequest;
pub use process::{EasyTierProcess, LaunchConfig, ProcessError, spawn};
pub use release::{ReleaseError, VERSION, ensure_archive, ensure_installed};
pub use status::{EasyTierMember, NatType, parse_peers};
pub use tunnel::{DEFAULT_RELAYS, Role, Tunnel, TunnelConfig, TunnelError};
