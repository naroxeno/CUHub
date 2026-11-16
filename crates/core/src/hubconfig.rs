//! Hub configuration: channel selection, conditional refresh, and https-only links.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use crate::network::{self, Fetch, Network};
use crate::version::HubVersion;

pub const BASE_URL: &str = "https://casualties-hub.github.io/Casualties-Hub-Config/";
pub const REFRESH_INTERVAL_MINUTES: i64 = 30;
pub const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

fn refresh_interval() -> Duration {
    Duration::minutes(REFRESH_INTERVAL_MINUTES)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HubConfigChannel {
    Stable,
    Prerelease,
}

impl HubConfigChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Prerelease => "prerelease",
        }
    }

    pub fn for_version(version: &HubVersion) -> Self {
        if version.is_prerelease() {
            Self::Prerelease
        } else {
            Self::Stable
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubAnnouncement {
    #[serde(default, alias = "Id")]
    pub id: String,
    #[serde(
        default = "default_published_at",
        deserialize_with = "deserialize_datetime",
        alias = "PublishedAtUtc"
    )]
    pub published_at_utc: DateTime<Utc>,
    #[serde(default, alias = "Message")]
    pub message: String,
}

impl HubAnnouncement {
    pub fn is_publishable(&self) -> bool {
        !self.id.trim().is_empty() && !self.message.trim().is_empty()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HubLinks {
    #[serde(alias = "DiscordUrl")]
    pub discord_url: String,
    #[serde(alias = "ReportUrl")]
    pub report_url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HubConfig {
    #[serde(alias = "CurrentAnnouncement")]
    pub current_announcement: Option<HubAnnouncement>,
    #[serde(
        alias = "PreviousAnnouncements",
        deserialize_with = "deserialize_nullable_vec"
    )]
    pub previous_announcements: Vec<HubAnnouncement>,
    #[serde(alias = "Links", deserialize_with = "deserialize_nullable_links")]
    pub links: HubLinks,
}

#[derive(Debug, Clone)]
pub struct HubConfigResult {
    pub config: HubConfig,
    pub is_online: bool,
    pub is_cached: bool,
    pub config_changed: bool,
    pub next_check_utc: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub enum HubConfigError {
    Io { path: PathBuf, source: io::Error },
    Json(serde_json::Error),
}

impl std::fmt::Display for HubConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Json(error) => write!(formatter, "hub config state: {error}"),
        }
    }
}

impl std::error::Error for HubConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json(error) => Some(error),
        }
    }
}

impl From<serde_json::Error> for HubConfigError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct CacheState {
    #[serde(rename = "ETag")]
    etag: Option<String>,
    #[serde(rename = "LastModified")]
    last_modified: Option<String>,
    #[serde(rename = "LastCheckedUtc")]
    last_checked_utc: Option<String>,
}

impl CacheState {
    fn last_checked(&self) -> Option<DateTime<Utc>> {
        self.last_checked_utc
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc))
    }
}

#[derive(Clone)]
pub struct HubConfigService {
    network: Network,
    app_data: PathBuf,
    channel: HubConfigChannel,
}

impl HubConfigService {
    pub fn new(network: Network, app_data: impl Into<PathBuf>, channel: HubConfigChannel) -> Self {
        Self {
            network,
            app_data: app_data.into(),
            channel,
        }
    }

    pub fn channel(&self) -> HubConfigChannel {
        self.channel
    }

    pub fn config_url(&self) -> String {
        match self.channel {
            HubConfigChannel::Stable => format!("{BASE_URL}v1/hub.json"),
            HubConfigChannel::Prerelease => format!("{BASE_URL}v1/prerelease/hub.json"),
        }
    }

    pub fn cache_path(&self) -> PathBuf {
        self.app_data
            .join(format!("HubConfigCache.{}.json", self.channel.as_str()))
    }

    pub fn state_path(&self) -> PathBuf {
        self.app_data
            .join(format!("HubConfigHttpState.{}.json", self.channel.as_str()))
    }

    pub fn load_cached(&self) -> HubConfigResult {
        let state = self.load_state();
        self.cached_result(&state)
    }

    pub fn is_check_due(&self) -> bool {
        self.is_check_due_with(&self.load_state())
    }

    pub fn refresh(&self, force: bool) -> Result<HubConfigResult, HubConfigError> {
        let mut state = self.load_state();
        if !force && !self.is_check_due_with(&state) {
            return Ok(self.cached_result(&state));
        }

        let fetched = self.network.get(
            &self.config_url(),
            state.etag.as_deref(),
            state.last_modified.as_deref(),
            MAX_CONFIG_BYTES,
        );
        let checked_at = Utc::now();
        match fetched {
            Ok(Fetch::NotModified) => self.unchanged(state, checked_at),
            Ok(Fetch::Fetched {
                body,
                etag,
                last_modified,
            }) => {
                let Some(text) = String::from_utf8(body).ok() else {
                    return self.fallback(state, checked_at);
                };
                let Some(config) = parse_config(&text) else {
                    return self.fallback(state, checked_at);
                };
                write_atomic(&self.cache_path(), text.as_bytes())?;
                state.etag = etag;
                state.last_modified = last_modified;
                state.last_checked_utc = Some(format_timestamp(checked_at));
                self.save_state(&state)?;
                Ok(HubConfigResult {
                    config,
                    is_online: true,
                    is_cached: false,
                    config_changed: true,
                    next_check_utc: Some(checked_at + refresh_interval()),
                })
            }
            Err(_) => self.fallback(state, checked_at),
        }
    }

    fn is_check_due_with(&self, state: &CacheState) -> bool {
        state
            .last_checked()
            .is_none_or(|last| Utc::now().signed_duration_since(last) >= refresh_interval())
    }

    fn cached_result(&self, state: &CacheState) -> HubConfigResult {
        HubConfigResult {
            config: self.load_best_available(),
            is_online: false,
            is_cached: true,
            config_changed: false,
            next_check_utc: state.last_checked().map(|last| last + refresh_interval()),
        }
    }

    fn unchanged(
        &self,
        mut state: CacheState,
        checked_at: DateTime<Utc>,
    ) -> Result<HubConfigResult, HubConfigError> {
        state.last_checked_utc = Some(format_timestamp(checked_at));
        self.save_state(&state)?;
        Ok(HubConfigResult {
            config: self.load_best_available(),
            is_online: true,
            is_cached: true,
            config_changed: false,
            next_check_utc: Some(checked_at + refresh_interval()),
        })
    }

    fn fallback(
        &self,
        mut state: CacheState,
        checked_at: DateTime<Utc>,
    ) -> Result<HubConfigResult, HubConfigError> {
        state.last_checked_utc = Some(format_timestamp(checked_at));
        self.save_state(&state)?;
        Ok(HubConfigResult {
            config: self.load_best_available(),
            is_online: false,
            is_cached: true,
            config_changed: false,
            next_check_utc: Some(checked_at + refresh_interval()),
        })
    }

    fn load_best_available(&self) -> HubConfig {
        let Ok(text) = fs::read_to_string(self.cache_path()) else {
            return HubConfig::default();
        };
        parse_config(&text).unwrap_or_default()
    }

    fn load_state(&self) -> CacheState {
        let Ok(text) = fs::read_to_string(self.state_path()) else {
            return CacheState::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    fn save_state(&self, state: &CacheState) -> Result<(), HubConfigError> {
        let mut text = serde_json::to_string_pretty(state)?;
        text.push('\n');
        write_atomic(&self.state_path(), text.as_bytes())
    }
}

fn parse_config(text: &str) -> Option<HubConfig> {
    if !text.trim_start().starts_with('{') {
        return None;
    }
    let mut config: HubConfig = serde_json::from_str(text).ok()?;
    if let Some(current) = &config.current_announcement
        && !current.is_publishable()
    {
        return None;
    }
    config
        .previous_announcements
        .retain(HubAnnouncement::is_publishable);
    config.links.discord_url = validate_link(&config.links.discord_url);
    config.links.report_url = validate_link(&config.links.report_url);
    Some(config)
}

fn validate_link(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    if network::is_https_url(value) {
        value.to_owned()
    } else {
        String::new()
    }
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), HubConfigError> {
    crate::atomic::replace(path, contents).map_err(|source| HubConfigError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn format_timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn default_published_at() -> DateTime<Utc> {
    DateTime::<Utc>::UNIX_EPOCH
}

fn deserialize_datetime<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    DateTime::parse_from_rfc3339(&value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(serde::de::Error::custom)
}

fn deserialize_nullable_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

fn deserialize_nullable_links<'de, D>(deserializer: D) -> Result<HubLinks, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<HubLinks>::deserialize(deserializer)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    const SAMPLE: &str = r#"{
  "currentAnnouncement": {
    "id": "v0.0.8",
    "publishedAtUtc": "2026-08-10T00:00:00Z",
    "message": "Welcome to 0.0.8."
  },
  "previousAnnouncements": [
    {
      "id": "v0.0.8-pre.7",
      "publishedAtUtc": "2026-08-05T00:00:00Z",
      "message": "Previous."
    }
  ],
  "links": {
    "discordUrl": "https://discord.gg/th62vFHWTR",
    "reportUrl": "https://discord.gg/xFHbg4Djrm"
  }
}"#;

    fn service(prefix: &str) -> (HubConfigService, PathBuf) {
        let directory = temp_dir(prefix);
        let service = HubConfigService::new(
            Network::new().unwrap(),
            directory.clone(),
            HubConfigChannel::Stable,
        );
        (service, directory)
    }

    #[test]
    fn selects_channel_from_version() {
        assert_eq!(
            HubConfigChannel::for_version(&HubVersion::parse("0.0.8").unwrap()),
            HubConfigChannel::Stable
        );
        assert_eq!(
            HubConfigChannel::for_version(&HubVersion::parse("0.0.8-pre.6").unwrap()),
            HubConfigChannel::Prerelease
        );
    }

    #[test]
    fn builds_channel_urls_and_cache_names() {
        let (service, directory) = service("hubconfig-paths");
        assert_eq!(service.config_url(), format!("{BASE_URL}v1/hub.json"));
        assert_eq!(
            service.cache_path().file_name().unwrap().to_string_lossy(),
            "HubConfigCache.stable.json"
        );
        assert_eq!(
            service.state_path().file_name().unwrap().to_string_lossy(),
            "HubConfigHttpState.stable.json"
        );
        let prerelease = HubConfigService::new(
            Network::new().unwrap(),
            directory.clone(),
            HubConfigChannel::Prerelease,
        );
        assert_eq!(
            prerelease.config_url(),
            format!("{BASE_URL}v1/prerelease/hub.json")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn parses_the_published_shape() {
        let config = parse_config(SAMPLE).unwrap();
        let current = config.current_announcement.unwrap();
        assert_eq!(current.id, "v0.0.8");
        assert_eq!(
            current.published_at_utc.to_rfc3339(),
            "2026-08-10T00:00:00+00:00"
        );
        assert_eq!(config.previous_announcements.len(), 1);
        assert_eq!(config.links.discord_url, "https://discord.gg/th62vFHWTR");
        assert_eq!(config.links.report_url, "https://discord.gg/xFHbg4Djrm");
    }

    #[test]
    fn drops_links_that_are_not_plain_https() {
        let config = parse_config(
            r#"{"links":{"discordUrl":"http://example.com","reportUrl":"https://user:pass@example.com/report"}}"#,
        )
        .unwrap();
        assert!(config.links.discord_url.is_empty());
        assert!(config.links.report_url.is_empty());
    }

    #[test]
    fn keeps_valid_https_links() {
        let config =
            parse_config(r#"{"links":{"discordUrl":"https://example.com/discord"}}"#).unwrap();
        assert_eq!(config.links.discord_url, "https://example.com/discord");
        assert!(config.links.report_url.is_empty());
    }

    #[test]
    fn rejects_an_unpublishable_current_announcement() {
        assert!(parse_config(r#"{"currentAnnouncement":{"id":"x","message":"   "}}"#).is_none());
        assert!(parse_config(r#"{"currentAnnouncement":{"message":"hello"}}"#).is_none());
    }

    #[test]
    fn filters_unpublishable_previous_announcements() {
        let config = parse_config(
            r#"{"previousAnnouncements":[{"id":"a","message":"ok"},{"id":"","message":"bad"},{"id":"b","message":" "}]}"#,
        )
        .unwrap();
        assert_eq!(config.previous_announcements.len(), 1);
        assert_eq!(config.previous_announcements[0].id, "a");
    }

    #[test]
    fn accepts_null_sections() {
        let config = parse_config(
            r#"{"currentAnnouncement":null,"previousAnnouncements":null,"links":null}"#,
        )
        .unwrap();
        assert!(config.current_announcement.is_none());
        assert!(config.previous_announcements.is_empty());
        assert!(config.links.discord_url.is_empty());
        assert!(config.links.report_url.is_empty());
    }

    #[test]
    fn treats_missing_sections_as_empty() {
        let config = parse_config("{}").unwrap();
        assert!(config.current_announcement.is_none());
        assert!(config.previous_announcements.is_empty());
        assert_eq!(config.links.discord_url, "");
    }

    #[test]
    fn rejects_empty_or_null_documents() {
        assert!(parse_config("").is_none());
        assert!(parse_config("null").is_none());
        assert!(parse_config("[]").is_none());
    }

    #[test]
    fn load_cached_without_files_returns_empty() {
        let (service, directory) = service("hubconfig-empty");
        let result = service.load_cached();
        assert!(result.config.current_announcement.is_none());
        assert!(result.config.previous_announcements.is_empty());
        assert!(result.config.links.discord_url.is_empty());
        assert!(!result.is_online);
        assert!(result.is_cached);
        assert!(!result.config_changed);
        assert!(result.next_check_utc.is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupt_cache_falls_back_to_empty() {
        let (service, directory) = service("hubconfig-corrupt");
        fs::write(service.cache_path(), b"not json").unwrap();
        let result = service.load_cached();
        assert!(result.config.current_announcement.is_none());
        assert!(result.config.previous_announcements.is_empty());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_recent_check_is_not_due() {
        let (service, directory) = service("hubconfig-due");
        let state = CacheState {
            last_checked_utc: Some(format_timestamp(Utc::now())),
            ..CacheState::default()
        };
        service.save_state(&state).unwrap();
        assert!(!service.is_check_due());
        let result = service.refresh(false).unwrap();
        assert!(!result.is_online);
        assert!(result.is_cached);
        assert!(!result.config_changed);
        assert!(result.next_check_utc.is_some());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn an_old_check_is_due() {
        let (service, directory) = service("hubconfig-old");
        let state = CacheState {
            last_checked_utc: Some(format_timestamp(Utc::now() - Duration::hours(1))),
            ..CacheState::default()
        };
        service.save_state(&state).unwrap();
        assert!(service.is_check_due());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn state_round_trips() {
        let (service, directory) = service("hubconfig-state");
        let state = CacheState {
            etag: Some("\"abc\"".to_owned()),
            last_modified: Some("Wed, 21 Oct 2015 07:28:00 GMT".to_owned()),
            last_checked_utc: Some("2026-11-14T12:00:00Z".to_owned()),
        };
        service.save_state(&state).unwrap();
        let loaded = service.load_state();
        assert_eq!(loaded.etag.as_deref(), Some("\"abc\""));
        assert_eq!(
            loaded.last_modified.as_deref(),
            Some("Wed, 21 Oct 2015 07:28:00 GMT")
        );
        assert_eq!(
            loaded.last_checked().unwrap().to_rfc3339(),
            "2026-11-14T12:00:00+00:00"
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
