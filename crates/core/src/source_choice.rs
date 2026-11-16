//! Per-mod source pins stored in <AppData>/source-choice.json. No network.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic;
use crate::sources::{SourceKind, normalize_guid};

pub const SCHEMA_VERSION: u32 = 1;

const FILE_NAME: &str = "source-choice.json";

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChoiceDocument {
    schema_version: u32,
    choices: BTreeMap<String, SourceKind>,
}

#[derive(Debug)]
pub enum ChoiceError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    Schema {
        path: PathBuf,
        version: u32,
    },
}

impl fmt::Display for ChoiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Parse { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Schema { path, version } => write!(
                formatter,
                "{}: unsupported schemaVersion {version}, this build reads {SCHEMA_VERSION}",
                path.display()
            ),
        }
    }
}

impl Error for ChoiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Schema { .. } => None,
        }
    }
}

pub struct SourceChoices {
    path: PathBuf,
    choices: BTreeMap<String, SourceKind>,
}

impl SourceChoices {
    pub fn load(app_data: &Path) -> Result<Self, ChoiceError> {
        let path = app_data.join(FILE_NAME);
        let choices = read_choices(&path)?;
        Ok(Self { path, choices })
    }

    pub fn empty(app_data: &Path) -> Self {
        Self {
            path: app_data.join(FILE_NAME),
            choices: BTreeMap::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn choice(&self, key: &str) -> Option<SourceKind> {
        self.choices.get(key).copied()
    }

    pub fn set(&mut self, key: &str, kind: SourceKind) -> Result<(), ChoiceError> {
        self.choices.insert(key.to_owned(), kind);
        self.store()
    }

    pub fn clear(&mut self, key: &str) -> Result<(), ChoiceError> {
        self.choices.remove(key);
        self.store()
    }

    fn store(&self) -> Result<(), ChoiceError> {
        let document = ChoiceDocument {
            schema_version: SCHEMA_VERSION,
            choices: self.choices.clone(),
        };
        let text =
            serde_json::to_string_pretty(&document).map_err(|source| ChoiceError::Parse {
                path: self.path.clone(),
                source,
            })?;
        atomic::replace(&self.path, text.as_bytes()).map_err(|source| ChoiceError::Io {
            path: self.path.clone(),
            source,
        })
    }
}

pub fn mod_key(guid: Option<&str>, dll_stems: &[&str]) -> Option<String> {
    if let Some(guid) = guid {
        let guid = normalize_guid(guid);
        if !guid.is_empty() {
            return Some(format!("guid:{guid}"));
        }
    }
    let stem = dll_stems.iter().map(|stem| key_stem(stem)).min()?;
    if stem.is_empty() {
        return None;
    }
    Some(format!("dll:{stem}"))
}

fn key_stem(stem: &str) -> String {
    let stem = stem.trim();
    if stem.to_ascii_lowercase().ends_with(".dll") {
        return stem[..stem.len() - 4].to_lowercase();
    }
    stem.to_lowercase()
}

fn read_choices(path: &Path) -> Result<BTreeMap<String, SourceKind>, ChoiceError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(source) => {
            return Err(ChoiceError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let document: ChoiceDocument =
        serde_json::from_str(&text).map_err(|source| ChoiceError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    if document.schema_version != SCHEMA_VERSION {
        return Err(ChoiceError::Schema {
            path: path.to_path_buf(),
            version: document.schema_version,
        });
    }
    Ok(document.choices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    #[test]
    fn pins_round_trip() {
        let dir = temp_dir("source-choice");
        {
            let mut choices = SourceChoices::load(&dir).expect("load");
            assert_eq!(choices.choice("guid:a.b"), None);
            choices.set("guid:a.b", SourceKind::Nexus).expect("set");
            choices.set("dll:scavlib", SourceKind::Github).expect("set");
        }
        let loaded = SourceChoices::load(&dir).expect("reload");
        assert_eq!(loaded.choice("guid:a.b"), Some(SourceKind::Nexus));
        assert_eq!(loaded.choice("dll:scavlib"), Some(SourceKind::Github));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_starts_empty() {
        let dir = temp_dir("source-choice-empty");
        let choices = SourceChoices::load(&dir).expect("load");
        assert_eq!(choices.choice("guid:a.b"), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = temp_dir("source-choice-corrupt");
        fs::write(dir.join(FILE_NAME), "{not json").expect("write");
        assert!(SourceChoices::load(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsupported_schema_is_an_error() {
        let dir = temp_dir("source-choice-schema");
        fs::write(
            dir.join(FILE_NAME),
            r#"{"schemaVersion": 2, "choices": {}}"#,
        )
        .expect("write");
        let error = SourceChoices::load(&dir).err().expect("error");
        assert!(error.to_string().contains("schemaVersion 2"), "{error}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn clear_removes_the_pin() {
        let dir = temp_dir("source-choice-clear");
        let mut choices = SourceChoices::load(&dir).expect("load");
        choices.set("guid:a.b", SourceKind::Github).expect("set");
        choices.clear("guid:a.b").expect("clear");
        let loaded = SourceChoices::load(&dir).expect("reload");
        assert_eq!(loaded.choice("guid:a.b"), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mod_key_prefers_a_normalized_guid() {
        assert_eq!(
            mod_key(Some("{A.B}"), &["zed"]),
            Some("guid:a.b".to_owned())
        );
        assert_eq!(
            mod_key(None, &["Zed", "ScavLib.dll"]),
            Some("dll:scavlib".to_owned())
        );
        assert_eq!(mod_key(None, &[]), None);
    }
}
