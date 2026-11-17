//! Pinned EasyTier assets and their verified download cache. No UI.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::install::{HashError, verify_sha256};

use super::detect::{self, Detection};
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

pub fn ensure_installed(network: &Network, app_data: &Path) -> Result<Detection, ReleaseError> {
    let archive = ensure_archive(network, app_data)?;
    let root = cache_dir(app_data);
    extract(&archive, &root).map_err(|source| ReleaseError::Extract {
        path: archive.clone(),
        source,
    })?;
    Ok(detect::detect_managed(app_data))
}

fn extract(archive: &Path, root: &Path) -> Result<(), io::Error> {
    use std::io::Read;

    let file = fs::File::open(archive).map_err(|source| {
        io::Error::new(source.kind(), format!("{}: {source}", archive.display()))
    })?;
    let mut zip = zip::ZipArchive::new(file).map_err(io::Error::other)?;
    let required = detect::required_files();
    let mut found = 0;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(io::Error::other)?;
        if entry.is_dir() {
            continue;
        }
        let Ok(entry_name) = entry.name() else {
            continue;
        };
        let Some(name) = std::path::Path::new(entry_name.as_ref())
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_owned())
        else {
            continue;
        };
        if !required.contains(&name.as_str()) {
            continue;
        }
        let target = root.join(&name);
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        fs::write(&target, &bytes)?;
        set_executable(&target)?;
        found += 1;
    }
    let missing: Vec<String> = required
        .iter()
        .filter(|name| !root.join(name).is_file())
        .map(|name| (*name).to_owned())
        .collect();
    if !missing.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "missing from archive: {} ({found} extracted)",
                missing.join(", ")
            ),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> io::Result<()> {
    Ok(())
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
    Extract {
        path: PathBuf,
        source: io::Error,
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
            Self::Extract { path, source } => {
                write!(formatter, "{}: failed to extract: {source}", path.display())
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
            Self::Extract { source, .. } => Some(source),
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

    fn write_zip(path: &Path, names: &[&str]) {
        use std::io::Write;
        let file = fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for name in names {
            writer
                .start_file(format!("easytier-linux-x86_64/{name}"), options)
                .unwrap();
            writer.write_all(b"stub").unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn extracts_required_files_from_archive() {
        let directory = temp_dir("easytier-release-extract");
        let archive = directory.join("release.zip");
        let required = detect::required_files();
        write_zip(&archive, required);
        let root = directory.join("out");
        fs::create_dir_all(&root).unwrap();
        extract(&archive, &root).unwrap();
        for name in required {
            assert!(root.join(name).is_file(), "missing {name}");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(root.join(required[0]))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn extraction_reports_missing_required_files() {
        let directory = temp_dir("easytier-release-extract-missing");
        let archive = directory.join("release.zip");
        write_zip(&archive, &[detect::required_files()[0]]);
        let root = directory.join("out");
        fs::create_dir_all(&root).unwrap();
        let error = extract(&archive, &root).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        let _ = fs::remove_dir_all(directory);
    }
}
