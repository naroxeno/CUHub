//! Preserves the confirmed unmanaged target while installing a replacement.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::Utc;

use super::{PackageError, after_backup};
use crate::links::{copy_file, copy_tree};
use crate::paths::is_link;

pub(super) fn save(source: &Path, root: &Path) -> Result<Option<PathBuf>, PackageError> {
    let metadata = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(source, error)),
    };
    check_plain(source, &metadata)?;
    let name = source
        .file_name()
        .ok_or_else(|| PackageError::InvalidRelative(source.to_path_buf()))?;
    fs::create_dir_all(root).map_err(|error| io_error(root, error))?;
    let stamp = Utc::now().format("Package-%Y%m%d-%H%M%S");
    let backup = (0..)
        .find_map(|suffix| {
            let path = if suffix == 0 {
                root.join(stamp.to_string())
            } else {
                root.join(format!("{stamp}-{suffix}"))
            };
            match fs::create_dir(&path) {
                Ok(()) => Some(Ok(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(io_error(&path, error))),
            }
        })
        .transpose()?
        .ok_or_else(|| io_error(root, io::Error::other("could not reserve a backup")))?;
    let target = backup.join(name);
    if let Err(error) = move_path(source, &target, &metadata) {
        if fs::remove_dir(&backup).is_ok() {
            return Err(error);
        }
        return Err(after_backup(error, &backup, None));
    }
    Ok(Some(backup))
}

pub(super) fn restore(target: &Path, backup: &Path) -> Result<(), PackageError> {
    let name = target
        .file_name()
        .ok_or_else(|| PackageError::InvalidRelative(target.to_path_buf()))?;
    let saved = backup.join(name);
    let metadata = fs::symlink_metadata(&saved).map_err(|error| io_error(&saved, error))?;
    check_plain(&saved, &metadata)?;
    match fs::symlink_metadata(target) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            move_path(&saved, target, &metadata)
        }
        Err(error) => Err(io_error(target, error)),
        Ok(_) => Err(io_error(
            target,
            io::Error::new(io::ErrorKind::AlreadyExists, "restore target is occupied"),
        )),
    }
}

fn check_plain(path: &Path, metadata: &fs::Metadata) -> Result<(), PackageError> {
    if !is_link(metadata) && (metadata.is_file() || metadata.is_dir()) {
        return Ok(());
    }
    Err(io_error(
        path,
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a regular file or directory",
        ),
    ))
}

fn move_path(source: &Path, target: &Path, metadata: &fs::Metadata) -> Result<(), PackageError> {
    match fs::rename(source, target) {
        Ok(()) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {}
        Err(error) => return Err(io_error(source, error)),
    }
    if metadata.is_dir() {
        copy_tree(source, target)?;
        fs::remove_dir_all(source).map_err(|error| io_error(source, error))
    } else {
        copy_file(source, target)?;
        fs::remove_file(source).map_err(|error| io_error(source, error))
    }
}

fn io_error(path: &Path, source: io::Error) -> PackageError {
    PackageError::Io {
        path: path.to_path_buf(),
        source,
    }
}
