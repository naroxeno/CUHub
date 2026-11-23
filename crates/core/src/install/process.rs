//! Detects whether the game process is still running. No writes.

use std::io;
use std::path::Path;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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
        if wine_game_process(&entry.path(), game_root) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(target_os = "linux")]
fn wine_game_process(proc_root: &Path, game_root: &Path) -> bool {
    let Ok(cwd) = std::fs::read_link(proc_root.join("cwd")) else {
        return false;
    };
    if !cwd.starts_with(game_root) {
        return false;
    }
    let Ok(cmdline) = std::fs::read(proc_root.join("cmdline")) else {
        return false;
    };
    let Some(first) = cmdline.split(|byte| *byte == 0).next() else {
        return false;
    };
    let first = String::from_utf8_lossy(first);
    let Some(name) = first.rsplit(['\\', '/']).next() else {
        return false;
    };
    name.to_ascii_lowercase().ends_with(".exe") && cwd.join(name).is_file()
}

#[cfg(target_os = "windows")]
fn windows_game_running() -> io::Result<bool> {
    use std::os::windows::process::CommandExt;

    let mut command = std::process::Command::new("tasklist");
    command
        .args(["/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW);
    let output = command.output()?;
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    fn fixture(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("cuhub-wine-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("game")).unwrap();
        fs::create_dir_all(root.join("proc")).unwrap();
        root
    }

    #[test]
    fn a_wine_game_is_detected_by_its_working_directory_and_cmdline() {
        let root = fixture("game");
        let game = root.join("game");
        fs::write(game.join("CasualtiesUnknown.exe"), b"x").unwrap();
        symlink(&game, root.join("proc").join("cwd")).unwrap();
        fs::write(
            root.join("proc").join("cmdline"),
            b"S:\\steamapps\\common\\Casualties Unknown\\CasualtiesUnknown.exe\0",
        )
        .unwrap();
        assert!(wine_game_process(&root.join("proc"), &game));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_wine_helper_is_not_mistaken_for_the_game() {
        let root = fixture("helper");
        let game = root.join("game");
        fs::write(game.join("CasualtiesUnknown.exe"), b"x").unwrap();
        symlink(&game, root.join("proc").join("cwd")).unwrap();
        fs::write(
            root.join("proc").join("cmdline"),
            b"c:\\windows\\system32\\steam.exe\0",
        )
        .unwrap();
        assert!(!wine_game_process(&root.join("proc"), &game));
        let _ = fs::remove_dir_all(&root);
    }
}
