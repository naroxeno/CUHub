//! The pages behind the shell navigation; data comes from the Hub facade.

mod about;
mod bepinex;
mod delete;
mod home;
mod local;
mod misc;
mod nexus;
mod remote;
mod settings;
mod shared;

pub use about::about;
pub use bepinex::bepinex_overlay;
pub use delete::delete_overlay;
pub use home::hub_home;
pub use local::local_mods;
pub use misc::{multiplayer, skins_backups};
pub use nexus::nexus;
pub use remote::remote_mods;
pub use settings::settings;
pub(crate) use settings::{language_button, zoom_button};
