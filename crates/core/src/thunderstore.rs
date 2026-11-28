//! Casualties: Unknown package catalogue and exact Thunderstore identities.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use flate2::read::MultiGzDecoder;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::network::{Fetch, Network};

const INDEX: &str = "https://thunderstore.io/c/casualties-unknown/api/v1/package-listing-index/";
const MAX_INDEX_BYTES: u64 = 128 * 1024;
const MAX_CHUNK_BYTES: u64 = 16 * 1024 * 1024;
const MAX_CATALOGUE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CHUNKS: usize = 32;
pub const SOURCE_PREFIX: &str = "thunderstore:";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub full_name: String,
    pub owner: String,
    pub versions: Vec<Version>,
    #[serde(default)]
    pub has_nsfw_content: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Version {
    pub version_number: String,
    pub description: String,
    pub dependencies: Vec<String>,
    pub(crate) download_url: String,
    pub(crate) file_size: u64,
}

impl Package {
    pub fn mod_id(&self) -> String {
        format!("thunderstore.{}", self.full_name.to_ascii_lowercase())
    }

    pub(crate) fn source(&self) -> String {
        format!("{SOURCE_PREFIX}{}", self.full_name)
    }
}

#[derive(Clone)]
pub struct Catalogue {
    path: PathBuf,
}

impl Catalogue {
    pub fn new(app_data: &Path) -> Self {
        Self {
            path: app_data.join("Cache/Thunderstore/packages.json"),
        }
    }

    pub fn cached(&self) -> io::Result<Vec<Package>> {
        let mut body = Vec::new();
        fs::File::open(&self.path)?
            .take(MAX_CATALOGUE_BYTES + 1)
            .read_to_end(&mut body)?;
        if body.len() as u64 > MAX_CATALOGUE_BYTES {
            return Err(invalid(String::from(
                "cached Thunderstore index exceeds the size limit",
            )));
        }
        parse_index(&body)
    }

    pub fn refresh(&self) -> io::Result<Vec<Package>> {
        let network = Network::thunderstore().map_err(io::Error::other)?;
        self.refresh_with(|url, limit| {
            match network
                .get(url, None, None, limit)
                .map_err(io::Error::other)?
            {
                Fetch::Fetched { body, .. } => Ok(body),
                Fetch::NotModified => Err(invalid(format!("{url}: unexpected HTTP 304"))),
            }
        })
    }

    fn refresh_with(
        &self,
        mut fetch: impl FnMut(&str, u64) -> io::Result<Vec<u8>>,
    ) -> io::Result<Vec<Package>> {
        let index = fetch(INDEX, MAX_INDEX_BYTES)?;
        let chunks = chunk_urls(&decode_blob(&index, MAX_INDEX_BYTES, INDEX)?)?;
        let mut packages = Vec::new();
        let mut remaining = MAX_CATALOGUE_BYTES;
        for url in chunks {
            let chunk = fetch(url.as_str(), MAX_CHUNK_BYTES)?;
            let body = decode_blob(&chunk, remaining.min(MAX_CHUNK_BYTES), url.as_str())?;
            remaining -= body.len() as u64;
            let entries: Vec<Package> = serde_json::from_slice(&body)
                .map_err(|error| invalid(format!("{url}: invalid Thunderstore chunk: {error}")))?;
            packages.extend(entries);
        }
        let packages = validate_packages(packages)?;
        let body = serde_json::to_vec(&packages)?;
        if body.len() as u64 > MAX_CATALOGUE_BYTES {
            return Err(invalid(String::from(
                "merged Thunderstore catalogue exceeds the size limit",
            )));
        }
        crate::atomic::replace(&self.path, &body)?;
        Ok(packages)
    }
}

fn chunk_urls(body: &[u8]) -> io::Result<Vec<Url>> {
    let entries: Vec<String> = serde_json::from_slice(body)
        .map_err(|error| invalid(format!("invalid Thunderstore chunk index: {error}")))?;
    if entries.is_empty() || entries.len() > MAX_CHUNKS {
        return Err(invalid(format!(
            "unexpected Thunderstore chunk count: {}",
            entries.len()
        )));
    }
    let mut seen = BTreeSet::new();
    let mut urls = Vec::with_capacity(entries.len());
    for (index, entry) in entries.into_iter().enumerate() {
        let number = index + 1;
        let url = Url::parse(&entry)
            .map_err(|_| invalid(format!("Thunderstore chunk {number}: invalid URL")))?;
        let host = url.host_str().unwrap_or("unknown host");
        if !allowed_url(&url) || url.query().is_some() {
            return Err(invalid(format!(
                "Thunderstore chunk {number}: unsupported URL on {host}"
            )));
        }
        if !valid_chunk_path(url.path()) {
            return Err(invalid(format!(
                "Thunderstore chunk {number}: unsupported blob path on {host}"
            )));
        }
        if !seen.insert(url.clone()) {
            return Err(invalid(format!("duplicate Thunderstore chunk: {url}")));
        }
        urls.push(url);
    }
    Ok(urls)
}

fn valid_chunk_path(path: &str) -> bool {
    let Some((directory, file)) = path.rsplit_once('/') else {
        return false;
    };
    matches!(
        directory,
        "/blob-storage/sha256" | "/live/blob-storage/sha256"
    ) && file.ends_with(".blob")
        && file
            .as_bytes()
            .get(..64)
            .is_some_and(|prefix| prefix.iter().all(u8::is_ascii_hexdigit))
        && file
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn decode_blob(input: &[u8], limit: u64, url: &str) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    if input.starts_with(&[0x1f, 0x8b]) {
        MultiGzDecoder::new(input)
            .take(limit + 1)
            .read_to_end(&mut body)
            .map_err(|error| invalid(format!("{url}: invalid gzip blob: {error}")))?;
    } else if input.len() as u64 <= limit {
        return Ok(input.to_vec());
    } else {
        return Err(invalid(format!(
            "{url}: Thunderstore response exceeds {limit} bytes"
        )));
    }
    if body.len() as u64 > limit {
        return Err(invalid(format!(
            "{url}: Thunderstore response exceeds {limit} bytes"
        )));
    }
    Ok(body)
}

fn parse_index(body: &[u8]) -> io::Result<Vec<Package>> {
    validate_packages(serde_json::from_slice(body)?)
}

fn validate_packages(mut packages: Vec<Package>) -> io::Result<Vec<Package>> {
    let mut seen = BTreeSet::new();
    for package in &mut packages {
        if !valid_name(&package.name)
            || !valid_name(&package.owner)
            || package.full_name != format!("{}-{}", package.owner, package.name)
            || !seen.insert(package.full_name.to_ascii_lowercase())
        {
            return Err(invalid(format!(
                "invalid Thunderstore package: {}",
                package.full_name
            )));
        }
        let mut versions = BTreeSet::new();
        for version in &package.versions {
            if version_key(&version.version_number).is_none()
                || !versions.insert(&version.version_number)
                || version
                    .dependencies
                    .iter()
                    .any(|dep| dependency(dep).is_none())
                || !valid_download(package, version)
            {
                return Err(invalid(format!(
                    "invalid Thunderstore release: {}-{}",
                    package.full_name, version.version_number
                )));
            }
        }
        package
            .versions
            .sort_by_key(|version| std::cmp::Reverse(version_key(&version.version_number)));
    }
    packages.retain(|package| !package.versions.is_empty());
    packages.sort_by_cached_key(|package| package.full_name.to_ascii_lowercase());
    Ok(packages)
}

fn valid_download(package: &Package, version: &Version) -> bool {
    let Ok(url) = Url::parse(&version.download_url) else {
        return false;
    };
    let expected = format!(
        "/package/download/{}/{}/{}/",
        package.owner, package.name, version.version_number
    );
    allowed_url(&url)
        && (url.path() == expected
            || (is_cdn(&url)
                && url.path().ends_with(&format!(
                    "/{}-{}.zip",
                    package.full_name, version.version_number
                ))))
}

fn is_cdn(url: &Url) -> bool {
    matches!(
        url.host_str(),
        Some("gcdn.thunderstore.io" | "cdn.thunderstore.io" | "ccdn.thunderstore.io")
    )
}

pub(crate) fn allowed_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
        && (is_cdn(url)
            || matches!(
                url.host_str(),
                Some("thunderstore.io" | "old.thunderstore.io")
            ))
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub(super) fn dependency(input: &str) -> Option<(&str, &str)> {
    let (name, version) = input.rsplit_once('-')?;
    let (owner, package) = name.split_once('-')?;
    (valid_name(owner) && valid_name(package) && version_key(version).is_some())
        .then_some((name, version))
}

fn version_key(input: &str) -> Option<[u64; 3]> {
    let mut parts = input.split('.');
    let mut numbers = [0; 3];
    for number in &mut numbers {
        let part = parts.next()?;
        if !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        *number = part.parse().ok()?;
    }
    parts.next().is_none().then_some(numbers)
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests;
