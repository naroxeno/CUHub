//! Detect an EasyTier installation on disk. No processes, no network.

use std::path::{Path, PathBuf};

use crate::paths;

use super::release;

#[cfg(windows)]
const REQUIRED: [&str; 4] = [
    "easytier-core.exe",
    "easytier-cli.exe",
    "wintun.dll",
    "Packet.dll",
];

#[cfg(not(windows))]
const REQUIRED: [&str; 2] = ["easytier-core", "easytier-cli"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub root: PathBuf,
    pub version: Option<String>,
    pub core: Option<PathBuf>,
    pub cli: Option<PathBuf>,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Complete,
    DirectoryMissing,
    MissingFiles { missing: Vec<PathBuf> },
}

pub fn detect_managed(app_data: &Path) -> Detection {
    let root = release::cache_dir(app_data);
    let version = if root.is_dir() {
        Some(release::VERSION.to_owned())
    } else {
        None
    };
    inspect(root, version)
}

pub fn detect_custom(directory: &Path) -> Detection {
    inspect(directory.to_path_buf(), None)
}

fn inspect(root: PathBuf, version: Option<String>) -> Detection {
    if !root.is_dir() {
        return Detection {
            root,
            version: None,
            core: None,
            cli: None,
            status: Status::DirectoryMissing,
        };
    }
    let mut found = Vec::with_capacity(REQUIRED.len());
    let mut missing = Vec::new();
    for name in &REQUIRED {
        match find_file(&root, name) {
            Some(path) => found.push(Some(path)),
            None => {
                found.push(None);
                missing.push(root.join(name));
            }
        }
    }
    let status = if missing.is_empty() {
        Status::Complete
    } else {
        Status::MissingFiles { missing }
    };
    Detection {
        root,
        version,
        core: found.first().cloned().flatten(),
        cli: found.get(1).cloned().flatten(),
        status,
    }
}

fn find_file(root: &Path, name: &str) -> Option<PathBuf> {
    paths::find_child(root, name).filter(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::test_support::temp_dir;

    fn write_required(root: &Path, skip: Option<&str>) {
        for name in &REQUIRED {
            if Some(*name) == skip {
                continue;
            }
            fs::write(root.join(name), b"stub").unwrap();
        }
    }

    #[test]
    fn reports_missing_directory() {
        let app_data = temp_dir("easytier-detect-missing");
        let detection = detect_managed(&app_data);
        assert_eq!(detection.root, release::cache_dir(&app_data));
        assert_eq!(detection.version, None);
        assert_eq!(detection.core, None);
        assert_eq!(detection.cli, None);
        assert_eq!(detection.status, Status::DirectoryMissing);
        let _ = fs::remove_dir_all(app_data);
    }

    #[test]
    fn reports_missing_files_in_empty_directory() {
        let app_data = temp_dir("easytier-detect-empty");
        let root = release::cache_dir(&app_data);
        fs::create_dir_all(&root).unwrap();
        let detection = detect_managed(&app_data);
        assert_eq!(detection.version, Some(release::VERSION.to_owned()));
        let expected: Vec<PathBuf> = REQUIRED.iter().map(|name| root.join(name)).collect();
        assert_eq!(detection.status, Status::MissingFiles { missing: expected });
        let _ = fs::remove_dir_all(app_data);
    }

    #[test]
    fn reports_missing_cli() {
        let app_data = temp_dir("easytier-detect-cli");
        let root = release::cache_dir(&app_data);
        fs::create_dir_all(&root).unwrap();
        let cli = REQUIRED[1];
        write_required(&root, Some(cli));
        let detection = detect_managed(&app_data);
        assert_eq!(detection.core, Some(root.join(REQUIRED[0])));
        assert_eq!(detection.cli, None);
        assert_eq!(
            detection.status,
            Status::MissingFiles {
                missing: vec![root.join(cli)]
            }
        );
        let _ = fs::remove_dir_all(app_data);
    }

    #[test]
    fn reports_complete_managed_install() {
        let app_data = temp_dir("easytier-detect-complete");
        let root = release::cache_dir(&app_data);
        fs::create_dir_all(&root).unwrap();
        write_required(&root, None);
        let detection = detect_managed(&app_data);
        assert_eq!(detection.status, Status::Complete);
        assert_eq!(detection.version, Some(release::VERSION.to_owned()));
        assert_eq!(detection.core, Some(root.join(REQUIRED[0])));
        assert_eq!(detection.cli, Some(root.join(REQUIRED[1])));
        let _ = fs::remove_dir_all(app_data);
    }

    #[test]
    fn custom_install_has_no_version_source() {
        let directory = temp_dir("easytier-detect-custom");
        write_required(&directory, None);
        let detection = detect_custom(&directory);
        assert_eq!(detection.status, Status::Complete);
        assert_eq!(detection.version, None);
        let _ = fs::remove_dir_all(directory);
    }
}
