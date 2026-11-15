//! Pinned EasyTier assets and their verified download cache. No UI.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::install::{HashError, verify_sha256};
use crate::network::{Network, NetworkError};

pub const VERSION: &str = "2.6.4";

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Asset {
    file_name: &'static str,
    size: u64,
    sha256: &'static str,
}

impl Asset {
    fn url(&self) -> String {
        format!(
            "https://github.com/EasyTier/EasyTier/releases/download/v{VERSION}/{}",
            self.file_name
        )
    }
}

const WINDOWS: Asset = Asset {
    file_name: "easytier-windows-x86_64-v2.6.4.zip",
    size: 32_657_720,
    sha256: "27af91e270e554709b048bd32327fefd2dfce5062ae1e8701af7550c6f525f84",
};

const LINUX: Asset = Asset {
    file_name: "easytier-linux-x86_64-v2.6.4.zip",
    size: 25_506_523,
    sha256: "61b659eaedba658fa66fe47d17e1426cdd77e5d02fa15fed447bb4357c09dfd6",
};

fn platform_asset() -> Result<Asset, ReleaseError> {
    if cfg!(windows) {
        Ok(WINDOWS)
    } else if cfg!(target_os = "linux") {
        Ok(LINUX)
    } else {
        Err(ReleaseError::UnsupportedPlatform)
    }
}

pub(crate) fn cache_dir(app_data: &Path) -> PathBuf {
    app_data.join("Tools").join("easytier").join(VERSION)
}

pub fn ensure_archive(network: &Network, app_data: &Path) -> Result<PathBuf, ReleaseError> {
    ensure_asset(network, &cache_dir(app_data), &platform_asset()?)
}

fn ensure_asset(
    network: &Network,
    directory: &Path,
    asset: &Asset,
) -> Result<PathBuf, ReleaseError> {
    fs::create_dir_all(directory).map_err(|source| ReleaseError::Directory {
        path: directory.to_path_buf(),
        source,
    })?;
    let archive = directory.join(asset.file_name);
    if archive_matches(&archive, asset) {
        return Ok(archive);
    }
    if archive.exists() {
        fs::remove_file(&archive).map_err(|source| ReleaseError::Stale {
            path: archive.clone(),
            source,
        })?;
    }
    let url = asset.url();
    network
        .download_with_timeout(&url, &archive, asset.size, DOWNLOAD_TIMEOUT)
        .map_err(|source| ReleaseError::Download {
            url: url.clone(),
            path: archive.clone(),
            source,
        })?;
    verify_sha256(&archive, asset.sha256).map_err(|source| ReleaseError::Checksum {
        path: archive.clone(),
        source,
    })?;
    Ok(archive)
}

fn archive_matches(path: &Path, asset: &Asset) -> bool {
    path.is_file() && verify_sha256(path, asset.sha256).is_ok()
}

#[derive(Debug)]
pub enum ReleaseError {
    UnsupportedPlatform,
    Directory {
        path: PathBuf,
        source: io::Error,
    },
    Stale {
        path: PathBuf,
        source: io::Error,
    },
    Download {
        url: String,
        path: PathBuf,
        source: NetworkError,
    },
    Checksum {
        path: PathBuf,
        source: HashError,
    },
}

impl std::fmt::Display for ReleaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                write!(
                    formatter,
                    "EasyTier has no pinned release for this platform"
                )
            }
            Self::Directory { path, source } | Self::Stale { path, source } => {
                write!(formatter, "{}: {source}", path.display())
            }
            Self::Download { url, path, source } => write!(
                formatter,
                "{}: failed to download {url}: {source}",
                path.display()
            ),
            Self::Checksum { path, source } => {
                write!(formatter, "{}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ReleaseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Directory { source, .. } | Self::Stale { source, .. } => Some(source),
            Self::Download { source, .. } => Some(source),
            Self::Checksum { source, .. } => Some(source),
            Self::UnsupportedPlatform => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;
    use reqwest::Url;

    const TEST_PAYLOAD: &[u8] = b"cuhub easytier cache test";
    const TEST_SHA256: &str = "09ae30721537fac50c8c1c75259b070fa696e7c65eccf0ee754c4ba1f2ca4739";

    fn test_asset() -> Asset {
        Asset {
            file_name: "easytier-test.zip",
            size: TEST_PAYLOAD.len() as u64,
            sha256: TEST_SHA256,
        }
    }

    #[test]
    fn pinned_assets_are_well_formed() {
        assert_eq!(VERSION, "2.6.4");
        assert_eq!(WINDOWS.file_name, "easytier-windows-x86_64-v2.6.4.zip");
        assert_eq!(WINDOWS.size, 32_657_720);
        assert_eq!(LINUX.file_name, "easytier-linux-x86_64-v2.6.4.zip");
        assert_eq!(LINUX.size, 25_506_523);
        for asset in [WINDOWS, LINUX] {
            assert_eq!(asset.sha256.len(), 64);
            assert!(
                asset
                    .sha256
                    .chars()
                    .all(|character| matches!(character, '0'..='9' | 'a'..='f'))
            );
            assert!(asset.file_name.contains(VERSION));
            let url = Url::parse(&asset.url()).unwrap();
            assert_eq!(url.scheme(), "https");
            assert_eq!(url.host_str(), Some("github.com"));
            assert!(url.path().ends_with(asset.file_name));
        }
    }

    #[test]
    fn cache_dir_is_versioned() {
        let app_data = Path::new("app-data");
        assert_eq!(
            cache_dir(app_data),
            app_data.join("Tools").join("easytier").join(VERSION)
        );
    }

    #[test]
    fn reuses_archive_with_matching_checksum() {
        let directory = temp_dir("easytier-release-reuse");
        let asset = test_asset();
        let archive = directory.join(asset.file_name);
        fs::write(&archive, TEST_PAYLOAD).unwrap();
        let network = Network::new().unwrap();
        assert_eq!(ensure_asset(&network, &directory, &asset).unwrap(), archive);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn rejects_archive_with_wrong_checksum() {
        let directory = temp_dir("easytier-release-mismatch");
        let asset = test_asset();
        let archive = directory.join(asset.file_name);
        fs::write(&archive, b"tampered").unwrap();
        assert!(!archive_matches(&archive, &asset));
        let _ = fs::remove_dir_all(directory);
    }
}
