//! Matches cached GitHub sources and identifies the plugin whose version is being replaced.

use std::path::Path;

use crate::hub::HubError;
use crate::install::{self, PackageFile};
use crate::mods::{self, InstalledMod};
use crate::sources::{LocalMod, MatchKind, SourceEntry, normalize_guid};
use crate::version::HubVersion;

pub(super) fn github_matches(
    item: &InstalledMod,
    entries: &[SourceEntry],
) -> Vec<(MatchKind, SourceEntry)> {
    let local = LocalMod {
        guid: item.guid.as_deref(),
        dll_stems: item
            .dll_paths
            .iter()
            .filter_map(|dll| dll.file_name()?.to_str())
            .collect(),
        name: &item.name,
    };
    let mut matches: Vec<_> = entries
        .iter()
        .filter_map(|entry| Some((entry.matcher.matches(&local)?, entry)))
        .collect();
    if let Some(best) = matches.iter().map(|(kind, _)| *kind).min() {
        matches.retain(|(kind, _)| *kind == best);
    }
    let mut candidates = Vec::new();
    for (kind, entry) in matches {
        let matched = (kind, entry.clone());
        if !candidates.contains(&matched) {
            candidates.push(matched);
        }
    }
    candidates
}

fn expected_guid<'a>(entry: &'a SourceEntry, item: &'a InstalledMod) -> Option<&'a str> {
    item.guid.as_deref().or(entry.matcher.guid.as_deref())
}

fn matches_guid(path: &Path, expected: &str) -> bool {
    mods::read_identity(path)
        .and_then(|(_, guid)| guid)
        .is_some_and(|guid| normalize_guid(&guid) == normalize_guid(expected))
}

pub(super) fn local_version(
    entry: &SourceEntry,
    item: &InstalledMod,
) -> Result<HubVersion, HubError> {
    if let (Some(local), Some(remote)) = (&item.guid, &entry.matcher.guid)
        && normalize_guid(local) != normalize_guid(remote)
    {
        return Err(HubError::Plan(format!(
            "installed plugin GUID {local} does not match the selected source GUID {remote}"
        )));
    }
    let dlls = item
        .dll_paths
        .iter()
        .filter(|dll| install::is_dll_file(dll));
    let dll = unique_dll(
        dlls.filter(|dll| expected_guid(entry, item).is_none_or(|guid| matches_guid(dll, guid))),
    )?;
    read_version(dll)
}

pub(in crate::hub) fn replacement_version(
    entry: &SourceEntry,
    item: &InstalledMod,
    files: &[PackageFile],
) -> Result<HubVersion, HubError> {
    let local = local_version(entry, item)?;
    let dlls = files
        .iter()
        .filter(|file| install::is_dll_file(&file.relative));
    let dll = unique_dll(dlls.filter(|file| match expected_guid(entry, item) {
        Some(guid) => matches_guid(&file.staged, guid),
        None => item.dll_paths.iter().any(|dll| {
            install::is_dll_file(dll)
                && dll
                    .file_name()
                    .zip(file.relative.file_name())
                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
                && mods::read_identity(&file.staged).is_some()
        }),
    }))?;
    let incoming = read_version(&dll.staged)?;
    if incoming < local {
        return Err(HubError::Update(format!(
            "the download contains version {} but {} is installed; refusing to downgrade",
            incoming.original(),
            local.original(),
        )));
    }
    Ok(incoming)
}

fn unique_dll<T>(mut matches: impl Iterator<Item = T>) -> Result<T, HubError> {
    let missing = || {
        HubError::Plan(String::from(
            "could not identify exactly one matching plugin DLL",
        ))
    };
    let dll = matches.next().ok_or_else(missing)?;
    if matches.next().is_some() {
        return Err(missing());
    }
    Ok(dll)
}

fn read_version(dll: &Path) -> Result<HubVersion, HubError> {
    mods::read_mod_version(dll)
        .and_then(|version| HubVersion::parse_external(&version))
        .ok_or_else(|| {
            HubError::Plan(format!(
                "could not read a comparable version from {}",
                dll.display()
            ))
        })
}
