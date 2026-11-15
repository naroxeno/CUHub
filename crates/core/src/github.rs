//! GitHub REST release responses parsed into Hub versions. No HTTP here.

use std::error::Error;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::version::HubVersion;

#[derive(Debug, Deserialize)]
struct ReleasePayload {
    tag_name: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<AssetPayload>,
}

#[derive(Debug, Deserialize)]
struct AssetPayload {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag: String,
    pub version: HubVersion,
    pub published_at: Option<DateTime<Utc>>,
    pub prerelease: bool,
    pub html_url: Option<String>,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
    pub size: u64,
}

#[derive(Debug)]
pub enum ReleaseError {
    Json(serde_json::Error),
    MissingTag,
    Version { tag: String },
    Timestamp { value: String },
    NoReleases,
}

impl fmt::Display for ReleaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(source) => write!(formatter, "release response is not valid JSON: {source}"),
            Self::MissingTag => formatter.write_str("release response has no tag_name"),
            Self::Version { tag } => write!(
                formatter,
                "release tag {tag} is not a three-segment version"
            ),
            Self::Timestamp { value } => write!(
                formatter,
                "released published_at is not an RFC 3339 timestamp: {value}"
            ),
            Self::NoReleases => formatter.write_str("repository has no published releases"),
        }
    }
}

impl Error for ReleaseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(source) => Some(source),
            _ => None,
        }
    }
}

pub fn parse_release(text: &str) -> Result<ReleaseInfo, ReleaseError> {
    let payload: ReleasePayload = serde_json::from_str(text).map_err(ReleaseError::Json)?;
    release_info(payload)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimit {
    pub limit: u64,
    pub remaining: u64,
}

#[derive(Debug, Deserialize)]
struct RateLimitPayload {
    rate: RateLimitCounts,
}

#[derive(Debug, Deserialize)]
struct RateLimitCounts {
    limit: u64,
    remaining: u64,
}

pub fn parse_rate_limit(text: &str) -> Result<RateLimit, serde_json::Error> {
    let payload: RateLimitPayload = serde_json::from_str(text)?;
    Ok(RateLimit {
        limit: payload.rate.limit,
        remaining: payload.rate.remaining,
    })
}

pub fn parse_release_list(text: &str) -> Result<ReleaseInfo, ReleaseError> {
    let payloads: Vec<ReleasePayload> = serde_json::from_str(text).map_err(ReleaseError::Json)?;
    let payload = payloads
        .into_iter()
        .next()
        .ok_or(ReleaseError::NoReleases)?;
    release_info(payload)
}

fn release_info(payload: ReleasePayload) -> Result<ReleaseInfo, ReleaseError> {
    let tag = payload.tag_name.trim();
    if tag.is_empty() {
        return Err(ReleaseError::MissingTag);
    }
    let version = HubVersion::parse(tag).ok_or_else(|| ReleaseError::Version {
        tag: tag.to_owned(),
    })?;
    let published_at = match payload.published_at {
        Some(value) => Some(
            DateTime::parse_from_rfc3339(&value)
                .map(|time| time.with_timezone(&Utc))
                .map_err(|_| ReleaseError::Timestamp {
                    value: value.trim().to_owned(),
                })?,
        ),
        None => None,
    };
    Ok(ReleaseInfo {
        tag: tag.to_owned(),
        version,
        published_at,
        prerelease: payload.prerelease,
        html_url: payload
            .html_url
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
        assets: payload.assets.into_iter().filter_map(asset_info).collect(),
    })
}

fn asset_info(payload: AssetPayload) -> Option<ReleaseAsset> {
    let name = payload.name.trim();
    let download_url = payload.browser_download_url.trim();
    if name.is_empty() || !download_url.starts_with("https://") {
        return None;
    }
    Some(ReleaseAsset {
        name: name.to_owned(),
        download_url: download_url.to_owned(),
        size: payload.size,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckFailure {
    RateLimited { reset_in_minutes: Option<u64> },
    Missing,
    Forbidden,
    Status { status: u16 },
    Network { detail: String },
    Unreadable { detail: String },
}

impl CheckFailure {
    pub fn from_status(status: u16, reset_in_minutes: Option<u64>) -> Self {
        match status {
            401 => Self::Forbidden,
            403 | 429 => Self::RateLimited { reset_in_minutes },
            404 => Self::Missing,
            status => Self::Status { status },
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::RateLimited {
                reset_in_minutes: Some(minutes),
            } => format!("GitHub is rate limiting checks; try again in about {minutes} min."),
            Self::RateLimited {
                reset_in_minutes: None,
            } => "GitHub is rate limiting checks; try again later.".to_owned(),
            Self::Missing => "No published release found for this repository.".to_owned(),
            Self::Forbidden => "GitHub refused the request.".to_owned(),
            Self::Status { status } => format!("GitHub returned HTTP {status}."),
            Self::Network { .. } => "Could not reach GitHub.".to_owned(),
            Self::Unreadable { .. } => {
                "GitHub returned a release this build cannot read.".to_owned()
            }
        }
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::Network { detail } | Self::Unreadable { detail } => Some(detail),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NORMAL: &str = r#"{"tag_name":"v1.2.3","published_at":"2026-10-01T12:00:00Z","prerelease":false,"html_url":"https://github.com/a/b/releases/tag/v1.2.3","id":1,"body":"notes"}"#;

    #[test]
    fn parses_the_first_release_of_a_list() {
        let text = r#"[{"tag_name":"v2.0.0"},{"tag_name":"v1.0.0"}]"#;
        let release = parse_release_list(text).expect("parse");
        assert_eq!(release.tag, "v2.0.0");
    }

    #[test]
    fn empty_release_list_is_reported() {
        assert!(matches!(
            parse_release_list("[]"),
            Err(ReleaseError::NoReleases)
        ));
    }

    #[test]
    fn parses_a_normal_release() {
        let release = parse_release(NORMAL).expect("parse");
        assert_eq!(release.tag, "v1.2.3");
        assert_eq!(
            release.version,
            HubVersion::parse("1.2.3").expect("version")
        );
        assert!(release.published_at.is_some());
        assert!(!release.prerelease);
        assert_eq!(
            release.html_url.as_deref(),
            Some("https://github.com/a/b/releases/tag/v1.2.3")
        );
    }

    #[test]
    fn release_assets_keep_only_https_links() {
        let text = r#"{"tag_name":"v1.0.0","assets":[{"name":"mod.zip","browser_download_url":"https://example.com/mod.zip","size":42},{"name":"bad.zip","browser_download_url":"http://example.com/bad.zip"},{"name":" ","browser_download_url":"https://example.com/blank.zip"}]}"#;
        let release = parse_release(text).expect("parse");
        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.assets[0].name, "mod.zip");
        assert_eq!(
            release.assets[0].download_url,
            "https://example.com/mod.zip"
        );
        assert_eq!(release.assets[0].size, 42);
    }
    #[test]
    fn four_segment_tag_is_rejected() {
        let text = r#"{"tag_name":"v1.2.3.4"}"#;
        assert!(matches!(
            parse_release(text),
            Err(ReleaseError::Version { .. })
        ));
    }

    #[test]
    fn missing_tag_is_rejected() {
        assert!(matches!(parse_release("{}"), Err(ReleaseError::Json(_))));
        assert!(matches!(
            parse_release(r#"{"tag_name":"  "}"#),
            Err(ReleaseError::MissingTag)
        ));
    }

    #[test]
    fn bad_timestamp_is_rejected() {
        let text = r#"{"tag_name":"v1.2.3","published_at":"yesterday"}"#;
        assert!(matches!(
            parse_release(text),
            Err(ReleaseError::Timestamp { .. })
        ));
    }

    #[test]
    fn http_statuses_map_to_failures() {
        assert_eq!(
            CheckFailure::from_status(403, Some(12)),
            CheckFailure::RateLimited {
                reset_in_minutes: Some(12)
            }
        );
        assert_eq!(
            CheckFailure::from_status(429, None),
            CheckFailure::RateLimited {
                reset_in_minutes: None
            }
        );
        assert_eq!(CheckFailure::from_status(404, None), CheckFailure::Missing);
        assert_eq!(
            CheckFailure::from_status(401, None),
            CheckFailure::Forbidden
        );
        assert_eq!(
            CheckFailure::from_status(500, None),
            CheckFailure::Status { status: 500 }
        );
    }

    #[test]
    fn rate_limits_are_read_from_the_rate_endpoint() {
        let text = r#"{"resources":{},"rate":{"limit":5000,"remaining":4999,"reset":1700000000}}"#;
        let limit = parse_rate_limit(text).unwrap();
        assert_eq!(limit.limit, 5000);
        assert_eq!(limit.remaining, 4999);
        assert!(parse_rate_limit(r#"{"rate":{}}"#).is_err());
        assert!(parse_rate_limit("nope").is_err());
    }

    #[test]
    fn summaries_are_one_line() {
        let limited = CheckFailure::RateLimited {
            reset_in_minutes: Some(12),
        };
        assert_eq!(
            limited.summary(),
            "GitHub is rate limiting checks; try again in about 12 min."
        );
        assert!(limited.detail().is_none());
        let network = CheckFailure::Network {
            detail: "connection reset".to_owned(),
        };
        assert_eq!(network.detail(), Some("connection reset"));
    }
}
