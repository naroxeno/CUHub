//! Atomic replacement of small state files: unique temp file, fsync, rename, parent fsync.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

enum Mode {
    Shared,
    Private,
}

pub(crate) fn replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write(path, bytes, Mode::Shared)
}

pub(crate) fn replace_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write(path, bytes, Mode::Private)
}

fn write(path: &Path, bytes: &[u8], mode: Mode) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = temporary_path(path);
    let result = write_then_rename(&temporary, path, bytes, &mode);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn write_then_rename(temporary: &Path, path: &Path, bytes: &[u8], mode: &Mode) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    // Windows has no mode bits; there the protection is the parent directory ACL only.
    #[cfg(unix)]
    if matches!(mode, Mode::Private) {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, path)?;
    sync_parent(path);
    Ok(())
}

fn sync_parent(path: &Path) {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        let _ = fs::File::open(parent).and_then(|directory| directory.sync_all());
    }
    #[cfg(not(unix))]
    let _ = path;
}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.{}-{counter}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    #[test]
    fn replaces_and_leaves_no_temporary_file() {
        let directory = temp_dir("atomic-replace");
        let path = directory.join("state.json");
        replace(&path, b"one").unwrap();
        replace(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        let leftovers = fs::read_dir(&directory)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name() != "state.json")
            .count();
        assert_eq!(leftovers, 0);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn creates_missing_parent_directories() {
        let directory = temp_dir("atomic-parents");
        let path = directory.join("nested").join("deep").join("state.json");
        replace(&path, b"{}").unwrap();
        assert!(path.is_file());
        let _ = fs::remove_dir_all(directory);
    }

    #[cfg(unix)]
    #[test]
    fn private_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let directory = temp_dir("atomic-private");
        let path = directory.join("secret.dat");
        replace_private(&path, b"secret").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_dir_all(directory);
    }
}
