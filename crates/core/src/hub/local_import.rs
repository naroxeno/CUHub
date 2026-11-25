use std::fs;
use std::path::Path;

use super::{Hub, HubError};
use crate::install;
use crate::mods;
use crate::packages::{self, PackagePlan};
use crate::paths;

pub(super) const SOURCE: &str = "local";

impl Hub {
    pub fn prepare_local_import(&self, file: &Path) -> Result<PackagePlan, HubError> {
        let plugins = self.plugins_dir()?;
        let metadata = fs::symlink_metadata(file)
            .map_err(|error| HubError::Install(format!("{}: {error}", file.display())))?;
        if !metadata.is_file() || paths::is_link(&metadata) {
            return Err(HubError::Install(format!(
                "{} is not a regular file",
                file.display()
            )));
        }
        let name = file
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                HubError::Install(format!("invalid mod file name: {}", file.display()))
            })?;
        let staged = self.stage_download(file, &self.staging_root())?;
        let requirements = self.catalogs.dependencies.requirements_for(&[name]);
        let result = self
            .finish_package(&staged, None, Some(name), SOURCE, requirements)
            .and_then(|plan| {
                self.check_local_import(&plan, &plugins, false)?;
                Ok(plan)
            });
        if result.is_err() {
            self.discard_staging(&staged.staging_root);
        }
        result
    }

    pub(super) fn check_local_import(
        &self,
        plan: &PackagePlan,
        plugins: &Path,
        applying: bool,
    ) -> Result<(), HubError> {
        self.check_link_index()?;
        if plan.target.parent() != Some(plugins) {
            return Err(crate::profiles::ProfileError::PlanChanged.into());
        }
        let retry_target = applying
            && self
                .links
                .get(&plan.target)
                .is_some_and(|entry| entry.mod_id == plan.mod_id && entry.version == plan.version);
        check_import_target(plan, plugins, retry_target)?;
        let profile = packages::active_or_default(&self.profiles, &self.links)?;
        let recovered_entry = retry_target && self.profiles.load(&profile.name).is_none();
        if !recovered_entry
            && profile
                .entries
                .iter()
                .any(|entry| same_id(&entry.mod_id, plan))
        {
            return Err(HubError::Plan(format!(
                "{} already exists in the {} profile",
                plan.mod_id, profile.name
            )));
        }
        if let Some((target, _)) = self.links.entries().find(|(target, entry)| {
            same_id(&entry.mod_id, plan)
                && !(retry_target && paths::same_target(Path::new(target), &plan.target))
        }) {
            return Err(existing_target(Path::new(target)));
        }
        self.check_import_plugins(plan, retry_target)
    }

    fn check_import_plugins(&self, plan: &PackagePlan, retry_target: bool) -> Result<(), HubError> {
        let mut dlls = plan
            .files
            .iter()
            .filter(|file| install::is_dll_file(&file.relative))
            .peekable();
        if dlls.peek().is_none() {
            return Err(HubError::Plan(String::from(
                "the package has no plugin DLLs to install",
            )));
        }
        let guids: Vec<_> = dlls
            .filter_map(|file| mods::read_identity(&file.staged).and_then(|(_, guid)| guid))
            .collect();
        if !guids.is_empty()
            && let Some(existing) = self.scan_mods()?.into_iter().find(|item| {
                !(retry_target && paths::same_target(&item.source_path, &plan.target))
                    && item.guid.as_ref().is_some_and(|guid| {
                        guids
                            .iter()
                            .any(|incoming| incoming.eq_ignore_ascii_case(guid))
                    })
            })
        {
            return Err(HubError::Plan(format!(
                "{} is already installed at {}",
                existing.name,
                existing.source_path.display()
            )));
        }
        Ok(())
    }
}

fn check_import_target(
    plan: &PackagePlan,
    plugins: &Path,
    retry_target: bool,
) -> Result<(), HubError> {
    let name = plan
        .target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| HubError::Plan(format!("invalid mod target: {}", plan.target.display())))?;
    if !retry_target && let Some(existing) = paths::find_child(plugins, name) {
        return Err(existing_target(&existing));
    }
    if plan.single
        && let Some(disabled) = paths::find_child(plugins, &format!("{name}.disabled"))
    {
        return Err(existing_target(&disabled));
    }
    Ok(())
}

fn same_id(id: &str, plan: &PackagePlan) -> bool {
    id.eq_ignore_ascii_case(&plan.mod_id)
}

fn existing_target(path: &Path) -> HubError {
    HubError::Plan(format!("{} already exists", path.display()))
}

#[cfg(test)]
mod tests;
