use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use aes_gcm::aead::{AeadCore, AeadInPlace, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce, Tag};

const DATA_FILE: &str = "NexusApiKey.dat";
const KEY_FILE: &str = "NexusApiKey.key";
const MAGIC: &[u8; 4] = b"CHK1";
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const KEY_LEN: usize = 32;

#[derive(Debug)]
pub enum NexusKeyError {
    Io { path: PathBuf, source: io::Error },
    Crypto,
}

impl std::fmt::Display for NexusKeyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Crypto => formatter.write_str("could not encrypt the Nexus API key"),
        }
    }
}

impl std::error::Error for NexusKeyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Crypto => None,
        }
    }
}

pub struct NexusKeyStore {
    data_dir: PathBuf,
}

impl NexusKeyStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    pub fn data_path(&self) -> PathBuf {
        self.data_dir.join(DATA_FILE)
    }

    pub fn key_path(&self) -> PathBuf {
        self.data_dir.join(KEY_FILE)
    }

    pub fn load(&self) -> Option<String> {
        let envelope = fs::read(self.data_path()).ok()?;
        if envelope.len() < MAGIC.len() + NONCE_LEN + TAG_LEN || &envelope[..MAGIC.len()] != MAGIC {
            return None;
        }
        let key = fs::read(self.key_path()).ok()?;
        if key.len() != KEY_LEN {
            return None;
        }
        let nonce = Nonce::from_slice(&envelope[MAGIC.len()..MAGIC.len() + NONCE_LEN]);
        let tag_start = MAGIC.len() + NONCE_LEN;
        let tag = Tag::from_slice(&envelope[tag_start..tag_start + TAG_LEN]);
        let mut plaintext = envelope[tag_start + TAG_LEN..].to_vec();
        let cipher = Aes256Gcm::new_from_slice(&key).ok()?;
        cipher
            .decrypt_in_place_detached(nonce, b"", &mut plaintext, tag)
            .ok()?;
        let key = String::from_utf8(plaintext).ok()?;
        let key = key.trim();
        if key.is_empty() {
            None
        } else {
            Some(key.to_owned())
        }
    }

    pub fn save(&self, key: &str) -> Result<(), NexusKeyError> {
        let key = key.trim();
        if key.is_empty() {
            return self.clear();
        }
        create_private_dir(&self.data_dir).map_err(|source| NexusKeyError::Io {
            path: self.data_dir.clone(),
            source,
        })?;
        let mut random = OsRng;
        let encryption_key = Aes256Gcm::generate_key(&mut random);
        let cipher = Aes256Gcm::new(&encryption_key);
        let nonce = Aes256Gcm::generate_nonce(&mut random);
        let mut ciphertext = key.as_bytes().to_vec();
        let tag = cipher
            .encrypt_in_place_detached(&nonce, b"", &mut ciphertext)
            .map_err(|_| NexusKeyError::Crypto)?;

        let mut envelope = Vec::with_capacity(MAGIC.len() + NONCE_LEN + TAG_LEN + ciphertext.len());
        envelope.extend_from_slice(MAGIC);
        envelope.extend_from_slice(nonce.as_slice());
        envelope.extend_from_slice(tag.as_slice());
        envelope.extend_from_slice(&ciphertext);

        let data_path = self.data_path();
        write_private(&data_path, &envelope).map_err(|source| NexusKeyError::Io {
            path: data_path.clone(),
            source,
        })?;
        let key_path = self.key_path();
        if let Err(error) = write_private(&key_path, encryption_key.as_slice()) {
            let _ = fs::remove_file(&data_path);
            return Err(NexusKeyError::Io {
                path: key_path,
                source: error,
            });
        }
        Ok(())
    }

    pub fn clear(&self) -> Result<(), NexusKeyError> {
        for path in [self.data_path(), self.key_path()] {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(source) => return Err(NexusKeyError::Io { path, source }),
            }
        }
        Ok(())
    }

    pub fn is_configured(&self) -> bool {
        self.load().is_some()
    }

    pub fn protection_note() -> &'static str {
        "Encrypted on disk and readable only by your user account. Anything running as you can still read it."
    }
}

fn create_private_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = file.metadata()?.permissions();
        permissions.set_mode(0o600);
        file.set_permissions(permissions)?;
    }
    file.write_all(contents)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    #[test]
    fn round_trips_the_key_without_plaintext_on_disk() {
        let directory = temp_dir("nexus-key-round-trip");
        let store = NexusKeyStore::new(directory.clone());
        store.save("  abc123  ").unwrap();
        assert_eq!(store.load().as_deref(), Some("abc123"));
        assert!(store.is_configured());
        let envelope = fs::read(store.data_path()).unwrap();
        assert!(envelope.starts_with(MAGIC));
        assert_eq!(
            envelope.len(),
            MAGIC.len() + NONCE_LEN + TAG_LEN + "abc123".len()
        );
        assert!(!envelope.windows(6).any(|window| window == b"abc123"));
        assert_eq!(fs::read(store.key_path()).unwrap().len(), KEY_LEN);
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn restricts_the_directory_and_files() {
        use std::os::unix::fs::PermissionsExt;
        let directory = temp_dir("nexus-key-permissions");
        let store = NexusKeyStore::new(directory.join("data"));
        store.save("abc123").unwrap();
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&directory.join("data")), 0o700);
        assert_eq!(mode(&store.data_path()), 0o600);
        assert_eq!(mode(&store.key_path()), 0o600);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn clearing_removes_both_files_and_an_empty_key_clears() {
        let directory = temp_dir("nexus-key-clear");
        let store = NexusKeyStore::new(directory.clone());
        store.save("abc123").unwrap();
        store.clear().unwrap();
        assert!(!store.data_path().exists());
        assert!(!store.key_path().exists());
        assert_eq!(store.load(), None);
        store.save("abc123").unwrap();
        store.save("   ").unwrap();
        assert_eq!(store.load(), None);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_missing_or_corrupt_envelope_loads_as_no_key() {
        let directory = temp_dir("nexus-key-corrupt");
        let store = NexusKeyStore::new(directory.clone());
        assert_eq!(store.load(), None);
        store.save("abc123").unwrap();
        let data = store.data_path();
        let bytes = fs::read(&data).unwrap();
        fs::write(&data, &bytes[..bytes.len() - 1]).unwrap();
        assert_eq!(store.load(), None);
        fs::write(&data, b"nope").unwrap();
        assert_eq!(store.load(), None);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_wrong_key_file_loads_as_no_key() {
        let directory = temp_dir("nexus-key-wrong-key");
        let store = NexusKeyStore::new(directory.clone());
        store.save("abc123").unwrap();
        fs::write(store.key_path(), [0_u8; KEY_LEN]).unwrap();
        assert_eq!(store.load(), None);
        fs::remove_dir_all(directory).unwrap();
    }
}
