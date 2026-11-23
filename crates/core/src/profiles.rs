//! Profiles: a named list of store versions plus the diff that makes the deployment match it.
//! Planning reads Links.json and the store only; applying owns every link mutation and saves the index.
//! \`Profiles/active.txt\` is authoritative for the active profile; \`Profile::active\` only mirrors it.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::links::{LinkError, LinkIndex, LinkKind};
use crate::store::{Store, StoreFile};

const PROFILES_DIR: &str = "Profiles";
const ACTIVE_FILE: &str = "active.txt";
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub entries: Vec<ProfileEntry>,
    #[serde(default)]
    pub config_overrides: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileEntry {
    pub mod_id: String,
    pub version: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<LinkKind>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            name: String::new(),
            active: false,
            entries: Vec::new(),
            config_overrides: Vec::new(),
        }
    }
}

impl Profile {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}

impl Default for ProfileEntry {
    fn default() -> Self {
        Self {
            mod_id: String::new(),
            version: String::new(),
            enabled: true,
            source: None,
            link: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProfileStore {
    root: PathBuf,
}

impl ProfileStore {
    pub fn new(app_data: impl Into<PathBuf>) -> Self {
        Self {
            root: app_data.into(),
        }
    }

    pub fn list(&self) -> Vec<String> {
        let mut names = Vec::new();
        let Ok(entries) = fs::read_dir(self.directory()) else {
            return names;
        };
        for entry in entries.flatten() {
            if !entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
            {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if is_valid_name(name) {
                names.push(name.to_owned());
            }
        }
        names.sort();
        names
    }

    pub fn load(&self, name: &str) -> Option<Profile> {
        if !is_valid_name(name) {
            return None;
        }
        let raw = fs::read_to_string(self.profile_file(name)).ok()?;
        let mut profile: Profile = serde_json::from_str(&raw).ok()?;
        profile.name = name.to_owned();
        profile.active = self.active().as_deref() == Some(name);
        Some(profile)
    }

    pub fn save(&self, profile: &Profile) -> io::Result<()> {
        self.write_file(profile)?;
        if profile.active {
            self.write_active(Some(&profile.name))?;
            self.rewrite_flags(Some(&profile.name))?;
        } else if self.active_marker().as_deref() == Some(profile.name.as_str()) {
            self.write_active(None)?;
        }
        Ok(())
    }

    pub(crate) fn save_deployed(&self, profile: &Profile) -> Result<(), ProfileError> {
        let saved = if self.active().as_deref() == Some(&profile.name) {
            self.write_file(profile)
        } else {
            self.save(profile)
        };
        saved.map_err(|source| ProfileError::Commit {
            name: profile.name.clone(),
            source,
        })
    }

    fn write_file(&self, profile: &Profile) -> io::Result<()> {
        validate_name(&profile.name)?;
        for entry in &profile.entries {
            if !is_valid_name(&entry.mod_id) || !is_valid_name(&entry.version) {
                return Err(invalid_input(format!(
                    "{}/{} is not a valid mod id and version",
                    entry.mod_id, entry.version
                )));
            }
        }
        let json = serde_json::to_string_pretty(profile).map_err(io::Error::other)?;
        crate::atomic::replace(
            &self.profile_file(&profile.name),
            format!("{json}\n").as_bytes(),
        )
    }

    fn rewrite_flags(&self, skip: Option<&str>) -> io::Result<()> {
        for name in self.list() {
            if skip == Some(name.as_str()) {
                continue;
            }
            if let Some(profile) = self.load(&name) {
                self.write_file(&profile)?;
            }
        }
        Ok(())
    }

    pub fn remove(&self, name: &str) -> io::Result<()> {
        validate_name(name)?;
        match fs::remove_file(self.profile_file(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if self.active_marker().as_deref() == Some(name) {
            self.write_active(None)?;
        }
        Ok(())
    }

    pub fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        validate_name(from)?;
        validate_name(to)?;
        if from == to {
            return Ok(());
        }
        let Some(mut profile) = self.load(from) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("profile \"{from}\" does not exist"),
            ));
        };
        if self.profile_file(to).is_file() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("profile \"{to}\" already exists"),
            ));
        }
        profile.name = to.to_owned();
        self.save(&profile)?;
        self.remove(from)?;
        Ok(())
    }

    pub fn active(&self) -> Option<String> {
        let name = self.active_marker()?;
        self.profile_file(&name).is_file().then_some(name)
    }

    pub fn set_active(&self, name: Option<&str>) -> io::Result<()> {
        if let Some(name) = name {
            validate_name(name)?;
            if !self.profile_file(name).is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("profile \"{name}\" does not exist"),
                ));
            }
        }
        self.write_active(name)?;
        self.rewrite_flags(None)
    }

    fn directory(&self) -> PathBuf {
        self.root.join(PROFILES_DIR)
    }

    fn profile_file(&self, name: &str) -> PathBuf {
        self.directory().join(format!("{name}.json"))
    }

    fn active_marker(&self) -> Option<String> {
        let raw = fs::read_to_string(self.directory().join(ACTIVE_FILE)).ok()?;
        let name = raw.trim();
        is_valid_name(name).then(|| name.to_owned())
    }

    fn write_active(&self, name: Option<&str>) -> io::Result<()> {
        let path = self.directory().join(ACTIVE_FILE);
        match name {
            Some(name) => crate::atomic::replace(&path, format!("{name}\n").as_bytes()),
            None => match fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOp {
    Deploy {
        target: PathBuf,
        mod_id: String,
        version: String,
    },
    Remove {
        target: PathBuf,
    },
}

impl ApplyOp {
    pub(crate) fn target(&self) -> &Path {
        match self {
            Self::Deploy { target, .. } | Self::Remove { target } => target,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub applied: Vec<ApplyOp>,
    pub remaining: Vec<ApplyOp>,
}

#[derive(Debug)]
pub enum ProfileError {
    InvalidName(String),
    InvalidEntry {
        mod_id: String,
        version: String,
    },
    MissingVersion {
        mod_id: String,
        version: String,
    },
    DuplicateMod(String),
    DuplicateTarget(PathBuf),
    OccupiedTarget(PathBuf),
    AdditionalTarget(PathBuf),
    PlanChanged,
    Commit {
        name: String,
        source: io::Error,
    },
    OutsideGame(PathBuf),
    Link(LinkError),
    Apply {
        report: Box<ApplyReport>,
        source: Box<ProfileError>,
        index_error: Option<io::Error>,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName(value) => {
                write!(formatter, "\"{value}\" is not a valid profile name")
            }
            Self::InvalidEntry { mod_id, version } => {
                write!(
                    formatter,
                    "\"{mod_id}\" or \"{version}\" is not a valid entry"
                )
            }
            Self::MissingVersion { mod_id, version } => {
                write!(formatter, "{mod_id}@{version} is missing from the store")
            }
            Self::DuplicateMod(mod_id) => {
                write!(formatter, "{mod_id} is enabled more than once")
            }
            Self::DuplicateTarget(path) => {
                write!(
                    formatter,
                    "{} is claimed by more than one entry",
                    path.display()
                )
            }
            Self::OccupiedTarget(path) => {
                write!(
                    formatter,
                    "{} is occupied by an unmanaged file or directory",
                    path.display()
                )
            }
            Self::PlanChanged => write!(formatter, "deployment changed; review the plan again"),
            Self::AdditionalTarget(path) => write!(
                formatter,
                "the deployment would also change {}; review and apply the profile before retrying",
                path.display()
            ),
            Self::Commit { name, source } => {
                write!(
                    formatter,
                    "deployment completed, but profile \"{name}\" could not be saved or activated: {source}"
                )
            }
            Self::OutsideGame(path) => {
                write!(formatter, "{} is outside the game folder", path.display())
            }
            Self::Link(error) => error.fmt(formatter),
            Self::Apply {
                report,
                source,
                index_error,
            } => {
                write!(
                    formatter,
                    "deployment failed after {} of {} operations: {source}",
                    report.applied.len(),
                    report.applied.len() + report.remaining.len()
                )?;
                if let Some(error) = index_error {
                    write!(formatter, "; could not save Links.json: {error}")?;
                }
                Ok(())
            }
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for ProfileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Link(error) => Some(error),
            Self::Apply { source, .. } => Some(source.as_ref()),
            Self::Io { source, .. } | Self::Commit { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<LinkError> for ProfileError {
    fn from(error: LinkError) -> Self {
        Self::Link(error)
    }
}

pub(crate) fn deployment_target(files: &[StoreFile], mod_id: &str, plugins_root: &Path) -> PathBuf {
    deployment(files).target(mod_id, plugins_root)
}

pub(crate) fn entry_target(
    entry: &ProfileEntry,
    store: &Store,
    plugins_root: &Path,
) -> Option<PathBuf> {
    let manifest = store.read_entry(&entry.mod_id, &entry.version)?;
    Some(deployment_target(
        &manifest.files,
        &entry.mod_id,
        plugins_root,
    ))
}

pub(crate) fn checkpoint(
    name: String,
    base: Option<&Profile>,
    index: &LinkIndex,
    adopted: Vec<ProfileEntry>,
) -> Profile {
    let mut profile = base.cloned().unwrap_or_else(|| Profile::new(name.clone()));
    profile.name = name;
    profile.active = true;
    profile
        .entries
        .retain(|entry| !adopted.iter().any(|item| item.mod_id == entry.mod_id));
    profile.entries.extend(adopted);
    for (_, link) in index.entries() {
        if profile
            .entries
            .iter()
            .any(|entry| entry.mod_id == link.mod_id)
        {
            continue;
        }
        profile.entries.push(ProfileEntry {
            mod_id: link.mod_id.clone(),
            version: link.version.clone(),
            enabled: true,
            source: None,
            link: Some(link.kind),
        });
    }
    profile
}

pub fn plan_apply(
    profile: &Profile,
    index: &LinkIndex,
    store: &Store,
    game_root: &Path,
    plugins_root: &Path,
) -> Result<Vec<ApplyOp>, ProfileError> {
    if !is_below(plugins_root, game_root) {
        return Err(ProfileError::OutsideGame(plugins_root.to_path_buf()));
    }
    index.check_loaded().map_err(|source| ProfileError::Io {
        path: index.path().to_path_buf(),
        source,
    })?;
    let mut desired: Vec<(PathBuf, String, String)> = Vec::new();
    let mut targets = BTreeSet::new();
    let mut mods = BTreeSet::new();
    for entry in profile.entries.iter().filter(|entry| entry.enabled) {
        if !is_valid_name(&entry.mod_id) || !is_valid_name(&entry.version) {
            return Err(ProfileError::InvalidEntry {
                mod_id: entry.mod_id.clone(),
                version: entry.version.clone(),
            });
        }
        if !mods.insert(entry.mod_id.as_str()) {
            return Err(ProfileError::DuplicateMod(entry.mod_id.clone()));
        }
        store
            .resolve(&entry.mod_id, &entry.version)
            .ok_or_else(|| missing_version(&entry.mod_id, &entry.version))?;
        let manifest = store
            .read_entry(&entry.mod_id, &entry.version)
            .ok_or_else(|| missing_version(&entry.mod_id, &entry.version))?;
        let target = deployment_target(&manifest.files, &entry.mod_id, plugins_root);
        index.check_target(game_root, &target)?;
        let target_key = target
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if !targets.insert(target_key) {
            return Err(ProfileError::DuplicateTarget(target));
        }
        desired.push((target, entry.mod_id.clone(), entry.version.clone()));
    }
    let mut ops = Vec::new();
    for (path, _) in index.entries() {
        let target = Path::new(path);
        if target
            .parent()
            .is_some_and(|parent| crate::paths::same_directory(parent, plugins_root))
            && target.file_name().is_some()
            && !desired
                .iter()
                .any(|(desired_target, _, _)| crate::paths::same_target(desired_target, target))
        {
            index.check_target(game_root, target)?;
            index.target_current(target)?;
            ops.push(ApplyOp::Remove {
                target: target.to_path_buf(),
            });
        }
    }
    for (target, mod_id, version) in desired {
        let current = match index.get(&target) {
            Some(entry) => {
                index.target_current(&target)? && entry.mod_id == mod_id && entry.version == version
            }
            None => false,
        };
        if !current {
            ops.push(ApplyOp::Deploy {
                target,
                mod_id,
                version,
            });
        }
    }
    Ok(ops)
}

pub(crate) fn check_deployment_targets(
    index: &LinkIndex,
    ops: &[ApplyOp],
    backed_up_target: Option<&Path>,
) -> Result<(), ProfileError> {
    for op in ops {
        let ApplyOp::Deploy { target, .. } = op else {
            continue;
        };
        if index.get(target).is_some()
            || backed_up_target.is_some_and(|previous| crate::paths::same_target(previous, target))
        {
            continue;
        }
        match target.symlink_metadata() {
            Ok(_) => return Err(ProfileError::OccupiedTarget(target.clone())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(ProfileError::Io {
                    path: target.clone(),
                    source,
                });
            }
        }
    }
    Ok(())
}

pub fn apply_plan(
    index: &mut LinkIndex,
    store: &Store,
    game_root: &Path,
    ops: &[ApplyOp],
) -> Result<ApplyReport, ProfileError> {
    index.save_if_changed().map_err(|source| ProfileError::Io {
        path: index.path().to_path_buf(),
        source,
    })?;
    let mut report = ApplyReport::default();
    let mut stopped = None;
    for (position, op) in ops.iter().enumerate() {
        if let Err(error) = apply_one(index, store, game_root, op) {
            report.remaining = ops[position..].to_vec();
            stopped = Some(error);
            break;
        }
        report.applied.push(op.clone());
    }
    let index_error = index.save_if_changed().err();
    let (source, index_error) = match (stopped, index_error) {
        (None, None) => return Ok(report),
        (Some(source), index_error) => (source, index_error),
        (None, Some(source)) => (
            ProfileError::Io {
                path: index.path().to_path_buf(),
                source,
            },
            None,
        ),
    };
    Err(ProfileError::Apply {
        report: Box::new(report),
        source: Box::new(source),
        index_error,
    })
}

fn apply_one(
    index: &mut LinkIndex,
    store: &Store,
    game_root: &Path,
    op: &ApplyOp,
) -> Result<(), ProfileError> {
    match op {
        ApplyOp::Remove { target } => index.remove(game_root, target)?,
        ApplyOp::Deploy {
            target,
            mod_id,
            version,
        } => {
            let payload = store
                .resolve(mod_id, version)
                .ok_or_else(|| missing_version(mod_id, version))?;
            let manifest = store
                .read_entry(mod_id, version)
                .ok_or_else(|| missing_version(mod_id, version))?;
            let source = match deployment(&manifest.files) {
                Deployment::File(name) => payload.join(name),
                Deployment::Directory => payload,
            };
            index.deploy(game_root, target, &source, mod_id, version)?;
        }
    }
    Ok(())
}

pub(crate) enum Deployment {
    File(String),
    Directory,
}

impl Deployment {
    pub(crate) fn target(&self, mod_id: &str, plugins_root: &Path) -> PathBuf {
        match self {
            Self::File(name) => plugins_root.join(name),
            Self::Directory => plugins_root.join(mod_id),
        }
    }
}

pub(crate) fn deployment(files: &[StoreFile]) -> Deployment {
    deployment_from_paths(files.iter().map(|file| file.path.as_str()))
}

pub(crate) fn deployment_from_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> Deployment {
    let mut paths = paths.into_iter();
    let Some(name) = paths.next() else {
        return Deployment::Directory;
    };
    if paths.next().is_some() {
        return Deployment::Directory;
    }
    // A single DLL is its own unit at the top of Plugins, so its target keeps the file name.
    if is_valid_name(name) && name.to_ascii_lowercase().ends_with(".dll") {
        return Deployment::File(name.to_owned());
    }
    Deployment::Directory
}

// These names become single path components, so the store's allow-list keeps ids, versions and profile names contained.
fn is_valid_name(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.bytes().all(
            |byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_'),
        )
}

fn validate_name(value: &str) -> io::Result<()> {
    if is_valid_name(value) {
        Ok(())
    } else {
        Err(invalid_input(format!(
            "\"{value}\" is not a valid profile name"
        )))
    }
}

fn invalid_input(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn missing_version(mod_id: &str, version: &str) -> ProfileError {
    ProfileError::MissingVersion {
        mod_id: mod_id.to_owned(),
        version: version.to_owned(),
    }
}

fn is_below(candidate: &Path, container: &Path) -> bool {
    candidate.starts_with(container)
        && candidate.components().count() > container.components().count()
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

fn default_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests;
