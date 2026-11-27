//! Cached GitHub source manifest: URL pinning, six-hour freshness and atomic writes.

use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::network::DEFAULT_MAX_BODY_BYTES;

use super::{ManifestError, SourceManifest};

pub const DEFAULT_MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/naroxeno/Cuprum-Store/main/manifest.json";
pub const CACHE_FILE: &str = "GithubSourcesCache.json";
pub const CACHE_LIFETIME_HOURS: i64 = 6;

const CACHE_SCHEMA_VERSION: u32 = 1;
const MAX_CACHE_BYTES: u64 = 2 * DEFAULT_MAX_BODY_BYTES;

#[derive(Debug)]
pub struct CachedSources {
    pub manifest: SourceManifest,
    pub fetched_at: DateTime<Utc>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

pub struct SourceCache {
    path: PathBuf,
}

impl SourceCache {
    pub fn new(app_data: &Path) -> Self {
        Self {
            path: app_data.join(CACHE_FILE),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Option<CachedSources> {
        let metadata = fs::metadata(&self.path).ok()?;
        if metadata.len() > MAX_CACHE_BYTES {
            return None;
        }
        let text = fs::read_to_string(&self.path).ok()?;
        let document: CacheDocument = serde_json::from_str(&text).ok()?;
        if document.schema_version != CACHE_SCHEMA_VERSION
            || document.url != DEFAULT_MANIFEST_URL
            || document.raw.len() as u64 > DEFAULT_MAX_BODY_BYTES
        {
            return None;
        }
        let fetched_at = DateTime::parse_from_rfc3339(&document.fetched_at)
            .ok()?
            .with_timezone(&Utc);
        let manifest = SourceManifest::parse(&document.raw).ok()?;
        Some(CachedSources {
            manifest,
            fetched_at,
            etag: document.etag,
            last_modified: document.last_modified,
        })
    }

    pub fn is_stale(&self) -> bool {
        let Some(cached) = self.load() else {
            return true;
        };
        Utc::now().signed_duration_since(cached.fetched_at) > Duration::hours(CACHE_LIFETIME_HOURS)
    }

    pub fn store(
        &self,
        raw: &str,
        etag: Option<String>,
        last_modified: Option<String>,
    ) -> Result<(), SourceCacheError> {
        if raw.len() as u64 > DEFAULT_MAX_BODY_BYTES {
            return Err(SourceCacheError::TooLarge {
                limit: DEFAULT_MAX_BODY_BYTES,
            });
        }
        SourceManifest::parse(raw).map_err(SourceCacheError::Manifest)?;
        let document = CacheDocument {
            schema_version: CACHE_SCHEMA_VERSION,
            url: DEFAULT_MANIFEST_URL.to_owned(),
            fetched_at: Utc::now().to_rfc3339(),
            etag,
            last_modified,
            raw: raw.to_owned(),
        };
        let encoded = serde_json::to_vec(&document).map_err(SourceCacheError::Encode)?;
        crate::atomic::replace(&self.path, &encoded).map_err(|source| SourceCacheError::Io {
            path: self.path.clone(),
            source,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CacheDocument {
    schema_version: u32,
    url: String,
    fetched_at: String,
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    last_modified: Option<String>,
    raw: String,
}

#[derive(Debug)]
pub enum SourceCacheError {
    Io { path: PathBuf, source: io::Error },
    Encode(serde_json::Error),
    Manifest(ManifestError),
    TooLarge { limit: u64 },
}

impl fmt::Display for SourceCacheError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Encode(error) => write!(formatter, "could not encode the source cache: {error}"),
            Self::Manifest(error) => {
                write!(
                    formatter,
                    "source cache does not contain a valid manifest: {error}"
                )
            }
            Self::TooLarge { limit } => {
                write!(formatter, "the source manifest exceeds {limit} bytes")
            }
        }
    }
}

impl Error for SourceCacheError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Encode(error) => Some(error),
            Self::Manifest(error) => Some(error),
            Self::TooLarge { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    const RAW: &str = r#"{"schemaVersion":1,"generatedAt":"2026-11-19T00:00:00Z","mods":[]}"#;

    #[test]
    fn stores_and_loads_a_manifest() {
        let directory = temp_dir("source-cache-store");
        fs::create_dir_all(&directory).expect("mkdir");
        let cache = SourceCache::new(&directory);
        cache
            .store(RAW, Some("etag".to_owned()), None)
            .expect("store");
        let loaded = cache.load().expect("load");
        assert!(loaded.manifest.entries.is_empty());
        assert_eq!(loaded.etag.as_deref(), Some("etag"));
        assert!(!cache.is_stale());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_different_url_is_ignored() {
        let directory = temp_dir("source-cache-url");
        fs::create_dir_all(&directory).expect("mkdir");
        let cache = SourceCache::new(&directory);
        let document = CacheDocument {
            schema_version: CACHE_SCHEMA_VERSION,
            url: "https://example.com/manifest.json".to_owned(),
            fetched_at: Utc::now().to_rfc3339(),
            etag: None,
            last_modified: None,
            raw: RAW.to_owned(),
        };
        fs::write(cache.path(), serde_json::to_vec(&document).expect("json")).expect("write");
        assert!(cache.load().is_none());
        assert!(cache.is_stale());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_old_manifest_is_stale() {
        let directory = temp_dir("source-cache-old");
        fs::create_dir_all(&directory).expect("mkdir");
        let cache = SourceCache::new(&directory);
        let document = CacheDocument {
            schema_version: CACHE_SCHEMA_VERSION,
            url: DEFAULT_MANIFEST_URL.to_owned(),
            fetched_at: (Utc::now() - Duration::hours(CACHE_LIFETIME_HOURS + 1)).to_rfc3339(),
            etag: None,
            last_modified: None,
            raw: RAW.to_owned(),
        };
        fs::write(cache.path(), serde_json::to_vec(&document).expect("json")).expect("write");
        assert!(cache.is_stale());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_invalid_manifest_is_not_stored() {
        let directory = temp_dir("source-cache-invalid");
        fs::create_dir_all(&directory).expect("mkdir");
        let cache = SourceCache::new(&directory);
        assert!(matches!(
            cache.store("{not json", None, None),
            Err(SourceCacheError::Manifest(_))
        ));
        let _ = fs::remove_dir_all(&directory);
    }
}
