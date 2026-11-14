//! Read-only scan of BepInEx plugins, matched against the cached community catalogue.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::catalogs::{Catalogs, names_match};
use crate::metadata::MetadataMod;
use crate::paths;

const DLL_SUFFIX: &str = ".dll";
const DISABLED_SUFFIX: &str = ".disabled";
const MAX_SCAN_DEPTH: usize = 16;

#[derive(Debug)]
pub enum ModsError {
    Missing(PathBuf),
    Read { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for ModsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(path) => write!(formatter, "{} was not found.", path.display()),
            Self::Read { path, source } => {
                write!(formatter, "Could not read {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ModsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Missing(_) => None,
            Self::Read { source, .. } => Some(source),
        }
    }
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
    pub missing_dependencies: Vec<String>,
    pub incompatible_with: Vec<String>,
}

impl InstalledMod {
    pub fn row_key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.name,
            self.status.label(),
            self.version_label(),
            self.disabled,
            self.source_path.display()
        )
    }

    pub fn version_label(&self) -> String {
        match (&self.installed_version, &self.expected_version) {
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
        match self.modified_at {
            Some(modified) => format!("Modified {}", modified.format("%Y-%m-%d %H:%M")),
            None => String::from("No version evidence found"),
        }
    }

    pub fn status_label(&self) -> &'static str {
        self.status.label()
    }

    pub fn status_detail(&self) -> String {
        match self.status {
            ModStatus::Disabled => String::from("Disabled — re-enable to manage updates."),
            ModStatus::Unlisted => String::from(
                "Not listed in the community catalogue, so update status is not checked.",
            ),
            ModStatus::UpToDate => String::from(
                "Up to date: the modified time is not older than the catalogue metadata.",
            ),
            ModStatus::OutOfDate => {
                String::from("Out of date: the local file is older than the catalogue metadata.")
            }
            ModStatus::Unknown => String::from("No comparable update data was found."),
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

pub fn scan(
    plugins: &Path,
    metadata: &[MetadataMod],
    catalogs: &Catalogs,
    ignored_dependencies: &[String],
) -> Result<Vec<InstalledMod>, ModsError> {
    let mut entries = Vec::new();
    let listing = fs::read_dir(plugins).map_err(|source| ModsError::Read {
        path: plugins.to_path_buf(),
        source,
    })?;
    for entry in listing.flatten() {
        let path = entry.path();
        if is_mod_entry(&path) {
            entries.push(path);
        }
    }
    entries.sort_by_key(|path| path.to_string_lossy().to_lowercase());

    let mut mods = entries
        .iter()
        .filter_map(|path| build_mod(path, metadata))
        .collect::<Vec<_>>();
    annotate_dependencies(&mut mods, catalogs, ignored_dependencies);
    mods.sort_by_key(|item| item.name.to_lowercase());
    Ok(mods)
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

fn build_mod(path: &Path, metadata: &[MetadataMod]) -> Option<InstalledMod> {
    let dlls = collect_dlls(path);
    if dlls.is_empty() {
        return None;
    }
    let disabled = dlls.iter().all(|dll| is_disabled_path(dll));
    let first_stem = dlls.iter().find_map(|dll| dll_stem(dll));
    let matched = dlls.iter().find_map(|dll| {
        let stem = dll_stem(dll)?;
        metadata_for(&stem, metadata)
    });

    let name = matched
        .map(|item| item.name.clone())
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
    let status = if disabled {
        ModStatus::Disabled
    } else if matched.is_none() {
        ModStatus::Unlisted
    } else {
        match (modified_at, expected_modified_at) {
            (Some(modified), Some(expected)) if modified >= expected => ModStatus::UpToDate,
            _ => ModStatus::Unknown,
        }
    };

    Some(InstalledMod {
        name,
        installed_version: None,
        expected_version,
        author: matched.map(|item| item.author.clone()),
        description: matched.map(|item| item.description.clone()),
        guid: matched.and_then(|item| item.plugin_guid().map(str::to_owned)),
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
        missing_dependencies: Vec::new(),
        incompatible_with: Vec::new(),
    })
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
            .any(|name| normalize(name) == normalized);
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

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
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

    for item in mods.iter_mut() {
        item.missing_dependencies.clear();
        item.incompatible_with.clear();
        if item.disabled {
            continue;
        }
        let requirements = catalogs
            .dependencies
            .requirements_for(&[item.name.as_str()]);
        item.missing_dependencies = requirements
            .into_iter()
            .filter(|requirement| {
                !active_names
                    .iter()
                    .any(|name| names_match(name, &requirement.name))
                    && !ignored_dependencies
                        .iter()
                        .any(|name| names_match(name, &requirement.name))
            })
            .map(|requirement| requirement.display_label())
            .collect();
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
    fn plugins_dir_resolves_the_real_case_of_the_game_layout() {
        let dir = temp_dir("mods-plugins");
        let game = dir.join("Casualties Unknown");
        fs::create_dir_all(game.join("BepInEx").join("Plugins")).unwrap();
        let found = plugins_dir(&game).unwrap();
        assert_eq!(found, game.join("BepInEx").join("Plugins"));
        let _ = fs::remove_dir_all(dir);
    }
}
