//! Match declared dependencies against installed mods and the manifest.

use std::collections::BTreeSet;

use crate::catalogs::{Catalogs, DependencyRequirement, normalize_key};
use crate::sources::{SourceEntry, SourceManifest};
use crate::version::HubVersion;

use super::InstalledMod;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    Missing,
    Outdated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyIssue {
    pub name: String,
    pub minimum_version: Option<String>,
    pub installed_version: Option<String>,
    pub kind: IssueKind,
    pub entry: Option<SourceEntry>,
}

pub(crate) fn requirements_for(
    entry: &SourceEntry,
    catalogs: &Catalogs,
) -> Vec<DependencyRequirement> {
    let mut result = entry.depends.clone();
    result.extend(
        catalogs
            .dependencies
            .requirements_for(&[entry.name.as_str()]),
    );
    let mut seen = BTreeSet::new();
    result.retain(|requirement| seen.insert(requirement_key(requirement)));
    result
}

pub(crate) fn check_requirements(
    requirements: &[DependencyRequirement],
    installed: &[InstalledMod],
    manifest: Option<&SourceManifest>,
) -> Vec<DependencyIssue> {
    let mut issues = Vec::new();
    let mut seen = BTreeSet::new();
    for requirement in requirements {
        if requirement.optional {
            continue;
        }
        let found = installed
            .iter()
            .find(|item| !item.disabled && requirement.matches(&item.name, item.guid.as_deref()));
        let (kind, installed_version) = match found {
            None => (IssueKind::Missing, None),
            Some(item) => {
                let version = installed_version(item);
                let outdated = requirement
                    .minimum_version
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .and_then(|minimum| version_less(version?, minimum))
                    .unwrap_or(false);
                if outdated {
                    (IssueKind::Outdated, version.map(str::to_owned))
                } else {
                    continue;
                }
            }
        };
        let key = format!(
            "{}|{}|{}",
            issue_name(requirement),
            requirement.minimum_version.as_deref().unwrap_or(""),
            matches!(kind, IssueKind::Missing)
        );
        if !seen.insert(key) {
            continue;
        }
        issues.push(DependencyIssue {
            name: issue_name(requirement),
            minimum_version: requirement.minimum_version.clone(),
            installed_version,
            kind,
            entry: source_for(requirement, manifest),
        });
    }
    issues
}

pub(crate) fn missing_labels(
    requirements: &[DependencyRequirement],
    active: &[InstalledMod],
    ignored: &[String],
) -> Vec<String> {
    let mut labels = Vec::new();
    for issue in check_requirements(requirements, active, None) {
        if ignored
            .iter()
            .any(|name| crate::catalogs::names_match(name, &issue.name))
        {
            continue;
        }
        let label = format_label(&issue.name, issue.minimum_version.as_deref());
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    labels
}

fn installed_version(item: &InstalledMod) -> Option<&str> {
    item.installed_version
        .as_deref()
        .or(item.bepinex_version.as_deref())
        .or(item.assembly_version.as_deref())
        .filter(|value| !value.trim().is_empty())
}

fn issue_name(requirement: &DependencyRequirement) -> String {
    if requirement.name.trim().is_empty() {
        requirement.guid.clone().unwrap_or_default()
    } else {
        requirement.name.clone()
    }
}

fn format_label(name: &str, minimum: Option<&str>) -> String {
    match minimum.map(str::trim).filter(|value| !value.is_empty()) {
        Some(minimum) => format!("{name} {minimum}+"),
        None => name.to_owned(),
    }
}

fn requirement_key(requirement: &DependencyRequirement) -> String {
    format!(
        "{}|{}|{}|{}",
        normalize_key(&requirement.name),
        requirement
            .guid
            .as_deref()
            .map(normalize_key)
            .unwrap_or_default(),
        requirement.minimum_version.as_deref().unwrap_or(""),
        requirement.optional
    )
}

fn source_for(
    requirement: &DependencyRequirement,
    manifest: Option<&SourceManifest>,
) -> Option<SourceEntry> {
    let manifest = manifest?;
    manifest
        .entries
        .iter()
        .find(|entry| {
            if let Some(guid) = requirement.guid.as_deref() {
                if entry
                    .matcher
                    .guid
                    .as_deref()
                    .is_some_and(|value| crate::catalogs::names_match(value, guid))
                {
                    return true;
                }
                if crate::catalogs::names_match(&entry.name, guid)
                    || entry
                        .matcher
                        .name
                        .iter()
                        .any(|value| crate::catalogs::names_match(value, guid))
                {
                    return true;
                }
            }
            crate::catalogs::names_match(&entry.name, &requirement.name)
                || entry
                    .matcher
                    .name
                    .iter()
                    .any(|value| crate::catalogs::names_match(value, &requirement.name))
        })
        .cloned()
}

fn version_less(installed: &str, minimum: &str) -> Option<bool> {
    let installed = parse_version(installed)?;
    let minimum = parse_version(minimum)?;
    Some(installed < minimum)
}

fn parse_version(value: &str) -> Option<HubVersion> {
    if let Some(version) = HubVersion::parse(value) {
        return Some(version);
    }
    let clean = value.split('+').next()?.trim();
    let (numeric, prerelease) = match clean.split_once('-') {
        Some((numeric, prerelease)) => (numeric, Some(prerelease)),
        None => (clean, None),
    };
    let parts: Vec<&str> = numeric.split('.').collect();
    if parts.len() < 4 {
        return None;
    }
    let mut normalized = format!("{}.{}.{}", parts[0], parts[1], parts[2]);
    if let Some(prerelease) = prerelease {
        normalized.push('-');
        normalized.push_str(prerelease);
    }
    HubVersion::parse(&normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::ModStatus;
    use crate::sources::{Matcher, Repo};
    use chrono::Utc;
    use std::path::PathBuf;

    fn requirement(name: &str, minimum: Option<&str>) -> DependencyRequirement {
        DependencyRequirement {
            name: name.to_owned(),
            minimum_version: minimum.map(str::to_owned),
            ..DependencyRequirement::default()
        }
    }

    fn installed(name: &str, version: Option<&str>, guid: Option<&str>) -> InstalledMod {
        InstalledMod {
            name: name.to_owned(),
            installed_version: version.map(str::to_owned),
            assembly_version: None,
            bepinex_version: None,
            expected_version: None,
            author: None,
            description: None,
            guid: guid.map(str::to_owned),
            nexus_url: None,
            metadata_id: None,
            source_path: PathBuf::from(name),
            dll_paths: Vec::new(),
            modified_at: None,
            expected_modified_at: None,
            disabled: false,
            status: ModStatus::UpToDate,
            declared_dependencies: Vec::new(),
            missing_dependencies: Vec::new(),
            incompatible_with: Vec::new(),
        }
    }

    fn manifest_entry(name: &str, guid: Option<&str>) -> SourceEntry {
        SourceEntry {
            name: name.to_owned(),
            repo: Repo::parse("owner/repo").expect("repo"),
            matcher: Matcher {
                guid: guid.map(str::to_owned),
                ..Matcher::default()
            },
            asset: None,
            notes: None,
            depends: Vec::new(),
        }
    }

    fn manifest(entry: SourceEntry) -> SourceManifest {
        SourceManifest {
            generated_at: Utc::now(),
            entries: vec![entry],
            skipped: Vec::new(),
        }
    }

    #[test]
    fn missing_requirement_reports_a_manifest_entry() {
        let entry = manifest_entry("ScavLib", Some("Starfi5h.ScavLib"));
        let source = manifest(entry);
        let issues = check_requirements(&[requirement("ScavLib", None)], &[], Some(&source));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::Missing);
        assert_eq!(
            issues[0].entry.as_ref().map(|item| item.name.as_str()),
            Some("ScavLib")
        );
        assert!(issues[0].installed_version.is_none());
    }

    #[test]
    fn optional_and_satisfied_requirements_are_ignored() {
        let mut optional = requirement("MissingOptional", None);
        optional.optional = true;
        let present = installed("ScavLib", Some("1.0.0"), None);
        let issues = check_requirements(
            &[optional, requirement("ScavLib", Some("1.0.0"))],
            &[present],
            None,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn version_below_minimum_is_outdated() {
        let issues = check_requirements(
            &[requirement("ScavLib", Some("1.2.0"))],
            &[installed("ScavLib", Some("1.0.0"), None)],
            None,
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::Outdated);
        assert_eq!(issues[0].installed_version.as_deref(), Some("1.0.0"));
        assert_eq!(issues[0].minimum_version.as_deref(), Some("1.2.0"));
    }

    #[test]
    fn disabled_dependency_counts_as_missing() {
        let mut disabled = installed("ScavLib", Some("1.0.0"), None);
        disabled.disabled = true;
        let issues = check_requirements(&[requirement("ScavLib", None)], &[disabled], None);
        assert_eq!(
            issues.first().map(|issue| issue.kind),
            Some(IssueKind::Missing)
        );
    }

    #[test]
    fn guid_requirement_matches_the_installed_guid() {
        let requirement = DependencyRequirement {
            name: String::new(),
            guid: Some("Starfi5h.ScavLib".to_owned()),
            ..DependencyRequirement::default()
        };
        let installed = installed("Renamed ScavLib", Some("1.0.0"), Some("{starfi5h.scavlib}"));
        assert!(check_requirements(&[requirement], &[installed], None).is_empty());
    }

    #[test]
    fn guid_mismatch_does_not_fall_back_to_name() {
        let requirement = DependencyRequirement {
            name: "ScavLib".to_owned(),
            guid: Some("Starfi5h.ScavLib".to_owned()),
            ..DependencyRequirement::default()
        };
        let other = installed("ScavLib", Some("1.0.0"), Some("Someone.Else"));
        assert_eq!(
            check_requirements(std::slice::from_ref(&requirement), &[other], None)
                .first()
                .map(|issue| issue.kind),
            Some(IssueKind::Missing)
        );
        let no_guid = installed("ScavLib", Some("1.0.0"), None);
        assert!(check_requirements(&[requirement], &[no_guid], None).is_empty());
    }

    #[test]
    fn declared_and_catalog_requirements_are_merged() {
        let dir = crate::test_support::temp_dir("dependencies-catalog");
        let path = dir
            .join("Data")
            .join("Catalogs")
            .join("DependencyCatalog.json");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(
            &path,
            r#"{"dependencies":[{"name":"Target","requires":[{"name":"ScavLib","minimumVersion":"2.0.0"}]}]}"#,
        )
        .expect("write");
        let catalogs = Catalogs::load(&dir);
        let mut entry = manifest_entry("Target", None);
        entry.depends = vec![
            requirement("CoreLib", None),
            requirement("ScavLib", Some("2.0.0")),
        ];
        let requirements = requirements_for(&entry, &catalogs);
        assert_eq!(requirements.len(), 2);
        assert!(requirements.iter().any(|item| item.name == "CoreLib"));
        assert!(requirements.iter().any(|item| item.name == "ScavLib"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn four_part_versions_are_compared_by_their_first_three_parts() {
        assert_eq!(version_less("1.2.3.9", "1.3.0"), Some(true));
        assert_eq!(version_less("1.2.4.0", "1.2.3"), Some(false));
        assert_eq!(version_less("1.2.3.4-beta", "1.2.3"), Some(true));
        assert_eq!(version_less("not a version", "1.0.0"), None);
    }
}
