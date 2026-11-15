//! The seven pages behind the shell navigation; data comes from the Hub facade.

mod bepinex;
mod delete;
mod local;
mod misc;
mod nexus;
mod remote;
mod settings;
mod shared;

pub use bepinex::bepinex_overlay;
pub use delete::delete_overlay;
pub use local::local_mods;
pub use misc::{hub_home, multiplayer, skins_backups};
pub use nexus::nexus;
pub use remote::remote_mods;
pub use settings::settings;
