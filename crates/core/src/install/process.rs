//! Detects whether the game process is still running. No writes.

use std::io;
use std::path::Path;

pub fn game_running(game_root: &Path) -> io::Result<bool> {
    #[cfg(target_os = "linux")]
    {
        linux_game_running(game_root)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = game_root;
        windows_game_running()
    }
    #[cfg(target_os = "macos")]
    {
        let _ = game_root;
        macos_game_running()
    }
}

#[cfg(target_os = "linux")]
fn linux_game_running(game_root: &Path) -> io::Result<bool> {
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
            .is_none()
        {
            continue;
        }
        let Ok(target) = std::fs::read_link(entry.path().join("exe")) else {
            continue;
        };
        if target.starts_with(game_root) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(target_os = "windows")]
fn windows_game_running() -> io::Result<bool> {
    let output = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    Ok(text.contains("casualtiesunknown.exe"))
}

#[cfg(target_os = "macos")]
fn macos_game_running() -> io::Result<bool> {
    let status = std::process::Command::new("pgrep")
        .args(["-f", "CasualtiesUnknown"])
        .status()?;
    Ok(status.success())
}
