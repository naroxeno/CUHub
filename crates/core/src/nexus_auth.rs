//! Encrypted local storage for the API credentials used by Cuprum.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{AeadCore, AeadInPlace, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce, Tag};

const NEXUS_DATA_FILE: &str = "NexusApiKey.dat";
const NEXUS_KEY_FILE: &str = "NexusApiKey.key";
const GITHUB_DATA_FILE: &str = "GithubToken.dat";
const GITHUB_KEY_FILE: &str = "GithubToken.key";
const MAGIC: &[u8; 4] = b"CHK1";
const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;
const KEY_LEN: usize = 32;
const PROTECTION_NOTE: &str =
    "Stored with a local key file. Anything running as you can still read it.";

#[derive(Debug)]
pub enum KeyStoreError {
    Io { path: PathBuf, source: io::Error },
    Crypto,
}

impl std::fmt::Display for KeyStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Crypto => formatter.write_str("could not encrypt the saved credential"),
        }
    }
}

impl std::error::Error for KeyStoreError {
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
        self.data_dir.join(NEXUS_DATA_FILE)
    }

    pub fn key_path(&self) -> PathBuf {
        self.data_dir.join(NEXUS_KEY_FILE)
    }

    pub fn load(&self) -> Option<String> {
        load_secret(&self.data_path(), &self.key_path())
    }

    pub fn save(&self, key: &str) -> Result<(), KeyStoreError> {
        save_secret(&self.data_dir, &self.data_path(), &self.key_path(), key)
    }

    pub fn clear(&self) -> Result<(), KeyStoreError> {
        clear_secret([self.data_path(), self.key_path()])
    }

    pub fn is_configured(&self) -> bool {
        self.load().is_some()
    }

    pub fn protection_note() -> &'static str {
        PROTECTION_NOTE
    }
}

pub struct GithubTokenStore {
    data_dir: PathBuf,
}

impl GithubTokenStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    pub fn data_path(&self) -> PathBuf {
        self.data_dir.join(GITHUB_DATA_FILE)
    }

    pub fn key_path(&self) -> PathBuf {
        self.data_dir.join(GITHUB_KEY_FILE)
    }

    pub fn load(&self) -> Option<String> {
        load_secret(&self.data_path(), &self.key_path())
    }

    pub fn save(&self, token: &str) -> Result<(), KeyStoreError> {
        save_secret(&self.data_dir, &self.data_path(), &self.key_path(), token)
    }

    pub fn clear(&self) -> Result<(), KeyStoreError> {
        clear_secret([self.data_path(), self.key_path()])
    }

    pub fn is_configured(&self) -> bool {
        self.load().is_some()
    }

    pub fn protection_note() -> &'static str {
        PROTECTION_NOTE
    }
}

fn load_secret(data_path: &Path, key_path: &Path) -> Option<String> {
    let envelope = fs::read(data_path).ok()?;
    if envelope.len() < MAGIC.len() + NONCE_LEN + TAG_LEN || &envelope[..MAGIC.len()] != MAGIC {
        return None;
    }
    let key = fs::read(key_path).ok()?;
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
    let secret = String::from_utf8(plaintext).ok()?;
    let secret = secret.trim();
    if secret.is_empty() {
        None
    } else {
        Some(secret.to_owned())
    }
}

fn save_secret(
    data_dir: &Path,
    data_path: &Path,
    key_path: &Path,
    secret: &str,
) -> Result<(), KeyStoreError> {
    let secret = secret.trim();
    if secret.is_empty() {
        return clear_secret([data_path.to_path_buf(), key_path.to_path_buf()]);
    }
    create_private_dir(data_dir).map_err(|source| KeyStoreError::Io {
        path: data_dir.to_path_buf(),
        source,
    })?;
    let mut random = OsRng;
    let encryption_key = Aes256Gcm::generate_key(&mut random);
    let cipher = Aes256Gcm::new(&encryption_key);
    let nonce = Aes256Gcm::generate_nonce(&mut random);
    let mut ciphertext = secret.as_bytes().to_vec();
    let tag = cipher
        .encrypt_in_place_detached(&nonce, b"", &mut ciphertext)
        .map_err(|_| KeyStoreError::Crypto)?;

    let mut envelope = Vec::with_capacity(MAGIC.len() + NONCE_LEN + TAG_LEN + ciphertext.len());
    envelope.extend_from_slice(MAGIC);
    envelope.extend_from_slice(nonce.as_slice());
    envelope.extend_from_slice(tag.as_slice());
    envelope.extend_from_slice(&ciphertext);

    write_private(data_path, &envelope).map_err(|source| KeyStoreError::Io {
        path: data_path.to_path_buf(),
        source,
    })?;
    if let Err(error) = write_private(key_path, encryption_key.as_slice()) {
        let _ = fs::remove_file(data_path);
        return Err(KeyStoreError::Io {
            path: key_path.to_path_buf(),
            source: error,
        });
    }
    Ok(())
}

fn clear_secret([data_path, key_path]: [PathBuf; 2]) -> Result<(), KeyStoreError> {
    for path in [data_path, key_path] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(KeyStoreError::Io { path, source }),
        }
    }
    Ok(())
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
    crate::atomic::replace_private(path, contents)
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

    #[test]
    fn the_github_token_store_uses_its_own_files() {
        let directory = temp_dir("github-token-round-trip");
        let nexus = NexusKeyStore::new(directory.clone());
        let github = GithubTokenStore::new(directory.clone());
        github.save("ghp_example").unwrap();
        assert_eq!(github.load().as_deref(), Some("ghp_example"));
        assert_eq!(github.data_path().file_name().unwrap(), GITHUB_DATA_FILE);
        assert_eq!(github.key_path().file_name().unwrap(), GITHUB_KEY_FILE);
        assert_eq!(nexus.load(), None);
        assert!(!nexus.data_path().exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn two_stores_in_one_directory_keep_separate_secrets() {
        let directory = temp_dir("github-token-separate");
        let nexus = NexusKeyStore::new(directory.clone());
        let github = GithubTokenStore::new(directory.clone());
        nexus.save("nexus-key").unwrap();
        github.save("github-token").unwrap();
        assert_eq!(nexus.load().as_deref(), Some("nexus-key"));
        assert_eq!(github.load().as_deref(), Some("github-token"));
        github.clear().unwrap();
        assert_eq!(github.load(), None);
        assert_eq!(nexus.load().as_deref(), Some("nexus-key"));
        fs::remove_dir_all(directory).unwrap();
    }
}
