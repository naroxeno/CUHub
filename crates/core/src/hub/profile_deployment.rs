//! Profile deployment entry points and confirmation checks.

use std::path::Path;

use super::{Hub, HubError};
use crate::profiles::{self, ApplyOp, ApplyReport, ProfileError};

impl Hub {
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
