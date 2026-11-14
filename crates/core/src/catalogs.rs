//! Dependency and incompatibility catalogs loaded from user-editable JSON beside the app.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const DEPENDENCY_FILE: &str = "DependencyCatalog.json";
const INCOMPATIBILITY_FILE: &str = "IncompatibilityCatalog.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DependencyRequirement {
    pub name: String,
    pub minimum_version: Option<String>,
    pub note: Option<String>,
}

impl DependencyRequirement {
    pub fn display_label(&self) -> String {
        let version = match self.minimum_version.as_deref() {
            Some(value) if !value.trim().is_empty() => format!(" {value}+"),
            _ => String::new(),
        };
        let note = match self.note.as_deref() {
            Some(value) if !value.trim().is_empty() => format!(" ({value})"),
            _ => String::new(),
        };
        format!("{}{version}{note}", self.name)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IncompatibilityEntry {
    pub mod_a: String,
    pub mod_b: String,
    pub note: Option<String>,
}

#[derive(Debug)]
pub enum CatalogError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Parse { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for CatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

#[derive(Debug, Default)]
pub struct DependencyCatalog {
    requirements: BTreeMap<String, Vec<DependencyRequirement>>,
}

#[derive(Debug, Default)]
pub struct IncompatibilityCatalog {
    entries: Vec<IncompatibilityEntry>,
}

#[derive(Debug, Default)]
pub struct Catalogs {
    pub dependencies: DependencyCatalog,
    pub incompatibilities: IncompatibilityCatalog,
    pub errors: Vec<CatalogError>,
}

impl Catalogs {
    pub fn load(app_dir: &Path) -> Self {
        let mut catalogs = Self::default();
        if let Some(path) = resolve_catalog(app_dir, DEPENDENCY_FILE) {
            match DependencyCatalog::load(&path) {
                Ok(catalog) => catalogs.dependencies = catalog,
                Err(error) => catalogs.errors.push(error),
            }
        }
        if let Some(path) = resolve_catalog(app_dir, INCOMPATIBILITY_FILE) {
            match IncompatibilityCatalog::load(&path) {
                Ok(catalog) => catalogs.incompatibilities = catalog,
                Err(error) => catalogs.errors.push(error),
            }
        }
        catalogs
    }
}

impl DependencyCatalog {
    pub fn load(path: &Path) -> Result<Self, CatalogError> {
        let text = fs::read_to_string(path).map_err(|source| CatalogError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let document: DependencyDocument =
            serde_json::from_str(&text).map_err(|source| CatalogError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        let mut requirements: BTreeMap<String, Vec<DependencyRequirement>> = BTreeMap::new();
        for entry in document.dependencies {
            let key = normalize_key(&entry.name);
            if key.is_empty() {
                continue;
            }
            let values = requirements.entry(key).or_default();
            values.extend(
                entry
                    .requires
                    .into_iter()
                    .filter(|requirement| !requirement.name.trim().is_empty()),
            );
        }
        Ok(Self { requirements })
    }

    pub fn requirements_for(&self, mod_names: &[&str]) -> Vec<DependencyRequirement> {
        let mut seen = BTreeSet::new();
        let mut result = Vec::new();
        for name in mod_names {
            let Some(values) = self.requirements.get(&normalize_key(name)) else {
                continue;
            };
            for requirement in values {
                let dedupe = format!(
                    "{}|{}",
                    normalize_key(&requirement.name),
                    requirement.minimum_version.as_deref().unwrap_or("")
                );
                if seen.insert(dedupe) {
                    result.push(requirement.clone());
                }
            }
        }
        result.sort_by_key(|requirement| requirement.name.to_lowercase());
        result
    }

    pub fn has_entry_for_any(&self, mod_names: &[&str]) -> bool {
        mod_names
            .iter()
            .any(|name| self.requirements.contains_key(&normalize_key(name)))
    }
}

impl IncompatibilityCatalog {
    pub fn load(path: &Path) -> Result<Self, CatalogError> {
        let text = fs::read_to_string(path).map_err(|source| CatalogError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let document: IncompatibilityDocument =
            serde_json::from_str(&text).map_err(|source| CatalogError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        let entries = document
            .incompatibilities
            .into_iter()
            .filter(|entry| !entry.mod_a.trim().is_empty() && !entry.mod_b.trim().is_empty())
            .collect();
        Ok(Self { entries })
    }

    pub fn conflicts(&self, mod_name: &str, installed_mod_names: &[String]) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut conflicts = Vec::new();
        for entry in &self.entries {
            let other = if names_match(&entry.mod_a, mod_name)
                && installed_mod_names
                    .iter()
                    .any(|name| names_match(name, &entry.mod_b))
            {
                Some(&entry.mod_b)
            } else if names_match(&entry.mod_b, mod_name)
                && installed_mod_names
                    .iter()
                    .any(|name| names_match(name, &entry.mod_a))
            {
                Some(&entry.mod_a)
            } else {
                None
            };
            if let Some(other) = other
                && seen.insert(other.to_lowercase())
            {
                conflicts.push(other.clone());
            }
        }
        conflicts
    }
}

pub fn names_match(left: &str, right: &str) -> bool {
    normalize_key(left) == normalize_key(right)
}

pub(crate) fn normalize_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn resolve_catalog(app_dir: &Path, file: &str) -> Option<PathBuf> {
    [
        app_dir.join("Data").join("Catalogs").join(file),
        app_dir.join("Services").join(file),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct DependencyDocument {
    dependencies: Vec<DependencyEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct DependencyEntry {
    #[serde(rename = "mod")]
    name: String,
    requires: Vec<DependencyRequirement>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct IncompatibilityDocument {
    incompatibilities: Vec<IncompatibilityEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    fn write_catalogs(dir: &Path, dependency: &str, incompatibility: &str) {
        let catalogs = dir.join("Data").join("Catalogs");
        fs::create_dir_all(&catalogs).unwrap();
        fs::write(catalogs.join(DEPENDENCY_FILE), dependency).unwrap();
        fs::write(catalogs.join(INCOMPATIBILITY_FILE), incompatibility).unwrap();
    }

    #[test]
    fn merges_and_deduplicates_requirements_for_several_mods() {
        let dir = temp_dir("catalogs-deps");
        write_catalogs(
            &dir,
            r#"{"dependencies":[
                {"mod":"EdgeSneak","requires":[{"name":"ScavLib API","minimumVersion":"0.3"}]},
                {"mod":"FullSave","requires":[{"name":"ScavLib API"}]},
                {"mod":"Mega","requires":[{"name":"ScavLib API","minimumVersion":"0.3"},{"name":"Other"}]}
            ]}"#,
            "{}",
        );
        let catalogs = Catalogs::load(&dir);
        assert!(catalogs.errors.is_empty());
        let requirements =
            catalogs
                .dependencies
                .requirements_for(&["EdgeSneak", "FullSave", "Mega", "Missing"]);
        assert_eq!(requirements.len(), 3);
        assert_eq!(requirements[0].name, "Other");
        assert_eq!(requirements[1].name, "ScavLib API");
        assert_eq!(requirements[1].minimum_version.as_deref(), Some("0.3"));
        assert_eq!(requirements[2].display_label(), "ScavLib API");
        assert!(catalogs.dependencies.has_entry_for_any(&["full save"]));
        assert!(!catalogs.dependencies.has_entry_for_any(&["Save"]));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn matches_names_ignoring_case_punctuation_and_spacing() {
        assert!(names_match(
            "ICEneco's Custom Item API",
            "icenecos custom item api"
        ));
        assert!(names_match("ScavLib API", "scav lib-api"));
        assert!(!names_match("ScavLib", "ScavLib API"));
    }

    #[test]
    fn labels_include_minimum_version_and_note() {
        let requirement = DependencyRequirement {
            name: "RshLib".to_string(),
            minimum_version: Some("2.0.0".to_string()),
            note: Some("Steam multiplayer".to_string()),
        };
        assert_eq!(
            requirement.display_label(),
            "RshLib 2.0.0+ (Steam multiplayer)"
        );
        let bare = DependencyRequirement {
            name: "CUCoreLib".to_string(),
            minimum_version: Some("  ".to_string()),
            note: None,
        };
        assert_eq!(bare.display_label(), "CUCoreLib");
    }

    #[test]
    fn reports_incompatibilities_both_ways_without_duplicates() {
        let dir = temp_dir("catalogs-incompat");
        write_catalogs(
            &dir,
            "{}",
            r#"{"incompatibilities":[
                {"modA":"Mod A","modB":"Mod B","note":"first"},
                {"modA":"Mod C","modB":"mod a"},
                {"modA":"x","modB":"y"}
            ]}"#,
        );
        let catalogs = Catalogs::load(&dir);
        assert!(catalogs.errors.is_empty());
        let installed = vec!["Mod B".to_string(), "Mod C".to_string()];
        assert_eq!(
            catalogs.incompatibilities.conflicts("mod a", &installed),
            vec!["Mod B".to_string(), "Mod C".to_string()]
        );
        assert_eq!(
            catalogs
                .incompatibilities
                .conflicts("Mod B", &["Mod A".to_string()]),
            vec!["Mod A".to_string()]
        );
        assert!(
            catalogs
                .incompatibilities
                .conflicts("Mod A", &["Unrelated".to_string()])
                .is_empty()
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn prefers_data_catalogs_over_legacy_services() {
        let dir = temp_dir("catalogs-precedence");
        write_catalogs(
            &dir,
            r#"{"dependencies":[{"mod":"New","requires":[{"name":"New Lib"}]}]}"#,
            "{}",
        );
        let legacy = dir.join("Services");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(
            legacy.join(DEPENDENCY_FILE),
            r#"{"dependencies":[{"mod":"Old","requires":[{"name":"Old Lib"}]}]}"#,
        )
        .unwrap();
        let catalogs = Catalogs::load(&dir);
        assert!(catalogs.dependencies.has_entry_for_any(&["New"]));
        assert!(!catalogs.dependencies.has_entry_for_any(&["Old"]));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_catalogs_are_empty_not_errors() {
        let dir = temp_dir("catalogs-missing");
        let catalogs = Catalogs::load(&dir);
        assert!(catalogs.errors.is_empty());
        assert!(!catalogs.dependencies.has_entry_for_any(&["Anything"]));
        assert!(
            catalogs
                .incompatibilities
                .conflicts("Anything", &[])
                .is_empty()
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn malformed_catalogs_report_the_path_and_stay_empty() {
        let dir = temp_dir("catalogs-malformed");
        write_catalogs(&dir, "{not json", "{\"incompatibilities\":[]}");
        let catalogs = Catalogs::load(&dir);
        assert_eq!(catalogs.errors.len(), 1);
        let message = catalogs.errors[0].to_string();
        assert!(message.contains("DependencyCatalog.json"));
        assert!(!catalogs.dependencies.has_entry_for_any(&["Anything"]));
        let _ = fs::remove_dir_all(dir);
    }
}
