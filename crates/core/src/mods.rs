//! Scan BepInEx plugins, match them against the cached community catalogue, and toggle or delete them.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::catalogs::{Catalogs, DependencyRequirement};
use crate::metadata::MetadataMod;
use crate::paths;

mod dependencies;
mod dotnet;

pub use dependencies::{DependencyIssue, IssueKind};
pub(crate) use dependencies::{check_requirements, requirements_for};

pub(crate) fn read_assembly_version(path: &Path) -> Option<String> {
    dotnet::read_dll(path).and_then(|evidence| evidence.assembly_version)
}

pub(crate) fn read_mod_version(path: &Path) -> Option<String> {
    let evidence = dotnet::read_dll(path)?;
    evidence.plugin_version.or(evidence.assembly_version)
}

pub(crate) fn read_dependencies(path: &Path) -> Vec<DependencyRequirement> {
    dotnet::read_dll(path)
        .map(|evidence| evidence.dependencies)
        .unwrap_or_default()
}

pub(crate) fn read_identity(path: &Path) -> Option<(String, Option<String>)> {
    let evidence = dotnet::read_dll(path)?;
    let name = path.file_stem()?.to_string_lossy().into_owned();
    Some((name, evidence.plugin_guid))
}

const DLL_SUFFIX: &str = ".dll";
const DISABLED_SUFFIX: &str = ".disabled";
const MAX_SCAN_DEPTH: usize = 16;

#[derive(Debug)]
pub enum ModsError {
    Missing(PathBuf),
    Read {
        path: PathBuf,
        source: io::Error,
    },
    OutsidePlugins(PathBuf),
    NotAFile(PathBuf),
    ToggleTargetExists {
        from: PathBuf,
        to: PathBuf,
    },
    Toggle {
        from: PathBuf,
        to: PathBuf,
        source: io::Error,
    },
    Delete {
        path: PathBuf,
        source: io::Error,
    },
}

impl std::fmt::Display for ModsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(path) => write!(formatter, "{} was not found.", path.display()),
            Self::Read { path, source } => {
                write!(formatter, "Could not read {}: {source}", path.display())
            }
            Self::OutsidePlugins(path) => {
                write!(
                    formatter,
                    "{} is outside the Plugins folder.",
                    path.display()
                )
            }
            Self::NotAFile(path) => {
                write!(formatter, "{} is not a regular file.", path.display())
            }
            Self::ToggleTargetExists { from, to } => write!(
                formatter,
                "Cannot rename '{}' because '{}' already exists.",
                file_name(from),
                file_name(to)
            ),
            Self::Toggle { from, to, source } => write!(
                formatter,
                "Could not rename '{}' to '{}': {source}",
                file_name(from),
                file_name(to)
            ),
            Self::Delete { path, source } => {
                write!(formatter, "Could not delete {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ModsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Missing(_) | Self::OutsidePlugins(_) | Self::NotAFile(_) => None,
            Self::ToggleTargetExists { .. } => None,
            Self::Read { source, .. }
            | Self::Toggle { source, .. }
            | Self::Delete { source, .. } => Some(source),
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModStatus {
    Disabled,
    Unlisted,
    UpToDate,
    OutOfDate,
    Unknown,
}

impl ModStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disabled => "Disabled",
            Self::Unlisted => "Not listed",
            Self::UpToDate => "Up to date",
            Self::OutOfDate => "Update available",
            Self::Unknown => "Update unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledMod {
    pub name: String,
    pub installed_version: Option<String>,
    pub assembly_version: Option<String>,
    pub bepinex_version: Option<String>,
    pub expected_version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub guid: Option<String>,
    pub nexus_url: Option<String>,
    pub metadata_id: Option<String>,
    pub source_path: PathBuf,
    pub dll_paths: Vec<PathBuf>,
    pub modified_at: Option<DateTime<Utc>>,
    pub expected_modified_at: Option<DateTime<Utc>>,
    pub disabled: bool,
    pub status: ModStatus,
    pub declared_dependencies: Vec<DependencyRequirement>,
    pub missing_dependencies: Vec<String>,
    pub incompatible_with: Vec<String>,
}

impl InstalledMod {
    pub fn row_key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}",
            self.name,
            self.status.label(),
            self.version_label(),
            self.evidence_label(),
            self.disabled,
            self.source_path.display()
        )
    }

    pub fn version_label(&self) -> String {
        match (&self.installed_version, &self.expected_version) {
            (Some(installed), Some(expected)) if installed != expected => {
                format!("{installed} → {expected}")
            }
            (Some(installed), _) => installed.clone(),
            (None, Some(expected)) => format!("{expected} (catalogue)"),
            _ => String::from("No version"),
        }
    }

    pub fn author_label(&self) -> String {
        self.author
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| String::from("Unknown author"))
    }

    pub fn evidence_label(&self) -> String {
        let mut evidence = Vec::new();
        if let Some(version) = &self.bepinex_version {
            evidence.push(format!("BepInEx {version}"));
        }
        if let Some(version) = &self.assembly_version {
            evidence.push(format!("Assembly {version}"));
        }
        if let Some(modified) = self.modified_at {
            evidence.push(format!("Modified {}", modified.format("%Y-%m-%d %H:%M")));
        }
        if evidence.is_empty() {
            String::from("No version evidence found")
        } else {
            evidence.join(" · ")
        }
    }

    pub fn status_label(&self) -> &'static str {
        self.status.label()
    }

    pub fn status_detail(&self) -> String {
        match self.status {
            ModStatus::Disabled => String::from("Disabled — re-enable to manage updates."),
            ModStatus::Unlisted => String::from(
                "Not listed in the community catalogue; release checks can still find updates.",
            ),
            ModStatus::UpToDate => String::from(
                "Up to date: the available version or file-time evidence is not older than the catalogue.",
            ),
            ModStatus::OutOfDate => String::from(
                "Out of date: the installed version is older than the catalogue version.",
            ),
            ModStatus::Unknown => String::from("No comparable version or modified time was found."),
        }
    }

    pub fn description_label(&self) -> String {
        self.description
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| String::from("No description was supplied by the mod author."))
    }
}

pub fn plugins_dir(game_root: &Path) -> Result<PathBuf, ModsError> {
    let bepinex = paths::find_child(game_root, "BepInEx")
        .ok_or_else(|| ModsError::Missing(game_root.join("BepInEx")))?;
    let plugins = paths::find_child(&bepinex, "Plugins")
        .ok_or_else(|| ModsError::Missing(bepinex.join("Plugins")))?;
    if !plugins.is_dir() {
        return Err(ModsError::Missing(plugins));
    }
    Ok(plugins)
}

pub fn find_plugins_dir(game_root: &Path) -> Option<PathBuf> {
    plugins_dir(game_root).ok()
}

pub fn scan(
    plugins: &Path,
    metadata: &[MetadataMod],
    catalogs: &Catalogs,
    ignored_dependencies: &[String],
) -> Result<Vec<InstalledMod>, ModsError> {
    let entries = mod_entries(plugins)?;

    let mut mods = entries
        .iter()
        .filter_map(|path| build_mod(path, metadata))
        .collect::<Vec<_>>();
    annotate(&mut mods, catalogs, ignored_dependencies);
    Ok(mods)
}

pub fn set_enabled(plugins: &Path, item: &InstalledMod, enabled: bool) -> Result<usize, ModsError> {
    rename_dlls(plugins, &item.dll_paths, enabled)
}

pub fn set_all_enabled(plugins: &Path, enabled: bool) -> Result<usize, ModsError> {
    set_all_enabled_except(plugins, enabled, &[])
}

pub(crate) fn set_all_enabled_except(
    plugins: &Path,
    enabled: bool,
    skip: &[PathBuf],
) -> Result<usize, ModsError> {
    let dlls = collect_dlls(plugins)
        .into_iter()
        .filter(|dll| !is_skipped(dll, skip))
        .collect::<Vec<_>>();
    rename_dlls(plugins, &dlls, enabled)
}

fn is_skipped(path: &Path, skip: &[PathBuf]) -> bool {
    skip.iter().any(|root| path.starts_with(root))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteScope {
    pub roots: Vec<PathBuf>,
    pub files: usize,
}

pub fn delete_scope(plugins: &Path, item: &InstalledMod) -> Result<DeleteScope, ModsError> {
    scope_of(plugins, vec![item.source_path.clone()])
}

pub fn delete_all_scope(plugins: &Path) -> Result<DeleteScope, ModsError> {
    delete_all_scope_except(plugins, &[])
}

pub(crate) fn delete_all_scope_except(
    plugins: &Path,
    skip: &[PathBuf],
) -> Result<DeleteScope, ModsError> {
    let roots = all_entries(plugins)?
        .into_iter()
        .filter(|root| !is_skipped(root, skip))
        .collect();
    scope_of(plugins, roots)
}

pub fn delete_mod(plugins: &Path, item: &InstalledMod) -> Result<usize, ModsError> {
    delete_roots(plugins, vec![item.source_path.clone()])
}

pub fn delete_all(plugins: &Path) -> Result<usize, ModsError> {
    delete_all_except(plugins, &[])
}

pub(crate) fn delete_all_except(plugins: &Path, skip: &[PathBuf]) -> Result<usize, ModsError> {
    let roots = all_entries(plugins)?
        .into_iter()
        .filter(|root| !is_skipped(root, skip))
        .collect();
    delete_roots(plugins, roots)
}

fn scope_of(plugins: &Path, roots: Vec<PathBuf>) -> Result<DeleteScope, ModsError> {
    for root in &roots {
        ensure_direct_child(plugins, root)?;
    }
    let files = roots.iter().map(|root| count_files(root)).sum();
    Ok(DeleteScope { roots, files })
}

fn delete_roots(plugins: &Path, roots: Vec<PathBuf>) -> Result<usize, ModsError> {
    for root in &roots {
        ensure_direct_child(plugins, root)?;
    }
    let mut removed = 0;
    for root in &roots {
        let metadata = fs::symlink_metadata(root).map_err(|source| ModsError::Read {
            path: root.clone(),
            source,
        })?;
        let files = count_files(root);
        let result = if metadata.is_dir() {
            fs::remove_dir_all(root)
        } else {
            fs::remove_file(root)
        };
        result.map_err(|source| ModsError::Delete {
            path: root.clone(),
            source,
        })?;
        removed += files;
    }
    Ok(removed)
}

fn ensure_direct_child(plugins: &Path, path: &Path) -> Result<(), ModsError> {
    let relative = path
        .strip_prefix(plugins)
        .map_err(|_| ModsError::OutsidePlugins(path.to_path_buf()))?;
    let mut parts = relative.components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(ModsError::OutsidePlugins(path.to_path_buf())),
    }
}

fn count_files(path: &Path) -> usize {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !metadata.is_dir() {
        return 1;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| count_files(&entry.path()))
        .sum()
}

fn mod_entries(plugins: &Path) -> Result<Vec<PathBuf>, ModsError> {
    Ok(all_entries(plugins)?
        .into_iter()
        .filter(|path| is_mod_entry(path))
        .collect())
}

fn all_entries(plugins: &Path) -> Result<Vec<PathBuf>, ModsError> {
    let listing = fs::read_dir(plugins).map_err(|source| ModsError::Read {
        path: plugins.to_path_buf(),
        source,
    })?;
    let mut entries = listing
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    entries.sort_by_key(|path| path.to_string_lossy().to_lowercase());
    Ok(entries)
}

fn rename_dlls(plugins: &Path, dlls: &[PathBuf], enabled: bool) -> Result<usize, ModsError> {
    let mut renames = Vec::new();
    for dll in dlls {
        if is_disabled_path(dll) != enabled {
            continue;
        }
        if !paths::is_inside(dll, plugins) {
            return Err(ModsError::OutsidePlugins(dll.clone()));
        }
        renames.push((dll.clone(), toggled_path(dll, enabled)?));
    }

    for (from, to) in &renames {
        let metadata = fs::symlink_metadata(from).map_err(|source| ModsError::Read {
            path: from.clone(),
            source,
        })?;
        if !metadata.file_type().is_file() {
            return Err(ModsError::NotAFile(from.clone()));
        }
        if fs::symlink_metadata(to).is_ok() {
            return Err(ModsError::ToggleTargetExists {
                from: from.clone(),
                to: to.clone(),
            });
        }
    }

    let mut done = Vec::new();
    for (from, to) in &renames {
        if let Err(source) = fs::rename(from, to) {
            for (done_from, done_to) in done.iter().rev() {
                let _ = fs::rename(done_to, done_from);
            }
            return Err(ModsError::Toggle {
                from: from.clone(),
                to: to.clone(),
                source,
            });
        }
        done.push((from.clone(), to.clone()));
    }
    Ok(done.len())
}

fn toggled_path(path: &Path, enabled: bool) -> Result<PathBuf, ModsError> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| ModsError::NotAFile(path.to_path_buf()))?;
    if enabled {
        let lower = name.to_ascii_lowercase();
        let stripped = if lower.ends_with(DISABLED_SUFFIX) {
            &name[..name.len() - DISABLED_SUFFIX.len()]
        } else {
            name
        };
        Ok(path.with_file_name(stripped))
    } else {
        Ok(path.with_file_name(format!("{name}{DISABLED_SUFFIX}")))
    }
}

fn is_mod_entry(path: &Path) -> bool {
    if path.is_dir() {
        return true;
    }
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    is_dll_name(name)
}

fn is_dll_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(DLL_SUFFIX) || lower.ends_with(&format!("{DLL_SUFFIX}{DISABLED_SUFFIX}"))
}

fn is_disabled_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| {
            name.to_ascii_lowercase()
                .ends_with(&format!("{DLL_SUFFIX}{DISABLED_SUFFIX}"))
        })
}

fn dll_stem(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let lower = name.to_ascii_lowercase();
    let without_disabled = if lower.ends_with(&format!("{DLL_SUFFIX}{DISABLED_SUFFIX}")) {
        &name[..name.len() - DISABLED_SUFFIX.len()]
    } else {
        name
    };
    let lower = without_disabled.to_ascii_lowercase();
    let stem_len = lower.strip_suffix(DLL_SUFFIX)?.len();
    Some(without_disabled[..stem_len].to_string())
}

fn collect_dlls(path: &Path) -> Vec<PathBuf> {
    let mut dlls = Vec::new();
    if path.is_file() {
        if is_dll_name(
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default(),
        ) {
            dlls.push(path.to_path_buf());
        }
        return dlls;
    }
    walk_directory(path, 0, &mut dlls);
    dlls.sort_by_key(|item| item.to_string_lossy().to_lowercase());
    dlls
}

fn walk_directory(directory: &Path, depth: usize, dlls: &mut Vec<PathBuf>) {
    if depth > MAX_SCAN_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            walk_directory(&path, depth + 1, dlls);
        } else if is_dll_name(
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default(),
        ) {
            dlls.push(path);
        }
    }
}

pub(crate) fn build_mod(path: &Path, metadata: &[MetadataMod]) -> Option<InstalledMod> {
    build_mod_with_preferred(path, metadata, &[], None)
}

pub(crate) fn build_mod_with_preferred(
    path: &Path,
    metadata: &[MetadataMod],
    preferred_stems: &[String],
    preferred_name: Option<&str>,
) -> Option<InstalledMod> {
    let dlls = collect_dlls(path);
    if dlls.is_empty() {
        return None;
    }
    let disabled = dlls.iter().all(|dll| is_disabled_path(dll));
    let primary = preferred_stems
        .iter()
        .find_map(|preferred| {
            let wanted = normalize(preferred);
            dlls.iter()
                .find(|dll| dll_stem(dll).is_some_and(|stem| normalize(&stem) == wanted))
        })
        .or_else(|| dlls.iter().find(|dll| !is_disabled_path(dll)))
        .or_else(|| dlls.first());
    let primary_stem = primary.and_then(|dll| dll_stem(dll));
    let first_stem = primary_stem
        .clone()
        .or_else(|| dlls.iter().find_map(|dll| dll_stem(dll)));
    let matched = preferred_name
        .and_then(|name| metadata_for_name(name, metadata))
        .filter(|item| {
            primary_stem.as_deref().is_none_or(|stem| {
                let wanted = normalize(stem);
                item.dll_names
                    .iter()
                    .any(|name| normalize_dll_name(name) == wanted)
            })
        })
        .or_else(|| {
            primary_stem
                .as_deref()
                .and_then(|stem| metadata_for(stem, metadata))
        })
        .or_else(|| {
            dlls.iter().find_map(|dll| {
                let stem = dll_stem(dll)?;
                metadata_for(&stem, metadata)
            })
        });

    let name = matched
        .map(|item| item.name.clone())
        .or_else(|| primary_stem.clone())
        .or_else(|| {
            dlls.iter()
                .find(|dll| !is_disabled_path(dll))
                .and_then(|dll| dll_stem(dll))
        })
        .or(first_stem)
        .unwrap_or_else(|| {
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Unknown mod")
                .to_string()
        });

    let modified_at = dlls.iter().filter_map(|dll| modified_utc(dll)).max();
    let expected_modified_at = matched.and_then(|item| item.latest_file_modified_utc());
    let expected_version = matched
        .map(|item| item.version.clone())
        .filter(|value| !value.trim().is_empty());
    let evidence = dll_evidence(&dlls, primary);
    let installed_version = evidence
        .plugin_version
        .clone()
        .or_else(|| evidence.assembly_version.clone())
        .filter(|value| !value.trim().is_empty());
    let status = if disabled {
        ModStatus::Disabled
    } else if matched.is_none() {
        ModStatus::Unlisted
    } else {
        compare_update(
            installed_version.as_deref(),
            expected_version.as_deref(),
            modified_at,
            expected_modified_at,
        )
    };

    Some(InstalledMod {
        name,
        installed_version,
        assembly_version: evidence.assembly_version,
        bepinex_version: evidence.plugin_version,
        expected_version,
        author: matched.map(|item| item.author.clone()),
        description: matched.map(|item| item.description.clone()),
        guid: evidence
            .plugin_guid
            .or_else(|| matched.and_then(|item| item.plugin_guid().map(str::to_owned))),
        nexus_url: matched.and_then(|item| item.nexus_url().map(str::to_owned)),
        metadata_id: matched
            .map(|item| item.id.clone())
            .filter(|value| !value.is_empty()),
        source_path: path.to_path_buf(),
        dll_paths: dlls,
        modified_at,
        expected_modified_at,
        disabled,
        status,
        declared_dependencies: evidence.dependencies,
        missing_dependencies: Vec::new(),
        incompatible_with: Vec::new(),
    })
}

fn dll_evidence(dlls: &[PathBuf], primary: Option<&PathBuf>) -> dotnet::DllEvidence {
    let primary_index = primary.and_then(|primary| dlls.iter().position(|dll| dll == primary));
    let mut order = Vec::with_capacity(dlls.len());
    if let Some(index) = primary_index {
        order.push(index);
    }
    order.extend(
        dlls.iter()
            .enumerate()
            .filter(|(index, dll)| Some(*index) != primary_index && !is_disabled_path(dll))
            .map(|(index, _)| index),
    );
    order.extend(
        dlls.iter()
            .enumerate()
            .filter(|(index, dll)| Some(*index) != primary_index && is_disabled_path(dll))
            .map(|(index, _)| index),
    );
    let mut evidence = dotnet::DllEvidence::default();
    for index in order {
        let dll = &dlls[index];
        let Some(found) = dotnet::read_dll(dll) else {
            continue;
        };
        if evidence.assembly_version.is_none() {
            evidence.assembly_version = found.assembly_version;
        }
        if evidence.plugin_guid.is_none() {
            evidence.plugin_guid = found.plugin_guid;
        }
        if evidence.plugin_version.is_none() {
            evidence.plugin_version = found.plugin_version;
        }
        for dependency in found.dependencies {
            if !evidence
                .dependencies
                .iter()
                .any(|existing| existing.matches(&dependency.name, dependency.guid.as_deref()))
            {
                evidence.dependencies.push(dependency);
            }
        }
    }
    evidence
}

fn compare_update(
    installed: Option<&str>,
    expected: Option<&str>,
    modified: Option<DateTime<Utc>>,
    expected_modified: Option<DateTime<Utc>>,
) -> ModStatus {
    if let (Some(installed), Some(expected)) = (installed, expected) {
        let installed = VersionKey::parse(installed).filter(|version| !version.is_zero());
        let expected = VersionKey::parse(expected).filter(|version| !version.is_zero());
        if let (Some(installed), Some(expected)) = (installed, expected) {
            return if installed < expected {
                ModStatus::OutOfDate
            } else {
                ModStatus::UpToDate
            };
        }
    }
    match (modified, expected_modified) {
        (Some(modified), Some(expected)) if modified >= expected => ModStatus::UpToDate,
        _ => ModStatus::Unknown,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct VersionKey([u16; 4]);

impl VersionKey {
    fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let value = value
            .strip_prefix('v')
            .or_else(|| value.strip_prefix('V'))
            .unwrap_or(value);
        let mut parts = [0_u16; 4];
        let mut count = 0;
        for part in value.split('.') {
            if count == parts.len() {
                return None;
            }
            parts[count] = part.parse().ok()?;
            count += 1;
        }
        if count == 0 { None } else { Some(Self(parts)) }
    }

    fn is_zero(self) -> bool {
        self.0 == [0; 4]
    }
}

fn modified_utc(path: &Path) -> Option<DateTime<Utc>> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()
        .map(DateTime::<Utc>::from)
}

fn metadata_for<'a>(stem: &str, metadata: &'a [MetadataMod]) -> Option<&'a MetadataMod> {
    let normalized = normalize(stem);
    let mut best: Option<(usize, &MetadataMod)> = None;
    for item in metadata {
        let name_match = normalize(&item.name) == normalized;
        let dll_match = item
            .dll_names
            .iter()
            .any(|name| normalize_dll_name(name) == normalized);
        if !name_match && !dll_match {
            continue;
        }
        let score = if name_match {
            0
        } else if item.dll_names.len() == 1 {
            1
        } else {
            item.dll_names.len()
        };
        if best.is_none_or(|(best_score, _)| score < best_score) {
            best = Some((score, item));
        }
    }
    best.map(|(_, item)| item)
}

fn metadata_for_name<'a>(name: &str, metadata: &'a [MetadataMod]) -> Option<&'a MetadataMod> {
    let normalized = normalize(name);
    metadata
        .iter()
        .find(|item| normalize(&item.name) == normalized)
}

fn normalize_dll_name(value: &str) -> String {
    let mut stem = value.trim();
    for suffix in [DISABLED_SUFFIX, DLL_SUFFIX] {
        let lower = stem.to_ascii_lowercase();
        if !lower.ends_with(suffix) {
            continue;
        }
        if let Some(prefix) = stem.get(..stem.len() - suffix.len()) {
            stem = prefix;
        }
    }
    normalize(stem)
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub(crate) fn annotate(
    mods: &mut [InstalledMod],
    catalogs: &Catalogs,
    ignored_dependencies: &[String],
) {
    annotate_dependencies(mods, catalogs, ignored_dependencies);
    mods.sort_by_key(|item| item.name.to_lowercase());
}

fn annotate_dependencies(
    mods: &mut [InstalledMod],
    catalogs: &Catalogs,
    ignored_dependencies: &[String],
) {
    let active_names = mods
        .iter()
        .filter(|item| !item.disabled)
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    let active_mods = mods
        .iter()
        .filter(|item| !item.disabled)
        .cloned()
        .collect::<Vec<_>>();

    for item in mods.iter_mut() {
        item.missing_dependencies.clear();
        item.incompatible_with.clear();
        if item.disabled {
            continue;
        }
        let mut requirements = item.declared_dependencies.clone();
        requirements.extend(
            catalogs
                .dependencies
                .requirements_for(&[item.name.as_str()]),
        );
        item.missing_dependencies =
            dependencies::missing_labels(&requirements, &active_mods, ignored_dependencies);
        item.incompatible_with = catalogs
            .incompatibilities
            .conflicts(&item.name, &active_names);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::parse_mods;
    use crate::test_support::temp_dir;

    fn catalogue(json: &str) -> Vec<MetadataMod> {
        parse_mods(json).unwrap()
    }

    #[test]
    fn scans_directories_files_and_disabled_dlls() {
        let dir = temp_dir("mods-scan");
        let plugins = dir.join("BepInEx").join("Plugins");
        fs::create_dir_all(plugins.join("Pack")).unwrap();
        fs::write(plugins.join("Pack").join("PackCore.dll"), b"x").unwrap();
        fs::write(plugins.join("Pack").join("PackExtra.dll.disabled"), b"x").unwrap();
        fs::create_dir_all(plugins.join("Off")).unwrap();
        fs::write(plugins.join("Off").join("Off.dll.disabled"), b"x").unwrap();
        fs::write(plugins.join("Loose.dll"), b"x").unwrap();

        let metadata = catalogue(
            r#"[
                {"Id":"1","Name":"Pack Mod","Version":"2.0","DllNames":["PackCore"],"Author":"Pack Author"},
                {"Id":"2","Name":"Loose","Version":"1.0","DllNames":["Loose"],"Author":"Loose Author"}
            ]"#,
        );
        let mods = scan(&plugins, &metadata, &Catalogs::default(), &[]).unwrap();
        assert_eq!(mods.len(), 3);
        let pack = mods.iter().find(|item| item.name == "Pack Mod").unwrap();
        assert!(!pack.disabled);
        assert_eq!(pack.expected_version.as_deref(), Some("2.0"));
        assert_eq!(pack.author_label(), "Pack Author");
        assert_eq!(pack.dll_paths.len(), 2);
        let off = mods.iter().find(|item| item.name == "Off").unwrap();
        assert!(off.disabled);
        assert_eq!(off.status, ModStatus::Disabled);
        let loose = mods.iter().find(|item| item.name == "Loose").unwrap();
        assert!(!loose.disabled);
        assert_eq!(loose.expected_version.as_deref(), Some("1.0"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn preferred_dll_wins_over_alphabetical_order() {
        let dir = temp_dir("mods-preferred-dll");
        let pack = dir.join("Casualties-Together");
        fs::create_dir_all(pack.join("Together")).unwrap();
        fs::write(pack.join("CUCoreLib.dll"), b"x").unwrap();
        fs::write(pack.join("Together").join("CasualtiesMP.dll"), b"x").unwrap();

        let metadata = catalogue(
            r#"[
                {"Id":"1","Name":"CUCoreLib","Version":"1.1","DllNames":["CUCoreLib.dll"]},
                {"Id":"2","Name":"Casualties Together","Version":"5.5.3","DllNames":["CasualtiesMP.dll"]},
                {"Id":"3","Name":"CasualtiesMP","Version":"9.9","DllNames":["CasualtiesMP.dll"]}
            ]"#,
        );
        let default = build_mod(&pack, &metadata).unwrap();
        assert_eq!(default.name, "CUCoreLib");

        let ambiguous =
            build_mod_with_preferred(&pack, &metadata, &[String::from("CasualtiesMP")], None)
                .unwrap();
        assert_eq!(ambiguous.expected_version.as_deref(), Some("9.9"));

        let preferred = build_mod_with_preferred(
            &pack,
            &metadata,
            &[String::from("CasualtiesMP")],
            Some("Casualties Together"),
        )
        .unwrap();
        assert_eq!(preferred.name, "Casualties Together");
        assert_eq!(preferred.expected_version.as_deref(), Some("5.5.3"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn uses_file_time_only_for_positive_update_evidence() {
        let dir = temp_dir("mods-status");
        let plugins = dir.join("Plugins");
        fs::create_dir_all(&plugins).unwrap();
        fs::write(plugins.join("Old.dll"), b"x").unwrap();
        fs::write(plugins.join("Future.dll"), b"x").unwrap();

        let metadata = catalogue(
            r#"[
                {"Id":"1","Name":"Old","Version":"1.0","DllNames":["Old"],"FileModifiedDate":"2000-01-01T00:00:00Z"},
                {"Id":"2","Name":"Future","Version":"1.0","DllNames":["Future"],"FileModifiedDate":"2999-01-01T00:00:00Z"}
            ]"#,
        );
        let mods = scan(&plugins, &metadata, &Catalogs::default(), &[]).unwrap();
        let old = mods.iter().find(|item| item.name == "Old").unwrap();
        let future = mods.iter().find(|item| item.name == "Future").unwrap();
        assert_eq!(old.status, ModStatus::UpToDate);
        assert_eq!(future.status, ModStatus::Unknown);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unmatched_mods_are_listed_without_a_catalogue_match() {
        let dir = temp_dir("mods-unmatched");
        let plugins = dir.join("Plugins");
        fs::create_dir_all(&plugins).unwrap();
        fs::write(plugins.join("UnknownThing.dll"), b"x").unwrap();

        let mods = scan(&plugins, &[], &Catalogs::default(), &[]).unwrap();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name, "UnknownThing");
        assert_eq!(mods[0].status, ModStatus::Unlisted);
        assert!(mods[0].version_label().contains("No version"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn fills_missing_dependencies_but_not_disabled_mods() {
        let dir = temp_dir("mods-dependencies");
        let plugins = dir.join("Plugins");
        fs::create_dir_all(&plugins).unwrap();
        fs::write(plugins.join("CorePack.dll"), b"x").unwrap();
        fs::write(plugins.join("NeedsCore.dll"), b"x").unwrap();
        fs::write(plugins.join("Off.dll.disabled"), b"x").unwrap();

        let metadata = catalogue(
            r#"[
                {"Id":"1","Name":"CorePack","Version":"1.0","DllNames":["CorePack"]},
                {"Id":"2","Name":"NeedsCore","Version":"1.0","DllNames":["NeedsCore"]},
                {"Id":"3","Name":"Off","Version":"1.0","DllNames":["Off"]}
            ]"#,
        );
        let catalogs = Catalogs::default();
        let mods = scan(&plugins, &metadata, &catalogs, &[]).unwrap();
        assert!(mods.iter().all(|item| item.missing_dependencies.is_empty()));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn compares_versions_and_ignores_placeholders() {
        assert_eq!(
            compare_update(Some("1.2.0"), Some("1.2.0.0"), None, None),
            ModStatus::UpToDate
        );
        assert_eq!(
            compare_update(Some("1.2.0.1"), Some("1.2.0"), None, None),
            ModStatus::UpToDate
        );
        assert_eq!(
            compare_update(Some("1.2.0"), Some("1.2.1"), None, None),
            ModStatus::OutOfDate
        );
        assert_eq!(
            compare_update(Some("0.0.0.0"), Some("1.0"), None, None),
            ModStatus::Unknown
        );
    }

    #[test]
    fn missing_plugins_is_not_an_error_for_discovery() {
        let dir = temp_dir("mods-find-plugins");
        let game = dir.join("Game");
        fs::create_dir_all(&game).unwrap();
        assert!(find_plugins_dir(&game).is_none());
        fs::create_dir_all(game.join("BepInEx").join("Plugins")).unwrap();
        assert!(find_plugins_dir(&game).is_some());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn plugins_dir_resolves_the_real_case_of_the_game_layout() {
        let dir = temp_dir("mods-plugins");
        let game = dir.join("Casualties Unknown");
        fs::create_dir_all(game.join("BepInEx").join("Plugins")).unwrap();
        let found = plugins_dir(&game).unwrap();
        assert_eq!(found, game.join("BepInEx").join("Plugins"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn toggles_every_dll_of_a_mod() {
        let dir = temp_dir("mods-toggle");
        let plugins = dir.join("Plugins");
        fs::create_dir_all(plugins.join("Pack")).unwrap();
        fs::write(plugins.join("Pack").join("PackCore.dll"), b"x").unwrap();
        fs::write(plugins.join("Pack").join("PackExtra.dll"), b"x").unwrap();

        let before = scan(&plugins, &[], &Catalogs::default(), &[]).unwrap();
        assert_eq!(before.len(), 1);
        assert!(!before[0].disabled);

        set_enabled(&plugins, &before[0], false).unwrap();
        assert!(plugins.join("Pack").join("PackCore.dll.disabled").exists());
        assert!(plugins.join("Pack").join("PackExtra.dll.disabled").exists());
        let disabled = scan(&plugins, &[], &Catalogs::default(), &[]).unwrap();
        assert!(disabled[0].disabled);
        assert_eq!(disabled[0].status, ModStatus::Disabled);

        set_enabled(&plugins, &disabled[0], true).unwrap();
        assert!(plugins.join("Pack").join("PackCore.dll").exists());
        assert!(!plugins.join("Pack").join("PackCore.dll.disabled").exists());
        let enabled = scan(&plugins, &[], &Catalogs::default(), &[]).unwrap();
        assert!(!enabled[0].disabled);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn enabling_a_partly_disabled_mod_enables_the_remaining_dlls() {
        let dir = temp_dir("mods-toggle-mixed");
        let plugins = dir.join("Plugins");
        fs::create_dir_all(plugins.join("Pack")).unwrap();
        fs::write(plugins.join("Pack").join("One.dll"), b"x").unwrap();
        fs::write(plugins.join("Pack").join("Two.dll.disabled"), b"x").unwrap();

        let mods = scan(&plugins, &[], &Catalogs::default(), &[]).unwrap();
        assert!(!mods[0].disabled);

        set_enabled(&plugins, &mods[0], true).unwrap();
        assert!(plugins.join("Pack").join("Two.dll").exists());
        let enabled = scan(&plugins, &[], &Catalogs::default(), &[]).unwrap();
        assert!(!enabled[0].disabled);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn refuses_to_overwrite_a_disabled_target() {
        let dir = temp_dir("mods-toggle-conflict");
        let plugins = dir.join("Plugins");
        fs::create_dir_all(plugins.join("Pack")).unwrap();
        fs::write(plugins.join("Pack").join("One.dll"), b"x").unwrap();
        fs::write(plugins.join("Pack").join("One.dll.disabled"), b"x").unwrap();

        let mods = scan(&plugins, &[], &Catalogs::default(), &[]).unwrap();
        let error = set_enabled(&plugins, &mods[0], false).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Cannot rename 'One.dll' because 'One.dll.disabled' already exists."
        );
        assert!(plugins.join("Pack").join("One.dll").exists());
        assert!(plugins.join("Pack").join("One.dll.disabled").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_scope_counts_files_before_removing_a_mod() {
        let dir = temp_dir("mods-delete");
        let plugins = dir.join("Plugins");
        fs::create_dir_all(plugins.join("Pack").join("Lib")).unwrap();
        fs::write(plugins.join("Pack").join("Core.dll"), b"x").unwrap();
        fs::write(plugins.join("Pack").join("Lib").join("Extra.dll"), b"x").unwrap();
        fs::write(plugins.join("Keep.dll"), b"x").unwrap();

        let mods = scan(&plugins, &[], &Catalogs::default(), &[]).unwrap();
        let pack = mods
            .iter()
            .find(|item| item.source_path == plugins.join("Pack"))
            .unwrap();
        let scope = delete_scope(&plugins, pack).unwrap();
        assert_eq!(scope.roots, vec![plugins.join("Pack")]);
        assert_eq!(scope.files, 2);

        assert_eq!(delete_mod(&plugins, pack).unwrap(), 2);
        assert!(!plugins.join("Pack").exists());
        assert!(plugins.join("Keep.dll").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_all_removes_every_mod_entry_only() {
        let dir = temp_dir("mods-delete-all");
        let plugins = dir.join("Plugins");
        fs::create_dir_all(plugins.join("Pack")).unwrap();
        fs::write(plugins.join("Pack").join("Core.dll"), b"x").unwrap();
        fs::write(plugins.join("Loose.dll"), b"x").unwrap();
        fs::write(plugins.join("Off.dll.disabled"), b"x").unwrap();
        fs::write(plugins.join("notes.txt"), b"x").unwrap();

        let scope = delete_all_scope(&plugins).unwrap();
        assert_eq!(scope.roots.len(), 4);
        assert_eq!(scope.files, 4);

        assert_eq!(delete_all(&plugins).unwrap(), 4);
        assert!(plugins.is_dir());
        assert!(!plugins.join("notes.txt").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn refuses_to_delete_outside_the_plugins_folder() {
        let dir = temp_dir("mods-delete-outside");
        let plugins = dir.join("Plugins");
        fs::create_dir_all(plugins.join("Pack")).unwrap();
        fs::write(dir.join("Outside.dll"), b"x").unwrap();

        let outside = dir.join("Outside.dll");
        let error = delete_roots(&plugins, vec![outside.clone()]).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("{} is outside the Plugins folder.", outside.display())
        );
        assert!(delete_roots(&plugins, vec![plugins.join("Pack").join("Core.dll")]).is_err());
        assert!(delete_roots(&plugins, vec![plugins.clone()]).is_err());
        assert!(outside.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn skips_managed_entries_when_changing_every_mod() {
        let dir = temp_dir("mods-skip-managed");
        let plugins = dir.join("Plugins");
        fs::create_dir_all(plugins.join("Pack")).unwrap();
        fs::write(plugins.join("Pack").join("Pack.dll"), b"x").unwrap();
        fs::write(plugins.join("Linked.dll"), b"x").unwrap();
        fs::write(plugins.join("Loose.dll"), b"x").unwrap();

        let managed = vec![plugins.join("Pack"), plugins.join("Linked.dll")];
        assert_eq!(
            set_all_enabled_except(&plugins, false, &managed).unwrap(),
            1
        );
        assert!(plugins.join("Loose.dll.disabled").exists());
        assert!(plugins.join("Pack").join("Pack.dll").exists());
        assert!(plugins.join("Linked.dll").exists());

        let scope = delete_all_scope_except(&plugins, &managed).unwrap();
        assert_eq!(scope.roots, vec![plugins.join("Loose.dll.disabled")]);
        assert_eq!(delete_all_except(&plugins, &managed).unwrap(), 1);
        assert!(plugins.join("Pack").join("Pack.dll").exists());
        assert!(plugins.join("Linked.dll").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_toggle_a_symlinked_dll() {
        use std::os::unix::fs::symlink;

        let dir = temp_dir("mods-toggle-symlink");
        let plugins = dir.join("Plugins");
        fs::create_dir_all(&plugins).unwrap();
        let outside = dir.join("Real.dll");
        fs::write(&outside, b"x").unwrap();
        symlink(&outside, plugins.join("Linked.dll")).unwrap();

        let mods = scan(&plugins, &[], &Catalogs::default(), &[]).unwrap();
        assert_eq!(mods.len(), 1);
        let error = set_enabled(&plugins, &mods[0], false).unwrap_err();
        assert!(matches!(error, ModsError::NotAFile(_)));
        assert!(outside.exists());
        assert!(!plugins.join("Linked.dll.disabled").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn toggles_every_dll_in_the_plugins_folder() {
        let dir = temp_dir("mods-toggle-all");
        let plugins = dir.join("Plugins");
        fs::create_dir_all(plugins.join("Nested")).unwrap();
        fs::write(plugins.join("One.dll"), b"x").unwrap();
        fs::write(plugins.join("Two.dll.disabled"), b"x").unwrap();
        fs::write(plugins.join("Nested").join("Three.dll"), b"x").unwrap();

        assert_eq!(set_all_enabled(&plugins, false).unwrap(), 2);
        assert!(plugins.join("One.dll.disabled").exists());
        assert!(plugins.join("Nested").join("Three.dll.disabled").exists());
        assert!(plugins.join("Two.dll.disabled").exists());

        assert_eq!(set_all_enabled(&plugins, true).unwrap(), 3);
        assert!(plugins.join("One.dll").exists());
        assert!(plugins.join("Nested").join("Three.dll").exists());
        assert!(plugins.join("Two.dll").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_batch_toggle_stops_before_renaming_anything() {
        let dir = temp_dir("mods-toggle-all-conflict");
        let plugins = dir.join("Plugins");
        fs::create_dir_all(&plugins).unwrap();
        fs::write(plugins.join("A.dll"), b"x").unwrap();
        fs::write(plugins.join("A.dll.disabled"), b"x").unwrap();
        fs::write(plugins.join("B.dll"), b"x").unwrap();

        let error = set_all_enabled(&plugins, false).unwrap_err();
        assert!(matches!(error, ModsError::ToggleTargetExists { .. }));
        assert!(plugins.join("B.dll").exists());
        assert!(!plugins.join("B.dll.disabled").exists());
        let _ = fs::remove_dir_all(dir);
    }
}
