//! Content store: immutable per-version payloads under `<appdata>/Store/mods/<modId>/<version>/payload`.
//! Each published version carries an `entry.json` manifest of its source, install time and file hashes.
//! Deployment, profiles and packs are separate modules; nothing here touches the game directory.

use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const STORE_DIR: &str = "Store";
const MODS_DIR: &str = "mods";
const PAYLOAD_DIR: &str = "payload";
const TEMPORARY_DIR: &str = "payload.tmp";
const ENTRY_FILE: &str = "entry.json";
const MAX_ENTRIES: usize = 4096;
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
const CHUNK_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMod {
    pub mod_id: String,
    pub version: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreEntry {
    pub source: String,
    pub installed_at: String,
    pub files: Vec<StoreFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreFile {
    pub path: String,
    pub sha256: String,
    pub len: u64,
}

#[derive(Debug)]
pub enum StoreError {
    InvalidId(String),
    InvalidVersion(String),
    EmptySource(PathBuf),
    TooManyFiles { path: PathBuf },
    FileTooLarge { path: PathBuf, len: u64 },
    SourceSymlink(PathBuf),
    AlreadyExists(PathBuf),
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId(value) => write!(formatter, "\"{value}\" is not a valid mod id"),
            Self::InvalidVersion(value) => {
                write!(formatter, "\"{value}\" is not a valid version")
            }
            Self::EmptySource(path) => write!(formatter, "{} contains no files", path.display()),
            Self::TooManyFiles { path } => write!(
                formatter,
                "{} holds more than {MAX_ENTRIES} files",
                path.display()
            ),
            Self::FileTooLarge { path, len } => write!(
                formatter,
                "{} is {len} bytes, above the {MAX_FILE_BYTES} byte limit",
                path.display()
            ),
            Self::SourceSymlink(path) => {
                write!(formatter, "{} is a symbolic link", path.display())
            }
            Self::AlreadyExists(path) => write!(formatter, "{} already exists", path.display()),
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub struct Store {
    root: PathBuf,
}

struct SourceFile {
    relative: String,
    absolute: PathBuf,
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn import_dir(
        &self,
        src: &Path,
        mod_id: &str,
        version: &str,
        source: &str,
    ) -> Result<StoredMod, StoreError> {
        if !fs::metadata(src)
            .map_err(|error| io_error(src, error))?
            .is_dir()
        {
            return Err(StoreError::Io {
                path: src.to_path_buf(),
                source: io::Error::new(io::ErrorKind::InvalidInput, "not a directory"),
            });
        }
        let files = scan_source(src)?;
        self.import_files(files, src, mod_id, version, source)
    }

    pub fn import_file(
        &self,
        src: &Path,
        mod_id: &str,
        version: &str,
        source: &str,
    ) -> Result<StoredMod, StoreError> {
        let metadata = fs::symlink_metadata(src).map_err(|error| io_error(src, error))?;
        if !metadata.file_type().is_file() {
            return Err(StoreError::Io {
                path: src.to_path_buf(),
                source: io::Error::new(io::ErrorKind::InvalidInput, "not a regular file"),
            });
        }
        if metadata.len() > MAX_FILE_BYTES {
            return Err(StoreError::FileTooLarge {
                path: src.to_path_buf(),
                len: metadata.len(),
            });
        }
        let name = src
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| StoreError::Io {
                path: src.to_path_buf(),
                source: io::Error::new(io::ErrorKind::InvalidData, "file name is not valid UTF-8"),
            })?;
        let files = vec![SourceFile {
            relative: name.to_owned(),
            absolute: src.to_path_buf(),
        }];
        self.import_files(files, src, mod_id, version, source)
    }

    fn import_files(
        &self,
        files: Vec<SourceFile>,
        src: &Path,
        mod_id: &str,
        version: &str,
        source: &str,
    ) -> Result<StoredMod, StoreError> {
        if !is_valid_component(mod_id) {
            return Err(StoreError::InvalidId(mod_id.to_owned()));
        }
        if !is_valid_component(version) {
            return Err(StoreError::InvalidVersion(version.to_owned()));
        }
        if files.is_empty() {
            return Err(StoreError::EmptySource(src.to_path_buf()));
        }
        let version_dir = self.version_dir(mod_id, version);
        // A version is immutable: refuse to merge into anything already on disk, dangling links included.
        if version_dir.symlink_metadata().is_ok() {
            return Err(StoreError::AlreadyExists(version_dir));
        }
        let temporary = version_dir.join(TEMPORARY_DIR);
        let payload = version_dir.join(PAYLOAD_DIR);
        let manifest = match copy_manifest(&files, &temporary) {
            Ok(manifest) => manifest,
            Err(error) => {
                discard(&temporary);
                return Err(error);
            }
        };
        if let Err(error) = fs::rename(&temporary, &payload) {
            discard(&temporary);
            return Err(io_error(&payload, error));
        }
        let entry = StoreEntry {
            source: source.to_owned(),
            installed_at: Utc::now().to_rfc3339(),
            files: manifest,
        };
        if let Err(error) = write_entry(&version_dir, &entry) {
            discard(&version_dir);
            return Err(error);
        }
        let size_bytes = entry.files.iter().map(|file| file.len).sum();
        Ok(StoredMod {
            mod_id: mod_id.to_owned(),
            version: version.to_owned(),
            path: payload,
            size_bytes,
        })
    }

    pub fn list(&self) -> Result<Vec<StoredMod>, StoreError> {
        let mods_root = self.mods_root();
        let mod_directories = match fs::read_dir(&mods_root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_error(&mods_root, error)),
        };
        let mut mods = Vec::new();
        for mod_directory in mod_directories {
            let mod_directory = mod_directory.map_err(|error| io_error(&mods_root, error))?;
            let Ok(mod_id) = mod_directory.file_name().into_string() else {
                continue;
            };
            if !is_valid_component(&mod_id) || !is_plain_directory(&mod_directory) {
                continue;
            }
            let version_root = mod_directory.path();
            let version_directories =
                fs::read_dir(&version_root).map_err(|error| io_error(&version_root, error))?;
            for version_directory in version_directories {
                let version_directory =
                    version_directory.map_err(|error| io_error(&version_root, error))?;
                let Ok(version) = version_directory.file_name().into_string() else {
                    continue;
                };
                if !is_valid_component(&version) || !is_plain_directory(&version_directory) {
                    continue;
                }
                let (Some(path), Some(entry)) = (
                    self.resolve(&mod_id, &version),
                    self.read_entry(&mod_id, &version),
                ) else {
                    continue;
                };
                let size_bytes = entry.files.iter().map(|file| file.len).sum();
                mods.push(StoredMod {
                    mod_id: mod_id.clone(),
                    version,
                    path,
                    size_bytes,
                });
            }
        }
        mods.sort_by(|left, right| {
            left.mod_id
                .cmp(&right.mod_id)
                .then_with(|| left.version.cmp(&right.version))
        });
        Ok(mods)
    }

    pub fn resolve(&self, mod_id: &str, version: &str) -> Option<PathBuf> {
        if !is_valid_component(mod_id) || !is_valid_component(version) {
            return None;
        }
        let version_dir = self.version_dir(mod_id, version);
        let payload = version_dir.join(PAYLOAD_DIR);
        (payload.is_dir() && version_dir.join(ENTRY_FILE).is_file()).then_some(payload)
    }

    pub fn read_entry(&self, mod_id: &str, version: &str) -> Option<StoreEntry> {
        if !is_valid_component(mod_id) || !is_valid_component(version) {
            return None;
        }
        let raw = fs::read_to_string(self.version_dir(mod_id, version).join(ENTRY_FILE)).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn remove(&self, mod_id: &str, version: &str) -> Result<(), StoreError> {
        if !is_valid_component(mod_id) {
            return Err(StoreError::InvalidId(mod_id.to_owned()));
        }
        if !is_valid_component(version) {
            return Err(StoreError::InvalidVersion(version.to_owned()));
        }
        let version_dir = self.version_dir(mod_id, version);
        match fs::remove_dir_all(&version_dir) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(&version_dir, error)),
        }
        if let Some(mod_dir) = version_dir.parent() {
            let _ = fs::remove_dir(mod_dir);
        }
        Ok(())
    }

    pub fn total_bytes(&self) -> u64 {
        self.list()
            .map(|mods| mods.iter().map(|stored| stored.size_bytes).sum())
            .unwrap_or_default()
    }

    fn mods_root(&self) -> PathBuf {
        self.root.join(STORE_DIR).join(MODS_DIR)
    }

    fn version_dir(&self, mod_id: &str, version: &str) -> PathBuf {
        self.mods_root().join(mod_id).join(version)
    }
}

// Ids and versions become single path components, so separators, traversal and anything outside the allow-list are rejected.
fn is_valid_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.bytes().all(
            |byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_'),
        )
}

fn io_error(path: &Path, source: io::Error) -> StoreError {
    StoreError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn discard(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn is_plain_directory(entry: &fs::DirEntry) -> bool {
    entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
}

fn scan_source(src: &Path) -> Result<Vec<SourceFile>, StoreError> {
    let mut files = Vec::new();
    let mut pending: Vec<(PathBuf, Vec<String>)> = vec![(src.to_path_buf(), Vec::new())];
    while let Some((directory, segments)) = pending.pop() {
        let entries = fs::read_dir(&directory).map_err(|error| io_error(&directory, error))?;
        for entry in entries {
            let entry = entry.map_err(|error| io_error(&directory, error))?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|error| io_error(&path, error))?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| StoreError::Io {
                    path: path.clone(),
                    source: io::Error::new(
                        io::ErrorKind::InvalidData,
                        "file name is not valid UTF-8",
                    ),
                })?;
            let mut child = segments.clone();
            child.push(name);
            if kind.is_symlink() {
                return Err(StoreError::SourceSymlink(path));
            }
            if kind.is_dir() {
                pending.push((path, child));
                continue;
            }
            if !kind.is_file() {
                return Err(StoreError::Io {
                    path,
                    source: io::Error::new(io::ErrorKind::Unsupported, "not a regular file"),
                });
            }
            let len = entry
                .metadata()
                .map_err(|error| io_error(&path, error))?
                .len();
            if len > MAX_FILE_BYTES {
                return Err(StoreError::FileTooLarge { path, len });
            }
            if files.len() == MAX_ENTRIES {
                return Err(StoreError::TooManyFiles { path });
            }
            files.push(SourceFile {
                relative: child.join("/"),
                absolute: path,
            });
        }
    }
    files.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(files)
}

fn copy_manifest(files: &[SourceFile], temporary: &Path) -> Result<Vec<StoreFile>, StoreError> {
    let mut manifest = Vec::with_capacity(files.len());
    for file in files {
        let destination = temporary.join(&file.relative);
        let (sha256, len) = copy_and_hash(&file.absolute, &destination)?;
        manifest.push(StoreFile {
            path: file.relative.clone(),
            sha256,
            len,
        });
    }
    Ok(manifest)
}

fn copy_and_hash(source: &Path, destination: &Path) -> Result<(String, u64), StoreError> {
    let mut input = fs::File::open(source).map_err(|error| io_error(source, error))?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    let mut output = fs::File::create(destination).map_err(|error| io_error(destination, error))?;
    let mut hasher = Sha256::new();
    let mut chunk = vec![0u8; CHUNK_BYTES];
    let mut copied = 0u64;
    loop {
        let read = match input.read(&mut chunk) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io_error(source, error)),
        };
        if read == 0 {
            break;
        }
        // A file can grow after the scan; enforce the hard limit on the bytes actually copied.
        copied += read as u64;
        if copied > MAX_FILE_BYTES {
            return Err(StoreError::FileTooLarge {
                path: source.to_path_buf(),
                len: copied,
            });
        }
        hasher.update(&chunk[..read]);
        output
            .write_all(&chunk[..read])
            .map_err(|error| io_error(destination, error))?;
    }
    output
        .sync_all()
        .map_err(|error| io_error(destination, error))?;
    Ok((hex_digest(hasher.finalize().as_ref()), copied))
}

fn write_entry(version_dir: &Path, entry: &StoreEntry) -> Result<(), StoreError> {
    let path = version_dir.join(ENTRY_FILE);
    let json = serde_json::to_string_pretty(entry)
        .map_err(|error| io_error(&path, io::Error::other(error)))?;
    crate::atomic::replace(&path, format!("{json}\n").as_bytes())
        .map_err(|error| io_error(&path, error))
}

fn hex_digest(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    fn payload_source(directory: &Path) -> PathBuf {
        let source = directory.join("source");
        fs::create_dir_all(source.join("bin")).unwrap();
        fs::write(source.join("plugin.dll"), b"MZ payload").unwrap();
        fs::write(source.join("bin").join("extra.txt"), b"extra").unwrap();
        source
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        hex_digest(Sha256::digest(bytes).as_ref())
    }

    #[test]
    fn imports_a_version_with_layout_manifest_and_hashes() {
        let directory = temp_dir("store-import");
        let source = payload_source(&directory);
        let store = Store::new(directory.clone());
        assert_eq!(store.root(), directory.as_path());
        let stored = store
            .import_dir(&source, "com.author.qol", "1.2.3", "nexus:12345")
            .unwrap();
        assert_eq!(stored.mod_id, "com.author.qol");
        assert_eq!(stored.version, "1.2.3");
        let version_dir = directory.join("Store/mods/com.author.qol/1.2.3");
        assert_eq!(stored.path, version_dir.join("payload"));
        assert_eq!(stored.size_bytes, 15);
        assert_eq!(
            fs::read(stored.path.join("plugin.dll")).unwrap(),
            b"MZ payload"
        );
        assert_eq!(
            fs::read(stored.path.join("bin/extra.txt")).unwrap(),
            b"extra"
        );
        let mut names: Vec<String> = fs::read_dir(&version_dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["entry.json".to_string(), "payload".to_string()]);
        let entry = store.read_entry("com.author.qol", "1.2.3").unwrap();
        assert_eq!(entry.source, "nexus:12345");
        assert!(chrono::DateTime::parse_from_rfc3339(&entry.installed_at).is_ok());
        assert_eq!(entry.files.len(), 2);
        assert_eq!(entry.files[0].path, "bin/extra.txt");
        assert_eq!(entry.files[0].len, 5);
        assert_eq!(entry.files[0].sha256, sha256_hex(b"extra"));
        assert_eq!(entry.files[1].path, "plugin.dll");
        assert_eq!(entry.files[1].len, 10);
        assert_eq!(
            entry.files[1].sha256,
            sha256_hex(&fs::read(stored.path.join("plugin.dll")).unwrap())
        );
        let raw = fs::read_to_string(version_dir.join("entry.json")).unwrap();
        assert!(raw.contains("\"installed_at\""));
        assert!(raw.contains("\"sha256\""));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn rejects_ids_and_versions_outside_the_allow_list() {
        let directory = temp_dir("store-ids");
        let source = payload_source(&directory);
        let store = Store::new(directory.clone());
        for bad in [
            "",
            ".",
            "..",
            "../evil",
            "a/b",
            "a\\b",
            "white space",
            "café",
        ] {
            let error = store
                .import_dir(&source, bad, "1.0", "nexus:1")
                .unwrap_err();
            assert!(
                matches!(error, StoreError::InvalidId(value) if value == bad),
                "mod id {bad}"
            );
        }
        for bad in ["", ".", "..", "../1.0", "1.0/beta"] {
            let error = store
                .import_dir(&source, "mod", bad, "nexus:1")
                .unwrap_err();
            assert!(
                matches!(error, StoreError::InvalidVersion(value) if value == bad),
                "version {bad}"
            );
        }
        assert!(matches!(
            store.remove("..", "1.0"),
            Err(StoreError::InvalidId(_))
        ));
        assert!(matches!(
            store.remove("mod", ".."),
            Err(StoreError::InvalidVersion(_))
        ));
        assert!(store.resolve("../evil", "1.0").is_none());
        assert!(!directory.join("Store").exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn refuses_to_replace_an_existing_version() {
        let directory = temp_dir("store-duplicate");
        let source = payload_source(&directory);
        let store = Store::new(directory.clone());
        store.import_dir(&source, "mod", "1.0", "nexus:1").unwrap();
        let error = store
            .import_dir(&source, "mod", "1.0", "nexus:2")
            .unwrap_err();
        assert!(matches!(error, StoreError::AlreadyExists(path) if path.ends_with("mod/1.0")));
        assert_eq!(store.read_entry("mod", "1.0").unwrap().source, "nexus:1");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn lists_resolves_and_removes_versions() {
        let directory = temp_dir("store-list");
        let source = payload_source(&directory);
        let store = Store::new(directory.clone());
        store
            .import_dir(&source, "beta", "2.0", "github:o/r")
            .unwrap();
        store
            .import_dir(&source, "alpha", "1.0", "github:o/r")
            .unwrap();
        let listed = store.list().unwrap();
        let keys: Vec<(&str, &str)> = listed
            .iter()
            .map(|stored| (stored.mod_id.as_str(), stored.version.as_str()))
            .collect();
        assert_eq!(keys, vec![("alpha", "1.0"), ("beta", "2.0")]);
        assert_eq!(store.total_bytes(), 30);
        let payload = store.resolve("alpha", "1.0").unwrap();
        assert!(payload.join("plugin.dll").is_file());
        assert!(store.resolve("alpha", "9.9").is_none());
        store.remove("alpha", "1.0").unwrap();
        assert!(store.resolve("alpha", "1.0").is_none());
        assert!(!directory.join("Store/mods/alpha").exists());
        store.remove("alpha", "1.0").unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(store.total_bytes(), 15);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn an_empty_store_lists_nothing() {
        let directory = temp_dir("store-none");
        let store = Store::new(directory.clone());
        assert!(store.list().unwrap().is_empty());
        assert_eq!(store.total_bytes(), 0);
        assert!(store.resolve("mod", "1.0").is_none());
        assert!(store.read_entry("mod", "1.0").is_none());
        store.remove("mod", "1.0").unwrap();
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn rejects_empty_and_non_directory_sources() {
        let directory = temp_dir("store-source");
        let empty = directory.join("empty");
        fs::create_dir_all(empty.join("nested")).unwrap();
        let store = Store::new(directory.clone());
        assert!(matches!(
            store.import_dir(&empty, "mod", "1.0", "nexus:1"),
            Err(StoreError::EmptySource(_))
        ));
        let file = directory.join("archive.zip");
        fs::write(&file, b"PK").unwrap();
        assert!(matches!(
            store.import_dir(&file, "mod", "1.0", "nexus:1"),
            Err(StoreError::Io { .. })
        ));
        assert!(matches!(
            store.import_dir(&directory.join("missing"), "mod", "1.0", "nexus:1"),
            Err(StoreError::Io { .. })
        ));
        assert!(!directory.join("Store").exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symbolic_links_inside_the_source() {
        use std::os::unix::fs::symlink;
        let directory = temp_dir("store-symlink");
        let source = directory.join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("real.dll"), b"MZ").unwrap();
        let outside = directory.join("outside.dll");
        fs::write(&outside, b"foreign").unwrap();
        symlink(&outside, source.join("link.dll")).unwrap();
        let store = Store::new(directory.clone());
        let error = store
            .import_dir(&source, "mod", "1.0", "nexus:1")
            .unwrap_err();
        assert!(matches!(error, StoreError::SourceSymlink(_)));
        fs::remove_file(source.join("link.dll")).unwrap();
        symlink(&directory, source.join("linked-dir")).unwrap();
        let error = store
            .import_dir(&source, "mod", "1.0", "nexus:1")
            .unwrap_err();
        assert!(matches!(error, StoreError::SourceSymlink(_)));
        assert!(store.resolve("mod", "1.0").is_none());
        assert!(!directory.join("Store").exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn refuses_more_files_than_the_entry_limit() {
        let directory = temp_dir("store-many");
        let source = directory.join("source");
        fs::create_dir_all(&source).unwrap();
        for index in 0..=MAX_ENTRIES {
            fs::write(source.join(format!("file{index:04}.txt")), b"x").unwrap();
        }
        let store = Store::new(directory.clone());
        let error = store
            .import_dir(&source, "mod", "1.0", "nexus:1")
            .unwrap_err();
        assert!(matches!(error, StoreError::TooManyFiles { .. }));
        assert!(store.resolve("mod", "1.0").is_none());
        let _ = fs::remove_dir_all(directory);
    }
}
