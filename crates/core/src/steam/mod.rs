//! Steam discovery: KeyValues parsing plus manifest-driven library and install lookup.

mod library;
mod vdf;

pub use library::{
    SteamGameInstall, candidate_roots, find_casualties_unknown, find_game_install,
    find_installed_games, find_libraries,
};
pub use vdf::VdfNode;
