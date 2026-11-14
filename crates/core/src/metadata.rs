//! Community Nexus metadata: parsing, display-derived fields and the offline cache.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const METADATA_URL: &str =
    "https://github.com/jimmyking9999999/Metadata-generator/raw/refs/heads/main/nexusmods.json";
pub const CACHE_LIFETIME: Duration = Duration::from_secs(6 * 60 * 60);

const CACHE_FILE: &str = "UniversalMetadataCache.json";
const STATE_FILE: &str = "UniversalMetadataHttpState.json";

const DEFAULT_NAME: &str = "Unknown mod";
const DEFAULT_VERSION: &str = "Unknown";
const DEFAULT_AUTHOR: &str = "Unknown author";
const DEFAULT_DESCRIPTION: &str = "No description was supplied by the mod author.";

#[derive(Debug, Clone)]
pub struct MetadataMod {
    pub id: String,
    pub name: String,
    pub version: String,
    pub bepinex_version: Option<String>,
    pub dll_version: Option<String>,
    pub dll_names: Vec<String>,
    pub dll_versions: BTreeMap<String, String>,
    pub author: String,
    pub description: String,
    pub links: MetadataLinks,
    pub statistics: MetadataStatistics,
    pub contains_adult_content: bool,
    extra: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetadataLinks {
    pub icon: Option<String>,
    pub nexus_mods: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetadataStatistics {
    pub endorsements: i64,
    pub unique_downloads: i64,
    pub total_downloads: i64,
}

impl MetadataMod {
    pub fn image_url(&self) -> Option<&str> {
        self.links.icon.as_deref()
    }

    pub fn nexus_url(&self) -> Option<&str> {
        self.links.nexus_mods.as_deref()
    }

    pub fn nexus_download_page_url(&self) -> Option<String> {
        let url = self.nexus_url()?;
        if url.trim().is_empty() {
            return None;
        }
        if url.contains('?') {
            Some(format!("{url}&tab=files"))
        } else {
            Some(format!("{url}?tab=files"))
        }
    }

    pub fn plugin_guid(&self) -> Option<&str> {
        for key in ["Guid", "GUID", "ModGuid", "PluginGuid", "BepInExGuid"] {
            if let Some(Value::String(value)) = self.extra_field(key) {
                return Some(value);
            }
        }
        None
    }

    pub fn latest_file_modified_utc(&self) -> Option<DateTime<Utc>> {
        for key in [
            "FileModifiedDate",
            "ModifiedDate",
            "LastUpdated",
            "UpdatedAt",
            "UpdatedDate",
            "UpdatedTimestamp",
        ] {
            match self.extra_field(key) {
                Some(Value::String(text)) => {
                    if let Some(parsed) = parse_timestamp(text) {
                        return Some(parsed);
                    }
                }
                Some(Value::Number(number)) => {
                    if let Some(seconds) = number.as_i64() {
                        return DateTime::from_timestamp(seconds, 0);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub fn latest_file_size_bytes(&self) -> Option<i64> {
        for key in [
            "FileSize",
            "FileSizeBytes",
            "SizeInBytes",
            "file_size",
            "size_in_bytes",
        ] {
            match self.extra_field(key) {
                Some(Value::Number(number)) => {
                    if let Some(size) = number.as_i64() {
                        return Some(size);
                    }
                }
                Some(Value::String(text)) => {
                    if let Ok(size) = text.trim().parse::<i64>() {
                        return Some(size);
                    }
                }
                _ => {}
            }
        }
        None
    }

    pub fn file_size_label(&self) -> String {
        match self.latest_file_size_bytes() {
            Some(bytes) => format!("Size {}", format_size(bytes)),
            None => "Size not supplied".to_string(),
        }
    }

    pub fn total_downloads_label(&self) -> String {
        format!(
            "{} total downloads",
            format_number(self.statistics.total_downloads)
        )
    }

    pub fn unique_downloads_label(&self) -> String {
        format!(
            "{} unique downloads",
            format_number(self.statistics.unique_downloads)
        )
    }

    pub fn endorsements_label(&self) -> String {
        format!(
            "{} endorsements",
            format_number(self.statistics.endorsements)
        )
    }

    pub fn is_adult_content(&self) -> bool {
        self.contains_adult_content
    }

    fn extra_field(&self, name: &str) -> Option<&Value> {
        self.extra
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)
    }
}

impl<'de> Deserialize<'de> for MetadataMod {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let Value::Object(object) = value else {
            return Err(de::Error::custom("metadata entry must be an object"));
        };
        let links = object
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("Links"))
            .map(|(_, value)| MetadataLinks::from_value(value))
            .unwrap_or_default();
        let statistics = object
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("Statistics"))
            .map(|(_, value)| MetadataStatistics::from_value(value))
            .unwrap_or_default();
        let extra = object
            .iter()
            .filter(|(key, _)| !is_known_key(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        Ok(Self {
            id: string_field(&object, "Id").unwrap_or_default(),
            name: string_or(&object, "Name", DEFAULT_NAME),
            version: string_or(&object, "Version", DEFAULT_VERSION),
            bepinex_version: string_field(&object, "BepinexVersion"),
            dll_version: string_field(&object, "DllVersion"),
            dll_names: string_list(&object, "DllNames"),
            dll_versions: string_map(&object, "DllVersions"),
            author: string_or(&object, "Author", DEFAULT_AUTHOR),
            description: string_or(&object, "Description", DEFAULT_DESCRIPTION),
            links,
            statistics,
            contains_adult_content: bool_field(&object, "ContainsAdultContent"),
            extra,
        })
    }
}

impl MetadataLinks {
    fn from_value(value: &Value) -> Self {
        let Some(object) = value.as_object() else {
            return Self::default();
        };
        Self {
            icon: string_field(object, "Icon"),
            nexus_mods: string_field(object, "NexusMods"),
        }
    }
}

impl MetadataStatistics {
    fn from_value(value: &Value) -> Self {
        let Some(object) = value.as_object() else {
            return Self::default();
        };
        Self {
            endorsements: integer_field(object, "Endorsements"),
            unique_downloads: integer_field(object, "UniqueDownloads"),
            total_downloads: integer_field(object, "TotalDownloads"),
        }
    }
}

pub fn parse_mods(json: &str) -> Result<Vec<MetadataMod>, serde_json::Error> {
    serde_json::from_str(json)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct HttpState {
    #[serde(rename = "ETag")]
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug)]
pub enum MetadataCacheError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for MetadataCacheError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Parse { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for MetadataCacheError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

pub struct MetadataCache {
    cache_path: PathBuf,
    state_path: PathBuf,
}

impl MetadataCache {
    pub fn new(app_data: &Path) -> Self {
        Self {
            cache_path: app_data.join(CACHE_FILE),
            state_path: app_data.join(STATE_FILE),
        }
    }

    pub fn cache_path(&self) -> &Path {
        &self.cache_path
    }

    pub fn state_path(&self) -> &Path {
        &self.state_path
    }

    pub fn has_cache(&self) -> bool {
        self.cache_path.is_file()
    }

    pub fn is_stale(&self) -> bool {
        let Ok(modified) = fs::metadata(&self.cache_path).and_then(|metadata| metadata.modified())
        else {
            return true;
        };
        SystemTime::now()
            .duration_since(modified)
            .map(|age| age > CACHE_LIFETIME)
            .unwrap_or(false)
    }

    pub fn load_cached(&self) -> Option<Vec<MetadataMod>> {
        let text = fs::read_to_string(&self.cache_path).ok()?;
        parse_mods(&text).ok()
    }

    pub fn load_state(&self) -> HttpState {
        let Ok(text) = fs::read_to_string(&self.state_path) else {
            return HttpState::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn store(&self, raw: &str, state: &HttpState) -> Result<(), MetadataCacheError> {
        parse_mods(raw).map_err(|source| MetadataCacheError::Parse {
            path: self.cache_path.clone(),
            source,
        })?;
        let state_text =
            serde_json::to_string(state).map_err(|source| MetadataCacheError::Parse {
                path: self.state_path.clone(),
                source,
            })?;
        write_atomic(&self.cache_path, raw).map_err(|source| MetadataCacheError::Io {
            path: self.cache_path.clone(),
            source,
        })?;
        write_atomic(&self.state_path, &state_text).map_err(|source| MetadataCacheError::Io {
            path: self.state_path.clone(),
            source,
        })
    }
}

fn is_known_key(key: &str) -> bool {
    [
        "Id",
        "Name",
        "Version",
        "BepinexVersion",
        "DllVersion",
        "DllNames",
        "DllVersions",
        "Author",
        "Description",
        "Links",
        "Statistics",
        "ContainsAdultContent",
    ]
    .iter()
    .any(|known| known.eq_ignore_ascii_case(key))
}

fn field<'a>(object: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    object
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value)
}

fn string_field(object: &Map<String, Value>, name: &str) -> Option<String> {
    field(object, name)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn string_or(object: &Map<String, Value>, name: &str, fallback: &str) -> String {
    string_field(object, name).unwrap_or_else(|| fallback.to_string())
}

fn bool_field(object: &Map<String, Value>, name: &str) -> bool {
    field(object, name)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn integer_field(object: &Map<String, Value>, name: &str) -> i64 {
    field(object, name)
        .and_then(Value::as_i64)
        .unwrap_or_default()
}

fn string_list(object: &Map<String, Value>, name: &str) -> Vec<String> {
    field(object, name)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn string_map(object: &Map<String, Value>, name: &str) -> BTreeMap<String, String> {
    let Some(Value::Object(values)) = field(object, name) else {
        return BTreeMap::new();
    };
    values
        .iter()
        .filter_map(|(key, value)| value.as_str().map(|text| (key.clone(), text.to_string())))
        .collect()
}

fn parse_timestamp(text: &str) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text.trim()) {
        return Some(parsed.with_timezone(&Utc));
    }
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d"] {
        if let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(text.trim(), format) {
            return Some(parsed.and_utc());
        }
    }
    None
}

fn format_number(value: i64) -> String {
    let negative = value < 0;
    let digits = value.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(character);
    }
    if negative {
        format!("-{grouped}")
    } else {
        grouped
    }
}

fn format_size(bytes: i64) -> String {
    let mut value = bytes as f64;
    let units = ["B", "KB", "MB", "GB"];
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let mut text = format!("{value:.2}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    format!("{text} {}", units[unit])
}

fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    const SAMPLE: &str = r#"[
        {
            "id": "12345",
            "Name": "Example Mod",
            "version": "1.2.3",
            "bepinexVersion": "5.4.21",
            "DllVersion": "1.0.0.0",
            "DllNames": ["Example.dll", "Shared.dll"],
            "DllVersions": {"Example.dll": "1.2.3"},
            "author": "Someone",
            "Description": "Line one<br>line two",
            "Links": {"icon": "https://example.com/icon.png", "NexusMods": "https://www.nexusmods.com/games/123"},
            "Statistics": {"endorsements": 12, "UniqueDownloads": 34, "totalDownloads": 5678},
            "ContainsAdultContent": true,
            "Guid": "example.guid",
            "FileModifiedDate": "2026-01-02T03:04:05Z",
            "FileSize": 2048
        },
        {
            "Id": "67890",
            "Name": "Bare Mod"
        }
    ]"#;

    #[test]
    fn parses_case_insensitive_fields_and_defaults() {
        let mods = parse_mods(SAMPLE).unwrap();
        assert_eq!(mods.len(), 2);
        let first = &mods[0];
        assert_eq!(first.id, "12345");
        assert_eq!(first.name, "Example Mod");
        assert_eq!(first.version, "1.2.3");
        assert_eq!(first.bepinex_version.as_deref(), Some("5.4.21"));
        assert_eq!(first.dll_names, vec!["Example.dll", "Shared.dll"]);
        assert_eq!(
            first.dll_versions.get("Example.dll").map(String::as_str),
            Some("1.2.3")
        );
        assert_eq!(first.author, "Someone");
        assert_eq!(first.statistics.total_downloads, 5678);
        assert!(first.contains_adult_content);

        let second = &mods[1];
        assert_eq!(second.name, "Bare Mod");
        assert_eq!(second.version, "Unknown");
        assert_eq!(second.author, "Unknown author");
        assert_eq!(second.description, DEFAULT_DESCRIPTION);
        assert!(second.dll_names.is_empty());
        assert_eq!(second.statistics, MetadataStatistics::default());
        assert!(!second.is_adult_content());
    }

    #[test]
    fn derives_urls_and_extension_fields() {
        let mods = parse_mods(SAMPLE).unwrap();
        let first = &mods[0];
        assert_eq!(
            first.nexus_download_page_url().as_deref(),
            Some("https://www.nexusmods.com/games/123?tab=files")
        );
        assert_eq!(first.plugin_guid(), Some("example.guid"));
        assert_eq!(
            first
                .latest_file_modified_utc()
                .map(|value| value.to_rfc3339()),
            Some("2026-01-02T03:04:05+00:00".to_string())
        );
        assert_eq!(first.latest_file_size_bytes(), Some(2048));
        assert_eq!(first.file_size_label(), "Size 2 KB");
        assert_eq!(first.total_downloads_label(), "5,678 total downloads");
        assert_eq!(first.endorsements_label(), "12 endorsements");

        let url = "https://example.com/mod?tab=description";
        let mut with_query = first.clone();
        with_query.links.nexus_mods = Some(url.to_string());
        assert_eq!(
            with_query.nexus_download_page_url().as_deref(),
            Some("https://example.com/mod?tab=description&tab=files")
        );
    }

    #[test]
    fn accepts_epoch_seconds_and_numeric_strings_for_extensions() {
        let raw = r#"[{"Id":"1","Name":"Mod","UpdatedAt":1700000000,"FileSize":"1048576"}]"#;
        let mods = parse_mods(raw).unwrap();
        assert_eq!(
            mods[0]
                .latest_file_modified_utc()
                .map(|value| value.timestamp()),
            Some(1_700_000_000)
        );
        assert_eq!(mods[0].latest_file_size_bytes(), Some(1_048_576));
        assert_eq!(mods[0].file_size_label(), "Size 1 MB");
    }

    #[test]
    fn rejects_non_object_entries() {
        assert!(parse_mods(r#"["not an object"]"#).is_err());
        assert!(parse_mods("{}").is_err());
    }

    #[test]
    fn caches_raw_json_and_conditional_state() {
        let dir = temp_dir("metadata-cache");
        let cache = MetadataCache::new(&dir);
        let state = HttpState {
            etag: Some("\"abc\"".to_string()),
            last_modified: Some("Wed, 21 Oct 2026 07:28:00 GMT".to_string()),
        };
        cache.store(SAMPLE, &state).unwrap();
        assert!(cache.has_cache());
        assert_eq!(cache.load_cached().unwrap().len(), 2);
        assert_eq!(cache.load_state(), state);
        assert_eq!(
            fs::read_to_string(cache.cache_path()).unwrap(),
            SAMPLE,
            "cache keeps the raw response body"
        );
        assert!(!cache.is_stale());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_cache_is_detected_by_modification_time() {
        let dir = temp_dir("metadata-stale");
        let cache = MetadataCache::new(&dir);
        cache.store(SAMPLE, &HttpState::default()).unwrap();
        let file = fs::OpenOptions::new()
            .write(true)
            .open(cache.cache_path())
            .unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(7 * 60 * 60))
            .unwrap();
        assert!(cache.is_stale());
        file.set_modified(SystemTime::now()).unwrap();
        assert!(!cache.is_stale());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_stores_do_not_replace_a_valid_cache() {
        let dir = temp_dir("metadata-invalid");
        let cache = MetadataCache::new(&dir);
        cache.store(SAMPLE, &HttpState::default()).unwrap();
        let error = cache
            .store("{not a list", &HttpState::default())
            .unwrap_err();
        assert!(matches!(error, MetadataCacheError::Parse { .. }));
        assert!(cache.load_cached().is_some());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_or_corrupt_cache_is_none_and_state_defaults() {
        let dir = temp_dir("metadata-missing");
        let cache = MetadataCache::new(&dir);
        assert!(!cache.has_cache());
        assert!(cache.load_cached().is_none());
        assert!(cache.is_stale());
        assert_eq!(cache.load_state(), HttpState::default());
        fs::write(cache.cache_path(), "{broken").unwrap();
        assert!(cache.load_cached().is_none());
        fs::write(cache.state_path(), "[]").unwrap();
        assert_eq!(cache.load_state(), HttpState::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn formats_negative_and_large_numbers() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(-1_234_567), "-1,234,567");
    }
}
