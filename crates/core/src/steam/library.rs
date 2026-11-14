//! Steam library and install discovery from Valve's own manifests. Read-only.

use std::fs;
use std::path::{Path, PathBuf};

use crate::paths::find_child;
use crate::platform;

use super::vdf::VdfNode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamGameInstall {
    pub app_id: String,
    pub install_dir: String,
    pub path: PathBuf,
    pub library_path: PathBuf,
}

pub fn candidate_roots() -> Vec<PathBuf> {
    platform::steam_candidate_roots()
}

pub fn find_libraries() -> Vec<PathBuf> {
    libraries_in(&candidate_roots())
}

pub fn find_installed_games() -> Vec<SteamGameInstall> {
    games_in(&find_libraries())
}

pub fn find_casualties_unknown() -> Option<SteamGameInstall> {
    games_in(&find_libraries()).into_iter().find(|install| {
        install
            .install_dir
            .to_ascii_lowercase()
            .contains("casualties")
    })
}

pub fn find_game_install() -> Option<PathBuf> {
    game_install_in(&find_libraries())
}

pub fn app_id_for_game(game_root: &Path) -> Option<String> {
    let target = resolve(game_root);
    let matches = |install: &SteamGameInstall| resolve(&install.path) == target;

    if let Some(install) = games_in(&find_libraries())
        .into_iter()
        .find(|install| matches(install))
    {
        return Some(install.app_id);
    }

    let steamapps = game_root.ancestors().find(|ancestor| {
        ancestor
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("steamapps"))
    })?;
    let library = steamapps.parent()?;
    games_in(&[library.to_path_buf()])
        .into_iter()
        .find(|install| matches(install))
        .map(|install| install.app_id)
}

fn game_install_in(libraries: &[PathBuf]) -> Option<PathBuf> {
    if let Some(install) = games_in(libraries).into_iter().find(|install| {
        install
            .install_dir
            .to_ascii_lowercase()
            .contains("casualties")
    }) {
        return Some(install.path);
    }
    libraries.iter().find_map(|library| {
        let common = find_child(library, "steamapps")
            .and_then(|steamapps| find_child(&steamapps, "common"))?;
        fs::read_dir(common).ok()?.flatten().find_map(|entry| {
            let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            (is_dir && name.contains("casualties")).then_some(entry.path())
        })
    })
}

fn libraries_in(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut libraries = Vec::new();
    for candidate in roots {
        if !candidate.is_dir() {
            continue;
        }
        let root = resolve(candidate);
        add_distinct(&mut libraries, root.clone());
        for vdf in [
            root.join("steamapps").join("libraryfolders.vdf"),
            root.join("config").join("libraryfolders.vdf"),
        ] {
            if !vdf.is_file() {
                continue;
            }
            for path in read_library_paths(&vdf) {
                add_distinct(&mut libraries, resolve(&path));
            }
        }
    }
    libraries
}

fn games_in(libraries: &[PathBuf]) -> Vec<SteamGameInstall> {
    let mut installs = Vec::new();
    for library in libraries {
        let Some(steamapps) = find_child(library, "steamapps") else {
            continue;
        };
        let Some(common) = find_child(&steamapps, "common") else {
            continue;
        };
        let Ok(entries) = fs::read_dir(&steamapps) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
                continue;
            }
            let Ok(text) = fs::read_to_string(entry.path()) else {
                continue;
            };
            let parsed = VdfNode::parse(&text);
            let Some(state) = parsed.children().next() else {
                continue;
            };
            let (Some(app_id), Some(install_dir)) =
                (state.child_value("appid"), state.child_value("installdir"))
            else {
                continue;
            };
            let (app_id, install_dir) = (app_id.trim(), install_dir.trim());
            if app_id.is_empty() || install_dir.is_empty() {
                continue;
            }
            let Some(path) = find_child(&common, install_dir) else {
                continue;
            };
            installs.push(SteamGameInstall {
                app_id: app_id.to_string(),
                install_dir: install_dir.to_string(),
                path,
                library_path: library.clone(),
            });
        }
    }
    installs
}

fn read_library_paths(vdf_path: &Path) -> Vec<PathBuf> {
    let Ok(text) = fs::read_to_string(vdf_path) else {
        return Vec::new();
    };
    let parsed = VdfNode::parse(&text);
    let container = parsed.children().next().unwrap_or(&parsed);
    container
        .collect_library_paths()
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .collect()
}

fn add_distinct(libraries: &mut Vec<PathBuf>, path: PathBuf) {
    let exists = libraries.iter().any(|existing| {
        #[cfg(windows)]
        {
            existing
                .to_string_lossy()
                .eq_ignore_ascii_case(&path.to_string_lossy())
        }
        #[cfg(not(windows))]
        {
            existing == &path
        }
    });
    if !exists {
        libraries.push(path);
    }
}

fn resolve(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;
    use std::path::Path;

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    fn vdf_path(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "\\\\")
    }

    #[test]
    fn finds_libraries_listed_by_the_root() {
        let dir = temp_dir("steam-libs");
        let root = dir.join("Steam");
        let second = dir.join("SecondLibrary");
        fs::create_dir_all(root.join("steamapps")).unwrap();
        fs::create_dir_all(second.join("steamapps")).unwrap();
        let text = format!(
            r#""libraryfolders"
{{
    "0"
    {{
        "path"        "{}"
    }}
    "1"
    {{
        "path"        "{}"
    }}
}}"#,
            vdf_path(&root),
            vdf_path(&second)
        );
        write(&root.join("steamapps").join("libraryfolders.vdf"), &text);

        let libraries = libraries_in(std::slice::from_ref(&root));
        assert_eq!(libraries.len(), 2);
        assert_eq!(libraries[0], fs::canonicalize(&root).unwrap());
        assert_eq!(libraries[1], fs::canonicalize(&second).unwrap());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn reads_app_manifests_and_resolves_real_folder_case() {
        let dir = temp_dir("steam-games");
        let library = dir.join("Steam");
        let steamapps = library.join("steamapps");
        let common = steamapps.join("common");
        fs::create_dir_all(common.join("casualties unknown")).unwrap();
        write(
            &steamapps.join("appmanifest_3167550.acf"),
            r#""AppState"
{
    "appid"        "3167550"
    "installdir"        "Casualties Unknown"
}"#,
        );

        let installs = games_in(std::slice::from_ref(&library));
        assert_eq!(installs.len(), 1);
        assert_eq!(installs[0].app_id, "3167550");
        assert_eq!(installs[0].install_dir, "Casualties Unknown");
        assert_eq!(installs[0].path, common.join("casualties unknown"));
        assert_eq!(
            game_install_in(std::slice::from_ref(&library)),
            Some(common.join("casualties unknown"))
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn finds_the_app_id_from_the_steamapps_ancestor() {
        let dir = temp_dir("steam-appid");
        let library = dir.join("Steam");
        let steamapps = library.join("steamapps");
        let common = steamapps.join("common");
        fs::create_dir_all(common.join("Casualties Unknown")).unwrap();
        write(
            &steamapps.join("appmanifest_4576490.acf"),
            r#""AppState"
{
    "appid"        "4576490"
    "installdir"        "Casualties Unknown"
}"#,
        );

        assert_eq!(
            app_id_for_game(&common.join("Casualties Unknown")),
            Some(String::from("4576490"))
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn falls_back_to_common_folder_names() {
        let dir = temp_dir("steam-fallback");
        let library = dir.join("Steam");
        let common = library.join("steamapps").join("common");
        fs::create_dir_all(common.join("Casualties Unknown")).unwrap();

        assert_eq!(
            game_install_in(&[library]),
            Some(common.join("Casualties Unknown"))
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    #[cfg(not(windows))]
    fn candidate_roots_cover_linux_locations() {
        let roots = candidate_roots();
        assert!(
            roots
                .iter()
                .any(|root| root.to_str() == Some("/usr/games/Steam"))
        );
        assert!(
            roots
                .iter()
                .any(|root| root.to_str() == Some("/usr/local/games/Steam"))
        );
    }
}
