//! Extracts zip, 7z and rar archives into a private staging directory. No game writes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use crate::paths;

const MAX_ENTRIES: usize = 4096;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    SevenZ,
    Rar,
}

impl ArchiveFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "zip" => Some(Self::Zip),
            "7z" => Some(Self::SevenZ),
            "rar" => Some(Self::Rar),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFile {
    pub relative: PathBuf,
    pub path: PathBuf,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedArchive {
    pub staging_root: PathBuf,
    pub files: Vec<StagedFile>,
}

#[derive(Debug)]
pub enum ArchiveError {
    UnsupportedFormat(PathBuf),
    UnsupportedEntry(String),
    UnsafeEntry(String),
    DuplicateEntry(String),
    TooManyEntries,
    EntryTooLarge {
        name: String,
        size: u64,
    },
    ArchiveTooLarge,
    EncryptedEntry(String),
    MissingEntry(String),
    StagingExists(PathBuf),
    SizeMismatch {
        name: String,
        expected: u64,
        actual: u64,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Zip(zip::result::ZipError),
    SevenZ(sevenz_rust::Error),
    Rar(rars::Error),
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormat(path) => {
                write!(formatter, "unsupported archive format: {}", path.display())
            }
            Self::UnsupportedEntry(name) => {
                write!(formatter, "archive entry is not a regular file: {name}")
            }
            Self::UnsafeEntry(name) => {
                write!(formatter, "archive entry is not a safe path: {name}")
            }
            Self::DuplicateEntry(name) => write!(formatter, "archive repeats path {name}"),
            Self::TooManyEntries => formatter.write_str("archive has too many entries"),
            Self::EntryTooLarge { name, size } => write!(
                formatter,
                "archive entry {name} is too large ({size} bytes)"
            ),
            Self::ArchiveTooLarge => formatter.write_str("archive expands to too many bytes"),
            Self::EncryptedEntry(name) => write!(formatter, "archive entry {name} is encrypted"),
            Self::MissingEntry(name) => write!(formatter, "archive entry {name} was not extracted"),
            Self::StagingExists(path) => write!(
                formatter,
                "staging directory already exists: {}",
                path.display()
            ),
            Self::SizeMismatch {
                name,
                expected,
                actual,
            } => write!(
                formatter,
                "archive entry {name} is {actual} bytes, expected {expected}"
            ),
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Zip(error) => write!(formatter, "zip: {error}"),
            Self::SevenZ(error) => write!(formatter, "7z: {error}"),
            Self::Rar(error) => write!(formatter, "rar: {error}"),
        }
    }
}

impl std::error::Error for ArchiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<zip::result::ZipError> for ArchiveError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::Zip(error)
    }
}

impl From<sevenz_rust::Error> for ArchiveError {
    fn from(error: sevenz_rust::Error) -> Self {
        Self::SevenZ(error)
    }
}

impl From<rars::Error> for ArchiveError {
    fn from(error: rars::Error) -> Self {
        Self::Rar(error)
    }
}

pub fn stage_archive(
    archive_path: &Path,
    staging_root: &Path,
) -> Result<StagedArchive, ArchiveError> {
    let format = ArchiveFormat::from_path(archive_path)
        .ok_or_else(|| ArchiveError::UnsupportedFormat(archive_path.to_path_buf()))?;
    if staging_root.exists() {
        return Err(ArchiveError::StagingExists(staging_root.to_path_buf()));
    }
    fs::create_dir_all(staging_root).map_err(|source| ArchiveError::Io {
        path: staging_root.to_path_buf(),
        source,
    })?;
    let extracted = match format {
        ArchiveFormat::Zip => stage_zip(archive_path, staging_root),
        ArchiveFormat::SevenZ => stage_7z(archive_path, staging_root),
        ArchiveFormat::Rar => stage_rar(archive_path, staging_root),
    };
    let expected = match extracted {
        Ok(expected) => expected,
        Err(error) => {
            let _ = fs::remove_dir_all(staging_root);
            return Err(error);
        }
    };
    match collect_staged(staging_root, &expected) {
        Ok(files) => Ok(StagedArchive {
            staging_root: staging_root.to_path_buf(),
            files,
        }),
        Err(error) => {
            let _ = fs::remove_dir_all(staging_root);
            Err(error)
        }
    }
}

fn stage_zip(path: &Path, staging_root: &Path) -> Result<BTreeMap<String, u64>, ArchiveError> {
    let file = File::open(path).map_err(|source| ArchiveError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut archive = zip::ZipArchive::new(file)?;
    if archive.len() > MAX_ENTRIES {
        return Err(ArchiveError::TooManyEntries);
    }
    let mut expected = BTreeMap::new();
    let mut total = 0;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        if entry.is_symlink() {
            return Err(ArchiveError::UnsupportedEntry(entry.name()?.into_owned()));
        }
        let name = entry.name()?.into_owned();
        let relative = normalize_entry_name(&name)?;
        let size = entry.size();
        record_entry(&name, &relative, size, &mut total, &mut expected)?;
        let mut target = create_staging_file(staging_root, &relative)?;
        let copied = io::copy(&mut entry.take(size + 1), &mut target).map_err(|source| {
            ArchiveError::Io {
                path: relative.clone(),
                source,
            }
        })?;
        if copied != size {
            return Err(ArchiveError::SizeMismatch {
                name,
                expected: size,
                actual: copied,
            });
        }
    }
    Ok(expected)
}

fn stage_7z(path: &Path, staging_root: &Path) -> Result<BTreeMap<String, u64>, ArchiveError> {
    let mut reader = sevenz_rust::SevenZReader::open(path, sevenz_rust::Password::empty())?;
    let mut expected = BTreeMap::new();
    let mut total = 0;
    let mut failure = None;
    reader.for_each_entries(|entry, data| {
        if failure.is_some() {
            return Ok(false);
        }
        if entry.is_directory() {
            return Ok(true);
        }
        if entry.is_anti_item() || entry.windows_attributes() & 0x400 != 0 {
            failure = Some(ArchiveError::UnsupportedEntry(entry.name().to_owned()));
            return Ok(false);
        }
        if !entry.has_stream() {
            return Ok(true);
        }
        let name = entry.name().to_owned();
        let relative = match normalize_entry_name(&name) {
            Ok(relative) => relative,
            Err(error) => {
                failure = Some(error);
                return Ok(false);
            }
        };
        let size = entry.size();
        if let Err(error) = record_entry(&name, &relative, size, &mut total, &mut expected) {
            failure = Some(error);
            return Ok(false);
        }
        let mut target = match create_staging_file(staging_root, &relative) {
            Ok(target) => target,
            Err(error) => {
                failure = Some(error);
                return Ok(false);
            }
        };
        let copied = match io::copy(&mut data.take(size + 1), &mut target) {
            Ok(copied) => copied,
            Err(source) => {
                failure = Some(ArchiveError::Io {
                    path: relative,
                    source,
                });
                return Ok(false);
            }
        };
        if copied != size {
            failure = Some(ArchiveError::SizeMismatch {
                name,
                expected: size,
                actual: copied,
            });
            return Ok(false);
        }
        Ok(true)
    })?;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(expected)
}

fn stage_rar(path: &Path, staging_root: &Path) -> Result<BTreeMap<String, u64>, ArchiveError> {
    let options = rars_read_options();
    let archive = rars::ArchiveReader::read_path_with_options(path, options)?;
    let mut expected = BTreeMap::new();
    let mut total = 0;
    for member in archive.members() {
        if member.meta.is_directory {
            continue;
        }
        let name = String::from_utf8(member.meta.name.clone())
            .map_err(|_| ArchiveError::UnsupportedEntry("non-UTF-8 rar entry name".to_owned()))?;
        if member.meta.is_encrypted {
            return Err(ArchiveError::EncryptedEntry(name));
        }
        if member.meta.is_redirection || member.meta.is_split_before || member.meta.is_split_after {
            return Err(ArchiveError::UnsupportedEntry(name));
        }
        let relative = normalize_entry_name(&name)?;
        record_entry(
            &name,
            &relative,
            member.meta.unpacked_size,
            &mut total,
            &mut expected,
        )?;
    }
    let options = rars_read_options();
    archive.extract_to_with_options(options, |meta| {
        if meta.is_directory {
            return Ok(Box::new(io::sink()));
        }
        let name = std::str::from_utf8(meta.name_bytes())
            .map_err(|_| rars::Error::from(io::Error::other("non-UTF-8 rar entry name")))?;
        let relative = normalize_entry_name(name)
            .map_err(|error| rars::Error::from(io::Error::other(error.to_string())))?;
        let target = create_staging_file(staging_root, &relative)
            .map_err(|error| rars::Error::from(io::Error::other(error.to_string())))?;
        Ok(Box::new(target))
    })?;
    Ok(expected)
}

fn rars_read_options() -> rars::ArchiveReadOptions<'static> {
    rars::ArchiveReadOptions::new()
        .with_max_header_count(MAX_ENTRIES as u64)
        .with_max_member_output_bytes(MAX_FILE_BYTES)
        .with_max_total_output_bytes(MAX_TOTAL_BYTES)
}

fn record_entry(
    name: &str,
    relative: &Path,
    size: u64,
    total: &mut u64,
    expected: &mut BTreeMap<String, u64>,
) -> Result<(), ArchiveError> {
    if size > MAX_FILE_BYTES {
        return Err(ArchiveError::EntryTooLarge {
            name: name.to_owned(),
            size,
        });
    }
    *total = total
        .checked_add(size)
        .ok_or(ArchiveError::ArchiveTooLarge)?;
    if *total > MAX_TOTAL_BYTES {
        return Err(ArchiveError::ArchiveTooLarge);
    }
    let key = path_key(relative);
    if expected.insert(key, size).is_some() {
        return Err(ArchiveError::DuplicateEntry(name.to_owned()));
    }
    Ok(())
}

fn create_staging_file(staging_root: &Path, relative: &Path) -> Result<File, ArchiveError> {
    let target = staging_root.join(relative);
    if !paths::is_inside(&target, staging_root) {
        return Err(ArchiveError::UnsafeEntry(relative.display().to_string()));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| ArchiveError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|source| ArchiveError::Io {
            path: target,
            source,
        })
}

fn normalize_entry_name(name: &str) -> Result<PathBuf, ArchiveError> {
    if name.is_empty() || name.contains('\0') {
        return Err(ArchiveError::UnsafeEntry(name.to_owned()));
    }
    let normalized = name.replace('\\', "/");
    let mut relative = PathBuf::new();
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(part) => {
                let part = part.to_string_lossy();
                if !is_safe_component(&part) {
                    return Err(ArchiveError::UnsafeEntry(name.to_owned()));
                }
                relative.push(part.as_ref());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ArchiveError::UnsafeEntry(name.to_owned()));
            }
        }
    }
    if relative.as_os_str().is_empty() || relative.components().count() > 32 {
        return Err(ArchiveError::UnsafeEntry(name.to_owned()));
    }
    Ok(relative)
}

fn is_safe_component(component: &str) -> bool {
    if component.is_empty() || component == "." || component == ".." {
        return false;
    }
    if component.contains(':') || component.ends_with('.') || component.ends_with(' ') {
        return false;
    }
    if component.len() > 200 {
        return false;
    }
    let stem = component.trim_end_matches(['.', ' ']).to_ascii_uppercase();
    let stem = stem.split('.').next().unwrap_or(&stem);
    !matches!(
        stem,
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().to_lowercase()
}

fn collect_staged(
    staging_root: &Path,
    expected: &BTreeMap<String, u64>,
) -> Result<Vec<StagedFile>, ArchiveError> {
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    collect_dir(staging_root, staging_root, expected, &mut seen, &mut files)?;
    if seen.len() != expected.len() {
        let missing = expected
            .keys()
            .find(|key| !seen.contains(*key))
            .cloned()
            .unwrap_or_default();
        return Err(ArchiveError::MissingEntry(missing));
    }
    files.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(files)
}

fn collect_dir(
    staging_root: &Path,
    directory: &Path,
    expected: &BTreeMap<String, u64>,
    seen: &mut BTreeSet<String>,
    files: &mut Vec<StagedFile>,
) -> Result<(), ArchiveError> {
    for entry in fs::read_dir(directory).map_err(|source| ArchiveError::Io {
        path: directory.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| ArchiveError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|source| ArchiveError::Io {
            path: path.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(ArchiveError::UnsupportedEntry(path.display().to_string()));
        }
        if metadata.is_dir() {
            collect_dir(staging_root, &path, expected, seen, files)?;
            continue;
        }
        let relative = path
            .strip_prefix(staging_root)
            .map_err(|_| ArchiveError::UnsafeEntry(path.display().to_string()))?
            .to_path_buf();
        let key = path_key(&relative);
        let expected_size = expected
            .get(&key)
            .copied()
            .ok_or_else(|| ArchiveError::UnsupportedEntry(relative.display().to_string()))?;
        if metadata.len() != expected_size {
            return Err(ArchiveError::SizeMismatch {
                name: relative.display().to_string(),
                expected: expected_size,
                actual: metadata.len(),
            });
        }
        seen.insert(key);
        files.push(StagedFile {
            relative,
            size: metadata.len(),
            path,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_nested_and_backslash_entry_names() {
        assert_eq!(
            normalize_entry_name("BepInEx/plugins/Mod/Mod.dll").unwrap(),
            PathBuf::from("BepInEx/plugins/Mod/Mod.dll")
        );
        assert_eq!(
            normalize_entry_name("Mod\\Data\\icon.png").unwrap(),
            PathBuf::from("Mod/Data/icon.png")
        );
    }

    #[test]
    fn rejects_traversal_absolute_and_drive_paths() {
        assert!(normalize_entry_name("../evil.dll").is_err());
        assert!(normalize_entry_name("/etc/passwd").is_err());
        assert!(normalize_entry_name("C:\\temp\\evil.dll").is_err());
        assert!(normalize_entry_name("").is_err());
    }

    #[test]
    fn rejects_windows_reserved_and_trailing_dot_names() {
        assert!(normalize_entry_name("CON").is_err());
        assert!(normalize_entry_name("Mod/foo.").is_err());
        assert!(normalize_entry_name("Mod/foo ").is_err());
    }

    #[test]
    fn stages_a_real_zip_archive() {
        use crate::test_support::temp_dir;
        use std::io::Write;

        let dir = temp_dir("archive-zip");
        let archive = dir.join("mod.zip");
        let file = File::create(&archive).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "BepInEx/plugins/Mod/Mod.dll",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"payload").unwrap();
        writer
            .start_file("README.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"readme").unwrap();
        writer.finish().unwrap();

        let staging = dir.join("staging");
        let staged = stage_archive(&archive, &staging).unwrap();
        assert_eq!(staged.files.len(), 2);
        assert_eq!(
            staged.files[0].relative,
            PathBuf::from("BepInEx/plugins/Mod/Mod.dll")
        );
        assert_eq!(fs::read(&staged.files[0].path).unwrap(), b"payload");
    }

    #[test]
    fn rejects_a_zip_with_a_traversal_entry() {
        use crate::test_support::temp_dir;
        use std::io::Write;

        let dir = temp_dir("archive-zip-escape");
        let archive = dir.join("mod.zip");
        let file = File::create(&archive).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("../evil.dll", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"payload").unwrap();
        writer.finish().unwrap();

        let staging = dir.join("staging");
        let error = stage_archive(&archive, &staging).unwrap_err();
        assert!(matches!(error, ArchiveError::UnsafeEntry(_)));
        assert!(!staging.exists());
    }
}
