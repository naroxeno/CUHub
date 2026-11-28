//! Deployment links: the managed entries that point the game directory at the store.
//! The index is the only authority for ownership; hardlinks cannot be traced back otherwise.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

mod copy;
mod ownership;

use crate::paths::is_link;
pub(crate) use copy::{copy_file, copy_tree};
use ownership::{TargetState, inspect};

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
    #[serde(skip)]
    dirty: bool,
    #[serde(skip)]
    load_error: Option<String>,
    #[serde(skip)]
    cleanup: Vec<(PathBuf, LinkEntry)>,
    entries: BTreeMap<String, LinkEntry>,
}

#[derive(Debug)]
pub enum LinkError {
    OutsideGame(PathBuf),
    NotManaged(PathBuf),
    MissingStore(PathBuf),
    ChangedTarget(PathBuf),
    Recovery {
        source: Box<LinkError>,
        recovery: Box<LinkError>,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutsideGame(path) => {
                write!(formatter, "{} is outside the game folder", path.display())
            }
            Self::NotManaged(path) => {
                write!(formatter, "{} is not deployed by Cuprum", path.display())
            }
            Self::MissingStore(path) => {
                write!(formatter, "{} is missing from the store", path.display())
            }
            Self::ChangedTarget(path) => {
                write!(formatter, "{} has changed outside Cuprum", path.display())
            }
            Self::Recovery { source, recovery } => {
                write!(formatter, "{source}; recovery failed: {recovery}")
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
    candidate.len() > container.len()
        && candidate[..container.len()] == container[..]
        && candidate[container.len()..]
            .iter()
            .all(|part| matches!(part, Component::Normal(_)))
}

pub(crate) fn verify_copy(path: &Path, entry: &LinkEntry) -> Result<(), LinkError> {
    if inspect(path, entry)? == TargetState::Owned { Ok(()) } else { Err(LinkError::ChangedTarget(path.to_owned())) }
}

impl LinkIndex {
    pub fn load(app_data: &Path) -> Self {
        let file = app_data.join(INDEX_FILE);
        let loaded = fs::read_to_string(&file)
            .and_then(|raw| serde_json::from_str::<Self>(&raw).map_err(io::Error::other));
        let mut index = match loaded {
            Ok(index) => index,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(error) => Self {
                load_error: Some(error.to_string()),
                ..Self::default()
            },
        };
        index.file = file;
        index
    }

    pub fn path(&self) -> &Path {
        &self.file
    }

    pub fn save(&mut self) -> io::Result<()> {
        self.check_loaded()?;
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        crate::atomic::replace(&self.file, format!("{json}\n").as_bytes())?;
        self.dirty = false;
        self.cleanup_replacements()
    }

    pub(crate) fn save_if_changed(&mut self) -> io::Result<()> {
        self.check_loaded()?;
        if self.dirty {
            self.save()?;
        } else {
            self.cleanup_replacements()?;
        }
        Ok(())
    }

    pub(crate) fn check_loaded(&self) -> io::Result<()> {
        match &self.load_error {
            Some(error) => Err(io::Error::new(io::ErrorKind::InvalidData, error.clone())),
            None => Ok(()),
        }
    }

    fn cleanup_replacements(&mut self) -> io::Result<()> {
        while let Some((saved, entry)) = self.cleanup.last() {
            match inspect(saved, entry).map_err(io::Error::other)? {
                TargetState::Missing => {}
                _ => remove_owned(saved, entry).map_err(io::Error::other)?,
            }
            if let Some(parent) = saved.parent() {
                match fs::remove_dir(parent) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(io::Error::other(io_error(parent, error))),
                }
            }
            self.cleanup.pop();
        }
        Ok(())
    }

    pub fn get(&self, target: &Path) -> Option<&LinkEntry> {
        self.entries.get(self.entry_key(target)?)
    }

    fn entry_key(&self, target: &Path) -> Option<&str> {
        if let Some((spelling, _)) = self.entries.get_key_value(&key(target)) {
            return Some(spelling);
        }
        #[cfg(windows)]
        return self
            .entries
            .keys()
            .find(|recorded| crate::paths::same_target(Path::new(recorded), target))
            .map(String::as_str);
        #[cfg(not(windows))]
        None
    }

    pub(crate) fn record_copy(&mut self, target: &Path, entry: LinkEntry) -> Result<(), LinkError> {
        if self.get(target).is_some_and(|existing| existing != &entry)
            || inspect(target, &entry)? != TargetState::Owned {
            return Err(LinkError::ChangedTarget(target.to_owned()));
        }
        self.entries.insert(key(target), entry);
        self.dirty = true;
        Ok(())
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
        self.check_target(game_root, target)?;
        source_metadata(store_source)?;
        let previous_key = self.entry_key(target).map(str::to_owned);
        let previous = previous_key.as_ref().and_then(|key| self.entries.get(key));
        let (kind, files) = match previous {
            Some(entry) if inspect(target, entry)? != TargetState::Missing => {
                let (deployed, saved) = replace_target(target, store_source, entry)?;
                self.cleanup.push((saved, entry.clone()));
                deployed
            }
            _ => deploy_new(target, store_source)?,
        };
        if let Some(previous_key) = previous_key {
            self.entries.remove(&previous_key);
        }
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
        self.dirty = true;
        Ok(kind)
    }

    pub fn remove(&mut self, game_root: &Path, target: &Path) -> Result<(), LinkError> {
        self.check_target(game_root, target)?;
        let Some(recorded) = self.entry_key(target).map(str::to_owned) else {
            return Err(LinkError::NotManaged(target.to_path_buf()));
        };
        let entry = &self.entries[&recorded];
        if inspect(target, entry)? != TargetState::Missing {
            remove_owned(target, entry)?;
        }
        self.entries.remove(&recorded);
        self.dirty = true;
        Ok(())
    }

    pub(crate) fn check_target(&self, game_root: &Path, target: &Path) -> Result<(), LinkError> {
        self.check_loaded()
            .map_err(|error| io_error(&self.file, error))?;
        if !lexical_inside(target, game_root) || !crate::paths::is_inside(target, game_root) {
            return Err(LinkError::OutsideGame(target.to_path_buf()));
        }
        Ok(())
    }

    pub(crate) fn target_current(&self, target: &Path) -> Result<bool, LinkError> {
        match self.get(target) {
            Some(entry) => Ok(inspect(target, entry)? == TargetState::Owned),
            None => match target.symlink_metadata() {
                Ok(_) => Err(LinkError::NotManaged(target.to_path_buf())),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(io_error(target, error)),
            },
        }
    }

    pub fn verify(&self, game_root: &Path) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(path, _)| {
                let path = Path::new(path);
                self.check_target(game_root, path).is_err()
                    || !matches!(self.target_current(path), Ok(true))
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
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
        if metadata.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0 {
            return fs::remove_dir(target).map_err(|error| io_error(target, error));
        }
    }
    if is_link(&metadata) {
        remove_file(target)
    } else {
        fs::remove_dir(target).map_err(|error| io_error(target, error))
    }
}

fn remove_owned(target: &Path, entry: &LinkEntry) -> Result<(), LinkError> {
    match entry.kind {
        LinkKind::Junction | LinkKind::Symlink => remove_link(target),
        LinkKind::Hardlink => remove_file(target),
        LinkKind::Copy if entry.files.is_empty() => remove_file(target),
        LinkKind::Copy => remove_copy(target, &entry.files),
    }
}

fn deploy_new(target: &Path, source: &Path) -> Result<(LinkKind, Vec<String>), LinkError> {
    match target.symlink_metadata() {
        Ok(_) => return Err(LinkError::NotManaged(target.to_path_buf())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(target, error)),
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    let files = if source_metadata(source)?.is_dir() {
        copy_tree(source, target)?
    } else {
        copy_file(source, target)?;
        Vec::new()
    };
    Ok((LinkKind::Copy, files))
}

fn source_metadata(source: &Path) -> Result<fs::Metadata, LinkError> {
    match fs::symlink_metadata(source) {
        Ok(metadata) if !is_link(&metadata) && (metadata.is_file() || metadata.is_dir()) => {
            Ok(metadata)
        }
        Ok(_) => Err(LinkError::MissingStore(source.to_path_buf())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Err(LinkError::MissingStore(source.to_path_buf()))
        }
        Err(error) => Err(io_error(source, error)),
    }
}

fn replace_target(
    target: &Path,
    source: &Path,
    entry: &LinkEntry,
) -> Result<((LinkKind, Vec<String>), PathBuf), LinkError> {
    let temporary = replacement_directory(target)?;
    let saved = temporary.join("previous");
    if let Err(error) = fs::rename(target, &saved) {
        let _ = fs::remove_dir(&temporary);
        return Err(io_error(target, error));
    }
    let result = inspect(&saved, entry).and_then(|state| {
        if state == TargetState::Missing {
            return Err(LinkError::ChangedTarget(saved.clone()));
        }
        deploy_new(target, source)
    });
    match result {
        Ok(deployed) => Ok((deployed, saved)),
        Err(error) => {
            let restored = match target.symlink_metadata() {
                Err(missing) if missing.kind() == io::ErrorKind::NotFound => {
                    fs::rename(&saved, target)
                }
                Err(error) => Err(error),
                Ok(_) => Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "deployment target is occupied",
                )),
            };
            if let Err(recovery) = restored {
                return Err(LinkError::Recovery {
                    source: Box::new(error),
                    recovery: Box::new(io_error(&saved, recovery)),
                });
            }
            let _ = fs::remove_dir(&temporary);
            Err(error)
        }
    }
}

fn replacement_directory(target: &Path) -> Result<PathBuf, LinkError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    loop {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = target.with_file_name(format!(".cuprum-replace-{}-{id}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(io_error(&path, error)),
        }
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
    prune_empty_dirs(target, files)
}

fn prune_empty_dirs(target: &Path, files: &[String]) -> Result<(), LinkError> {
    let mut dirs: Vec<PathBuf> = files
        .iter()
        .flat_map(|relative| Path::new(relative).ancestors().skip(1))
        .filter(|directory| !directory.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .collect();
    dirs.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));
    dirs.dedup();
    for path in dirs
        .iter()
        .map(|dir| target.join(dir))
        .chain([target.to_path_buf()])
    {
        match fs::remove_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(&path, error)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
