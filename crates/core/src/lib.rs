//! Cuprum domain logic. No UI, no window state, no nsh dependency.

mod atomic;

pub mod adopt;
pub mod bridge;
pub mod catalogs;
pub mod easytier;
pub mod github;
pub mod hub;
pub mod hubconfig;
pub mod install;
pub mod launch;
pub mod links;
pub mod logs;
pub mod metadata;
pub mod modlist;
pub mod mods;
pub mod multiplayer;
pub mod network;
pub mod nexus;
pub mod nexus_auth;
pub mod packages;
pub mod paths;
pub mod platform;
pub mod profiles;
pub mod room;
pub mod settings;
pub mod source_choice;
pub mod sources;
pub mod steam;
pub mod store;
pub mod thunderstore;
pub mod version;

#[cfg(test)]
mod test_support;
