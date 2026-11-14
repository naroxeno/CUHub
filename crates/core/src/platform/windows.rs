//! Windows defaults: local app data root and the Downloads known folder.

use std::path::PathBuf;

pub fn app_data_root() -> PathBuf {
    let base = dirs::data_local_dir()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("C:\\"));
    base.join("CUHub")
}

pub fn steam_candidate_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(folder) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
            roots.push(PathBuf::from(folder).join("Steam"));
        }
    }
    roots
}

pub fn downloads_dir() -> PathBuf {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("C:\\"))
}
