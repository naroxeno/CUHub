//! Platform defaults behind same-name functions.

#[cfg(all(not(windows), not(target_os = "macos")))]
mod linux;
#[cfg(all(not(windows), target_os = "macos"))]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(all(not(windows), not(target_os = "macos")))]
pub use linux::{
    app_data_root, downloads_dir, elevated, open_url, steam_app_launch, steam_candidate_roots,
    steam_executable,
};
#[cfg(all(not(windows), target_os = "macos"))]
pub use macos::{
    app_data_root, downloads_dir, elevated, open_url, steam_app_launch, steam_candidate_roots,
    steam_executable,
};
#[cfg(windows)]
pub use windows::{
    app_data_root, downloads_dir, elevated, open_url, steam_app_launch, steam_candidate_roots,
    steam_executable,
};
