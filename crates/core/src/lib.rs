//! Cuhub domain logic. No UI, no window state, no nsh dependency.

pub mod catalogs;
pub mod github;
pub mod hub;
pub mod hubconfig;
pub mod launch;
pub mod logs;
pub mod metadata;
pub mod modlist;
pub mod mods;
pub mod network;
pub mod nexus;
pub mod nexus_auth;
pub mod paths;
pub mod platform;
pub mod settings;
pub mod source_choice;
pub mod sources;
pub mod steam;
pub mod version;

#[cfg(test)]
mod test_support;
