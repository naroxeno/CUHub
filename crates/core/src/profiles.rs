//! Profiles: a named list of store versions plus the diff that makes the deployment match it.
//! Planning reads Links.json and the store only; applying owns every link mutation and saves the index.
//! \`Profiles/active.txt\` is authoritative for the active profile; \`Profile::active\` only mirrors it.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

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
        )?;
        if profile.active {
            self.write_active(Some(&profile.name))?;
        } else if self.active_marker().as_deref() == Some(profile.name.as_str()) {
            self.write_active(None)?;
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

    pub fn active(&self) -> Option<String> {
        let name = self.active_marker()?;
        self.profile_file(&name).is_file().then_some(name)
    }

    pub fn set_active(&self, name: Option<&str>) -> io::Result<()> {
        let Some(name) = name else {
            return self.write_active(None);
        };
        validate_name(name)?;
        if !self.profile_file(name).is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("profile \"{name}\" does not exist"),
            ));
        }
        self.write_active(Some(name))
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
    OutsideGame(PathBuf),
    Link(LinkError),
    Apply {
        report: Box<ApplyReport>,
        source: Box<ProfileError>,
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
            Self::OutsideGame(path) => {
                write!(formatter, "{} is outside the game folder", path.display())
            }
            Self::Link(error) => error.fmt(formatter),
            Self::Apply { report, source } => write!(
                formatter,
                "deployment stopped after {} of {} operations: {source}",
                report.applied.len(),
                report.applied.len() + report.remaining.len()
            ),
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for ProfileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Link(error) => Some(error),
            Self::Apply { source, .. } => Some(source.as_ref()),
            Self::Io { source, .. } => Some(source),
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
    match deployment(files) {
        Deployment::File(name) => plugins_root.join(name),
        Deployment::Directory => plugins_root.join(mod_id),
    }
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
        if !targets.insert(target.clone()) {
            return Err(ProfileError::DuplicateTarget(target));
        }
        desired.push((target, entry.mod_id.clone(), entry.version.clone()));
    }
    let mut ops: Vec<ApplyOp> = index
        .entries()
        .filter_map(|(path, entry)| {
            let target = Path::new(path);
            single_component_below(target, plugins_root)?;
            let current = desired.iter().any(|(desired_target, mod_id, version)| {
                desired_target.as_path() == target
                    && *mod_id == entry.mod_id
                    && *version == entry.version
            });
            (!current).then(|| ApplyOp::Remove {
                target: target.to_path_buf(),
            })
        })
        .collect();
    for (target, mod_id, version) in desired {
        let current = matches!(
            index.get(&target),
            Some(entry) if entry.mod_id == mod_id && entry.version == version
        );
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

pub fn apply_plan(
    index: &mut LinkIndex,
    store: &Store,
    game_root: &Path,
    ops: &[ApplyOp],
) -> Result<ApplyReport, ProfileError> {
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
    let saved = if report.applied.is_empty() {
        Ok(())
    } else {
        index.save().map_err(|source| ProfileError::Io {
            path: index.path().to_path_buf(),
            source,
        })
    };
    match stopped {
        Some(source) => Err(ProfileError::Apply {
            report: Box::new(report),
            source: Box::new(source),
        }),
        None => {
            saved?;
            Ok(report)
        }
    }
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

pub(crate) fn deployment(files: &[StoreFile]) -> Deployment {
    let [file] = files else {
        return Deployment::Directory;
    };
    let name = file.path.as_str();
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

fn single_component_below(target: &Path, root: &Path) -> Option<String> {
    let relative = target.strip_prefix(root).ok()?;
    let mut parts = relative.components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(name)), None) => Some(name.to_string_lossy().into_owned()),
        _ => None,
    }
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

fn default_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests;
