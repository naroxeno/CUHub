//! Prepares confirmed package targets and applies staged packages.

use std::path::{Path, PathBuf};
use std::{fs, io};

use chrono::Utc;

use super::{Hub, HubError};
use crate::catalogs::DependencyRequirement;
use crate::install;
use crate::mods::{self, InstalledMod};
use crate::packages::{self, PackageContext, PackagePlan, PackageReport};
use crate::sources::SourceEntry;
use crate::version::HubVersion;
use crate::{paths, profiles};

pub(super) struct PackageRequest<'a> {
    pub installed: Option<&'a InstalledMod>,
    pub name: Option<&'a str>,
    pub source: &'a str,
    pub requirements: Vec<DependencyRequirement>,
    pub migration: Option<(&'a SourceEntry, &'a str)>,
}

impl Hub {
    pub(super) fn stage_download(
        &self,
        download: &Path,
        staging: &Path,
    ) -> Result<install::StagedArchive, HubError> {
        let staged = if install::is_dll_file(download) {
            install::stage_dll(download, staging)
        } else {
            install::stage_archive(download, staging)
        };
        match staged {
            Ok(staged) => Ok(staged),
            Err(error) => {
                let _ = fs::remove_dir_all(staging);
                Err(HubError::Install(error.to_string()))
            }
        }
    }

    pub(super) fn finish_package(
        &self,
        staged: &install::StagedArchive,
        installed: Option<&InstalledMod>,
        name: Option<&str>,
        source: &str,
        requirements: Vec<DependencyRequirement>,
    ) -> Result<PackagePlan, HubError> {
        self.build_package(
            staged,
            PackageRequest {
                installed,
                name,
                source,
                requirements,
                migration: None,
            },
        )
    }

    pub(super) fn build_package(
        &self,
        staged: &install::StagedArchive,
        request: PackageRequest<'_>,
    ) -> Result<PackagePlan, HubError> {
        let PackageRequest {
            installed,
            name,
            source,
            requirements,
            migration,
        } = request;
        let payload =
            install::plan_package(staged).map_err(|error| HubError::Plan(error.to_string()))?;
        let plugins = self.plugins_dir()?;
        let installed_mods = self.scan_mods()?;
        let (mut mod_id, previous) = self.package_identity(installed, name)?;
        if let Some((_, id)) = migration {
            mod_id = id.to_owned();
        }
        let provisional_target = package_deployment(&payload.files).target(&mod_id, &plugins);
        let mut warnings = Vec::new();
        let files = Self::package_files_without_duplicates(
            &payload,
            &installed_mods,
            installed.map_or(provisional_target.as_path(), |item| {
                item.source_path.as_path()
            }),
            &mut warnings,
        );
        if files.is_empty() {
            return Err(HubError::Plan(String::from(
                "the package only contains plugins that are already installed",
            )));
        }
        let deployment = package_deployment(&files);
        let target = deployment.target(&mod_id, &plugins);
        let managed_previous = installed
            .filter(|item| self.links.get(&item.source_path).is_some())
            .map(|item| item.source_path.clone());
        if !paths::is_inside(&target, &plugins) {
            return Err(HubError::Plan(format!(
                "{} is outside the Plugins folder",
                target.display()
            )));
        }
        if installed.is_none_or(|item| !paths::same_target(&target, &item.source_path)) {
            match target.symlink_metadata() {
                Ok(_) => {
                    return Err(HubError::Plan(format!(
                        "{} already exists",
                        target.display()
                    )));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(HubError::Plan(format!("{}: {error}", target.display()))),
            }
        }
        let mut requirements = requirements;
        let mut staged_dlls = Vec::new();
        for file in &files {
            if !install::is_dll_file(&file.relative) {
                continue;
            }
            requirements.extend(mods::read_dependencies(&file.staged));
            staged_dlls.push(file.staged.clone());
        }
        let replacement = installed.filter(|_| migration.is_some());
        let issues = if let Some(item) = replacement {
            let mut available: Vec<_> = staged_dlls
                .iter()
                .filter_map(|dll| mods::build_mod(dll, &[]))
                .collect();
            available.extend(
                installed_mods
                    .into_iter()
                    .filter(|other| !paths::same_target(&other.source_path, &item.source_path)),
            );
            mods::check_requirements(&requirements, &available, self.sources.as_ref())
        } else {
            self.package_dependency_issues(requirements, &staged_dlls, &installed_mods)
        };
        if !issues.is_empty() {
            return Err(HubError::Dependencies(issues));
        }
        let detected = if let Some(((entry, _), item)) = migration.zip(installed) {
            Some(super::migration::replacement_version(entry, item, &files)?)
        } else {
            files
                .iter()
                .filter(|file| install::is_dll_file(&file.relative))
                .filter_map(|file| mods::read_mod_version(&file.staged))
                .filter_map(|value| HubVersion::parse_external(value.as_str()))
                .max()
        };
        let (version, detected) = match detected {
            Some(version) => (version.original().to_string(), Some(version)),
            None => {
                let fallback = format!("0.0.0-{}", Utc::now().format("%Y%m%d%H%M%S"));
                warnings.push(format!(
                    "the package has no readable version; using {fallback}"
                ));
                (fallback, None)
            }
        };
        if migration.is_none()
            && let Some(installed) = installed
            && let Some(detected) = detected.as_ref()
            && let Some(installed_version) = installed
                .installed_version
                .as_deref()
                .and_then(HubVersion::parse_external)
            && detected < &installed_version
        {
            return Err(HubError::Update(format!(
                "the download contains version {} but {} is installed; refusing to downgrade",
                detected.original(),
                installed_version.original()
            )));
        }
        if let Some(installed) = installed
            && !paths::same_target(&installed.source_path, &target)
        {
            warnings.push(format!(
                "replaces the existing deployment at {}",
                installed.source_path.display()
            ));
        }
        Ok(PackagePlan {
            mod_id,
            version,
            source: source.to_string(),
            target,
            previous,
            managed_previous,
            staging: staged.staging_root.clone(),
            files,
            single: matches!(deployment, profiles::Deployment::File(_)),
            warnings,
        })
    }

    fn package_files_without_duplicates(
        payload: &install::PackagePayload,
        installed: &[InstalledMod],
        target: &Path,
        warnings: &mut Vec<String>,
    ) -> Vec<install::PackageFile> {
        if payload.single {
            return payload.files.clone();
        }
        let mut files = Vec::with_capacity(payload.files.len());
        for file in &payload.files {
            if !install::is_dll_file(&file.relative) {
                files.push(file.clone());
                continue;
            }
            let Some((_, Some(guid))) = mods::read_identity(&file.staged) else {
                files.push(file.clone());
                continue;
            };
            let Some(existing) = installed.iter().find(|item| {
                !item.disabled
                    && item.source_path != target
                    && item
                        .guid
                        .as_deref()
                        .is_some_and(|installed| installed.eq_ignore_ascii_case(&guid))
            }) else {
                files.push(file.clone());
                continue;
            };
            warnings.push(format!(
                "skipped bundled {} because {} is already installed",
                file.relative.display(),
                existing.name
            ));
        }
        files
    }

    fn package_identity(
        &self,
        installed: Option<&InstalledMod>,
        name: Option<&str>,
    ) -> Result<(String, Option<PathBuf>), HubError> {
        let managed = installed.and_then(|item| self.links.get(&item.source_path));
        let profile_id = installed.and_then(|item| {
            self.profile_entry_index(&item.source_path)
                .map(|(_, profile, index)| profile.entries[index].mod_id.clone())
        });
        let is_managed = managed.is_some() || profile_id.is_some();
        let mod_id = if let Some(id) = managed.map(|entry| entry.mod_id.clone()).or(profile_id) {
            id
        } else if let Some(item) = installed {
            let raw = if item.source_path.is_dir() {
                item.source_path.file_name()
            } else {
                item.source_path.file_stem()
            }
            .and_then(|value| value.to_str())
            .unwrap_or("mod");
            install::normalize_install_name(raw)
        } else {
            let name = name.ok_or_else(|| HubError::Plan(String::from("missing mod name")))?;
            install::normalize_install_name(name)
        };
        let previous = installed
            .filter(|_| !is_managed)
            .map(|item| item.source_path.clone());
        Ok((mod_id, previous))
    }

    pub fn prepare_package_update(
        &self,
        download: &Path,
        installed: &InstalledMod,
        staging: &Path,
        source: &str,
    ) -> Result<PackagePlan, HubError> {
        let staged = self.stage_download(download, staging)?;
        let requirements = self
            .catalogs
            .dependencies
            .requirements_for(&[installed.name.as_str()]);
        let result = self.finish_package(&staged, Some(installed), None, source, requirements);
        if result.is_err() {
            self.discard_staging(&staged.staging_root);
        }
        result
    }

    pub fn check_package_guid(
        &self,
        package: &PackagePlan,
        expected: &str,
    ) -> Result<(), HubError> {
        if package.files.iter().any(|file| {
            install::is_dll_file(&file.relative)
                && mods::read_identity(&file.staged)
                    .and_then(|(_, guid)| guid)
                    .is_some_and(|guid| guid.eq_ignore_ascii_case(expected))
        }) {
            return Ok(());
        }
        self.discard_staging(&package.staging);
        Err(HubError::Plan(format!(
            "the selected package does not contain the expected BepInEx plugin {expected}"
        )))
    }

    pub fn prepare_nexus_package(
        &self,
        archive: &Path,
        id: u64,
        name: &str,
    ) -> Result<PackagePlan, HubError> {
        let staged = self.stage_download(archive, &self.staging_root())?;
        let requirements = self.catalogs.dependencies.requirements_for(&[name]);
        let result = self.finish_package(
            &staged,
            None,
            Some(name),
            &format!("nexus:{id}"),
            requirements,
        );
        if result.is_err() {
            self.discard_staging(&staged.staging_root);
        }
        result
    }

    pub fn apply_package(&mut self, plan: &PackagePlan) -> Result<PackageReport, HubError> {
        self.ensure_game_closed()?;
        let game = self.game_folder().ok_or(HubError::NoGameFolder)?;
        let plugins = self.plugins_dir()?;
        let backup = self.backup_root();
        let report = packages::apply(
            plan,
            PackageContext {
                store: &self.store,
                links: &mut self.links,
                profiles: &self.profiles,
                game_root: &game,
                plugins_root: &plugins,
                backup_root: &backup,
            },
        )?;
        self.logs.activity(
            "Packages",
            &format!(
                "Installed {} file(s) from {} into the store; profile {}.",
                report.files, plan.mod_id, report.profile
            ),
        );
        Ok(report)
    }
}

fn package_deployment(files: &[install::PackageFile]) -> profiles::Deployment {
    profiles::deployment_from_paths(
        files
            .iter()
            .map(|file| file.relative.to_str().unwrap_or("")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_the_dll_target_from_the_files_remaining_after_filtering() {
        let plugins = Path::new("Plugins");
        let mut files = ["main.dll", "bundled.dll"]
            .map(|name| install::PackageFile {
                staged: PathBuf::from(name),
                relative: PathBuf::from(name),
                size: 1,
            })
            .to_vec();
        assert_eq!(
            package_deployment(&files).target("mod", plugins),
            plugins.join("mod")
        );
        files.pop();
        assert_eq!(
            package_deployment(&files).target("mod", plugins),
            plugins.join("main.dll")
        );
    }
}
