//! Row models and the pure functions that build them from hub data.

use super::widgets::NEXUS_PAGE_SIZE;
use crate::i18n;
use cuhub_core::github::{CheckFailure, ReleaseInfo};
use cuhub_core::hub::Hub;
use cuhub_core::metadata::MetadataMod;
use cuhub_core::mods::{InstalledMod, ModStatus};
use cuhub_core::source_choice::mod_key;
use cuhub_core::sources::{self, LocalMod, MatchKind, Repo, SourceEntry, SourceKind};
use cuhub_core::version::HubVersion;
use nsh_core::prelude::*;
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Eq)]
pub struct NexusModRow {
    pub id: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub downloads: String,
    pub adult: bool,
    pub has_page: bool,
    pub can_download: bool,
    pub status: Option<ModStatus>,
    pub installed_version: Option<String>,
}

impl NexusModRow {
    pub fn row_key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.id,
            self.name,
            self.author,
            self.version,
            self.downloads,
            self.state_key(),
            self.installed_version.as_deref().unwrap_or(""),
            self.adult,
            self.has_page
        )
    }

    pub fn state_key(&self) -> &'static str {
        self.status
            .map(i18n::status_key)
            .unwrap_or("status.not_installed")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SourceRow {
    pub text: String,
    pub warning: bool,
    pub can_check: bool,
    pub repo: Option<Repo>,
    pub options: Vec<SourceKind>,
    pub pinned: Option<SourceKind>,
    pub effective: Option<SourceKind>,
    pub release_version: Option<HubVersion>,
    pub asset: Option<String>,
}

impl SourceRow {
    pub fn can_pin(&self) -> bool {
        self.options.len() > 1 || self.pinned.is_some()
    }

    pub fn pin_key(&self) -> &'static str {
        match self.pinned {
            None => "local.pin_auto",
            Some(SourceKind::Github) => "local.pin_github",
            Some(SourceKind::Nexus) => "local.pin_nexus",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct LocalModRow {
    pub key: String,
    pub source_key: String,
    pub item: InstalledMod,
    pub source: SourceRow,
    pub managed: bool,
}

impl LocalModRow {
    pub fn row_key(&self) -> String {
        self.key.clone()
    }

    pub fn can_update(&self) -> bool {
        if self.item.disabled {
            return false;
        }
        let has_source = self.item.metadata_id.is_some() || self.source.repo.is_some();
        if !has_source {
            return false;
        }
        if self.item.status == ModStatus::OutOfDate {
            return true;
        }
        if self.source.effective != Some(SourceKind::Github) {
            return false;
        }
        let Some(release) = self.source.release_version.as_ref() else {
            return false;
        };
        self.item
            .installed_version
            .as_deref()
            .and_then(HubVersion::parse)
            .is_some_and(|installed| installed < *release)
    }
}

#[derive(Clone, PartialEq)]
pub enum CheckState {
    Checking,
    Release(ReleaseInfo),
    Failed(CheckFailure),
}

struct SourceInput<'a> {
    entries: &'a [SourceEntry],
    item: &'a InstalledMod,
    check: Option<&'a CheckState>,
    pinned: Option<SourceKind>,
}

pub(super) fn build_local_rows(
    hub: &Hub,
    mods: &[InstalledMod],
    checks: &BTreeMap<String, CheckState>,
) -> Vec<LocalModRow> {
    let entries = hub
        .sources()
        .map(|manifest| manifest.entries.as_slice())
        .unwrap_or(&[]);
    mods.iter()
        .map(|item| {
            let key = source_key(item);
            let check = checks.get(&key);
            let pinned = hub.source_choice(&key);
            let source = build_source_row(SourceInput {
                entries,
                item,
                check,
                pinned,
            });
            let managed = hub.links().get(&item.source_path).is_some();
            LocalModRow {
                key: format!(
                    "{key}|{}|{}|{}|{managed}",
                    check_tag(check),
                    pin_tag(pinned),
                    item.disabled
                ),
                source_key: key,
                item: item.clone(),
                source,
                managed,
            }
        })
        .collect()
}

fn build_source_row(input: SourceInput<'_>) -> SourceRow {
    let stems = dll_stems(input.item);
    let local = LocalMod {
        guid: input.item.guid.as_deref(),
        dll_stems: stems,
        name: &input.item.name,
    };
    let github = match input.check {
        Some(CheckState::Release(release)) => Some(release.version.clone()),
        _ => None,
    };
    let nexus = input
        .item
        .expected_version
        .as_deref()
        .and_then(HubVersion::parse);
    let resolution = sources::resolve_mod(input.entries, &local, github, nexus, input.pinned);
    let Some(matched) = resolution.matched else {
        let text = match input.pinned {
            Some(kind) => t!("source.not_listed_pinned", kind = source_kind_label(kind)),
            None => t!("source.not_listed"),
        };
        return SourceRow {
            text,
            warning: false,
            can_check: false,
            repo: None,
            options: Vec::new(),
            pinned: input.pinned,
            effective: None,
            release_version: None,
            asset: None,
        };
    };
    let repo = &matched.entry.repo;
    let mut parts = vec![
        t!("source.github", repo = repo.slug()),
        t!(match_key(matched.kind)),
    ];
    match input.check {
        None => parts.push(t!("source.not_checked")),
        Some(CheckState::Checking) => parts.push(t!("source.checking")),
        Some(CheckState::Release(release)) => {
            parts.push(release.tag.clone());
            if release.prerelease {
                parts.push(t!("source.prerelease_only"));
            }
        }
        Some(CheckState::Failed(failure)) => parts.push(failure.summary()),
    }
    if let Some(effective) = &resolution.effective {
        parts.push(t!(
            "source.uses",
            source = source_label(effective.kind, input.check, input.item)
        ));
    }
    if resolution.pinned.is_some() {
        parts.push(t!("source.pinned"));
    }
    let warning = matched.kind == MatchKind::Name
        || matches!(input.check, Some(CheckState::Release(release)) if release.prerelease);
    let options: Vec<SourceKind> = resolution
        .options
        .iter()
        .map(|option| option.kind)
        .collect();
    SourceRow {
        text: parts.join(" · "),
        warning,
        can_check: !matches!(input.check, Some(CheckState::Checking)),
        repo: Some(repo.clone()),
        options,
        pinned: resolution.pinned,
        effective: resolution.effective.as_ref().map(|option| option.kind),
        release_version: match input.check {
            Some(CheckState::Release(release)) => Some(release.version.clone()),
            _ => None,
        },
        asset: matched.entry.asset.clone(),
    }
}

pub(super) fn source_key(item: &InstalledMod) -> String {
    let stems = dll_stems(item);
    mod_key(item.guid.as_deref(), &stems)
        .unwrap_or_else(|| format!("name:{}", item.name.to_lowercase()))
}

pub(super) fn dll_stems(item: &InstalledMod) -> Vec<&str> {
    item.dll_paths
        .iter()
        .filter_map(|path| path.file_stem().and_then(|stem| stem.to_str()))
        .collect()
}

fn check_tag(check: Option<&CheckState>) -> String {
    match check {
        None => String::from("none"),
        Some(CheckState::Checking) => String::from("checking"),
        Some(CheckState::Release(release)) => format!("release:{}", release.tag),
        Some(CheckState::Failed(failure)) => format!("failed:{}", failure.summary()),
    }
}

fn pin_tag(pinned: Option<SourceKind>) -> &'static str {
    match pinned {
        None => "auto",
        Some(SourceKind::Github) => "github",
        Some(SourceKind::Nexus) => "nexus",
    }
}

pub(super) fn source_kind_label(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Github => "GitHub",
        SourceKind::Nexus => "Nexus",
    }
}

fn source_label(kind: SourceKind, check: Option<&CheckState>, item: &InstalledMod) -> String {
    match kind {
        SourceKind::Github => match check {
            Some(CheckState::Release(release)) => t!("source.github", repo = release.tag),
            _ => t!("source.github_not_checked"),
        },
        SourceKind::Nexus => item
            .expected_version
            .clone()
            .map(|version| format!("Nexus {version}"))
            .unwrap_or_else(|| String::from("Nexus")),
    }
}

fn match_key(kind: MatchKind) -> &'static str {
    match kind {
        MatchKind::Guid => "source.match_guid",
        MatchKind::Dll => "source.match_dll",
        MatchKind::Name => "source.match_name",
    }
}

pub(super) fn release_notice(repo: &Repo, release: &ReleaseInfo) -> String {
    if release.prerelease {
        t!(
            "notice.release_prerelease",
            repo = repo.slug(),
            tag = release.tag
        )
    } else {
        t!(
            "notice.release_found",
            repo = repo.slug(),
            tag = release.tag
        )
    }
}

pub(super) fn failure_notice(failure: &CheckFailure) -> String {
    match failure.detail() {
        Some(detail) => format!("{} {detail}", failure.summary()),
        None => failure.summary(),
    }
}

pub(super) fn build_nexus_rows(
    metadata: &[MetadataMod],
    mods: &[InstalledMod],
) -> Vec<NexusModRow> {
    metadata
        .iter()
        .map(|entry| {
            let installed = mods
                .iter()
                .find(|installed| installed.metadata_id.as_deref() == Some(entry.id.as_str()));
            NexusModRow {
                id: entry.id.clone(),
                name: if entry.name.trim().is_empty() {
                    t!("nexus.unknown_mod")
                } else {
                    entry.name.clone()
                },
                author: if entry.author.trim().is_empty() {
                    t!("nexus.unknown_author")
                } else {
                    entry.author.clone()
                },
                version: if entry.version.trim().is_empty() {
                    t!("nexus.unknown_version")
                } else {
                    entry.version.clone()
                },
                downloads: entry.total_downloads_label(),
                adult: entry.is_adult_content(),
                has_page: entry.nexus_download_page_url().is_some(),
                can_download: !entry.id.trim().is_empty(),
                status: installed.map(|installed| installed.status),
                installed_version: installed
                    .and_then(|installed| installed.installed_version.clone()),
            }
        })
        .collect()
}

pub(super) fn page_slice<T: Clone>(rows: &[T], page: usize) -> Vec<T> {
    let start = page * NEXUS_PAGE_SIZE;
    if start >= rows.len() {
        return Vec::new();
    }
    let end = (start + NEXUS_PAGE_SIZE).min(rows.len());
    rows[start..end].to_vec()
}

pub fn nexus_page_count(total: usize) -> usize {
    total.div_ceil(NEXUS_PAGE_SIZE).max(1)
}
