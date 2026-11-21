//! The only module allowed to perform HTTP. UI code never calls it directly.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use reqwest::Url;
use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest::header::{
    CONTENT_LENGTH, ETAG, HeaderName, HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED,
};
use reqwest::redirect::Policy;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
pub const MAX_REDIRECTS: usize = 3;
pub const DEFAULT_MAX_BODY_BYTES: u64 = 8 * 1024 * 1024;
pub const DOWNLOAD_CHUNK_BYTES: usize = 32 * 1024;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct Network {
    client: Client,
    covers: Client,
}

impl Network {
    pub fn new() -> Result<Self, NetworkError> {
        Self::with_user_agent(concat!("Cuhub/", env!("CARGO_PKG_VERSION")))
    }

    pub fn with_user_agent(user_agent: &str) -> Result<Self, NetworkError> {
        let build = |redirect| {
            Client::builder()
                .user_agent(user_agent)
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
                .https_only(true)
                .redirect(redirect)
                .build()
                .map_err(|error| NetworkError::Request(Box::new(error)))
        };
        Ok(Self {
            client: build(redirect_policy())?,
            covers: build(Policy::none())?,
        })
    }

    pub(crate) fn get_cover(&self, url: Url, max_bytes: u64) -> Result<Fetch, NetworkError> {
        if !crate::metadata::is_cover_url(&url) {
            return Err(NetworkError::InvalidUrl(url.to_string()));
        }
        self.send(self.covers.get(url.clone()), url, max_bytes)
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
        self.send(request, url, max_bytes)
    }

    pub fn get_with_headers(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        max_bytes: u64,
    ) -> Result<Fetch, NetworkError> {
        let url = validate_url(url)?;
        let request = apply_headers(self.client.get(url.clone()), headers)?;
        self.send(request, url, max_bytes)
    }

    fn send(
        &self,
        request: RequestBuilder,
        url: Url,
        max_bytes: u64,
    ) -> Result<Fetch, NetworkError> {
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
        self.download_with_progress(url, destination, max_bytes, &mut |_| {})
    }

    pub fn download_with_timeout(
        &self,
        url: &str,
        destination: &Path,
        max_bytes: u64,
        timeout: Duration,
    ) -> Result<u64, NetworkError> {
        self.download_with_timeout_progress(url, destination, max_bytes, timeout, &mut |_| {})
    }

    pub fn download_with_progress(
        &self,
        url: &str,
        destination: &Path,
        max_bytes: u64,
        progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<u64, NetworkError> {
        self.download_with_timeout_progress(url, destination, max_bytes, REQUEST_TIMEOUT, progress)
    }

    pub fn download_with_timeout_progress(
        &self,
        url: &str,
        destination: &Path,
        max_bytes: u64,
        timeout: Duration,
        progress: &mut dyn FnMut(DownloadProgress),
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
                .timeout(timeout)
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
            let total = declared_length(&response);
            let written = copy_with_progress(
                &mut response.take(max_bytes.saturating_add(1)),
                &mut file,
                &url,
                total,
                progress,
            )?;
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DownloadProgress {
    pub written: u64,
    pub total: Option<u64>,
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
    InvalidHeader { name: String },
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
            Self::InvalidHeader { name } => {
                write!(formatter, "{name}: invalid HTTP header name or value")
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

fn apply_headers(
    mut request: RequestBuilder,
    headers: &[(&str, &str)],
) -> Result<RequestBuilder, NetworkError> {
    for (name, value) in headers {
        let header_name =
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| NetworkError::InvalidHeader {
                name: (*name).to_owned(),
            })?;
        let header_value =
            HeaderValue::from_str(value).map_err(|_| NetworkError::InvalidHeader {
                name: (*name).to_owned(),
            })?;
        request = request.header(header_name, header_value);
    }
    Ok(request)
}

fn header_string(response: &Response, name: reqwest::header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn declared_length(response: &Response) -> Option<u64> {
    response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

fn reject_oversized(response: &Response, max_bytes: u64, url: &Url) -> Result<(), NetworkError> {
    if let Some(length) = declared_length(response)
        && length > max_bytes
    {
        return Err(NetworkError::TooLarge {
            url: url.to_string(),
            limit: max_bytes,
        });
    }
    Ok(())
}

fn copy_with_progress(
    reader: &mut impl Read,
    writer: &mut impl Write,
    url: &Url,
    total: Option<u64>,
    progress: &mut dyn FnMut(DownloadProgress),
) -> Result<u64, NetworkError> {
    let mut buffer = vec![0u8; DOWNLOAD_CHUNK_BYTES];
    let mut written = 0u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|source| NetworkError::Read {
                url: url.to_string(),
                source,
            })?;
        if read == 0 {
            return Ok(written);
        }
        writer
            .write_all(&buffer[..read])
            .map_err(|source| NetworkError::Read {
                url: url.to_string(),
                source,
            })?;
        written += read as u64;
        progress(DownloadProgress { written, total });
    }
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
    fn rejects_invalid_header_names_before_requesting() {
        let network = Network::new().unwrap();
        let error = network
            .get_with_headers("https://example.com/", &[("bad header", "value")], 1024)
            .unwrap_err();
        assert!(matches!(error, NetworkError::InvalidHeader { ref name } if name == "bad header"));
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

    struct Chunked {
        data: Vec<u8>,
        offset: usize,
        chunk: usize,
    }

    impl Read for Chunked {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let end = (self.offset + self.chunk)
                .min(self.data.len())
                .min(self.offset + buffer.len());
            let count = end.saturating_sub(self.offset);
            buffer[..count].copy_from_slice(&self.data[self.offset..end]);
            self.offset = end;
            Ok(count)
        }
    }

    fn chunked(data: &[u8], chunk: usize) -> Chunked {
        Chunked {
            data: data.to_vec(),
            offset: 0,
            chunk,
        }
    }

    #[test]
    fn reports_progress_after_each_chunk_and_keeps_the_body() {
        let url = Url::parse("https://example.com/a").unwrap();
        let mut reader = chunked(b"0123456789abcdef", 5);
        let mut output = Vec::new();
        let mut seen = Vec::new();
        let written =
            copy_with_progress(&mut reader, &mut output, &url, Some(16), &mut |progress| {
                seen.push(progress)
            })
            .unwrap();
        assert_eq!(written, 16);
        assert_eq!(output, b"0123456789abcdef");
        assert_eq!(
            seen.iter()
                .map(|progress| progress.written)
                .collect::<Vec<_>>(),
            vec![5, 10, 15, 16]
        );
        assert!(seen.iter().all(|progress| progress.total == Some(16)));
    }

    #[test]
    fn reports_no_total_when_content_length_is_missing() {
        let url = Url::parse("https://example.com/a").unwrap();
        let mut reader = chunked(b"abcd", 2);
        let mut output = Vec::new();
        let mut totals = Vec::new();
        copy_with_progress(&mut reader, &mut output, &url, None, &mut |progress| {
            totals.push(progress.total)
        })
        .unwrap();
        assert_eq!(totals, vec![None, None]);
        assert_eq!(output, b"abcd");
    }

    #[test]
    fn progress_downloads_keep_the_destination_and_url_checks() {
        let directory = crate::test_support::temp_dir("network-progress-checks");
        let destination = directory.join("archive.zip");
        fs::write(&destination, b"old").unwrap();
        let network = Network::new().unwrap();
        let mut calls = 0;
        let error = network
            .download_with_progress(
                "https://example.com/archive.zip",
                &destination,
                1024,
                &mut |_| calls += 1,
            )
            .unwrap_err();
        assert!(matches!(error, NetworkError::DestinationExists(ref path) if path == &destination));
        assert_eq!(calls, 0);
        let missing = directory.join("missing.zip");
        let error = network
            .download_with_progress("not a url", &missing, 1024, &mut |_| calls += 1)
            .unwrap_err();
        assert!(matches!(error, NetworkError::InvalidUrl(_)));
        assert!(!missing.exists());
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn downloads_without_a_callback_keep_the_old_checks() {
        let directory = crate::test_support::temp_dir("network-legacy-checks");
        let destination = directory.join("archive.zip");
        let network = Network::new().unwrap();
        let error = network
            .download_with_timeout("http://example.com/a", &destination, 1024, REQUEST_TIMEOUT)
            .unwrap_err();
        assert!(matches!(error, NetworkError::InsecureUrl(_)));
        assert!(!destination.exists());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir_all(directory).unwrap();
    }
}
