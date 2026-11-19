//! GitHub release sources: manifest parsing, validation, matching and cache state. No network here.

mod cache;
pub use cache::{CachedSources, DEFAULT_MANIFEST_URL, SourceCache, SourceCacheError};

use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::catalogs::{DependencyRequirement, normalize_key};
use crate::version::HubVersion;

pub const SCHEMA_VERSION: u32 = 1;

const MAX_TEXT: usize = 200;
const MAX_REPO_PART: usize = 100;

pub const DEFAULT_SNAPSHOT_MAX_AGE_MINUTES: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchKind {
    Guid,
    Dll,
    Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    owner: String,
    name: String,
}

impl Repo {
    pub fn parse(value: &str) -> Result<Self, RepoError> {
        let mut parts = value.split('/');
        let owner = parts.next().ok_or(RepoError::NotSlug)?;
        let name = parts.next().ok_or(RepoError::NotSlug)?;
        if parts.next().is_some() {
            return Err(RepoError::NotSlug);
        }
        Ok(Self {
            owner: validate_part(owner)?.to_owned(),
            name: validate_part(name)?.to_owned(),
        })
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

fn validate_part(part: &str) -> Result<&str, RepoError> {
    let allowed = part
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'));
    // Repo identifiers are pasted into api.github.com paths, so dots that could
    // walk the path are rejected instead of escaped.
    let tricky = part.contains("..") || part.to_ascii_lowercase().ends_with(".git");
    if part.is_empty()
        || part.len() > MAX_REPO_PART
        || !allowed
        || tricky
        || part.starts_with('.')
        || part.starts_with('-')
        || part.ends_with('.')
        || part.ends_with('-')
    {
        return Err(RepoError::BadPart);
    }
    Ok(part)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoError {
    NotSlug,
    BadPart,
}

impl fmt::Display for RepoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSlug => formatter.write_str("repository must be owner/name"),
            Self::BadPart => formatter.write_str("repository must use GitHub-safe characters"),
        }
    }
}

impl Error for RepoError {}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Matcher {
    pub guid: Option<String>,
    pub dll: Vec<String>,
    pub name: Vec<String>,
}

impl Matcher {
    pub fn is_empty(&self) -> bool {
        self.guid.is_none() && self.dll.is_empty() && self.name.is_empty()
    }

    pub fn matches(&self, local: &LocalMod<'_>) -> Option<MatchKind> {
        if let (Some(expected), Some(actual)) = (&self.guid, local.guid)
            && same_guid(expected, actual)
        {
            return Some(MatchKind::Guid);
        }
        let dll_hit = local
            .dll_stems
            .iter()
            .any(|stem| self.dll.iter().any(|name| same_dll(name, stem)));
        if dll_hit {
            return Some(MatchKind::Dll);
        }
        if self.name.iter().any(|name| same_key(name, local.name)) {
            return Some(MatchKind::Name);
        }
        None
    }
}

pub struct LocalMod<'a> {
    pub guid: Option<&'a str>,
    pub dll_stems: Vec<&'a str>,
    pub name: &'a str,
}

fn same_guid(left: &str, right: &str) -> bool {
    let left = normalize_guid(left);
    !left.is_empty() && left == normalize_guid(right)
}

pub(crate) fn normalize_guid(value: &str) -> String {
    value
        .trim()
        .trim_matches(|character| character == '{' || character == '}')
        .to_lowercase()
}

fn same_key(left: &str, right: &str) -> bool {
    let left = normalize_key(left);
    !left.is_empty() && left == normalize_key(right)
}

fn same_dll(left: &str, right: &str) -> bool {
    let left = dll_key(left);
    !left.is_empty() && left == dll_key(right)
}

fn dll_key(value: &str) -> String {
    let value = strip_suffix_ignore_ascii_case(value, ".disabled").unwrap_or(value);
    let value = strip_suffix_ignore_ascii_case(value, ".dll").unwrap_or(value);
    normalize_key(value)
}

fn strip_suffix_ignore_ascii_case<'a>(value: &'a str, suffix: &str) -> Option<&'a str> {
    let index = value.len().checked_sub(suffix.len())?;
    let tail = value.get(index..)?;
    if tail.eq_ignore_ascii_case(suffix) {
        Some(&value[..index])
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEntry {
    pub name: String,
    pub repo: Repo,
    pub matcher: Matcher,
    pub asset: Option<String>,
    pub notes: Option<String>,
    pub depends: Vec<DependencyRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedEntry {
    pub index: usize,
    pub name: Option<String>,
    pub reason: String,
}

#[derive(Debug)]
pub struct SourceManifest {
    pub generated_at: DateTime<Utc>,
    pub entries: Vec<SourceEntry>,
    pub skipped: Vec<SkippedEntry>,
}

impl SourceManifest {
    pub fn is_stale(&self, now: DateTime<Utc>, max_age: Duration) -> bool {
        now.signed_duration_since(self.generated_at) > max_age
    }

    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let document: ManifestDocument = serde_json::from_str(text)?;
        if document.schema_version != SCHEMA_VERSION {
            return Err(ManifestError::Schema(document.schema_version));
        }
        let generated_at = parse_timestamp(&document.generated_at)?;
        let mut entries = Vec::new();
        let mut skipped = Vec::new();
        for (index, value) in document.mods.into_iter().enumerate() {
            match parse_entry(index, value) {
                Ok(entry) => entries.push(entry),
                Err(skip) => skipped.push(skip),
            }
        }
        Ok(Self {
            generated_at,
            entries,
            skipped,
        })
    }
}

fn parse_entry(index: usize, value: Value) -> Result<SourceEntry, SkippedEntry> {
    let fallback_name = value.get("name").and_then(Value::as_str).map(str::to_owned);
    parse_entry_fields(value).map_err(|reason| SkippedEntry {
        index,
        name: fallback_name,
        reason,
    })
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, ManifestError> {
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|_| ManifestError::Timestamp(value.to_owned()))
}

fn parse_entry_fields(value: Value) -> Result<SourceEntry, String> {
    let document: EntryDocument =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    let name = clean_text(&document.name).ok_or("entry name is empty or invalid")?;
    let repo = Repo::parse(&document.repo).map_err(|error| error.to_string())?;
    let matcher = build_matcher(document.matcher)?;
    let asset = match document.asset.as_deref() {
        Some(value) => Some(clean_asset(value).ok_or("asset name is empty or invalid")?),
        None => None,
    };
    Ok(SourceEntry {
        name,
        repo,
        matcher,
        asset,
        notes: document.notes.as_deref().and_then(clean_text),
        depends: clean_dependencies(document.depends),
    })
}

fn build_matcher(document: MatcherDocument) -> Result<Matcher, String> {
    let guid = match document.guid {
        Some(value) => Some(clean_text(&value).ok_or("guid is empty or invalid")?),
        None => None,
    };
    let dll = document
        .dll
        .iter()
        .map(|value| clean_dll(value).ok_or("dll name is empty or invalid"))
        .collect::<Result<Vec<_>, _>>()?;
    let name = document
        .name
        .iter()
        .map(|value| clean_text(value).ok_or("match name is empty or invalid"))
        .collect::<Result<Vec<_>, _>>()?;
    let matcher = Matcher { guid, dll, name };
    if matcher.is_empty() {
        return Err(String::from("entry has no match keys"));
    }
    Ok(matcher)
}

fn clean_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_TEXT
        || trimmed.chars().any(char::is_control)
    {
        return None;
    }
    Some(trimmed.to_owned())
}

fn clean_asset(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_TEXT
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
        || trimmed.chars().any(char::is_control)
    {
        return None;
    }
    Some(trimmed.to_owned())
}

fn clean_dependencies(values: Vec<DependencyRequirement>) -> Vec<DependencyRequirement> {
    values
        .into_iter()
        .filter_map(|mut requirement| {
            let name = clean_text(&requirement.name);
            let guid = requirement.guid.as_deref().and_then(clean_text);
            if name.is_none() && guid.is_none() {
                return None;
            }
            requirement.name = name.or_else(|| guid.clone()).unwrap_or_default();
            requirement.guid = guid;
            requirement.minimum_version =
                requirement.minimum_version.as_deref().and_then(clean_text);
            requirement.note = requirement.note.as_deref().and_then(clean_text);
            Some(requirement)
        })
        .collect()
}

fn clean_dll(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return None;
    }
    let stem = if trimmed.to_ascii_lowercase().ends_with(".dll") {
        &trimmed[..trimmed.len() - 4]
    } else {
        trimmed
    };
    clean_text(stem)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestDocument {
    schema_version: u32,
    generated_at: String,
    #[serde(default)]
    mods: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryDocument {
    name: String,
    repo: String,
    #[serde(rename = "match")]
    matcher: MatcherDocument,
    #[serde(default)]
    asset: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    depends: Vec<DependencyRequirement>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct MatcherDocument {
    #[serde(default)]
    guid: Option<String>,
    #[serde(default)]
    dll: Vec<String>,
    #[serde(default)]
    name: Vec<String>,
}

#[derive(Debug)]
pub enum ManifestError {
    Json(serde_json::Error),
    Schema(u32),
    Timestamp(String),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "manifest is not valid JSON: {error}"),
            Self::Timestamp(value) => {
                write!(
                    formatter,
                    "generatedAt is not an RFC 3339 timestamp: {value}"
                )
            }
            Self::Schema(version) => write!(
                formatter,
                "unsupported schemaVersion {version}, this build reads {SCHEMA_VERSION}"
            ),
        }
    }
}

impl Error for ManifestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Schema(_) | Self::Timestamp(_) => None,
        }
    }
}

impl From<serde_json::Error> for ManifestError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Github,
    Nexus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceOption {
    pub kind: SourceKind,
    pub version: Option<HubVersion>,
}

impl SourceOption {
    pub fn new(kind: SourceKind, version: Option<HubVersion>) -> Self {
        Self { kind, version }
    }

    fn outranks(&self, other: &Self) -> bool {
        match (&self.version, &other.version) {
            (Some(left), Some(right)) => {
                left > right
                    || (left == right
                        && self.kind == SourceKind::Github
                        && other.kind == SourceKind::Nexus)
            }
            (Some(_), None) => true,
            (None, Some(_)) | (None, None) => false,
        }
    }
}

pub fn default_source(options: &[SourceOption]) -> Option<&SourceOption> {
    options.iter().reduce(|best, candidate| {
        if candidate.outranks(best) {
            candidate
        } else {
            best
        }
    })
}

pub fn effective_source(
    options: &[SourceOption],
    pinned: Option<SourceKind>,
) -> Option<&SourceOption> {
    pinned
        .and_then(|kind| options.iter().find(|option| option.kind == kind))
        .or_else(|| default_source(options))
}

#[derive(Debug, Clone)]
pub struct MatchedEntry<'a> {
    pub entry: &'a SourceEntry,
    pub kind: MatchKind,
}

pub fn best_match<'a>(
    entries: &'a [SourceEntry],
    local: &LocalMod<'_>,
) -> Option<MatchedEntry<'a>> {
    entries
        .iter()
        .filter_map(|entry| {
            entry
                .matcher
                .matches(local)
                .map(|kind| MatchedEntry { entry, kind })
        })
        .min_by_key(|matched| matched.kind)
}

#[derive(Debug, Clone)]
pub struct ModResolution<'a> {
    pub matched: Option<MatchedEntry<'a>>,
    pub options: Vec<SourceOption>,
    pub pinned: Option<SourceKind>,
    pub effective: Option<SourceOption>,
}

pub fn resolve_mod<'a>(
    entries: &'a [SourceEntry],
    local: &LocalMod<'_>,
    github: Option<HubVersion>,
    nexus: Option<HubVersion>,
    pinned: Option<SourceKind>,
) -> ModResolution<'a> {
    let matched = best_match(entries, local);
    let mut options = Vec::new();
    if matched.is_some() {
        options.push(SourceOption::new(SourceKind::Github, github));
    }
    if nexus.is_some() {
        options.push(SourceOption::new(SourceKind::Nexus, nexus));
    }
    let effective = effective_source(&options, pinned).cloned();
    ModResolution {
        matched,
        options,
        pinned,
        effective,
    }
}

pub fn manifest_path(app_data: &Path) -> PathBuf {
    app_data.join("Sources").join("manifest.json")
}

pub fn load_manifest_optional(path: &Path) -> Result<Option<SourceManifest>, LoadError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(LoadError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    SourceManifest::parse(&text)
        .map(Some)
        .map_err(LoadError::Manifest)
}

#[derive(Debug)]
pub enum LoadError {
    Io { path: PathBuf, source: io::Error },
    Manifest(ManifestError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Manifest(error) => write!(formatter, "manifest: {error}"),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Manifest(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod load_tests {
    use super::*;
    use crate::test_support::temp_dir;

    const DOC: &str = r#"{"schemaVersion":1,"generatedAt":"2026-10-01T00:00:00Z","mods":[{"name":"ScavLib","repo":"starfi5h/ScavLib","match":{"guid":"Starfi5h.ScavLib"}}]}"#;

    #[test]
    fn missing_file_is_none() {
        let dir = temp_dir("manifest-missing");
        let loaded = load_manifest_optional(&manifest_path(&dir)).expect("load");
        assert!(loaded.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_a_local_manifest() {
        let dir = temp_dir("manifest-local");
        let path = manifest_path(&dir);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(&path, DOC).expect("write");
        let loaded = load_manifest_optional(&path).expect("load").expect("some");
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].repo.slug(), "starfi5h/ScavLib");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn entries_carry_declared_dependencies() {
        let text = r#"{"schemaVersion":1,"generatedAt":"2026-10-01T00:00:00Z","mods":[{"name":"Target","repo":"owner/target","match":{"dll":["Target"]},"depends":[{"name":"ScavLib","minimumVersion":"1.2.0"},{"guid":"Starfi5h.CoreLib","optional":true}]}]}"#;
        let manifest = SourceManifest::parse(text).expect("parse");
        let entry = manifest.entries.first().expect("entry");
        assert_eq!(entry.depends.len(), 2);
        assert_eq!(entry.depends[0].name, "ScavLib");
        assert_eq!(entry.depends[0].minimum_version.as_deref(), Some("1.2.0"));
        assert_eq!(entry.depends[1].name, "Starfi5h.CoreLib");
        assert!(entry.depends[1].optional);
    }

    #[test]
    fn broken_manifest_is_an_error() {
        let dir = temp_dir("manifest-broken");
        let path = manifest_path(&dir);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(&path, "{not json").expect("write");
        assert!(matches!(
            load_manifest_optional(&path),
            Err(LoadError::Manifest(_))
        ));
        let _ = fs::remove_dir_all(&dir);
    }
}
#[cfg(test)]
mod resolve_tests {
    use super::*;

    fn local<'a>(name: &'a str, guid: Option<&'a str>, dll_stems: &[&'a str]) -> LocalMod<'a> {
        LocalMod {
            guid,
            dll_stems: dll_stems.to_vec(),
            name,
        }
    }

    fn entry(name: &str, repo: &str, matcher: Matcher) -> SourceEntry {
        SourceEntry {
            name: name.to_owned(),
            repo: Repo::parse(repo).expect("repo"),
            matcher,
            asset: None,
            notes: None,
            depends: Vec::new(),
        }
    }

    fn version(value: &str) -> HubVersion {
        HubVersion::parse(value).expect("version")
    }

    fn guid_matcher(value: &str) -> Matcher {
        Matcher {
            guid: Some(value.to_owned()),
            ..Matcher::default()
        }
    }

    fn name_matcher(value: &str) -> Matcher {
        Matcher {
            name: vec![value.to_owned()],
            ..Matcher::default()
        }
    }

    #[test]
    fn guid_match_outranks_dll_and_name() {
        let entries = vec![
            entry("by-name", "owner/a", name_matcher("ScavLib")),
            entry(
                "by-dll",
                "owner/b",
                Matcher {
                    dll: vec!["scavlib".to_owned()],
                    ..Matcher::default()
                },
            ),
            entry("by-guid", "owner/c", guid_matcher("starfi5h.scavlib")),
        ];
        let local = local("ScavLib", Some("{starfi5h.scavlib}"), &["ScavLib.dll"]);
        let matched = best_match(&entries, &local).expect("match");
        assert_eq!(matched.entry.name, "by-guid");
        assert_eq!(matched.kind, MatchKind::Guid);
    }

    #[test]
    fn equal_matches_keep_manifest_order() {
        let entries = vec![
            entry("first", "owner/a", name_matcher("ScavLib")),
            entry("second", "owner/b", name_matcher("ScavLib")),
        ];
        let matched = best_match(&entries, &local("ScavLib", None, &[])).expect("match");
        assert_eq!(matched.entry.name, "first");
    }

    #[test]
    fn unknown_github_version_loses_to_a_known_nexus_one() {
        let entries = vec![entry("lib", "owner/a", guid_matcher("mod.guid"))];
        let local = local("ScavLib", Some("mod.guid"), &[]);
        let resolved = resolve_mod(&entries, &local, None, Some(version("1.0.0")), None);
        assert_eq!(resolved.options.len(), 2);
        assert_eq!(
            resolved.effective.expect("effective").kind,
            SourceKind::Nexus
        );
    }

    #[test]
    fn unmatched_mods_get_no_github_option() {
        let entries: Vec<SourceEntry> = Vec::new();
        let local = local("ScavLib", Some("mod.guid"), &[]);
        let resolved = resolve_mod(&entries, &local, None, Some(version("1.0.0")), None);
        assert!(resolved.matched.is_none());
        assert_eq!(resolved.options.len(), 1);
        assert_eq!(
            resolved.effective.expect("effective").kind,
            SourceKind::Nexus
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn local<'a>(name: &'a str, guid: Option<&'a str>, dll_stems: &[&'a str]) -> LocalMod<'a> {
        LocalMod {
            guid,
            dll_stems: dll_stems.to_vec(),
            name,
        }
    }

    #[test]
    fn repo_keeps_owner_and_name() {
        let repo = Repo::parse("starfi5h/ScavLib").expect("repo");
        assert_eq!(repo.owner(), "starfi5h");
        assert_eq!(repo.name(), "ScavLib");
        assert_eq!(repo.slug(), "starfi5h/ScavLib");
    }

    #[test]
    fn repo_rejects_path_tricks() {
        for value in [
            "",
            "owner",
            "owner/name/extra",
            "owner/..",
            "../name",
            "owner/-name",
            "owner/name.git",
            "owner/na me",
        ] {
            assert!(Repo::parse(value).is_err(), "{value} should be rejected");
        }
    }

    #[test]
    fn guid_match_ignores_case_and_braces() {
        let matcher = Matcher {
            guid: Some("{Starfi5h.ScavLib}".to_owned()),
            ..Matcher::default()
        };
        assert_eq!(
            matcher.matches(&local("ScavLib", Some("starfi5h.scavlib"), &[])),
            Some(MatchKind::Guid)
        );
        assert_eq!(matcher.matches(&local("ScavLib", None, &[])), None);
    }

    #[test]
    fn dll_match_uses_stem_and_ignores_case() {
        let matcher = Matcher {
            dll: vec!["ScavLib".to_owned()],
            ..Matcher::default()
        };
        assert_eq!(
            matcher.matches(&local("Other", None, &["scavlib"])),
            Some(MatchKind::Dll)
        );
        assert_eq!(matcher.matches(&local("Other", None, &["Console"])), None);
    }

    #[test]
    fn dll_match_strips_disabled_and_dll_suffixes() {
        let matcher = Matcher {
            dll: vec!["Foo".to_owned()],
            ..Matcher::default()
        };
        assert_eq!(
            matcher.matches(&local("Other", None, &["Foo.dll"])),
            Some(MatchKind::Dll)
        );
        assert_eq!(
            matcher.matches(&local("Other", None, &["FOO.DLL.DISABLED"])),
            Some(MatchKind::Dll)
        );

        let matcher = Matcher {
            dll: vec!["Foo.dll".to_owned()],
            ..Matcher::default()
        };
        assert_eq!(
            matcher.matches(&local("Other", None, &["Foo"])),
            Some(MatchKind::Dll)
        );
    }

    #[test]
    fn dll_match_does_not_over_strip_suffixes() {
        let matcher = Matcher {
            dll: vec!["Foo".to_owned()],
            ..Matcher::default()
        };
        for stem in ["Foo.dll.dll", "Foo.dll.disabled.dll", "Foodll"] {
            assert_eq!(
                matcher.matches(&local("Other", None, &[stem])),
                None,
                "{stem} should not match Foo"
            );
        }
    }

    #[test]
    fn name_key_does_not_strip_dll_suffix() {
        let matcher = Matcher {
            name: vec!["Foo.dll".to_owned()],
            ..Matcher::default()
        };
        assert_eq!(matcher.matches(&local("Foo", None, &[])), None);
    }

    #[test]
    fn name_match_is_the_fallback() {
        let matcher = Matcher {
            guid: Some("Starfi5h.ScavLib".to_owned()),
            dll: vec!["ScavLib".to_owned()],
            name: vec!["Scav Lib".to_owned()],
        };
        assert_eq!(
            matcher.matches(&local("Scav Lib", None, &[])),
            Some(MatchKind::Name)
        );
        assert_eq!(
            matcher.matches(&local("Scav Lib", None, &["ScavLib"])),
            Some(MatchKind::Dll)
        );
        assert_eq!(
            matcher.matches(&local("Unrelated", None, &["Console"])),
            None
        );
    }

    #[test]
    fn match_kinds_rank_from_guid_to_name() {
        assert!(MatchKind::Guid < MatchKind::Dll);
        assert!(MatchKind::Dll < MatchKind::Name);
    }

    #[test]
    fn manifest_reads_entries() {
        let manifest = SourceManifest::parse(SAMPLE).expect("manifest");
        assert!(manifest.skipped.is_empty());
        assert_eq!(manifest.entries.len(), 2);
        let first = &manifest.entries[0];
        assert_eq!(first.name, "ScavLib");
        assert_eq!(first.repo.slug(), "starfi5h/ScavLib");
        assert_eq!(first.matcher.dll, vec!["ScavLib".to_owned()]);
        assert_eq!(first.notes.as_deref(), Some("unofficial mirror"));
    }

    #[test]
    fn manifest_reads_an_asset_name() {
        let text = r#"{"schemaVersion":1,"generatedAt":"2026-10-01T00:00:00Z","mods":[{"name":"QoL Unknown","repo":"jimmyking9999999/QoL-Unknown","match":{"dll":["QoL Unknown"]},"asset":"QoL-Unknown.zip"}]}"#;
        let manifest = SourceManifest::parse(text).expect("manifest");
        assert_eq!(
            manifest.entries[0].asset.as_deref(),
            Some("QoL-Unknown.zip")
        );
    }

    #[test]
    fn manifest_skips_an_unsafe_asset_name() {
        let text = r#"{"schemaVersion":1,"generatedAt":"2026-10-01T00:00:00Z","mods":[{"name":"Bad asset","repo":"a/b","match":{"dll":["Bad"]},"asset":"../evil.zip"}]}"#;
        let manifest = SourceManifest::parse(text).expect("manifest");
        assert!(manifest.entries.is_empty());
        assert!(manifest.skipped[0].reason.contains("asset"));
    }
    #[test]
    fn manifest_skips_bad_entries_with_their_index() {
        let manifest = SourceManifest::parse(BAD_SAMPLE).expect("manifest");
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.skipped.len(), 2);
        assert_eq!(manifest.skipped[0].index, 0);
        assert_eq!(manifest.skipped[0].name.as_deref(), Some("Bad repo"));
        assert_eq!(manifest.skipped[1].index, 1);
        assert!(manifest.skipped[1].reason.contains("no match keys"));
    }

    #[test]
    fn manifest_rejects_unknown_schema() {
        let error = SourceManifest::parse(
            r#"{"schemaVersion": 2, "generatedAt": "2026-11-15T00:00:00Z", "mods": []}"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("schemaVersion 2"));
    }

    #[test]
    fn staleness_uses_the_snapshot_timestamp() {
        let manifest = SourceManifest::parse(SAMPLE).expect("manifest");
        let max_age = Duration::minutes(DEFAULT_SNAPSHOT_MAX_AGE_MINUTES as i64);
        let fresh = manifest.generated_at + Duration::minutes(119);
        let stale = manifest.generated_at + Duration::minutes(121);
        assert!(!manifest.is_stale(fresh, max_age));
        assert!(manifest.is_stale(stale, max_age));
    }

    #[test]
    fn manifest_rejects_a_bad_timestamp() {
        let error = SourceManifest::parse(
            r#"{"schemaVersion": 1, "generatedAt": "yesterday", "mods": []}"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("RFC 3339"), "{error}");
    }

    fn option(kind: SourceKind, version: &str) -> SourceOption {
        SourceOption::new(kind, HubVersion::parse(version))
    }

    #[test]
    fn latest_version_wins() {
        let options = [
            option(SourceKind::Nexus, "1.2.0"),
            option(SourceKind::Github, "1.3.0"),
        ];
        assert_eq!(
            default_source(&options).expect("source").kind,
            SourceKind::Github
        );
    }

    #[test]
    fn unversioned_sources_lose() {
        let options = [
            option(SourceKind::Github, "0.0.0"),
            SourceOption::new(SourceKind::Nexus, None),
        ];
        assert_eq!(
            default_source(&options).expect("source").kind,
            SourceKind::Github
        );
    }

    #[test]
    fn ties_prefer_github() {
        let options = [
            option(SourceKind::Nexus, "1.2.0"),
            option(SourceKind::Github, "1.2.0"),
        ];
        assert_eq!(
            default_source(&options).expect("source").kind,
            SourceKind::Github
        );
    }

    #[test]
    fn pinned_source_overrides_the_newest() {
        let options = [
            option(SourceKind::Github, "1.3.0"),
            option(SourceKind::Nexus, "1.2.0"),
        ];
        assert_eq!(
            effective_source(&options, Some(SourceKind::Nexus))
                .expect("source")
                .kind,
            SourceKind::Nexus
        );
        assert_eq!(
            effective_source(&options, Some(SourceKind::Github))
                .expect("source")
                .kind,
            SourceKind::Github
        );
    }

    #[test]
    fn missing_pin_falls_back_to_the_newest() {
        let options = [option(SourceKind::Nexus, "1.2.0")];
        assert_eq!(
            effective_source(&options, Some(SourceKind::Github))
                .expect("source")
                .kind,
            SourceKind::Nexus
        );
    }
    const SAMPLE: &str = r#"{
      "schemaVersion": 1,
      "generatedAt": "2026-11-15T00:00:00Z",
      "mods": [
        {
          "name": "ScavLib",
          "repo": "starfi5h/ScavLib",
          "match": { "guid": "Starfi5h.ScavLib", "dll": ["ScavLib.dll"] },
          "notes": "unofficial mirror"
        },
        {
          "name": "Console",
          "repo": "example/Console",
          "match": { "name": ["Console Mod"] }
        }
      ]
    }"#;

    const BAD_SAMPLE: &str = r#"{
      "schemaVersion": 1,
      "generatedAt": "2026-11-15T00:00:00Z",
      "mods": [
        { "name": "Bad repo", "repo": "not-a-slug", "match": { "guid": "x.y" } },
        { "name": "No keys", "repo": "example/none", "match": {} },
        { "name": "Fine", "repo": "example/fine", "match": { "dll": ["Fine.dll"] } }
      ]
    }"#;
}
