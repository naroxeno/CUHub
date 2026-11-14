//! Platform defaults behind same-name functions.

#[cfg(not(windows))]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
pub use linux::{
    app_data_root, downloads_dir, open_url, steam_app_launch, steam_candidate_roots,
    steam_executable,
};
#[cfg(windows)]
pub use windows::{
    app_data_root, downloads_dir, open_url, steam_app_launch, steam_candidate_roots,
    steam_executable,
};
