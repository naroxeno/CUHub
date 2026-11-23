//! Verifies recorded deployments against the filesystem before changing them.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path};

use super::{LinkEntry, LinkError, LinkKind, io_error};
use crate::paths::is_link;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TargetState {
    Missing,
    Incomplete,
    Owned,
}

pub(super) fn inspect(target: &Path, entry: &LinkEntry) -> Result<TargetState, LinkError> {
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(TargetState::Missing),
        Err(error) => return Err(io_error(target, error)),
    };
    let source = Path::new(&entry.store_path);
    let matches = match entry.kind {
        LinkKind::Junction | LinkKind::Symlink if is_link(&metadata) => {
            let link = fs::read_link(target).map_err(|error| io_error(target, error))?;
            let resolved = target.with_file_name(link);
            resolved == source
                || matches!((resolved.canonicalize(), source.canonicalize()), (Ok(a), Ok(b)) if a == b)
        }
        LinkKind::Hardlink if metadata.is_file() && !is_link(&metadata) => {
            same_file(target, source).map_err(|error| io_error(target, error))?
        }
        LinkKind::Copy if metadata.is_file() && !is_link(&metadata) && entry.files.is_empty() => {
            same_contents(target, source).map_err(|error| io_error(target, error))?
        }
        LinkKind::Copy if metadata.is_dir() && !is_link(&metadata) && !entry.files.is_empty() => {
            return inspect_copy(target, source, &entry.files);
        }
        _ => false,
    };
    matches
        .then_some(TargetState::Owned)
        .ok_or_else(|| LinkError::ChangedTarget(target.to_path_buf()))
}

fn inspect_copy(target: &Path, source: &Path, files: &[String]) -> Result<TargetState, LinkError> {
    let mut remaining = std::collections::BTreeSet::new();
    for path in files.iter().map(Path::new) {
        if path.as_os_str().is_empty()
            || !path
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(LinkError::ChangedTarget(target.to_path_buf()));
        }
        remaining.insert(path);
    }
    let mut pending = vec![target.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for child in fs::read_dir(&directory).map_err(|error| io_error(&directory, error))? {
            let child = child.map_err(|error| io_error(&directory, error))?;
            let path = child.path();
            let relative = path.strip_prefix(target).map_err(|error| {
                io_error(&path, io::Error::new(io::ErrorKind::InvalidData, error))
            })?;
            let metadata = fs::symlink_metadata(&path).map_err(|error| io_error(&path, error))?;
            if !is_link(&metadata)
                && metadata.is_dir()
                && files.iter().any(|file| {
                    Path::new(file)
                        .parent()
                        .is_some_and(|parent| parent.starts_with(relative))
                })
            {
                pending.push(path);
            } else if is_link(&metadata)
                || !metadata.is_file()
                || !remaining.remove(relative)
                || !same_contents(&path, &source.join(relative))
                    .map_err(|error| io_error(&path, error))?
            {
                return Err(LinkError::ChangedTarget(path));
            }
        }
    }
    Ok(if remaining.is_empty() {
        TargetState::Owned
    } else {
        TargetState::Incomplete
    })
}

fn same_contents(target: &Path, source: &Path) -> io::Result<bool> {
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.is_file() || is_link(&metadata) {
        return Ok(false);
    }
    let mut target = File::open(target)?;
    let mut source = File::open(source)?;
    let mut remaining = source.metadata()?.len();
    if target.metadata()?.len() != remaining {
        return Ok(false);
    }
    let (mut actual, mut expected) = ([0; 8192], [0; 8192]);
    while remaining != 0 {
        let count = remaining.min(actual.len() as u64) as usize;
        target.read_exact(&mut actual[..count])?;
        source.read_exact(&mut expected[..count])?;
        if actual[..count] != expected[..count] {
            return Ok(false);
        }
        remaining -= count as u64;
    }
    Ok(target.read(&mut actual[..1])? == 0 && source.read(&mut expected[..1])? == 0)
}

#[cfg(unix)]
fn same_file(target: &Path, source: &Path) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let target = fs::symlink_metadata(target)?;
    let source = fs::symlink_metadata(source)?;
    Ok(source.is_file() && target.dev() == source.dev() && target.ino() == source.ino())
}

#[cfg(windows)]
fn same_file(target: &Path, source: &Path) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;

    #[derive(Default, PartialEq, Eq)]
    #[repr(C)]
    struct FileId {
        volume: u64,
        id: [u8; 16],
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandleEx(
            file: std::os::windows::io::RawHandle,
            class: i32,
            data: *mut FileId,
            size: u32,
        ) -> i32;
    }

    fn file_id(path: &Path) -> io::Result<FileId> {
        const FILE_ID_INFO_CLASS: i32 = 18;
        let file = File::open(path)?;
        let mut id = FileId::default();
        // SAFETY: The open handle and the writable FILE_ID_INFO buffer live across this call.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FILE_ID_INFO_CLASS,
                &mut id,
                std::mem::size_of::<FileId>() as u32,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(id)
    }

    let metadata = fs::symlink_metadata(source)?;
    Ok(metadata.is_file() && !is_link(&metadata) && file_id(target)? == file_id(source)?)
}
