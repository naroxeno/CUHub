//! Previews and deploys loose mods into a single migration profile.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::package_plan::PackageRequest;
use super::{Hub, HubError};
use crate::adopt::{self, AdoptItem, AdoptSkip};
use crate::install::{PackageFile, StagedArchive};
use crate::mods::{self, InstalledMod};
use crate::packages::{self, PackageContext, PackageError, PackagePlan, PackageReport};
use crate::profiles::{self, Profile, ProfileError};
use crate::sources::{MatchKind, SourceEntry};
use crate::{paths, store};

mod identity;
pub(super) use identity::replacement_version;
use identity::{github_matches, local_version};

const LOCAL_SOURCE: &str = "adopt";

#[derive(Clone, PartialEq, Eq)]
pub struct MigrationSession {
    name: String,
    active: Option<Profile>,
    pub backup_root: PathBuf,
}

#[derive(Clone, PartialEq, Eq)]
pub struct MigrationRow {
    pub name: String,
    pub version: Option<String>,
    pub path: PathBuf,
    pub sources: Vec<(MatchKind, SourceEntry)>,
    pub local: Result<AdoptItem, AdoptSkip>,
    files: Vec<LocalFile>,
}

#[derive(Clone, PartialEq, Eq)]
struct LocalFile {
    package: PackageFile,
    modified: Option<SystemTime>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct MigrationPreview {
    row: MigrationRow,
    entry: SourceEntry,
    staged: StagedArchive,
    package: PackagePlan,
    pub local_version: String,
    pub backup_root: PathBuf,
}

impl MigrationPreview {
    pub fn row(&self) -> &MigrationRow {
        &self.row
    }

    pub fn package(&self) -> &PackagePlan {
        &self.package
    }
}

impl Hub {
    pub fn scan_migration(&self) -> Result<(MigrationSession, Vec<MigrationRow>), HubError> {
        let (active, rows) = self.migration_rows()?;
        let session = MigrationSession {
            name: self.adopt_profile_name(),
            active,
            backup_root: self.backup_root(),
        };
        Ok((session, rows.into_iter().map(|(row, _)| row).collect()))
    }

    fn migration_rows(
        &self,
    ) -> Result<(Option<Profile>, Vec<(MigrationRow, InstalledMod)>), HubError> {
        self.check_link_index()?;
        let plugins = self.plugins_dir()?;
        let mut installed = mods::scan(&plugins, &self.metadata, &self.catalogs, &[])?;
        self.apply_profile_identity(&plugins, &mut installed);
        installed.extend(self.disabled_profile_mods(&plugins, &installed));
        let active = self.active_profile().map(|(_, profile)| profile);
        let catalog = self
            .sources
            .as_ref()
            .map_or(&[][..], |sources| &sources.entries);
        let rows = adopt::classify_mods(&installed, &plugins, &self.store, &self.links)
            .map(|(item, candidate)| {
                let mut files = Vec::new();
                let local = candidate.and_then(|candidate| {
                    if active
                        .as_ref()
                        .is_some_and(|p| managed_in(p, &candidate, &self.store, &plugins))
                    {
                        return Err(AdoptSkip::AlreadyManaged);
                    }
                    if !paths::same_target(&candidate.target, &candidate.source)
                        && candidate.target.symlink_metadata().is_ok()
                    {
                        return Err(AdoptSkip::DuplicateTarget);
                    }
                    files = local_files(&candidate).map_err(|_| AdoptSkip::UnsafeFiles)?;
                    Ok(candidate)
                });
                let row = MigrationRow {
                    name: item.name.clone(),
                    version: item
                        .installed_version
                        .clone()
                        .or_else(|| item.assembly_version.clone()),
                    path: item.source_path.clone(),
                    sources: github_matches(item, catalog),
                    local,
                    files,
                };
                (row, item.clone())
            })
            .collect();
        Ok((active, rows))
    }

    pub fn prepare_migration_reinstall(
        &self,
        session: &MigrationSession,
        row: &MigrationRow,
        entry: &SourceEntry,
    ) -> Result<MigrationPreview, HubError> {
        let (_, rows) = self.checked_migration_rows(session)?;
        let installed = checked_github_row(row, entry, &rows)?;
        local_version(entry, installed)?;
        let download = self.download_github_asset(
            &entry.repo,
            entry.asset.as_deref(),
            &self.download_cache(),
        )?;
        let staged = self.stage_download(&download, &self.staging_root())?;
        let (package, local_version) = match self.migration_package(row, entry, installed, &staged)
        {
            Ok(result) => result,
            Err(error) => {
                self.discard_staging(&staged.staging_root);
                return Err(error);
            }
        };
        Ok(MigrationPreview {
            row: row.clone(),
            entry: entry.clone(),
            staged,
            package,
            local_version,
            backup_root: session.backup_root.clone(),
        })
    }

    pub fn discard_migration_preview(&self, preview: &MigrationPreview) {
        self.discard_staging(&preview.staged.staging_root);
    }

    pub fn apply_migration_reinstall(
        &mut self,
        session: &mut MigrationSession,
        preview: &MigrationPreview,
    ) -> Result<PackageReport, HubError> {
        let result = (|| {
            let (profile, rows) = self.checked_migration_rows(session)?;
            let installed = checked_github_row(&preview.row, &preview.entry, &rows)?;
            let (package, local_version) =
                self.migration_package(&preview.row, &preview.entry, installed, &preview.staged)?;
            if (package, local_version) != (preview.package.clone(), preview.local_version.clone())
                || session.backup_root != preview.backup_root
            {
                return Err(ProfileError::PlanChanged.into());
            }
            self.deploy_migration(session, &preview.package, profile)
        })();
        self.discard_migration_preview(preview);
        result
    }

    pub fn apply_migration_local(
        &mut self,
        session: &mut MigrationSession,
        confirmed: &[MigrationRow],
    ) -> Result<Vec<Result<PackageReport, HubError>>, HubError> {
        let (_, rows) = self.checked_migration_rows(session)?;
        for row in confirmed {
            checked_row(row, &rows)?;
        }
        Ok(confirmed
            .iter()
            .map(|row| self.deploy_local(session, row))
            .collect())
    }

    fn checked_migration_rows(
        &self,
        session: &MigrationSession,
    ) -> Result<(Profile, Vec<(MigrationRow, InstalledMod)>), HubError> {
        self.ensure_game_closed()?;
        self.check_migration_storage(&session.backup_root)?;
        let (active, rows) = self.migration_rows()?;
        let profile = self.migration_profile(session, active)?;
        Ok((profile, rows))
    }

    fn check_migration_storage(&self, backup_root: &Path) -> Result<(), HubError> {
        let plugins = self.plugins_dir()?;
        let bepinex = plugins.parent().ok_or(ProfileError::PlanChanged)?;
        let config = paths::resolve_child(bepinex, "config");
        for root in [backup_root, self.store.root()] {
            let resolved = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
            if paths::is_inside(&resolved, &plugins) || paths::is_inside(&resolved, &config) {
                return Err(HubError::Plan(format!(
                    "move the content library or backup folder outside Plugins and config: {}",
                    root.display()
                )));
            }
        }
        Ok(())
    }

    fn migration_profile(
        &self,
        session: &MigrationSession,
        active: Option<Profile>,
    ) -> Result<Profile, HubError> {
        if active != session.active || self.backup_root() != session.backup_root {
            return Err(ProfileError::PlanChanged.into());
        }
        if let Some(profile) = &active
            && profile.name == session.name
        {
            return Ok(profile.clone());
        }
        if self
            .profiles
            .list()
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&session.name))
        {
            return Err(ProfileError::PlanChanged.into());
        }
        Ok(profiles::checkpoint(
            session.name.clone(),
            active.as_ref(),
            &self.links,
            Vec::new(),
        ))
    }

    fn deploy_local(
        &mut self,
        session: &mut MigrationSession,
        row: &MigrationRow,
    ) -> Result<PackageReport, HubError> {
        let profile =
            self.migration_profile(session, self.active_profile().map(|(_, profile)| profile))?;
        let item = row.local.as_ref().map_err(|_| ProfileError::PlanChanged)?;
        let files = local_files(item)?;
        if files != row.files {
            return Err(ProfileError::PlanChanged.into());
        }
        let plan = PackagePlan {
            mod_id: item.mod_id.clone(),
            version: item.version.clone(),
            source: LOCAL_SOURCE.into(),
            dependencies: Vec::new(),
            target: item.target.clone(),
            previous: Some(item.source.clone()),
            managed_previous: None,
            staging: self.staging_root(),
            files: files.into_iter().map(|file| file.package).collect(),
            single: item.file.is_some(),
            warnings: Vec::new(),
        };
        let result = self.deploy_migration(session, &plan, profile);
        self.discard_staging(&plan.staging);
        result
    }

    fn deploy_migration(
        &mut self,
        session: &mut MigrationSession,
        plan: &PackagePlan,
        profile: Profile,
    ) -> Result<PackageReport, HubError> {
        self.ensure_game_closed()?;
        let game_root = self.game_folder().ok_or(HubError::NoGameFolder)?;
        let plugins_root = self.plugins_dir()?;
        let report = packages::apply_to_profile(
            plan,
            PackageContext {
                store: &self.store,
                links: &mut self.links,
                profiles: &self.profiles,
                game_root: &game_root,
                plugins_root: &plugins_root,
                backup_root: &session.backup_root,
            },
            profile,
            (plan.source != LOCAL_SOURCE).then_some(plan.source.as_str()),
        )?;
        session.active = self.active_profile().map(|(_, profile)| profile);
        Ok(report)
    }

    fn migration_package(
        &self,
        row: &MigrationRow,
        entry: &SourceEntry,
        installed: &InstalledMod,
        staged: &StagedArchive,
    ) -> Result<(PackagePlan, String), HubError> {
        let item = row.local.as_ref().map_err(|_| ProfileError::PlanChanged)?;
        let source = format!("github:{}", entry.repo.slug());
        let mut requirements = mods::requirements_for(entry, &self.catalogs);
        requirements.extend(
            self.catalogs
                .dependencies
                .requirements_for(&[installed.name.as_str()]),
        );
        let package = self.build_package(
            staged,
            PackageRequest {
                installed: Some(installed),
                name: None,
                source: &source,
                requirements,
                migration: Some((entry, &item.mod_id)),
            },
        )?;
        if package.previous.as_deref() != Some(&item.source) || package.managed_previous.is_some() {
            return Err(ProfileError::PlanChanged.into());
        }
        if self
            .profile_targets()
            .iter()
            .any(|target| paths::same_target(target, &package.target))
        {
            return Err(ProfileError::AdditionalTarget(package.target).into());
        }
        let local = local_version(entry, installed)?;
        Ok((package, local.original().to_owned()))
    }
}

fn checked_row<'a>(
    confirmed: &MigrationRow,
    rows: &'a [(MigrationRow, InstalledMod)],
) -> Result<&'a (MigrationRow, InstalledMod), HubError> {
    rows.iter()
        .find(|(row, _)| {
            row.path == confirmed.path
                && row.local.is_ok()
                && row.local == confirmed.local
                && row.files == confirmed.files
        })
        .ok_or_else(|| ProfileError::PlanChanged.into())
}

fn checked_github_row<'a>(
    confirmed: &MigrationRow,
    entry: &SourceEntry,
    rows: &'a [(MigrationRow, InstalledMod)],
) -> Result<&'a InstalledMod, HubError> {
    let (row, installed) = checked_row(confirmed, rows)?;
    if row.sources != confirmed.sources
        || !row.sources.iter().any(|(_, candidate)| candidate == entry)
    {
        return Err(ProfileError::PlanChanged.into());
    }
    Ok(installed)
}

fn managed_in(profile: &Profile, item: &AdoptItem, store: &store::Store, plugins: &Path) -> bool {
    profile.entries.iter().any(|entry| {
        entry.mod_id.eq_ignore_ascii_case(&item.mod_id)
            || profiles::entry_target(entry, store, plugins).is_some_and(|target| {
                paths::same_target(&target, &item.source)
                    || paths::same_target(&target, &item.target)
            })
    })
}

fn local_files(item: &AdoptItem) -> Result<Vec<LocalFile>, HubError> {
    let paths = if item.directory {
        store::scan_source_paths(&item.source).map_err(PackageError::from)?
    } else {
        item.file.iter().cloned().collect()
    };
    if paths.is_empty() {
        return Err(PackageError::MissingPayload(item.source.clone()).into());
    }
    paths
        .into_iter()
        .map(|path| {
            let relative = PathBuf::from(path);
            let staged = if item.directory {
                item.source.join(&relative)
            } else {
                item.source.clone()
            };
            let metadata = fs::symlink_metadata(&staged).map_err(|source| PackageError::Io {
                path: staged.clone(),
                source,
            })?;
            if paths::is_link(&metadata) || !metadata.is_file() {
                return Err(PackageError::MissingPayload(staged).into());
            }
            Ok(LocalFile {
                package: PackageFile {
                    relative,
                    staged,
                    size: metadata.len(),
                },
                modified: metadata.modified().ok(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
