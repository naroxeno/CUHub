//! The only module allowed to perform HTTP. UI code never calls it directly.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use reqwest::Url;
use reqwest::blocking::{Client, Response};
use reqwest::header::{CONTENT_LENGTH, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use reqwest::redirect::Policy;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
pub const MAX_REDIRECTS: usize = 3;
pub const DEFAULT_MAX_BODY_BYTES: u64 = 8 * 1024 * 1024;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct Network {
    client: Client,
}

impl Network {
    pub fn new() -> Result<Self, NetworkError> {
        Self::with_user_agent(concat!("Cuhub/", env!("CARGO_PKG_VERSION")))
    }

    pub fn with_user_agent(user_agent: &str) -> Result<Self, NetworkError> {
        let client = Client::builder()
            .user_agent(user_agent)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .https_only(true)
            .redirect(redirect_policy())
            .build()
            .map_err(|error| NetworkError::Request(Box::new(error)))?;
        Ok(Self { client })
    }

    pub fn get(
        &self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
        max_bytes: u64,
    ) -> Result<Fetch, NetworkError> {
        let url = validate_url(url)?;
        let mut request = self.client.get(url.clone());
        if let Some(value) = etag.filter(|value| !value.trim().is_empty()) {
            request = request.header(IF_NONE_MATCH, value);
        }
        if let Some(value) = last_modified.filter(|value| !value.trim().is_empty()) {
            request = request.header(IF_MODIFIED_SINCE, value);
        }
        let response = request
            .send()
            .map_err(|error| NetworkError::Request(Box::new(error)))?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(Fetch::NotModified);
        }
        if status != reqwest::StatusCode::OK {
            return Err(NetworkError::HttpStatus {
                url: url.to_string(),
                status: status.as_u16(),
            });
        }
        let etag = header_string(&response, ETAG);
        let last_modified = header_string(&response, LAST_MODIFIED);
        reject_oversized(&response, max_bytes, &url)?;
        let body = read_limited(response, max_bytes, &url)?;
        Ok(Fetch::Fetched {
            body,
            etag,
            last_modified,
        })
    }

    pub fn download(
        &self,
        url: &str,
        destination: &Path,
        max_bytes: u64,
    ) -> Result<u64, NetworkError> {
        let url = validate_url(url)?;
        if destination.exists() {
            return Err(NetworkError::DestinationExists(destination.to_path_buf()));
        }
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));
        let (temporary, mut file) = create_temporary(parent)?;
        let outcome = (|| -> Result<u64, NetworkError> {
            let response = self
                .client
                .get(url.clone())
                .send()
                .map_err(|error| NetworkError::Request(Box::new(error)))?;
            let status = response.status();
            if status != reqwest::StatusCode::OK {
                return Err(NetworkError::HttpStatus {
                    url: url.to_string(),
                    status: status.as_u16(),
                });
            }
            reject_oversized(&response, max_bytes, &url)?;
            let written = io::copy(&mut response.take(max_bytes.saturating_add(1)), &mut file)
                .map_err(|source| NetworkError::Read {
                    url: url.to_string(),
                    source,
                })?;
            if written > max_bytes {
                return Err(NetworkError::TooLarge {
                    url: url.to_string(),
                    limit: max_bytes,
                });
            }
            file.flush().map_err(|source| NetworkError::Io {
                path: temporary.clone(),
                source,
            })?;
            file.sync_all().map_err(|source| NetworkError::Io {
                path: temporary.clone(),
                source,
            })?;
            Ok(written)
        })();
        drop(file);
        match outcome {
            Ok(written) => {
                if destination.exists() {
                    let _ = fs::remove_file(&temporary);
                    return Err(NetworkError::DestinationExists(destination.to_path_buf()));
                }
                fs::rename(&temporary, destination).map_err(|source| {
                    let _ = fs::remove_file(&temporary);
                    NetworkError::Io {
                        path: destination.to_path_buf(),
                        source,
                    }
                })?;
                Ok(written)
            }
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                Err(error)
            }
        }
    }
}

#[derive(Debug)]
pub enum Fetch {
    NotModified,
    Fetched {
        body: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
    },
}

#[derive(Debug)]
pub enum NetworkError {
    InvalidUrl(String),
    InsecureUrl(String),
    UrlHasCredentials(String),
    Request(Box<reqwest::Error>),
    HttpStatus { url: String, status: u16 },
    TooLarge { url: String, limit: u64 },
    DestinationExists(PathBuf),
    Io { path: PathBuf, source: io::Error },
    Read { url: String, source: io::Error },
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUrl(url) => write!(formatter, "{url}: not an absolute URL"),
            Self::InsecureUrl(url) => write!(formatter, "{url}: only https URLs are allowed"),
            Self::UrlHasCredentials(url) => {
                write!(
                    formatter,
                    "{url}: URLs with embedded credentials are not allowed"
                )
            }
            Self::Request(error) => write!(formatter, "request failed: {error}"),
            Self::HttpStatus { url, status } => {
                write!(formatter, "{url}: unexpected HTTP status {status}")
            }
            Self::TooLarge { url, limit } => {
                write!(formatter, "{url}: response exceeds {limit} bytes")
            }
            Self::DestinationExists(path) => {
                write!(formatter, "{}: destination already exists", path.display())
            }
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Read { url, source } => {
                write!(formatter, "{url}: failed to read response: {source}")
            }
        }
    }
}

impl std::error::Error for NetworkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request(error) => Some(&**error),
            Self::Io { source, .. } | Self::Read { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for NetworkError {
    fn from(error: reqwest::Error) -> Self {
        Self::Request(Box::new(error))
    }
}

fn redirect_policy() -> Policy {
    Policy::custom(|attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            return attempt.error("too many redirects");
        }
        let url = attempt.url();
        let insecure = url.scheme() != "https";
        let credentials = !url.username().is_empty() || url.password().is_some();
        if insecure {
            return attempt.error("redirect left https");
        }
        if credentials {
            return attempt.error("redirect contains credentials");
        }
        attempt.follow()
    })
}

pub fn is_https_url(value: &str) -> bool {
    validate_url(value).is_ok()
}

fn validate_url(input: &str) -> Result<Url, NetworkError> {
    let text = input.trim();
    if text.is_empty() {
        return Err(NetworkError::InvalidUrl(input.to_owned()));
    }
    let url = Url::parse(text).map_err(|_| NetworkError::InvalidUrl(input.to_owned()))?;
    if url.scheme() != "https" {
        return Err(NetworkError::InsecureUrl(text.to_owned()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(NetworkError::UrlHasCredentials(text.to_owned()));
    }
    Ok(url)
}

fn header_string(response: &Response, name: reqwest::header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn reject_oversized(response: &Response, max_bytes: u64, url: &Url) -> Result<(), NetworkError> {
    let declared = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    if let Some(length) = declared
        && length > max_bytes
    {
        return Err(NetworkError::TooLarge {
            url: url.to_string(),
            limit: max_bytes,
        });
    }
    Ok(())
}

fn read_limited(reader: impl Read, max_bytes: u64, url: &Url) -> Result<Vec<u8>, NetworkError> {
    let mut body = Vec::new();
    reader
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut body)
        .map_err(|source| NetworkError::Read {
            url: url.to_string(),
            source,
        })?;
    if body.len() as u64 > max_bytes {
        return Err(NetworkError::TooLarge {
            url: url.to_string(),
            limit: max_bytes,
        });
    }
    Ok(body)
}

fn create_temporary(parent: &Path) -> Result<(PathBuf, File), NetworkError> {
    for _ in 0..16 {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".cuhub-{}-{counter}.part", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(NetworkError::Io { path, source }),
        }
    }
    Err(NetworkError::Io {
        path: parent.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a temporary download file",
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_https_urls() {
        assert!(validate_url("https://example.com/a?b=c").is_ok());
    }

    #[test]
    fn rejects_non_https_urls() {
        let error = validate_url("http://example.com/a").unwrap_err();
        assert!(matches!(error, NetworkError::InsecureUrl(_)));
        let error = validate_url("ftp://example.com/a").unwrap_err();
        assert!(matches!(error, NetworkError::InsecureUrl(_)));
    }

    #[test]
    fn rejects_invalid_urls() {
        let error = validate_url("not a url").unwrap_err();
        assert!(matches!(error, NetworkError::InvalidUrl(_)));
        let error = validate_url("").unwrap_err();
        assert!(matches!(error, NetworkError::InvalidUrl(_)));
    }

    #[test]
    fn rejects_embedded_credentials() {
        let error = validate_url("https://user:secret@example.com/a").unwrap_err();
        assert!(matches!(error, NetworkError::UrlHasCredentials(_)));
    }

    #[test]
    fn body_limit_is_inclusive_and_one_byte_over_fails() {
        let url = Url::parse("https://example.com/a").unwrap();
        let body = read_limited(io::Cursor::new(b"1234"), 4, &url).unwrap();
        assert_eq!(body, b"1234");
        let error = read_limited(io::Cursor::new(b"12345"), 4, &url).unwrap_err();
        assert!(matches!(error, NetworkError::TooLarge { limit: 4, .. }));
    }

    #[test]
    fn download_refuses_existing_destination_before_requesting() {
        let directory = crate::test_support::temp_dir("network-download");
        let destination = directory.join("archive.zip");
        fs::write(&destination, b"old").unwrap();
        let network = Network::new().unwrap();
        let error = network
            .download("https://example.com/archive.zip", &destination, 1024)
            .unwrap_err();
        assert!(matches!(error, NetworkError::DestinationExists(ref path) if path == &destination));
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        fs::remove_dir_all(directory).unwrap();
    }
}
