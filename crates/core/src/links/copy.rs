//! Copy fallback with exclusive destination creation and cleanup on failure.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use super::{LinkError, MAX_COPY_ENTRIES, io_error, is_link};

pub(crate) fn copy_file(source: &Path, target: &Path) -> Result<(), LinkError> {
    let mut input = File::open(source).map_err(|error| io_error(source, error))?;
    let permissions = input
        .metadata()
        .map_err(|error| io_error(source, error))?
        .permissions();
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|error| io_error(target, error))?;
    let result = io::copy(&mut input, &mut output)
        .and_then(|_| output.sync_all())
        .and_then(|_| output.set_permissions(permissions))
        .map_err(|error| io_error(target, error));
    drop(output);
    if let Err(error) = result {
        return Err(with_cleanup(error, target, fs::remove_file(target)));
    }
    Ok(())
}

pub(crate) fn copy_tree(source: &Path, target: &Path) -> Result<Vec<String>, LinkError> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    if let Err(mut error) = fill_tree(source, target, &mut files, &mut dirs) {
        for path in files.iter().rev().map(|path| target.join(path)) {
            error = with_cleanup(error, &path, fs::remove_file(&path));
        }
        for path in dirs.iter().rev() {
            error = with_cleanup(error, path, fs::remove_dir(path));
        }
        return Err(error);
    }
    Ok(files)
}

fn fill_tree(
    source: &Path,
    target: &Path,
    files: &mut Vec<String>,
    dirs: &mut Vec<PathBuf>,
) -> Result<(), LinkError> {
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let from = source.join(&relative);
        let to = target.join(&relative);
        fs::create_dir(&to).map_err(|error| io_error(&to, error))?;
        dirs.push(to);
        for entry in fs::read_dir(&from).map_err(|error| io_error(&from, error))? {
            let entry = entry.map_err(|error| io_error(&from, error))?;
            let path = entry.path();
            let child = relative.join(entry.file_name());
            let metadata = fs::symlink_metadata(&path).map_err(|error| io_error(&path, error))?;
            if is_link(&metadata) || !(metadata.is_file() || metadata.is_dir()) {
                return Err(LinkError::ChangedTarget(path));
            }
            if metadata.is_dir() {
                pending.push(child);
            } else {
                if files.len() >= MAX_COPY_ENTRIES {
                    return Err(io_error(&from, io::Error::other("too many files to copy")));
                }
                let name = child
                    .to_str()
                    .ok_or_else(|| LinkError::ChangedTarget(path.clone()))?;
                copy_file(&path, &target.join(&child))?;
                files.push(name.to_owned());
            }
        }
    }
    Ok(())
}

fn with_cleanup(error: LinkError, path: &Path, cleanup: io::Result<()>) -> LinkError {
    match cleanup {
        Err(recovery) if recovery.kind() != io::ErrorKind::NotFound => LinkError::Recovery {
            source: Box::new(error),
            recovery: Box::new(io_error(path, recovery)),
        },
        _ => error,
    }
}
