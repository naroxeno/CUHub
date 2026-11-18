//! Installs a staged package into the store, then makes the active profile match it.
//! Only this module moves unmanaged game files; each move goes through Backups first.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::install::{FileAction, Layout, PackageFile, PlannedFile, UpgradePlan};
use crate::links::{LinkError, LinkIndex};
use crate::profiles::{
    ApplyOp, ApplyReport, Profile, ProfileEntry, ProfileError, ProfileStore, apply_plan, plan_apply,
};
use crate::store::{Store, StoreEntry, StoreError, StoredMod};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackagePlan {
    pub mod_id: String,
    pub version: String,
    pub source: String,
    pub target: PathBuf,
    pub previous: Option<PathBuf>,
    pub staging: PathBuf,
    pub files: Vec<PackageFile>,
    pub single: bool,
    pub warnings: Vec<String>,
}

impl PackagePlan {
    pub fn preview(&self) -> UpgradePlan {
        let replacing = self.previous.is_some() || self.target.exists();
        let action = if replacing {
            FileAction::Replace
        } else {
            FileAction::Create
        };
        let (target_root, files) = if self.single {
            let root = self
                .target
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.target.clone());
            let file = &self.files[0];
            (
                root,
                vec![PlannedFile {
                    staged: file.staged.clone(),
                    target: self.target.clone(),
                    action,
                    size: file.size,
                }],
            )
        } else {
            (
                self.target.clone(),
                self.files
                    .iter()
                    .map(|file| PlannedFile {
                        staged: file.staged.clone(),
                        target: self.target.join(&file.relative),
                        action,
                        size: file.size,
                    })
                    .collect(),
            )
        };
        UpgradePlan {
            layout: if self.single {
                Layout::Dll
            } else {
                Layout::Mod
            },
            target_root,
            files,
            conflicts: Vec::new(),
            warnings: self.warnings.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackageReport {
    pub stored: StoredMod,
    pub files: usize,
    pub backup: Option<PathBuf>,
    pub profile: String,
    pub applied: ApplyReport,
}

pub struct PackageContext<'a> {
    pub store: &'a Store,
    pub links: &'a mut LinkIndex,
    pub profiles: &'a ProfileStore,
    pub game_root: &'a Path,
    pub plugins_root: &'a Path,
    pub backup_root: &'a Path,
}

#[derive(Debug)]
pub enum PackageError {
    Store(StoreError),
    Profile(ProfileError),
    Link(LinkError),
    MissingPayload(PathBuf),
    PayloadConflict { mod_id: String, version: String },
    InvalidRelative(PathBuf),
    ProfileMissing(String),
    ProfileSave(io::Error),
    Io { path: PathBuf, source: io::Error },
}

impl From<StoreError> for PackageError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<ProfileError> for PackageError {
    fn from(error: ProfileError) -> Self {
        Self::Profile(error)
    }
}

impl From<LinkError> for PackageError {
    fn from(error: LinkError) -> Self {
        Self::Link(error)
    }
}

impl std::fmt::Display for PackageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Store(error) => write!(formatter, "store: {error}"),
            Self::Profile(error) => write!(formatter, "profile: {error}"),
            Self::Link(error) => write!(formatter, "link: {error}"),
            Self::MissingPayload(path) => {
                write!(
                    formatter,
                    "package payload is missing at {}",
                    path.display()
                )
            }
            Self::PayloadConflict { mod_id, version } => write!(
                formatter,
                "the store already holds different bytes for {mod_id}@{version}"
            ),
            Self::InvalidRelative(path) => {
                write!(formatter, "package path is not safe: {}", path.display())
            }
            Self::ProfileMissing(name) => {
                write!(formatter, "profile \"{name}\" is missing")
            }
            Self::ProfileSave(error) => write!(formatter, "could not save the profile: {error}"),
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for PackageError {}

pub fn apply(
    plan: &PackagePlan,
    context: PackageContext<'_>,
) -> Result<PackageReport, PackageError> {
    let stored = import_package(plan, context.store)?;
    let mut profile = active_or_default(context.profiles, context.links)?;
    profile.active = true;
    profile.entries.retain(|entry| entry.mod_id != plan.mod_id);
    profile.entries.push(ProfileEntry {
        mod_id: plan.mod_id.clone(),
        version: plan.version.clone(),
        enabled: true,
        source: Some(plan.source.clone()),
        link: None,
    });
    let ops = plan_apply(
        &profile,
        context.links,
        context.store,
        context.game_root,
        context.plugins_root,
    )?;
    let backup = if plan.previous.as_ref().is_some_and(|path| path.exists())
        && ops
            .iter()
            .any(|op| matches!(op, ApplyOp::Deploy { target, .. } if target == &plan.target))
    {
        plan.previous
            .as_deref()
            .map(|path| backup_path(path, context.backup_root))
            .transpose()?
    } else {
        None
    };
    let applied = apply_plan(context.links, context.store, context.game_root, &ops)?;
    context
        .profiles
        .save(&profile)
        .map_err(PackageError::ProfileSave)?;
    let profile_name = profile.name.clone();
    Ok(PackageReport {
        stored,
        files: plan.files.len(),
        backup,
        profile: profile_name,
        applied,
    })
}

fn import_package(plan: &PackagePlan, store: &Store) -> Result<StoredMod, PackageError> {
    let temporary = plan.staging.join("package-payload");
    if temporary.symlink_metadata().is_ok() {
        fs::remove_dir_all(&temporary).map_err(|source| PackageError::Io {
            path: temporary.clone(),
            source,
        })?;
    }
    fs::create_dir_all(&temporary).map_err(|source| PackageError::Io {
        path: temporary.clone(),
        source,
    })?;
    let copied = copy_payload(plan, &temporary);
    if let Err(error) = copied {
        let _ = fs::remove_dir_all(&temporary);
        return Err(error);
    }
    let imported = store.import_dir(&temporary, &plan.mod_id, &plan.version, &plan.source);
    let _ = fs::remove_dir_all(&temporary);
    match imported {
        Ok(stored) => Ok(stored),
        Err(StoreError::AlreadyExists(_)) => {
            let path = store
                .resolve(&plan.mod_id, &plan.version)
                .ok_or_else(|| PackageError::MissingPayload(plan.target.clone()))?;
            let entry = store
                .read_entry(&plan.mod_id, &plan.version)
                .ok_or_else(|| PackageError::MissingPayload(path.clone()))?;
            if !same_payload(plan, &entry)? {
                return Err(PackageError::PayloadConflict {
                    mod_id: plan.mod_id.clone(),
                    version: plan.version.clone(),
                });
            }
            Ok(StoredMod {
                mod_id: plan.mod_id.clone(),
                version: plan.version.clone(),
                path,
                size_bytes: entry.files.iter().map(|file| file.len).sum(),
            })
        }
        Err(error) => Err(PackageError::Store(error)),
    }
}

fn same_payload(plan: &PackagePlan, entry: &StoreEntry) -> Result<bool, PackageError> {
    if plan.files.len() != entry.files.len() {
        return Ok(false);
    }
    let mut expected = BTreeMap::new();
    for file in &plan.files {
        let path = file.relative.to_string_lossy().replace('\\', "/");
        expected.insert(path, (file.size, hash_file(&file.staged)?));
    }
    for stored in &entry.files {
        let Some((size, hash)) = expected.remove(&stored.path) else {
            return Ok(false);
        };
        if size != stored.len || hash != stored.sha256 {
            return Ok(false);
        }
    }
    Ok(expected.is_empty())
}

fn hash_file(path: &Path) -> Result<String, PackageError> {
    let mut input = fs::File::open(path).map_err(|source| PackageError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let read = input.read(&mut chunk).map_err(|source| PackageError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&chunk[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn copy_payload(plan: &PackagePlan, root: &Path) -> Result<(), PackageError> {
    for file in &plan.files {
        if !is_plain_relative(&file.relative) {
            return Err(PackageError::InvalidRelative(file.relative.clone()));
        }
        let target = root.join(&file.relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|source| PackageError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::copy(&file.staged, &target).map_err(|source| PackageError::Io {
            path: target.clone(),
            source,
        })?;
    }
    Ok(())
}

fn active_or_default(profiles: &ProfileStore, links: &LinkIndex) -> Result<Profile, PackageError> {
    if let Some(name) = profiles.active() {
        return profiles
            .load(&name)
            .ok_or(PackageError::ProfileMissing(name));
    }
    if let Some(profile) = profiles.load(DEFAULT_PROFILE) {
        return Ok(profile);
    }
    let mut profile = Profile::new(DEFAULT_PROFILE);
    for (_, entry) in links.entries() {
        profile.entries.push(ProfileEntry {
            mod_id: entry.mod_id.clone(),
            version: entry.version.clone(),
            enabled: true,
            source: None,
            link: Some(entry.kind),
        });
    }
    Ok(profile)
}

fn backup_path(source: &Path, root: &Path) -> Result<PathBuf, PackageError> {
    let stamp = Utc::now().format("Package-%Y%m%d-%H%M%S").to_string();
    let mut directory = root.join(&stamp);
    let mut counter = 2;
    while directory.symlink_metadata().is_ok() {
        directory = root.join(format!("{stamp}-{counter}"));
        counter += 1;
    }
    fs::create_dir_all(&directory).map_err(|source| PackageError::Io {
        path: directory.clone(),
        source,
    })?;
    let name = source
        .file_name()
        .ok_or_else(|| PackageError::InvalidRelative(source.to_path_buf()))?;
    let destination = directory.join(name);
    move_path(source, &destination)?;
    Ok(directory)
}

fn move_path(source: &Path, destination: &Path) -> Result<(), PackageError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| PackageError::Io {
            path: parent.to_path_buf(),
            source: error,
        })?;
    }
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_tree(source, destination)?;
            remove_path(source)
        }
    }
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), PackageError> {
    let metadata = fs::symlink_metadata(source).map_err(|error| PackageError::Io {
        path: source.to_path_buf(),
        source: error,
    })?;
    if metadata.is_dir() {
        fs::create_dir_all(destination).map_err(|error| PackageError::Io {
            path: destination.to_path_buf(),
            source: error,
        })?;
        for entry in fs::read_dir(source).map_err(|error| PackageError::Io {
            path: source.to_path_buf(),
            source: error,
        })? {
            let entry = entry.map_err(|error| PackageError::Io {
                path: source.to_path_buf(),
                source: error,
            })?;
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(source, destination)
            .map(|_| ())
            .map_err(|error| PackageError::Io {
                path: destination.to_path_buf(),
                source: error,
            })
    }
}

fn remove_path(path: &Path) -> Result<(), PackageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path).map_err(|source| PackageError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
        Ok(_) => fs::remove_file(path).map_err(|source| PackageError::Io {
            path: path.to_path_buf(),
            source,
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(PackageError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn is_plain_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

const DEFAULT_PROFILE: &str = "Default";

#[cfg(test)]
mod tests;
