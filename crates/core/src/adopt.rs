//! Adopts mods already deployed in Plugins into the store, then replaces each one with a link.
//! Planning only reads; applying imports, moves the original into the backup folder, deploys, and saves the index.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::links::{LinkError, LinkIndex};
use crate::mods::InstalledMod;
use crate::profiles::{self, Deployment};
use crate::store::{self, Store, StoreError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdoptItem {
    pub mod_id: String,
    pub version: String,
    pub source: PathBuf,
    pub target: PathBuf,
    pub directory: bool,
    pub file: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdoptSkip {
    Disabled,
    AlreadyManaged,
    OutsidePlugins,
    InvalidName,
    MissingVersion,
    StoreHasVersion,
    DuplicateName,
    DuplicateTarget,
    BuiltIn,
    UnsafeFiles,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdoptPlan {
    pub items: Vec<AdoptItem>,
    pub skipped: Vec<(String, AdoptSkip)>,
}

#[derive(Clone, Debug, Default)]
pub struct AdoptReport {
    pub adopted: Vec<AdoptItem>,
    pub remaining: Vec<AdoptItem>,
    pub backup_dir: PathBuf,
}

#[derive(Debug)]
pub enum AdoptError {
    Store(StoreError),
    Link(LinkError),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Adopt {
        report: Box<AdoptReport>,
        source: Box<AdoptError>,
    },
}

impl From<StoreError> for AdoptError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<LinkError> for AdoptError {
    fn from(error: LinkError) -> Self {
        Self::Link(error)
    }
}

impl std::fmt::Display for AdoptError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "store: {error}"),
            Self::Link(error) => write!(formatter, "link: {error}"),
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Adopt { source, .. } => write!(formatter, "adopt stopped early: {source}"),
        }
    }
}

impl std::error::Error for AdoptError {}

pub fn plan_adopt(
    mods: &[InstalledMod],
    plugins_root: &Path,
    store: &Store,
    links: &LinkIndex,
) -> AdoptPlan {
    let mut plan = AdoptPlan::default();
    for (item, candidate) in classify_mods(mods, plugins_root, store, links) {
        match candidate {
            Ok(candidate) => plan.items.push(candidate),
            Err(reason) => plan.skipped.push((item.name.clone(), reason)),
        }
    }
    plan
}

pub(crate) fn classify_mods<'a>(
    mods: &'a [InstalledMod],
    plugins_root: &'a Path,
    store: &'a Store,
    links: &'a LinkIndex,
) -> impl Iterator<Item = (&'a InstalledMod, Result<AdoptItem, AdoptSkip>)> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    let mut targets: BTreeSet<PathBuf> = BTreeSet::new();
    mods.iter().map(move |item| {
        let candidate = (|| {
            let Some(file_name) = item.source_path.file_name().and_then(|name| name.to_str())
            else {
                return Err(AdoptSkip::InvalidName);
            };
            let id_source = if item.source_path.is_dir() {
                file_name
            } else {
                item.source_path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or(file_name)
            };
            let Some(mod_id) = mod_id_for(id_source) else {
                return Err(AdoptSkip::InvalidName);
            };
            if item.disabled {
                return Err(AdoptSkip::Disabled);
            }
            if !is_direct_child(&item.source_path, plugins_root) {
                return Err(AdoptSkip::OutsidePlugins);
            }
            if crate::paths::same_target(
                &item.source_path,
                &plugins_root.join(crate::bridge::install::BRIDGE_DIR),
            ) {
                return Err(AdoptSkip::BuiltIn);
            }
            let (target, file) = if item.source_path.is_dir() {
                let deployment = store::scan_source_paths(&item.source_path)
                    .map(|paths| profiles::deployment_from_paths(paths.iter().map(String::as_str)))
                    .unwrap_or(Deployment::Directory);
                match deployment {
                    Deployment::File(name) => {
                        let target = plugins_root.join(&name);
                        (target, Some(name))
                    }
                    Deployment::Directory => (plugins_root.join(&mod_id), None),
                }
            } else {
                (plugins_root.join(file_name), Some(file_name.to_owned()))
            };
            let managed = links.get(&item.source_path).is_some()
                || links.get(&target).is_some()
                || links.entries().any(|(_, link)| link.mod_id == mod_id);
            if managed {
                return Err(AdoptSkip::AlreadyManaged);
            }
            let version = item
                .installed_version
                .as_deref()
                .or(item.assembly_version.as_deref())
                .and_then(version_for);
            let Some(version) = version else {
                return Err(AdoptSkip::MissingVersion);
            };
            if store.read_entry(&mod_id, &version).is_some() {
                return Err(AdoptSkip::StoreHasVersion);
            }
            if names.contains(&mod_id) {
                return Err(AdoptSkip::DuplicateName);
            }
            if !targets.insert(target.clone()) {
                return Err(AdoptSkip::DuplicateTarget);
            }
            names.insert(mod_id.clone());
            Ok(AdoptItem {
                mod_id,
                version,
                source: item.source_path.clone(),
                target,
                directory: item.source_path.is_dir(),
                file,
            })
        })();
        (item, candidate)
    })
}

pub fn apply_adopt(
    plan: &AdoptPlan,
    store: &Store,
    links: &mut LinkIndex,
    game_root: &Path,
    backup_root: &Path,
) -> Result<AdoptReport, AdoptError> {
    links.save_if_changed().map_err(|source| AdoptError::Io {
        path: links.path().to_path_buf(),
        source,
    })?;
    let mut report = AdoptReport {
        backup_dir: unique_backup_dir(backup_root),
        ..AdoptReport::default()
    };
    for (position, item) in plan.items.iter().enumerate() {
        if let Err(error) = adopt_one(item, store, links, game_root, &report.backup_dir) {
            report.remaining = plan.items[position..].to_vec();
            return Err(AdoptError::Adopt {
                report: Box::new(report),
                source: Box::new(error),
            });
        }
        report.adopted.push(item.clone());
        links.save().map_err(|source| AdoptError::Io {
            path: links.path().to_path_buf(),
            source,
        })?;
    }
    Ok(report)
}

fn adopt_one(
    item: &AdoptItem,
    store: &Store,
    links: &mut LinkIndex,
    game_root: &Path,
    backup_dir: &Path,
) -> Result<(), AdoptError> {
    let stored = if item.directory {
        store.import_dir(&item.source, &item.mod_id, &item.version, "adopt")?
    } else {
        store.import_file(&item.source, &item.mod_id, &item.version, "adopt")?
    };
    let payload = match &item.file {
        Some(file) => stored.path.join(file),
        None => stored.path.clone(),
    };
    let backup = backup_dir.join(match item.target.file_name() {
        Some(file_name) => file_name,
        None => {
            return Err(AdoptError::Io {
                path: item.target.clone(),
                source: io::Error::new(io::ErrorKind::InvalidInput, "target has no file name"),
            });
        }
    });
    move_path(&item.source, &backup)?;
    links.deploy(
        game_root,
        &item.target,
        &payload,
        &item.mod_id,
        &item.version,
    )?;
    Ok(())
}

fn mod_id_for(name: &str) -> Option<String> {
    let mut id = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
            id.push(character.to_ascii_lowercase());
        } else if !id.ends_with('-') {
            id.push('-');
        }
    }
    let trimmed = id.trim_matches(|character| matches!(character, '-' | '.' | '_'));
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn version_for(version: &str) -> Option<String> {
    let trimmed = version.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return None;
    }
    trimmed
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        .then(|| trimmed.to_owned())
}

fn is_direct_child(path: &Path, parent: &Path) -> bool {
    path.parent() == Some(parent) && path.file_name().is_some()
}

fn unique_backup_dir(root: &Path) -> PathBuf {
    let stamp = Utc::now().format("Adopt-%Y%m%d-%H%M%S").to_string();
    let mut candidate = root.join(&stamp);
    let mut counter = 2;
    while candidate.symlink_metadata().is_ok() {
        candidate = root.join(format!("{stamp}-{counter}"));
        counter += 1;
    }
    candidate
}

fn move_path(source: &Path, destination: &Path) -> Result<(), AdoptError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_tree(source, destination)?;
            remove_path(source)
        }
    }
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), AdoptError> {
    let metadata = fs::symlink_metadata(source).map_err(|error| io_error(source, error))?;
    if metadata.is_dir() {
        fs::create_dir_all(destination).map_err(|error| io_error(destination, error))?;
        let entries = fs::read_dir(source).map_err(|error| io_error(source, error))?;
        for entry in entries {
            let entry = entry.map_err(|error| io_error(source, error))?;
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        }
        return Ok(());
    }
    if metadata.is_file() {
        fs::copy(source, destination).map_err(|error| io_error(destination, error))?;
        return Ok(());
    }
    Err(AdoptError::Io {
        path: source.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::Unsupported,
            "not a regular file or directory",
        ),
    })
}

fn remove_path(path: &Path) -> Result<(), AdoptError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error(path, error))?;
    if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|error| io_error(path, error))
    } else {
        fs::remove_file(path).map_err(|error| io_error(path, error))
    }
}

fn io_error(path: &Path, source: io::Error) -> AdoptError {
    AdoptError::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests;
