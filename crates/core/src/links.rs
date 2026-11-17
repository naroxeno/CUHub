//! Deployment links: the managed entries that point the game directory at the store.
//! The index is the only authority for ownership; hardlinks cannot be traced back otherwise.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

const INDEX_FILE: &str = "Links.json";
const MAX_COPY_ENTRIES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkKind {
    Junction,
    Hardlink,
    Symlink,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkEntry {
    pub mod_id: String,
    pub version: String,
    pub kind: LinkKind,
    pub store_path: String,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LinkIndex {
    #[serde(skip)]
    file: PathBuf,
    entries: BTreeMap<String, LinkEntry>,
}

#[derive(Debug)]
pub enum LinkError {
    OutsideGame(PathBuf),
    NotManaged(PathBuf),
    MissingStore(PathBuf),
    Io { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutsideGame(path) => {
                write!(formatter, "{} is outside the game folder", path.display())
            }
            Self::NotManaged(path) => {
                write!(formatter, "{} is not deployed by Cuhub", path.display())
            }
            Self::MissingStore(path) => {
                write!(formatter, "{} is missing from the store", path.display())
            }
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for LinkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn io_error(path: &Path, source: io::Error) -> LinkError {
    LinkError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn lexical_inside(candidate: &Path, container: &Path) -> bool {
    let candidate: Vec<Component<'_>> = candidate
        .components()
        .filter(|part| !matches!(part, Component::CurDir))
        .collect();
    let container: Vec<Component<'_>> = container.components().collect();
    candidate.len() > container.len() && candidate[..container.len()] == container[..]
}

impl LinkIndex {
    pub fn load(app_data: &Path) -> Self {
        let file = app_data.join(INDEX_FILE);
        match fs::read_to_string(&file) {
            Ok(raw) => match serde_json::from_str::<Self>(&raw) {
                Ok(mut index) => {
                    index.file = file;
                    index
                }
                Err(_) => Self {
                    file,
                    entries: BTreeMap::new(),
                },
            },
            Err(_) => Self {
                file,
                entries: BTreeMap::new(),
            },
        }
    }

    pub fn path(&self) -> &Path {
        &self.file
    }

    pub fn save(&self) -> io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        crate::atomic::replace(&self.file, format!("{json}\n").as_bytes())
    }

    pub fn get(&self, target: &Path) -> Option<&LinkEntry> {
        self.entries.get(&key(target))
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, &LinkEntry)> {
        self.entries
            .iter()
            .map(|(path, entry)| (path.as_str(), entry))
    }

    pub fn deployed_for(&self, mod_id: &str, version: &str) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(_, entry)| entry.mod_id == mod_id && entry.version == version)
            .map(|(path, _)| path.clone())
            .collect()
    }

    pub fn deploy(
        &mut self,
        game_root: &Path,
        target: &Path,
        store_source: &Path,
        mod_id: &str,
        version: &str,
    ) -> Result<LinkKind, LinkError> {
        if !lexical_inside(target, game_root) {
            return Err(LinkError::OutsideGame(target.to_path_buf()));
        }
        if !store_source.exists() {
            return Err(LinkError::MissingStore(store_source.to_path_buf()));
        }
        if target.exists() || target.symlink_metadata().is_ok() {
            self.remove(game_root, target)?;
        }
        let (kind, files) = if store_source.is_dir() {
            deploy_dir(target, store_source)?
        } else {
            (deploy_file(target, store_source)?, Vec::new())
        };
        self.entries.insert(
            key(target),
            LinkEntry {
                mod_id: mod_id.to_owned(),
                version: version.to_owned(),
                kind,
                store_path: store_source.to_string_lossy().into_owned(),
                files,
            },
        );
        Ok(kind)
    }

    pub fn remove(&mut self, game_root: &Path, target: &Path) -> Result<(), LinkError> {
        if !lexical_inside(target, game_root) {
            return Err(LinkError::OutsideGame(target.to_path_buf()));
        }
        let Some(entry) = self.entries.remove(&key(target)) else {
            return Err(LinkError::NotManaged(target.to_path_buf()));
        };
        match entry.kind {
            LinkKind::Junction | LinkKind::Symlink => remove_link(target),
            LinkKind::Hardlink => remove_file(target),
            LinkKind::Copy => remove_copy(target, &entry.files),
        }?;
        Ok(())
    }

    pub fn verify(&self, game_root: &Path) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(path, _)| {
                let path = Path::new(path);
                !lexical_inside(path, game_root) || path.symlink_metadata().is_err()
            })
            .map(|(path, _)| path.clone())
            .collect()
    }
}

fn key(target: &Path) -> String {
    target.to_string_lossy().into_owned()
}

fn remove_file(target: &Path) -> Result<(), LinkError> {
    fs::remove_file(target).map_err(|error| io_error(target, error))
}

fn remove_link(target: &Path) -> Result<(), LinkError> {
    let metadata = target
        .symlink_metadata()
        .map_err(|error| io_error(target, error))?;
    if metadata.file_type().is_symlink() {
        remove_file(target)
    } else {
        fs::remove_dir(target).map_err(|error| io_error(target, error))
    }
}

fn remove_copy(target: &Path, files: &[String]) -> Result<(), LinkError> {
    for relative in files {
        let path = target.join(relative);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(&path, error)),
        }
    }
    prune_empty_dirs(target, files);
    Ok(())
}

fn prune_empty_dirs(target: &Path, files: &[String]) {
    let mut dirs: Vec<PathBuf> = files
        .iter()
        .filter_map(|relative| Path::new(relative).parent().map(Path::to_path_buf))
        .collect();
    dirs.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));
    dirs.dedup();
    for dir in dirs {
        let _ = fs::remove_dir(target.join(dir));
    }
    let _ = fs::remove_dir(target);
}

fn deploy_dir(target: &Path, source: &Path) -> Result<(LinkKind, Vec<String>), LinkError> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    if dir_link(target, source).is_ok() {
        let kind = if cfg!(windows) {
            LinkKind::Junction
        } else {
            LinkKind::Symlink
        };
        return Ok((kind, Vec::new()));
    }
    let files = copy_tree(source, target)?;
    Ok((LinkKind::Copy, files))
}

fn deploy_file(target: &Path, source: &Path) -> Result<LinkKind, LinkError> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    if fs::hard_link(source, target).is_ok() {
        return Ok(LinkKind::Hardlink);
    }
    if file_link(target, source).is_ok() {
        return Ok(LinkKind::Symlink);
    }
    fs::copy(source, target).map_err(|error| io_error(target, error))?;
    Ok(LinkKind::Copy)
}

#[cfg(windows)]
fn dir_link(target: &Path, source: &Path) -> io::Result<()> {
    let status = std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(target)
        .arg(source)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("mklink /J failed"))
    }
}

#[cfg(not(windows))]
fn dir_link(target: &Path, source: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn file_link(target: &Path, source: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(source, target)
}

#[cfg(not(windows))]
fn file_link(target: &Path, source: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

fn copy_tree(source: &Path, target: &Path) -> Result<Vec<String>, LinkError> {
    let mut files = Vec::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let from = source.join(&relative);
        let to = target.join(&relative);
        fs::create_dir_all(&to).map_err(|error| io_error(&to, error))?;
        let entries = fs::read_dir(&from).map_err(|error| io_error(&from, error))?;
        for entry in entries {
            let entry = entry.map_err(|error| io_error(&from, error))?;
            let child = relative.join(entry.file_name());
            let kind = entry.file_type().map_err(|error| io_error(&from, error))?;
            if kind.is_dir() {
                pending.push(child);
            } else if kind.is_file() {
                if files.len() >= MAX_COPY_ENTRIES {
                    return Err(LinkError::Io {
                        path: from.clone(),
                        source: io::Error::other("too many files to copy"),
                    });
                }
                fs::copy(entry.path(), target.join(&child))
                    .map_err(|error| io_error(&target.join(&child), error))?;
                files.push(child.to_string_lossy().into_owned());
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    fn store_file(root: &Path) -> PathBuf {
        let path = root.join("Store/mods/mod/1.0/payload/plugin.dll");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"MZ").unwrap();
        path
    }

    #[test]
    fn deploy_file_then_remove_keeps_store() {
        let dir = temp_dir("links-file");
        let game = dir.join("game");
        fs::create_dir_all(game.join("BepInEx/Plugins")).unwrap();
        let source = store_file(&dir);
        let mut index = LinkIndex::load(&dir);
        let target = game.join("BepInEx/Plugins/plugin.dll");
        let kind = index.deploy(&game, &target, &source, "mod", "1.0").unwrap();
        assert!(matches!(
            kind,
            LinkKind::Hardlink | LinkKind::Symlink | LinkKind::Copy
        ));
        assert!(target.is_file());
        index.remove(&game, &target).unwrap();
        assert!(!target.exists());
        assert!(source.is_file());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn deploy_dir_then_remove_keeps_store() {
        let dir = temp_dir("links-dir");
        let game = dir.join("game");
        fs::create_dir_all(game.join("BepInEx/Plugins")).unwrap();
        let source = dir.join("Store/mods/big/1.0/payload");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("a.dll"), b"MZ").unwrap();
        let mut index = LinkIndex::load(&dir);
        let target = game.join("BepInEx/Plugins/big");
        index.deploy(&game, &target, &source, "big", "1.0").unwrap();
        index.remove(&game, &target).unwrap();
        assert!(target.symlink_metadata().is_err());
        assert!(source.join("a.dll").is_file());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn remove_refuses_unmanaged_target() {
        let dir = temp_dir("links-unmanaged");
        let game = dir.join("game");
        fs::create_dir_all(game.join("BepInEx/Plugins")).unwrap();
        let mut index = LinkIndex::load(&dir);
        let target = game.join("BepInEx/Plugins/user.dll");
        assert!(matches!(
            index.remove(&game, &target),
            Err(LinkError::NotManaged(_))
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn deploy_rejects_targets_outside_game() {
        let dir = temp_dir("links-outside");
        let game = dir.join("game");
        fs::create_dir_all(&game).unwrap();
        let source = store_file(&dir);
        let mut index = LinkIndex::load(&dir);
        let outside = dir.join("elsewhere/plugin.dll");
        assert!(matches!(
            index.deploy(&game, &outside, &source, "mod", "1.0"),
            Err(LinkError::OutsideGame(_))
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn index_round_trips() {
        let dir = temp_dir("links-index");
        let game = dir.join("game");
        fs::create_dir_all(game.join("BepInEx/Plugins")).unwrap();
        let source = store_file(&dir);
        let mut index = LinkIndex::load(&dir);
        let target = game.join("BepInEx/Plugins/plugin.dll");
        index.deploy(&game, &target, &source, "mod", "1.0").unwrap();
        index.save().unwrap();
        let loaded = LinkIndex::load(&dir);
        assert!(loaded.get(&target).is_some());
        assert_eq!(loaded.deployed_for("mod", "1.0").len(), 1);
        let _ = fs::remove_dir_all(dir);
    }
}
