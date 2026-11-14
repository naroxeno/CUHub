//! Cuhub domain logic. No UI, no window state, no nsh dependency.

pub mod catalogs;
pub mod logs;
pub mod modlist;
pub mod paths;
pub mod platform;
pub mod settings;
pub mod steam;
pub mod version;

#[cfg(test)]
mod test_support;
