//! The pages behind the shell navigation; data comes from the Hub facade.

mod about;
mod bepinex;
mod delete;
mod home;
mod local;
mod misc;
mod multiplayer;
mod nexus;
mod profiles;
mod remote;
mod settings;
mod shared;

pub use about::about;
pub use bepinex::bepinex_overlay;
pub use delete::delete_overlay;
pub use home::hub_home;
pub use local::local_mods;
pub use misc::skins_backups;
pub use multiplayer::multiplayer;
pub use nexus::nexus;
pub use profiles::profiles;
pub use remote::remote_mods;
pub use settings::settings;
pub(crate) use settings::{language_button, zoom_button};
