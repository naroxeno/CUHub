//! macOS defaults: Application Support data root and the Steam roots CrossOver uses.

use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

pub fn app_data_root() -> PathBuf {
    let base = dirs::data_local_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Library").join("Application Support")))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"));
    base.join("CUHub")
}

pub fn steam_candidate_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        roots.push(
            home.join("Library")
                .join("Application Support")
                .join("Steam"),
        );
        roots.push(home.join(".steam").join("steam"));
    }
    roots
}

pub fn downloads_dir() -> PathBuf {
    dirs::download_dir()
        .filter(|path| path.is_dir())
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn elevated() -> bool {
    true
}

pub fn open_url(url: &str) -> io::Result<Child> {
    Command::new("open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

pub fn steam_executable() -> PathBuf {
    PathBuf::from("open")
}

pub fn steam_app_launch(app_id: &str) -> io::Result<Child> {
    Command::new(steam_executable())
        .arg(format!("steam://rungameid/{app_id}"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}
