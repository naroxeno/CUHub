//! Windows defaults: local app data root and the Downloads known folder.

use std::io;
use std::path::PathBuf;
use std::process::Command;

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

pub fn open_url(url: &str) -> io::Result<()> {
    let status = Command::new("cmd")
        .args(["/C", "start", "", url])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("cmd exited with {status}")))
    }
}

pub fn steam_executable() -> PathBuf {
    for root in steam_candidate_roots() {
        if let Some(executable) = crate::paths::find_child(&root, "steam.exe") {
            return executable;
        }
    }
    PathBuf::from("steam.exe")
}

pub fn steam_app_launch(app_id: &str) -> io::Result<()> {
    let mut command = Command::new(steam_executable());
    let status = command.arg("-applaunch").arg(app_id).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("steam.exe exited with {status}")))
    }
}
