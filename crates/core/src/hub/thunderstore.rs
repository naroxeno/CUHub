//! Plans additions from the Thunderstore catalogue without replacing existing mods.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use super::{Hub, HubError, unique_download_dir};
use crate::install::{self, PackageFile, StagedArchive};
use crate::network::{DownloadProgress, Network, NetworkError};
use crate::packages::{self, PackagePlan};
use crate::profiles::{self, Profile};
use crate::thunderstore::{self, Catalogue, Package, SOURCE_PREFIX, Version};

mod dependencies;
mod local_dependency;
mod publish;
mod transaction;
pub use dependencies::{DependencyPlan, DependencyState, PlannedDependency};

const BEPINEX_PACK: &str = "BepInEx-BepInExPack";
const BEPINEX_PACK_VERSION: &str = "5.4.2305";
const MANIFEST_BYTES: u64 = 64 * 1024;

#[derive(Deserialize)]
struct Manifest {
    name: String,
    version_number: String,
    dependencies: Vec<String>,
}

impl Hub {
    pub(super) fn thunderstore_apply_lock(&self) -> Result<fs::File, HubError> {
        publish::lock(&self.app_data).map_err(HubError::Io)
    }

    pub fn thunderstore_catalogue(&self) -> Catalogue {
        Catalogue::new(&self.app_data)
    }

    pub fn thunderstore_installed(&self) -> Vec<(String, String)> {
        self.active_profile()
            .into_iter()
            .flat_map(|(_, profile)| profile.entries)
            .filter_map(|entry| {
                let source = entry.source?.strip_prefix(SOURCE_PREFIX)?.to_owned();
                Some((source, entry.version))
            })
            .collect()
    }

    pub fn prepare_thunderstore(
        &self,
        package_id: &str,
        number: &str,
        progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<PackagePlan, HubError> {
        let catalogue = self
            .thunderstore_catalogue()
            .cached()
            .map_err(HubError::Io)?;
        let package = catalogue
            .iter()
            .find(|package| package.full_name == package_id)
            .ok_or_else(|| {
                plan_error(format!("Thunderstore package {package_id} is unavailable"))
            })?;
        let version = package
            .versions
            .iter()
            .find(|version| version.version_number == number)
            .ok_or_else(|| {
                plan_error(format!(
                    "Thunderstore version {package_id}@{number} is unavailable"
                ))
            })?;
        let plugins = self.plugins_dir()?;
        let profile = packages::active_or_default(&self.profiles, &self.links)?;
        if profile
            .entries
            .iter()
            .any(|entry| entry.mod_id.eq_ignore_ascii_case(&package.mod_id()))
        {
            return Err(plan_error(format!(
                "{} is already in the {} profile",
                package.full_name, profile.name
            )));
        }
        self.check_thunderstore_dependencies(&version.dependencies, &profile, &plugins)?;
        let download = unique_download_dir(&self.download_cache())?;
        let result = self
            .stage_thunderstore(package, version, &download, progress)
            .and_then(|plan| {
                if let Err(error) = self.check_thunderstore_plan(&plan, &plugins, false) {
                    self.discard_staging(&plan.staging);
                    return Err(error);
                }
                Ok(plan)
            });
        let _ = fs::remove_dir_all(download);
        result
    }

    fn stage_thunderstore(
        &self,
        package: &Package,
        version: &Version,
        directory: &Path,
        progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<PackagePlan, HubError> {
        let archive = directory.join("package.zip");
        if version.file_size == 0 || version.file_size > install::MAX_ARCHIVE_BYTES {
            return Err(plan_error(format!(
                "unsupported archive size for {}",
                package.full_name
            )));
        }
        let size = Network::thunderstore()?
            .download_with_timeout_progress(
                &version.download_url,
                &archive,
                version.file_size,
                Duration::from_secs(300),
                progress,
            )
            .map_err(NetworkError::without_url_parameters)?;
        if size != version.file_size {
            return Err(plan_error(format!(
                "download size does not match the catalogue for {}",
                package.full_name
            )));
        }
        let staged = self.stage_download(&archive, &self.staging_root())?;
        let result = self.plan_thunderstore(package, version, &staged);
        if result.is_err() {
            self.discard_staging(&staged.staging_root);
        }
        result
    }

    fn plan_thunderstore(
        &self,
        package: &Package,
        version: &Version,
        staged: &StagedArchive,
    ) -> Result<PackagePlan, HubError> {
        let manifest = read_manifest(&staged.staging_root)?;
        let declared: BTreeSet<_> = manifest.dependencies.iter().collect();
        let indexed: BTreeSet<_> = version.dependencies.iter().collect();
        if manifest.name != package.name
            || manifest.version_number != version.version_number
            || declared != indexed
        {
            return Err(plan_error(format!(
                "archive manifest does not match {}@{}",
                package.full_name, version.version_number
            )));
        }
        let files = plugin_payload(staged)?;
        let mod_id = package.mod_id();
        let plugins = self.plugins_dir()?;
        let deployment =
            profiles::deployment_from_paths(files.iter().filter_map(|file| file.relative.to_str()));
        let target = deployment.target(&mod_id, &plugins);
        let plan = PackagePlan {
            mod_id,
            version: version.version_number.clone(),
            source: package.source(),
            dependencies: version.dependencies.clone(),
            target,
            previous: None,
            managed_previous: None,
            staging: staged.staging_root.clone(),
            files,
            single: matches!(deployment, profiles::Deployment::File(_)),
            warnings: Vec::new(),
        };
        Ok(plan)
    }

    pub(super) fn check_thunderstore_plan(
        &self,
        plan: &PackagePlan,
        plugins: &Path,
        applying: bool,
    ) -> Result<(), HubError> {
        if plan.previous.is_some() || plan.managed_previous.is_some() {
            return Err(profiles::ProfileError::PlanChanged.into());
        }
        self.check_local_import(plan, plugins, applying)?;
        if self
            .store
            .read_entry(&plan.mod_id, &plan.version)
            .is_some_and(|entry| entry.source != plan.source)
        {
            return Err(plan_error(format!(
                "{}@{} belongs to a different source",
                plan.mod_id, plan.version
            )));
        }
        let profile = packages::active_or_default(&self.profiles, &self.links)?;
        let manifest = read_manifest(&plan.staging)?;
        let name = plan
            .source
            .strip_prefix(SOURCE_PREFIX)
            .is_some_and(|id| id.ends_with(&format!("-{}", manifest.name)));
        if !name
            || manifest.version_number != plan.version
            || manifest.dependencies.iter().collect::<BTreeSet<_>>()
                != plan.dependencies.iter().collect::<BTreeSet<_>>()
        {
            return Err(profiles::ProfileError::PlanChanged.into());
        }
        self.check_thunderstore_dependencies(&manifest.dependencies, &profile, plugins)?;
        self.check_thunderstore_plugins(plan, &manifest.name, &[])
    }

    fn check_thunderstore_plugins(
        &self,
        plan: &PackagePlan,
        name: &str,
        prior: &[crate::mods::InstalledMod],
    ) -> Result<(), HubError> {
        let dlls: Vec<_> = plan
            .files
            .iter()
            .filter(|file| install::is_dll_file(&file.relative))
            .map(|file| file.staged.clone())
            .collect();
        let mut guids = BTreeSet::new();
        for guid in dlls
            .iter()
            .filter_map(|path| crate::mods::read_identity(path)?.1)
        {
            if !guids.insert(guid.to_ascii_lowercase()) {
                return Err(plan_error(format!(
                    "duplicate plugin GUID in Thunderstore package: {guid}"
                )));
            }
        }
        let mut installed = self.scan_mods()?;
        installed.extend_from_slice(prior);
        for plugin in installed
            .iter()
            .filter(|item| !crate::paths::same_target(&item.source_path, &plan.target))
        {
            if let Some(guid) = plugin
                .dll_paths
                .iter()
                .filter_map(|path| crate::mods::read_identity(path)?.1)
                .find(|guid| guids.contains(&guid.to_ascii_lowercase()))
            {
                return Err(plan_error(format!(
                    "{guid} is already installed at {}",
                    plugin.source_path.display()
                )));
            }
        }
        let mut requirements = self.catalogs.dependencies.requirements_for(&[name]);
        requirements.extend(
            dlls.iter()
                .flat_map(|path| crate::mods::read_dependencies(path)),
        );
        let mut available: Vec<_> = dlls
            .iter()
            .filter_map(|path| crate::mods::build_mod(path, &[]))
            .collect();
        available.extend(installed);
        let issues =
            crate::mods::check_requirements(&requirements, &available, self.sources.as_ref());
        if !issues.is_empty() {
            return Err(HubError::Dependencies(issues));
        }
        Ok(())
    }

    fn check_thunderstore_dependencies(
        &self,
        dependencies: &[String],
        profile: &Profile,
        plugins: &Path,
    ) -> Result<(), HubError> {
        let mut missing = Vec::new();
        for dependency in dependencies {
            let (id, version) = thunderstore::dependency(dependency).ok_or_else(|| {
                plan_error(format!("invalid Thunderstore dependency: {dependency}"))
            })?;
            if id.eq_ignore_ascii_case(BEPINEX_PACK) && version == BEPINEX_PACK_VERSION {
                let status = self.bepinex_status()?;
                if !satisfies_bepinex_pack(&status) {
                    let detected = status.version.as_deref().unwrap_or("unknown");
                    return Err(plan_error(format!(
                        "{dependency} needs a complete Mono BepInEx 5 installation, version 5.4.23.5 or newer. Check BepInEx in Settings (detected version: {detected})."
                    )));
                }
                continue;
            }
            match self.installed_thunderstore_dependency(id, version, profile, plugins) {
                Some(state) if state.satisfied() => {}
                Some(DependencyState::LocalConflict(details)) => {
                    return Err(plan_error(format!("{dependency}: {details}")));
                }
                _ => missing.push(dependency.as_str()),
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        Err(plan_error(format!(
            "required Thunderstore versions are missing, disabled, or different: {}",
            missing.join(", ")
        )))
    }
}

fn satisfies_bepinex_pack(status: &install::Status) -> bool {
    if !status.complete || status.major != Some(5) || status.il2cpp {
        return false;
    }
    let Some(version) = status.version.as_deref() else {
        return false;
    };
    let mut parts = version.split('.');
    let mut installed = [0u16; 4];
    for component in &mut installed {
        let Some(value) = parts.next().and_then(|part| part.parse().ok()) else {
            return false;
        };
        *component = value;
    }
    parts.next().is_none() && installed[0] == 5 && installed >= [5, 4, 23, 5]
}

fn read_manifest(staging: &Path) -> Result<Manifest, HubError> {
    use std::io::Read;
    let file = fs::File::open(staging.join("manifest.json"))
        .map_err(|error| plan_error(format!("cannot open Thunderstore manifest: {error}")))?;
    let mut bytes = Vec::new();
    file.take(MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(HubError::Io)?;
    if bytes.len() as u64 > MANIFEST_BYTES {
        return Err(plan_error(String::from(
            "Thunderstore manifest exceeds the size limit",
        )));
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| plan_error(format!("invalid Thunderstore manifest: {error}")))
}

fn plugin_payload(staged: &StagedArchive) -> Result<Vec<PackageFile>, HubError> {
    let mut files = Vec::new();
    let companions: BTreeSet<_> = staged
        .files
        .iter()
        .map(|file| file.relative.as_path())
        .filter(|path| path.components().count() == 1 && is_root_plugin(path))
        .filter_map(|path| path.file_stem()?.to_str().map(str::to_lowercase))
        .collect();
    let mut roots = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for file in &staged.files {
        let path = &file.relative;
        if path.components().count() == 1
            && [
                "manifest.json",
                "README.md",
                "icon.png",
                "CHANGELOG.md",
                "LICENSE",
                "LICENSE.txt",
            ]
            .iter()
            .any(|name| path.eq(Path::new(name)))
        {
            continue;
        }
        let relative = plugin_relative(path, &companions, &mut roots).ok_or_else(|| {
            plan_error(format!(
                "unsupported Thunderstore package path: {}",
                path.display()
            ))
        })?;
        if !seen.insert(relative.to_string_lossy().to_lowercase()) {
            return Err(plan_error(format!(
                "overlapping Thunderstore package path: {}",
                path.display()
            )));
        }
        files.push(PackageFile {
            staged: file.path.clone(),
            relative,
            size: file.size,
        });
    }
    if roots.len() != 1
        || !files
            .iter()
            .any(|file| install::is_dll_file(&file.relative))
    {
        return Err(plan_error(String::from(
            "a Thunderstore package must have one Plugins layout containing a DLL",
        )));
    }
    Ok(files)
}

fn is_root_plugin(path: &Path) -> bool {
    install::is_dll_file(path)
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                !["winhttp.dll", "version.dll", "doorstop.dll", "BepInEx.dll"]
                    .iter()
                    .any(|reserved| name.eq_ignore_ascii_case(reserved))
            })
}

fn plugin_relative(
    path: &Path,
    companions: &BTreeSet<String>,
    roots: &mut BTreeSet<u8>,
) -> Option<PathBuf> {
    let mut parts = path.components();
    let first = parts.next()?.as_os_str().to_str()?;
    let root = if parts.as_path().as_os_str().is_empty() && is_root_plugin(path) {
        roots.insert(0);
        return Some(path.to_path_buf());
    } else if first.eq_ignore_ascii_case("plugins") {
        1
    } else if first.eq_ignore_ascii_case("bepinex")
        && parts
            .next()?
            .as_os_str()
            .to_str()?
            .eq_ignore_ascii_case("plugins")
    {
        2
    } else if !parts.as_path().as_os_str().is_empty()
        && !["config", "patchers", "core", "monomod", "doorstop_libs"]
            .iter()
            .any(|reserved| first.eq_ignore_ascii_case(reserved))
        && companions.contains(&first.to_lowercase())
    {
        roots.insert(0);
        return Some(path.to_path_buf());
    } else {
        return None;
    };
    let relative = parts.as_path();
    if relative.as_os_str().is_empty() {
        return None;
    }
    roots.insert(root);
    Some(relative.to_path_buf())
}

fn plan_error(message: String) -> HubError {
    HubError::Plan(message)
}

#[cfg(test)]
mod tests;
