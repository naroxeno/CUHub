//! Path discovery, real-case resolution and containment checks. No writes, no caches.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamePaths {
    pub game_root: PathBuf,
    pub plugins_dir: PathBuf,
}

#[derive(Debug)]
pub enum PathError {
    Missing(PathBuf),
    Io { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for PathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(path) => write!(formatter, "{} does not exist", path.display()),
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for PathError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Missing(_) => None,
            Self::Io { source, .. } => Some(source),
        }
    }
}

pub fn find_child(parent: &Path, name: &str) -> Option<PathBuf> {
    let exact = parent.join(name);
    if exact.exists() {
        return Some(exact);
    }
    let entries = fs::read_dir(parent).ok()?;
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(name)
        {
            return Some(entry.path());
        }
    }
    None
}

pub fn resolve_child(parent: &Path, name: &str) -> PathBuf {
    find_child(parent, name).unwrap_or_else(|| parent.join(name))
}

pub fn is_inside(candidate: &Path, container: &Path) -> bool {
    let (Some(candidate), Some(container)) = (normalize(candidate), normalize(container)) else {
        return false;
    };
    let candidate: Vec<Component<'_>> = candidate.components().collect();
    let container: Vec<Component<'_>> = container.components().collect();
    if candidate.len() < container.len() {
        return false;
    }
    candidate
        .iter()
        .zip(&container)
        .all(|(a, b)| same_component(a, b))
}

pub fn game_paths(selected: &Path) -> Result<GamePaths, PathError> {
    let selected = std::path::absolute(selected).map_err(|source| PathError::Io {
        path: selected.to_path_buf(),
        source,
    })?;
    if !selected.is_dir() {
        return Err(PathError::Missing(selected));
    }
    let name = selected
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase());
    let (game_root, plugins_dir) = match name.as_deref() {
        Some("plugins") => {
            let bep = selected
                .parent()
                .ok_or_else(|| PathError::Missing(selected.clone()))?;
            let root = bep
                .parent()
                .ok_or_else(|| PathError::Missing(bep.to_path_buf()))?;
            (root.to_path_buf(), selected)
        }
        Some("bepinex") => {
            let root = selected
                .parent()
                .ok_or_else(|| PathError::Missing(selected.clone()))?;
            let plugins = find_child(&selected, "Plugins")
                .ok_or_else(|| PathError::Missing(selected.join("Plugins")))?;
            (root.to_path_buf(), plugins)
        }
        _ => {
            let bep = find_child(&selected, "BepInEx")
                .ok_or_else(|| PathError::Missing(selected.join("BepInEx")))?;
            let plugins = find_child(&bep, "Plugins")
                .ok_or_else(|| PathError::Missing(bep.join("Plugins")))?;
            (selected, plugins)
        }
    };
    if !plugins_dir.is_dir() {
        return Err(PathError::Missing(plugins_dir));
    }
    Ok(GamePaths {
        game_root,
        plugins_dir,
    })
}

fn normalize(path: &Path) -> Option<PathBuf> {
    let absolute = std::path::absolute(path).ok()?;
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Some(normalized)
}

fn same_component(a: &Component<'_>, b: &Component<'_>) -> bool {
    #[cfg(windows)]
    {
        a.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    #[test]
    fn find_child_matches_case_insensitively_and_returns_real_case() {
        let dir = temp_dir("paths-find");
        fs::create_dir_all(dir.join("BepInEx")).unwrap();
        let found = find_child(&dir, "bepinex").unwrap();
        assert_eq!(found.file_name().unwrap(), "BepInEx");
        assert!(find_child(&dir, "missing").is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn resolve_child_keeps_requested_case_when_missing() {
        let dir = temp_dir("paths-resolve");
        assert_eq!(resolve_child(&dir, "Plugins"), dir.join("Plugins"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn is_inside_rejects_siblings_and_parent_escapes() {
        let dir = temp_dir("paths-inside");
        let container = dir.join("b");
        let sibling = dir.join("bc");
        assert!(is_inside(&container.join("c"), &container));
        assert!(is_inside(&container, &container));
        assert!(!is_inside(&sibling, &container));
        assert!(!is_inside(&dir, &container));
        assert!(!is_inside(&container.join("..").join("c"), &container));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn game_paths_accepts_root_bepinex_and_plugins() {
        let dir = temp_dir("paths-game");
        let root = dir.join("Casualties Unknown");
        let bep = root.join("BepInEx");
        let plugins = bep.join("Plugins");
        fs::create_dir_all(&plugins).unwrap();
        assert_eq!(game_paths(&root).unwrap().plugins_dir, plugins);
        assert_eq!(game_paths(&bep).unwrap().game_root, root);
        assert_eq!(game_paths(&plugins).unwrap().game_root, root);
        assert!(game_paths(&dir.join("missing")).is_err());
        let _ = fs::remove_dir_all(dir);
    }
}
