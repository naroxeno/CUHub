//! Windows defaults: local app data root, Downloads, and Steam roots.

use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use winreg::HKEY;
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

pub fn app_data_root() -> PathBuf {
    let base = dirs::data_local_dir()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("C:\\"));
    base.join("Cuprum")
}

pub fn steam_candidate_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(path) =
        registry_steam_root(HKEY_CURRENT_USER, "Software\\Valve\\Steam", "SteamPath")
    {
        push_unique(&mut roots, path);
    }
    if let Some(path) = registry_steam_root(
        HKEY_LOCAL_MACHINE,
        "SOFTWARE\\WOW6432Node\\Valve\\Steam",
        "InstallPath",
    ) {
        push_unique(&mut roots, path);
    }
    if let Some(path) =
        registry_steam_root(HKEY_LOCAL_MACHINE, "SOFTWARE\\Valve\\Steam", "InstallPath")
    {
        push_unique(&mut roots, path);
    }
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(folder) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
            push_unique(&mut roots, PathBuf::from(folder).join("Steam"));
        }
    }
    roots
}

fn registry_steam_root(root: HKEY, key: &str, value: &str) -> Option<PathBuf> {
    let key = RegKey::predef(root).open_subkey(key).ok()?;
    let path: String = key.get_value(value).ok()?;
    let path = path.trim();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn push_unique(roots: &mut Vec<PathBuf>, path: PathBuf) {
    let candidate = path.to_string_lossy().to_string();
    if !roots
        .iter()
        .any(|known| known.to_string_lossy().eq_ignore_ascii_case(&candidate))
    {
        roots.push(path);
    }
}

pub fn downloads_dir() -> PathBuf {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("C:\\"))
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn IsUserAnAdmin() -> i32;
}

pub fn elevated() -> bool {
    // SAFETY: the call takes no arguments and only reads the current token.
    unsafe { IsUserAnAdmin() != 0 }
}

pub fn open_url(url: &str) -> io::Result<Child> {
    Command::new("cmd")
        .args(["/C", "start", "", url])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

pub fn steam_executable() -> PathBuf {
    for root in steam_candidate_roots() {
        if let Some(executable) = crate::paths::find_child(&root, "steam.exe") {
            return executable;
        }
    }
    PathBuf::from("steam.exe")
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
