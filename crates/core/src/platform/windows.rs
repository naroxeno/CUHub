//! Windows defaults: local app data root and the Downloads known folder.

use std::path::PathBuf;

pub fn app_data_root() -> PathBuf {
    let base = dirs::data_local_dir()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("C:\\"));
    base.join("CUHub")
}

pub fn downloads_dir() -> PathBuf {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("C:\\"))
}
