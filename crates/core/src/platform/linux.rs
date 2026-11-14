//! Non-Windows defaults: XDG data root and the user download directory.

use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

pub fn app_data_root() -> PathBuf {
    let base = dirs::data_local_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join(".local").join("share")))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"));
    base.join("CUHub")
}

pub fn steam_candidate_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(data) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        roots.push(PathBuf::from(data).join("Steam"));
    }
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".local").join("share").join("Steam"));
        roots.push(home.join(".steam").join("steam"));
        roots.push(home.join(".steam").join("root"));
        roots.push(
            home.join(".var")
                .join("app")
                .join("com.valvesoftware.Steam")
                .join("data")
                .join("Steam"),
        );
    }
    roots.push(PathBuf::from("/usr/local/games/Steam"));
    roots.push(PathBuf::from("/usr/games/Steam"));
    roots
}

pub fn downloads_dir() -> PathBuf {
    dirs::download_dir()
        .filter(|path| path.is_dir())
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn open_url(url: &str) -> io::Result<Child> {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(target_os = "macos"))]
    let program = "xdg-open";
    Command::new(program)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

pub fn steam_executable() -> PathBuf {
    PathBuf::from("steam")
}

pub fn steam_app_launch(app_id: &str) -> io::Result<Child> {
    Command::new(steam_executable())
        .arg("-applaunch")
        .arg(app_id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}
