//! Installs a staged package into the store, then makes the active profile match it.
//! Only this module moves unmanaged game files; each move goes through Backups first.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::install::{FileAction, Layout, PackageFile, PlannedFile, UpgradePlan};
use crate::links::{LinkError, LinkIndex};
use crate::profiles::{
    ApplyOp, ApplyReport, Profile, ProfileEntry, ProfileError, ProfileStore, apply_plan,
    check_deployment_targets, deployment_target, plan_apply,
};
use crate::store::{Store, StoreEntry, StoreError, StoredMod, scan_source_paths};

mod backup;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackagePlan {
    pub mod_id: String,
    pub version: String,
    pub source: String,
    pub target: PathBuf,
    pub previous: Option<PathBuf>,
    pub managed_previous: Option<PathBuf>,
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
    PayloadConflict {
        mod_id: String,
        version: String,
    },
    InvalidRelative(PathBuf),
    ProfileMissing(String),
    StoreChanged(PathBuf),
    AfterBackup {
        backup: PathBuf,
        source: Box<PackageError>,
        recovery: Option<Result<(), Box<PackageError>>>,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
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
            Self::StoreChanged(path) => {
                write!(formatter, "stored payload has changed: {}", path.display())
            }
            Self::AfterBackup {
                backup,
                source,
                recovery,
            } => {
                write!(formatter, "{source}; backup: {}", backup.display())?;
                match recovery {
                    Some(Ok(())) => write!(formatter, "; previous files were restored")?,
                    Some(Err(error)) => write!(formatter, "; recovery failed: {error}")?,
                    None => {}
                }
                Ok(())
            }
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for PackageError {}

pub fn apply(
    plan: &PackagePlan,
    context: PackageContext<'_>,
) -> Result<PackageReport, PackageError> {
    let profile = active_or_default(context.profiles, context.links)?;
    apply_to_profile(plan, context, profile, Some(&plan.source))
}

pub(crate) fn apply_to_profile(
    plan: &PackagePlan,
    context: PackageContext<'_>,
    mut profile: Profile,
    profile_source: Option<&str>,
) -> Result<PackageReport, PackageError> {
    let stored = import_package(plan, context.store)?;
    profile.active = true;
    profile.entries.retain(|entry| entry.mod_id != plan.mod_id);
    profile.entries.push(ProfileEntry {
        mod_id: plan.mod_id.clone(),
        version: plan.version.clone(),
        enabled: true,
        source: profile_source.map(str::to_owned),
        link: None,
    });
    let mut ops = plan_apply(
        &profile,
        context.links,
        context.store,
        context.game_root,
        context.plugins_root,
    )?;
    check_scope(plan, &context, &ops)?;
    let deploying = ops.iter().any(|op| matches!(op, ApplyOp::Deploy { .. }));
    let previous = plan.previous.as_deref().filter(|_| deploying);
    check_deployment_targets(context.links, &ops, previous)?;
    context
        .links
        .save_if_changed()
        .map_err(|source| PackageError::Io {
            path: context.links.path().to_path_buf(),
            source,
        })?;
    let backup = previous
        .map(|path| backup::save(path, context.backup_root))
        .transpose()?
        .flatten();
    ops.sort_by_key(|op| matches!(op, ApplyOp::Remove { .. }));
    let applied =
        apply_plan(context.links, context.store, context.game_root, &ops).map_err(|error| {
            if let (Some(previous), Some(backup)) = (previous, &backup) {
                let deployed = context.links.get(&plan.target).is_some_and(|entry| {
                    entry.mod_id == plan.mod_id && entry.version == plan.version
                });
                let recovery = (!deployed).then(|| backup::restore(previous, backup));
                after_backup(error.into(), backup, recovery)
            } else {
                error.into()
            }
        })?;
    context
        .profiles
        .save_deployed(&profile)
        .map_err(|error| match &backup {
            Some(backup) => after_backup(error.into(), backup, None),
            None => error.into(),
        })?;
    let profile_name = profile.name.clone();
    Ok(PackageReport {
        stored,
        files: plan.files.len(),
        backup,
        profile: profile_name,
        applied,
    })
}

fn check_scope(
    plan: &PackagePlan,
    context: &PackageContext<'_>,
    ops: &[ApplyOp],
) -> Result<(), PackageError> {
    let manifest = context
        .store
        .read_entry(&plan.mod_id, &plan.version)
        .ok_or_else(|| PackageError::MissingPayload(plan.target.clone()))?;
    if deployment_target(&manifest.files, &plan.mod_id, context.plugins_root) != plan.target
        || context
            .links
            .get(&plan.target)
            .is_some_and(|entry| entry.mod_id != plan.mod_id)
    {
        return Err(ProfileError::PlanChanged.into());
    }
    for op in ops {
        let allowed = match op {
            ApplyOp::Deploy {
                target,
                mod_id,
                version,
            } => target == &plan.target && mod_id == &plan.mod_id && version == &plan.version,
            ApplyOp::Remove { target } => {
                plan.managed_previous
                    .as_ref()
                    .is_some_and(|previous| crate::paths::same_target(previous, target))
                    && context
                        .links
                        .get(target)
                        .is_some_and(|entry| entry.mod_id == plan.mod_id)
            }
        };
        if !allowed {
            return Err(ProfileError::AdditionalTarget(op.target().to_path_buf()).into());
        }
    }
    if let Some(previous) = &plan.previous {
        if previous.parent() != Some(context.plugins_root) || context.links.get(previous).is_some()
        {
            return Err(ProfileError::PlanChanged.into());
        }
        context.links.check_target(context.game_root, previous)?;
    }
    if let Some(previous) = &plan.managed_previous {
        context.links.check_target(context.game_root, previous)?;
        if context.links.get(previous).is_none() {
            match previous.symlink_metadata() {
                Ok(_) => return Err(ProfileError::PlanChanged.into()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(PackageError::Io {
                        path: previous.clone(),
                        source,
                    });
                }
            }
        }
    }
    Ok(())
}

fn after_backup(
    error: PackageError,
    backup: &Path,
    recovery: Option<Result<(), PackageError>>,
) -> PackageError {
    PackageError::AfterBackup {
        backup: backup.to_path_buf(),
        source: Box::new(error),
        recovery: recovery.map(|result| result.map_err(Box::new)),
    }
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
            verify_stored_payload(&path, &entry)?;
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

fn verify_stored_payload(root: &Path, entry: &StoreEntry) -> Result<(), PackageError> {
    let actual =
        scan_source_paths(root).map_err(|_| PackageError::StoreChanged(root.to_path_buf()))?;
    if actual.len() != entry.files.len() {
        return Err(PackageError::StoreChanged(root.to_path_buf()));
    }
    for file in &entry.files {
        let path = root.join(&file.path);
        if actual.binary_search(&file.path).is_err() || hash_file(&path)? != file.sha256 {
            return Err(PackageError::StoreChanged(path));
        }
    }
    Ok(())
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

fn is_plain_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

const DEFAULT_PROFILE: &str = "Default";

#[cfg(test)]
mod tests;
