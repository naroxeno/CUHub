//! Path discovery, real-case resolution and containment checks. No writes, no caches.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

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
    #[cfg(not(windows))]
    let exact = parent.join(name);
    #[cfg(not(windows))]
    if exact.exists() {
        return Some(exact);
    }
    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(_) => {
            let exact = parent.join(name);
            return exact.exists().then_some(exact);
        }
    };
    let mut folded = None;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        if file_name == name {
            return Some(entry.path());
        }
        if folded.is_none() && file_name.to_string_lossy().eq_ignore_ascii_case(name) {
            folded = Some(entry.path());
        }
    }
    folded
}

pub(crate) fn same_directory(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    #[cfg(windows)]
    return matches!((left.canonicalize(), right.canonicalize()), (Ok(left), Ok(right)) if left == right);
    #[cfg(not(windows))]
    false
}

pub(crate) fn same_target(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    #[cfg(windows)]
    {
        let (Some(left_parent), Some(right_parent)) = (left.parent(), right.parent()) else {
            return false;
        };
        let names = left
            .file_name()
            .and_then(|name| name.to_str())
            .zip(right.file_name().and_then(|name| name.to_str()));
        names.is_some_and(|(left_name, right_name)| {
            left_name.eq_ignore_ascii_case(right_name)
                && same_directory(left_parent, right_parent)
                && (left_name == right_name
                    || (left.symlink_metadata().is_ok()
                        && right.symlink_metadata().is_ok()
                        && find_child(left_parent, left_name)
                            .zip(find_child(right_parent, right_name))
                            .is_some_and(|(left, right)| left.file_name() == right.file_name())))
        })
    }
    #[cfg(not(windows))]
    false
}

pub fn resolve_child(parent: &Path, name: &str) -> PathBuf {
    find_child(parent, name).unwrap_or_else(|| parent.join(name))
}

pub(crate) fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    metadata.file_type().is_symlink()
}

pub fn is_inside(candidate: &Path, container: &Path) -> bool {
    let (Some(candidate), Some(container)) = (resolve_leaf(candidate), canonical_path(container))
    else {
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

pub fn game_paths(selected: &Path) -> Result<PathBuf, PathError> {
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
    let game_root = match name.as_deref() {
        Some("plugins") => selected
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| PathError::Missing(selected.clone()))?
            .to_path_buf(),
        Some("bepinex") => selected
            .parent()
            .ok_or_else(|| PathError::Missing(selected.clone()))?
            .to_path_buf(),
        _ => selected,
    };
    Ok(game_root)
}

fn resolve_leaf(path: &Path) -> Option<PathBuf> {
    let absolute = std::path::absolute(path).ok()?;
    let name = absolute.file_name()?.to_os_string();
    let mut resolved = canonical_path(absolute.parent()?)?;
    resolved.push(name);
    normalize(&resolved)
}

fn canonical_path(path: &Path) -> Option<PathBuf> {
    let absolute = std::path::absolute(path).ok()?;
    let mut existing = absolute.as_path();
    let mut missing: Vec<std::ffi::OsString> = Vec::new();
    let canonical = loop {
        if let Ok(canonical) = fs::canonicalize(existing) {
            break canonical;
        }
        missing.push(existing.file_name()?.to_os_string());
        existing = existing.parent()?;
    };
    let mut resolved = canonical;
    for name in missing.iter().rev() {
        resolved.push(name);
    }
    normalize(&resolved)
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
        assert_eq!(game_paths(&root).unwrap(), root);
        assert_eq!(game_paths(&bep).unwrap(), root);
        assert_eq!(game_paths(&plugins).unwrap(), root);
        assert!(game_paths(&dir.join("missing")).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn game_paths_accepts_a_root_without_bepinex() {
        let dir = temp_dir("paths-game-vanilla");
        let root = dir.join("Casualties Unknown");
        fs::create_dir_all(&root).unwrap();
        assert_eq!(game_paths(&root).unwrap(), root);
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn containment_follows_directory_symlinks_but_keeps_a_symlinked_leaf() {
        use std::os::unix::fs::symlink;

        let dir = temp_dir("paths-symlink");
        let container = dir.join("Plugins");
        let outside = dir.join("outside");
        fs::create_dir_all(&container).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("Real.dll"), b"x").unwrap();
        symlink(&outside, container.join("link")).unwrap();
        assert!(!is_inside(
            &container.join("link").join("Real.dll"),
            &container
        ));
        symlink(outside.join("Real.dll"), container.join("Leaf.dll")).unwrap();
        assert!(is_inside(&container.join("Leaf.dll"), &container));
        let _ = fs::remove_dir_all(dir);
    }
}
