//! Launch Casualties Unknown through Steam so Proton and BepInEx are active.

use std::io;
use std::path::{Path, PathBuf};

use crate::platform;
use crate::steam;

#[derive(Debug)]
pub enum LaunchError {
    NoAppId(PathBuf),
    CouldNotReachSteam {
        app_id: String,
        url: io::Error,
        fallback: io::Error,
    },
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAppId(path) => write!(
                formatter,
                "Casualties Unknown at {} cannot be launched through Steam because its app id was not found. Start it from your Steam library instead.",
                path.display()
            ),
            Self::CouldNotReachSteam {
                app_id,
                url,
                fallback,
            } => write!(
                formatter,
                "Could not reach Steam for app {app_id}: {url}. The steam -applaunch fallback also failed: {fallback}."
            ),
        }
    }
}

impl std::error::Error for LaunchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoAppId(_) => None,
            Self::CouldNotReachSteam { url, .. } => Some(url),
        }
    }
}

pub fn launch_game(game_root: &Path) -> Result<String, LaunchError> {
    let app_id = steam::app_id_for_game(game_root)
        .ok_or_else(|| LaunchError::NoAppId(game_root.to_path_buf()))?;
    let url = format!("steam://rungameid/{app_id}");
    match platform::open_url(&url) {
        Ok(()) => Ok(app_id),
        Err(url) => match platform::steam_app_launch(&app_id) {
            Ok(()) => Ok(app_id),
            Err(fallback) => Err(LaunchError::CouldNotReachSteam {
                app_id,
                url,
                fallback,
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;
    use std::fs;

    #[test]
    fn reports_a_missing_app_id() {
        let dir = temp_dir("launch-no-appid");
        let error = launch_game(&dir).unwrap_err();
        assert!(matches!(error, LaunchError::NoAppId(_)));
        assert!(
            error
                .to_string()
                .contains("cannot be launched through Steam")
        );
        let _ = fs::remove_dir_all(dir);
    }
}
