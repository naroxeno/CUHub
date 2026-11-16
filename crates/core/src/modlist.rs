//! Serverless modlist share codes and the pending imported list. No network, no downloads.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const PREFIX: &str = "CUH1:";
const PENDING_FILE: &str = "ImportedModlist.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ModlistEntry {
    pub id: String,
    pub name: String,
    pub guid: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ModlistShare {
    pub format_version: u32,
    pub mods: Vec<ModlistEntry>,
}

impl Default for ModlistShare {
    fn default() -> Self {
        Self {
            format_version: 1,
            mods: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum ModlistError {
    NotCode,
    NoGuids,
    EmptyCode,
    Io { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for ModlistError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotCode => write!(
                formatter,
                "That is not a Casualties Hub modlist code. It must begin with CUH1:."
            ),
            Self::NoGuids => write!(
                formatter,
                "No BepInEx mod GUIDs were found in the installed recognized mods."
            ),
            Self::EmptyCode => write!(formatter, "The modlist code does not contain any mods."),
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for ModlistError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub fn create_share_code(mods: &[ModlistEntry]) -> Result<String, ModlistError> {
    let mut unique: BTreeMap<String, &ModlistEntry> = BTreeMap::new();
    for entry in mods {
        if entry.guid.is_empty() {
            continue;
        }
        unique
            .entry(entry.guid.to_ascii_lowercase())
            .or_insert(entry);
    }
    if unique.is_empty() {
        return Err(ModlistError::NoGuids);
    }
    let mut entries: Vec<&ModlistEntry> = unique.into_values().collect();
    entries.sort_by_key(|entry| entry.name.to_ascii_lowercase());
    let encoded = entries
        .into_iter()
        .map(|entry| {
            format!(
                "{}~{}@{}",
                escape(&entry.guid),
                escape(&entry.id),
                escape(entry.version.as_deref().unwrap_or(""))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!("{PREFIX}{encoded}"))
}

pub struct ModlistStore {
    pending_path: PathBuf,
}

impl ModlistStore {
    pub fn new(app_data: &Path) -> Self {
        Self {
            pending_path: app_data.join(PENDING_FILE),
        }
    }

    pub fn import(&self, code: &str) -> Result<Vec<ModlistEntry>, ModlistError> {
        let share = parse_share_code(code)?;
        let text = serde_json::to_string_pretty(&share).map_err(|source| ModlistError::Io {
            path: self.pending_path.clone(),
            source: io::Error::other(source),
        })?;
        crate::atomic::replace(&self.pending_path, text.as_bytes()).map_err(|source| {
            ModlistError::Io {
                path: self.pending_path.clone(),
                source,
            }
        })?;
        Ok(share.mods)
    }

    pub fn load_imported(&self) -> Vec<ModlistEntry> {
        let Ok(text) = fs::read_to_string(&self.pending_path) else {
            return Vec::new();
        };
        serde_json::from_str::<ModlistShare>(&text)
            .map(|share| share.mods)
            .unwrap_or_default()
    }

    pub fn ignore(&self, entry: &ModlistEntry) -> Result<(), ModlistError> {
        let current: Vec<ModlistEntry> = self
            .load_imported()
            .into_iter()
            .filter(|item| !matches(item, entry))
            .collect();
        let share = ModlistShare {
            format_version: 1,
            mods: current,
        };
        let text = serde_json::to_string_pretty(&share).map_err(|source| ModlistError::Io {
            path: self.pending_path.clone(),
            source: io::Error::other(source),
        })?;
        crate::atomic::replace(&self.pending_path, text.as_bytes()).map_err(|source| {
            ModlistError::Io {
                path: self.pending_path.clone(),
                source,
            }
        })
    }
}

fn parse_share_code(code: &str) -> Result<ModlistShare, ModlistError> {
    let trimmed = code.trim();
    if !trimmed
        .get(..PREFIX.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(PREFIX))
    {
        return Err(ModlistError::NotCode);
    }
    let payload = trimmed[PREFIX.len()..].trim();
    let mods: Vec<ModlistEntry> = payload
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(parse_entry)
        .filter(|entry| !entry.guid.is_empty())
        .collect();
    if mods.is_empty() {
        return Err(ModlistError::EmptyCode);
    }
    Ok(ModlistShare {
        format_version: 1,
        mods,
    })
}

fn parse_entry(value: &str) -> ModlistEntry {
    if !value.contains('|') {
        let compact = value.split_once('@');
        let (guid, version) = match compact {
            Some((guid, version)) => (guid, version),
            None => (value, ""),
        };
        let (guid, id) = match guid.split_once('~') {
            Some((guid, id)) => (unescape(guid), unescape(id)),
            None => (unescape(guid), String::new()),
        };
        return ModlistEntry {
            name: guid.clone(),
            guid,
            id,
            version: Some(unescape(version)),
        };
    }
    let mut parts = value.splitn(4, '|');
    let guid = unescape(parts.next().unwrap_or(""));
    let id = unescape(parts.next().unwrap_or(""));
    let version = unescape(parts.next().unwrap_or(""));
    let name = parts
        .next()
        .map(unescape)
        .unwrap_or_else(|| "Unknown mod".to_string());
    ModlistEntry {
        name,
        guid,
        id,
        version: Some(version),
    }
}

fn matches(item: &ModlistEntry, entry: &ModlistEntry) -> bool {
    let same = |left: &str, right: &str| left.eq_ignore_ascii_case(right);
    (!entry.guid.is_empty() && same(&item.guid, &entry.guid))
        || (!entry.id.is_empty() && same(&item.id, &entry.id))
        || same(&item.name, &entry.name)
}

fn escape(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn unescape(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            decoded.push(high << 4 | low);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    fn entry(name: &str, guid: &str, id: &str, version: &str) -> ModlistEntry {
        ModlistEntry {
            name: name.to_string(),
            guid: guid.to_string(),
            id: id.to_string(),
            version: Some(version.to_string()),
        }
    }

    #[test]
    fn creates_codes_deduplicated_and_sorted_by_name() {
        let mods = vec![
            entry("Zeta", "zeta.guid", "900", "1.0"),
            entry("alpha", "alpha.guid", "", "2.0"),
            entry("Alpha duplicate", "ALPHA.GUID", "901", "3.0"),
            entry("No guid", "", "902", "1.0"),
        ];
        let code = create_share_code(&mods).unwrap();
        assert_eq!(code, "CUH1:alpha.guid~@2.0,zeta.guid~900@1.0");
    }

    #[test]
    fn rejects_mods_without_guids() {
        let error = create_share_code(&[entry("No guid", "", "1", "1.0")]).unwrap_err();
        assert!(matches!(error, ModlistError::NoGuids));
    }

    #[test]
    fn round_trips_through_the_pending_file() {
        let dir = temp_dir("modlist");
        let store = ModlistStore::new(&dir);
        let mods = vec![entry("Author", "author.guid", "12345", "1.2 beta")];
        let code = create_share_code(&mods).unwrap();
        assert!(code.contains("1.2%20beta"));
        let imported = store.import(&code).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].guid, "author.guid");
        assert_eq!(imported[0].id, "12345");
        assert_eq!(imported[0].version.as_deref(), Some("1.2 beta"));
        assert_eq!(imported[0].name, "author.guid");
        let reloaded = store.load_imported();
        assert_eq!(reloaded, imported);
        let text = fs::read_to_string(dir.join(PENDING_FILE)).unwrap();
        assert!(text.contains("\"FormatVersion\""));
        assert!(text.contains("\"Mods\""));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn imports_legacy_codes() {
        let mods = parse_share_code("CUH1:guid.one@1.0,guid.two").unwrap().mods;
        assert_eq!(mods[0].guid, "guid.one");
        assert_eq!(mods[0].version.as_deref(), Some("1.0"));
        assert_eq!(mods[1].name, "guid.two");
        assert_eq!(mods[1].version.as_deref(), Some(""));
    }

    #[test]
    fn imports_four_part_entries() {
        let mods = parse_share_code("CUH1:a|b|c|Some%20Mod").unwrap().mods;
        assert_eq!(mods[0].guid, "a");
        assert_eq!(mods[0].id, "b");
        assert_eq!(mods[0].version.as_deref(), Some("c"));
        assert_eq!(mods[0].name, "Some Mod");
    }

    #[test]
    fn rejects_foreign_or_empty_codes() {
        assert!(matches!(
            parse_share_code("hello"),
            Err(ModlistError::NotCode)
        ));
        assert!(matches!(
            parse_share_code("CUH1:"),
            Err(ModlistError::EmptyCode)
        ));
        assert!(matches!(
            parse_share_code("cuh1:guid@1"),
            Ok(share) if share.mods.len() == 1
        ));
    }

    #[test]
    fn ignore_removes_matching_entries_by_any_identity() {
        let dir = temp_dir("modlist-ignore");
        let store = ModlistStore::new(&dir);
        store
            .import("CUH1:one.guid~11@1.0,two.guid~22@2.0")
            .unwrap();
        store.ignore(&entry("Renamed", "ONE.guid", "", "")).unwrap();
        let remaining = store.load_imported();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].guid, "two.guid");
        let _ = fs::remove_dir_all(dir);
    }
}
