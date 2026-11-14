use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::network::{Fetch, Network, NetworkError};
use crate::nexus_auth::{NexusKeyError, NexusKeyStore};

const API_BASE: &str = "https://api.nexusmods.com/v1/games/scavprototype";
const VALIDATE_URL: &str = "https://api.nexusmods.com/v1/users/validate.json";
const APPLICATION_NAME: &str = "Cuhub";
const APPLICATION_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_DOWNLOAD_BYTES: u64 = 1024 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug)]
pub enum NexusError {
    MissingKey,
    Key(NexusKeyError),
    InvalidModId(String),
    HttpStatus { url: String, status: u16 },
    Network(NetworkError),
    Json(serde_json::Error),
    NoFile,
    NoDownloadLink,
    NoFileName,
    Destination { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for NexusError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingKey => formatter.write_str(
                "No Nexus API key is saved. Add one in Settings to use direct downloads.",
            ),
            Self::Key(error) => write!(formatter, "{error}"),
            Self::InvalidModId(id) => write!(formatter, "\"{id}\" is not a Nexus mod id"),
            Self::HttpStatus { url, status } => match status {
                401 => formatter.write_str(
                    "Nexus rejected the API key. Save a valid key in Settings and try again.",
                ),
                403 => formatter.write_str(
                    "Nexus refused the request (403). Direct downloads need a Premium account, and the API key must be valid.",
                ),
                404 => write!(formatter, "Nexus did not find this mod or file ({url})."),
                _ => write!(formatter, "Nexus returned HTTP {status} for {url}."),
            },
            Self::Network(error) => write!(formatter, "{error}"),
            Self::Json(error) => write!(formatter, "Nexus returned an unexpected response: {error}"),
            Self::NoFile => formatter.write_str(
                "Nexus did not list any main or update files for this mod.",
            ),
            Self::NoDownloadLink => formatter.write_str("Nexus did not return a download link."),
            Self::NoFileName => formatter.write_str("Nexus did not provide a file name."),
            Self::Destination { path, source } => {
                write!(formatter, "could not create {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for NexusError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Key(error) => Some(error),
            Self::Network(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Destination { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<NetworkError> for NexusError {
    fn from(error: NetworkError) -> Self {
        match error {
            NetworkError::HttpStatus { url, status } => Self::HttpStatus { url, status },
            other => Self::Network(other),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NexusFile {
    #[serde(default)]
    pub file_id: u64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub category_id: Option<u64>,
    #[serde(default)]
    pub uploaded_timestamp: Option<i64>,
}

impl NexusFile {
    fn download_name(&self) -> Option<&str> {
        self.file_name
            .as_deref()
            .or(self.name.as_deref())
            .filter(|value| !value.trim().is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NexusDownload {
    pub path: PathBuf,
    pub file_name: String,
    pub file_id: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NexusAccount {
    pub name: String,
    pub is_premium: bool,
}

#[derive(Deserialize)]
struct AccountResponse {
    #[serde(default)]
    name: String,
    #[serde(default)]
    is_premium: bool,
}

pub struct Nexus {
    network: Network,
    keys: NexusKeyStore,
}

impl Nexus {
    pub fn new(network: Network, app_data: PathBuf) -> Self {
        Self {
            network,
            keys: NexusKeyStore::new(app_data),
        }
    }

    pub fn has_key(&self) -> bool {
        self.keys.is_configured()
    }

    pub fn save_key(&self, key: &str) -> Result<(), NexusError> {
        self.keys.save(key).map_err(NexusError::Key)
    }

    pub fn clear_key(&self) -> Result<(), NexusError> {
        self.keys.clear().map_err(NexusError::Key)
    }

    pub fn validate_key(&self) -> Result<NexusAccount, NexusError> {
        let key = self.api_key()?;
        let body = self.get_json(VALIDATE_URL, &key)?;
        let account: AccountResponse = serde_json::from_slice(&body).map_err(NexusError::Json)?;
        Ok(NexusAccount {
            name: account.name,
            is_premium: account.is_premium,
        })
    }

    pub fn fetch_files(&self, mod_id: &str) -> Result<Vec<NexusFile>, NexusError> {
        check_mod_id(mod_id)?;
        let key = self.api_key()?;
        let url = format!("{API_BASE}/mods/{mod_id}/files.json");
        let body = self.get_json(&url, &key)?;
        parse_files(&body)
    }

    pub fn download_latest(
        &self,
        mod_id: &str,
        destination_dir: &Path,
    ) -> Result<NexusDownload, NexusError> {
        let files = self.fetch_files(mod_id)?;
        let file = select_file(&files).ok_or(NexusError::NoFile)?;
        let key = self.api_key()?;
        let url = format!(
            "{API_BASE}/mods/{mod_id}/files/{}/download_link.json",
            file.file_id
        );
        let body = self.get_json(&url, &key)?;
        let link = parse_download_link(&body).ok_or(NexusError::NoDownloadLink)?;

        fs::create_dir_all(destination_dir).map_err(|source| NexusError::Destination {
            path: destination_dir.to_path_buf(),
            source,
        })?;
        let raw_name = file
            .download_name()
            .or_else(|| link_file_name(&link))
            .ok_or(NexusError::NoFileName)?;
        let file_name = sanitize_file_name(raw_name).ok_or(NexusError::NoFileName)?;
        let destination = unique_destination(destination_dir, &file_name);
        let bytes = self.network.download_with_timeout(
            &link,
            &destination,
            MAX_DOWNLOAD_BYTES,
            DOWNLOAD_TIMEOUT,
        )?;
        Ok(NexusDownload {
            path: destination,
            file_name,
            file_id: file.file_id,
            bytes,
        })
    }

    fn api_key(&self) -> Result<String, NexusError> {
        self.keys.load().ok_or(NexusError::MissingKey)
    }

    fn get_json(&self, url: &str, key: &str) -> Result<Vec<u8>, NexusError> {
        let headers = [
            ("apikey", key),
            ("Application-Name", APPLICATION_NAME),
            ("Application-Version", APPLICATION_VERSION),
        ];
        match self
            .network
            .get_with_headers(url, &headers, MAX_RESPONSE_BYTES)?
        {
            Fetch::Fetched { body, .. } => Ok(body),
            Fetch::NotModified => Err(NexusError::NoFile),
        }
    }
}

fn check_mod_id(mod_id: &str) -> Result<(), NexusError> {
    if !mod_id.is_empty() && mod_id.chars().all(|value| value.is_ascii_digit()) {
        Ok(())
    } else {
        Err(NexusError::InvalidModId(mod_id.to_owned()))
    }
}

fn parse_files(body: &[u8]) -> Result<Vec<NexusFile>, NexusError> {
    let value: Value = serde_json::from_slice(body).map_err(NexusError::Json)?;
    let array = match value {
        Value::Array(array) => array,
        Value::Object(mut object) => match object.remove("files") {
            Some(Value::Array(array)) => array,
            _ => return Err(NexusError::NoFile),
        },
        _ => return Err(NexusError::NoFile),
    };
    let value = Value::Array(array);
    serde_json::from_value(value).map_err(NexusError::Json)
}

fn select_file(files: &[NexusFile]) -> Option<&NexusFile> {
    let newest = |file: &&NexusFile| (file.uploaded_timestamp.unwrap_or_default(), file.file_id);
    let main = files
        .iter()
        .filter(|file| file.file_id != 0 && matches!(file.category_id, Some(1) | Some(2)))
        .max_by_key(newest);
    main.or_else(|| {
        files
            .iter()
            .filter(|file| file.file_id != 0)
            .max_by_key(newest)
    })
}

fn parse_download_link(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let items = match value {
        Value::Array(items) => items,
        Value::Object(_) => vec![value],
        _ => return None,
    };
    items.iter().find_map(|item| {
        item.get("URI")
            .or_else(|| item.get("uri"))
            .and_then(Value::as_str)
            .filter(|value| value.starts_with("https://"))
            .map(str::to_owned)
    })
}

fn link_file_name(link: &str) -> Option<&str> {
    link.split(['?', '#'])
        .next()
        .and_then(|value| value.rsplit('/').next())
        .filter(|value| !value.is_empty())
}

fn sanitize_file_name(value: &str) -> Option<String> {
    let leaf = value.rsplit(['/', '\\']).next().unwrap_or(value);
    let mut name = String::with_capacity(leaf.len());
    for character in leaf.chars() {
        if character.is_control()
            || matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            )
        {
            name.push('_');
        } else {
            name.push(character);
        }
    }
    let name = name.trim().trim_matches('.').trim();
    if name.is_empty() {
        return None;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        return Some(format!("_{name}"));
    }
    let name: String = name.chars().take(140).collect();
    Some(name)
}

fn unique_destination(directory: &Path, file_name: &str) -> PathBuf {
    let candidate = directory.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, extension) = match file_name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => {
            (stem, Some(extension))
        }
        _ => (file_name, None),
    };
    for index in 2..=999 {
        let name = match extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = directory.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    match extension {
        Some(extension) => directory.join(format!("{stem} (1000).{extension}")),
        None => directory.join(format!("{stem} (1000)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    fn file(file_id: u64, category_id: Option<u64>, uploaded_timestamp: i64) -> NexusFile {
        NexusFile {
            file_id,
            name: Some(format!("file-{file_id}")),
            file_name: Some(format!("file-{file_id}.zip")),
            category_id,
            uploaded_timestamp: Some(uploaded_timestamp),
        }
    }

    #[test]
    fn selects_the_newest_main_or_update_file() {
        let files = vec![
            file(1, Some(1), 100),
            file(2, Some(2), 200),
            file(3, Some(4), 300),
        ];
        assert_eq!(select_file(&files).unwrap().file_id, 2);
        assert_eq!(select_file(&files[..1]).unwrap().file_id, 1);
    }

    #[test]
    fn falls_back_to_other_categories_and_skips_zero_ids() {
        let files = vec![
            file(0, Some(1), 999),
            file(4, Some(4), 10),
            file(5, None, 20),
        ];
        assert_eq!(select_file(&files).unwrap().file_id, 5);
        assert!(select_file(&[]).is_none());
    }

    #[test]
    fn parses_files_from_an_array_or_a_files_object() {
        let array =
            br#"[{"file_id": 7, "file_name": "x.zip", "category_id": 1, "uploaded_timestamp": 5}]"#;
        assert_eq!(parse_files(array).unwrap()[0].file_id, 7);
        let object = br#"{"files": [{"file_id": 8}]}"#;
        assert_eq!(parse_files(object).unwrap()[0].file_id, 8);
        assert!(matches!(parse_files(b"[]").unwrap().as_slice(), []));
        assert!(matches!(parse_files(b"{}"), Err(NexusError::NoFile)));
    }

    #[test]
    fn parses_download_links_from_an_array_or_an_object() {
        let array = br#"[{"name": "x.zip", "URI": "https://example.com/x.zip?token=1"}]"#;
        assert_eq!(
            parse_download_link(array).as_deref(),
            Some("https://example.com/x.zip?token=1")
        );
        let object = br#"{"uri": "https://example.com/y.zip"}"#;
        assert_eq!(
            parse_download_link(object).as_deref(),
            Some("https://example.com/y.zip")
        );
        assert_eq!(
            parse_download_link(br#"{"URI": "http://example.com/x.zip"}"#),
            None
        );
        assert_eq!(parse_download_link(b"[]"), None);
    }

    #[test]
    fn strips_paths_and_replaces_windows_illegal_characters() {
        assert_eq!(
            sanitize_file_name("../../a:b*c?.zip").as_deref(),
            Some("a_b_c_.zip")
        );
        assert_eq!(sanitize_file_name("...").as_deref(), None);
        assert_eq!(sanitize_file_name("CON.txt").as_deref(), Some("_CON.txt"));
    }

    #[test]
    fn avoids_overwriting_existing_downloads() {
        let directory = temp_dir("nexus-download-name");
        fs::write(directory.join("mod.zip"), b"x").unwrap();
        fs::write(directory.join("mod (2).zip"), b"x").unwrap();
        assert_eq!(
            unique_destination(&directory, "mod.zip"),
            directory.join("mod (3).zip")
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn explains_http_statuses_without_hiding_the_cause() {
        let forbidden = NexusError::HttpStatus {
            url: "https://api.nexusmods.com/x".to_owned(),
            status: 403,
        };
        assert!(forbidden.to_string().contains("Premium"));
        let missing = NexusError::MissingKey.to_string();
        assert!(missing.contains("Settings"));
    }

    #[test]
    fn rejects_mod_ids_that_could_change_the_url() {
        assert!(matches!(check_mod_id("12"), Ok(())));
        assert!(matches!(
            check_mod_id("12/files"),
            Err(NexusError::InvalidModId(_))
        ));
        assert!(matches!(check_mod_id(""), Err(NexusError::InvalidModId(_))));
    }
}
