//! Profile deployment entry points and confirmation checks.

use std::path::Path;

use super::{Hub, HubError};
use crate::mods::{self, InstalledMod, ModStatus};
use crate::paths;
use crate::profiles::{self, ApplyOp, ApplyReport, Profile, ProfileEntry, ProfileError};

impl Hub {
    pub fn set_managed_mod_enabled(
        &mut self,
        item: &InstalledMod,
        enabled: bool,
    ) -> Result<(usize, Option<InstalledMod>), HubError> {
        self.check_link_index()?;
        let (_, profile, index) = self
            .profile_entry_index(&item.source_path)
            .ok_or(HubError::ManagedByProfile)?;
        let entry = profile.entries[index].clone();
        let count = self.set_profile_entry_enabled(item, enabled, (profile, index))?;
        let updated = if enabled {
            self.build_sourced_profile_mod(&entry, &item.source_path)
                .or_else(|| mods::build_mod(&item.source_path, &self.metadata))
        } else {
            self.store
                .resolve(&entry.mod_id, &entry.version)
                .zip(self.store.read_entry(&entry.mod_id, &entry.version))
                .and_then(|(payload, manifest)| {
                    self.build_disabled_profile_mod(
                        &entry,
                        &payload,
                        &item.source_path,
                        profiles::deployment(&manifest.files),
                    )
                })
        };
        Ok((count, updated))
    }

    pub(super) fn build_disabled_profile_mod(
        &self,
        entry: &ProfileEntry,
        payload: &Path,
        target: &Path,
        deployment: profiles::Deployment,
    ) -> Option<InstalledMod> {
        let source = self.source_entry_for(entry);
        let preferred = source.map_or(&[][..], |source| source.matcher.dll.as_slice());
        let mut item = mods::build_mod_with_preferred(
            payload,
            &self.metadata,
            preferred,
            source.map(|source| source.name.as_str()),
        )?;
        if let Some(source) = source {
            item.name.clone_from(&source.name);
        }
        item.source_path = target.to_path_buf();
        item.dll_paths = match deployment {
            profiles::Deployment::File(_) => vec![target.to_path_buf()],
            profiles::Deployment::Directory => item
                .dll_paths
                .iter()
                .map(|dll| target.join(dll.strip_prefix(payload).unwrap_or(dll)))
                .collect(),
        };
        item.disabled = true;
        item.status = ModStatus::Disabled;
        Some(item)
    }

    pub(super) fn build_sourced_profile_mod(
        &self,
        entry: &ProfileEntry,
        target: &Path,
    ) -> Option<InstalledMod> {
        let source = self
            .source_entry_for(entry)
            .filter(|source| !source.matcher.dll.is_empty())?;
        let mut item = mods::build_mod_with_preferred(
            target,
            &self.metadata,
            &source.matcher.dll,
            Some(&source.name),
        )?;
        item.name.clone_from(&source.name);
        Some(item)
    }

    pub(super) fn apply_profile_toggle(
        &mut self,
        profile: &Profile,
        index: usize,
        target: &Path,
    ) -> Result<(), HubError> {
        let game = self.game_folder().ok_or(HubError::NoGameFolder)?;
        let plugins = self.plugins_dir()?;
        let entry = &profile.entries[index];
        let expected = profiles::entry_target(entry, &self.store, &plugins).ok_or_else(|| {
            ProfileError::MissingVersion {
                mod_id: entry.mod_id.clone(),
                version: entry.version.clone(),
            }
        })?;
        if !paths::same_target(&expected, target) || expected.parent() != Some(plugins.as_path()) {
            return Err(ProfileError::PlanChanged.into());
        }
        self.links
            .check_target(&game, &expected)
            .map_err(ProfileError::from)?;
        self.check_toggle_claim(profile, index, &expected, &plugins)?;
        let recorded = self.links.get(&expected);
        if recorded.is_some_and(|link| link.mod_id != entry.mod_id || link.version != entry.version)
        {
            return Err(ProfileError::OccupiedTarget(expected).into());
        }
        let op = if entry.enabled {
            let current = self
                .links
                .target_current(&expected)
                .map_err(ProfileError::from)?;
            (!current).then(|| ApplyOp::Deploy {
                target: expected,
                mod_id: entry.mod_id.clone(),
                version: entry.version.clone(),
            })
        } else if recorded.is_some() {
            Some(ApplyOp::Remove { target: expected })
        } else {
            self.links
                .target_current(&expected)
                .map_err(ProfileError::from)?;
            None
        };
        profiles::apply_plan(&mut self.links, &self.store, &game, op.as_slice())?;
        self.profiles.save_deployed(profile)?;
        Ok(())
    }

    fn check_toggle_claim(
        &self,
        profile: &Profile,
        index: usize,
        target: &Path,
        plugins: &Path,
    ) -> Result<(), ProfileError> {
        let entry = &profile.entries[index];
        let target_name = folded_name(target);
        for (other_index, other) in profile.entries.iter().enumerate() {
            if other_index == index || !other.enabled {
                continue;
            }
            if other.mod_id == entry.mod_id {
                return Err(ProfileError::DuplicateMod(entry.mod_id.clone()));
            }
            if profiles::entry_target(other, &self.store, plugins)
                .is_some_and(|path| folded_name(&path) == target_name)
            {
                return Err(ProfileError::DuplicateTarget(target.to_path_buf()));
            }
        }
        if entry.enabled
            && let Some((other, _)) = self.links.entries().find(|(other, link)| {
                link.mod_id == entry.mod_id && !paths::same_target(Path::new(other), target)
            })
        {
            return Err(ProfileError::AdditionalTarget(
                Path::new(other).to_path_buf(),
            ));
        }
        Ok(())
    }

    pub fn plan_profile(&self, name: &str) -> Result<Vec<ApplyOp>, HubError> {
        let profile = self
            .profiles
            .load(name)
            .ok_or_else(|| HubError::ProfileNotFound(name.to_owned()))?;
        let game_root = self.game_folder().ok_or(HubError::NoGameFolder)?;
        let plugins_root = self.plugins_dir()?;
        let ops = profiles::plan_apply(
            &profile,
            &self.links,
            &self.store,
            &game_root,
            &plugins_root,
        )?;
        profiles::check_deployment_targets(&self.links, &ops, None)?;
        Ok(ops)
    }

    pub fn apply_profile(&mut self, name: &str) -> Result<ApplyReport, HubError> {
        self.ensure_game_closed()?;
        let ops = self.plan_profile(name)?;
        self.apply_profile_ops(name, &ops)
    }

    fn apply_profile_ops(&mut self, name: &str, ops: &[ApplyOp]) -> Result<ApplyReport, HubError> {
        let game_root = self.game_folder().ok_or(HubError::NoGameFolder)?;
        let report = profiles::apply_plan(&mut self.links, &self.store, &game_root, ops)?;
        self.logs.activity(
            "Profiles",
            &format!(
                "Applied profile {name}: {} operation(s).",
                report.applied.len()
            ),
        );
        Ok(report)
    }

    pub fn apply_confirmed_profile(
        &mut self,
        name: &str,
        confirmed: &[ApplyOp],
    ) -> Result<ApplyReport, HubError> {
        self.ensure_game_closed()?;
        let ops = self.plan_profile(name)?;
        if ops != confirmed {
            return Err(ProfileError::PlanChanged.into());
        }
        let report = self.apply_profile_ops(name, &ops)?;
        self.profiles
            .set_active(Some(name))
            .map_err(|source| ProfileError::Commit {
                name: name.to_owned(),
                source,
            })?;
        Ok(report)
    }

    pub(super) fn apply_profile_change(
        &mut self,
        profile: &profiles::Profile,
        only_target: Option<&Path>,
    ) -> Result<(), HubError> {
        let game_root = self.game_folder().ok_or(HubError::NoGameFolder)?;
        let plugins_root = self.plugins_dir()?;
        let ops =
            profiles::plan_apply(profile, &self.links, &self.store, &game_root, &plugins_root)?;
        if let Some(only) = only_target
            && let Some(op) = ops
                .iter()
                .find(|op| !crate::paths::same_target(op.target(), only))
        {
            return Err(ProfileError::AdditionalTarget(op.target().to_path_buf()).into());
        }
        profiles::check_deployment_targets(&self.links, &ops, None)?;
        profiles::apply_plan(&mut self.links, &self.store, &game_root, &ops)?;
        self.profiles.save_deployed(profile)?;
        Ok(())
    }
}

fn folded_name(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
}

#[cfg(test)]
mod tests;
