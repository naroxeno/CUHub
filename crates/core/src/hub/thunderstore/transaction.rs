//! Stages all additions before publishing any plugin. A durable receipt allows offline replay.

use super::*;
use crate::links::{LinkEntry, LinkKind};
use crate::profiles::{ApplyOp, Deployment, ProfileEntry};
use serde::Serialize;
use std::io;

#[derive(Serialize, Deserialize)]
pub(super) struct Receipt {
    plan: DependencyPlan,
    before: Option<Profile>,
    active: Option<String>,
    after: Profile,
    pub(super) workspace: String,
    store: PathBuf,
}

fn changed() -> HubError {
    profiles::ProfileError::PlanChanged.into()
}

impl Hub {
    fn receipt_path(&self) -> PathBuf {
        self.app_data.join("Thunderstore-install.json")
    }

    fn receipt(&self) -> Result<Option<Receipt>, HubError> {
        match fs::read(self.receipt_path()) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(HubError::Io(error)),
        }
    }

    pub fn interrupted_thunderstore_plan(&self) -> Result<Option<DependencyPlan>, HubError> {
        self.receipt()?
            .map(|mut receipt| {
                self.refresh_legacy_reuse(&mut receipt)?;
                Ok(receipt.plan)
            })
            .transpose()
    }

    pub fn apply_thunderstore_dependencies(
        &mut self,
        plan: &DependencyPlan,
        confirm_local: bool,
        progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<(), HubError> {
        let _lock = self.thunderstore_apply_lock()?;
        if self.game_running()? {
            return Err(HubError::GameRunning);
        }
        if plan.requires_local_confirmation() && !confirm_local {
            return Err(plan_error(
                "Confirm use of the local dependency before installing".into(),
            ));
        }
        if let Some(mut receipt) = self.receipt()? {
            let upgraded = self.refresh_legacy_reuse(&mut receipt)?;
            if receipt.plan != *plan {
                return Err(plan_error(
                    "Resume the pending Thunderstore installation first".into(),
                ));
            }
            if upgraded {
                crate::atomic::replace(&self.receipt_path(), &serde_json::to_vec(&receipt)?)
                    .map_err(HubError::Io)?;
            }
            return self.finish_receipt(&receipt);
        }
        self.verify_selection(plan)?;
        let catalogue = self
            .thunderstore_catalogue()
            .cached()
            .map_err(HubError::Io)?;
        let mut staged = Vec::new();
        let result = (|| {
            for entry in plan
                .entries
                .iter()
                .filter(|entry| entry.state == DependencyState::Install)
            {
                let package = catalogue
                    .iter()
                    .find(|p| p.full_name.eq_ignore_ascii_case(&entry.package))
                    .ok_or_else(changed)?;
                let version = package
                    .versions
                    .iter()
                    .find(|v| v.version_number == entry.version)
                    .ok_or_else(changed)?;
                let dir = unique_download_dir(&self.download_cache())?;
                let result = self.stage_thunderstore(package, version, &dir, progress);
                let _ = fs::remove_dir_all(dir);
                staged.push(result.map_err(|e| {
                    plan_error(format!("{}@{}: {e}", entry.package, entry.version))
                })?);
            }
            self.begin_install(plan, &staged)
        })();
        for package in staged {
            self.discard_staging(&package.staging);
        }
        self.finish_receipt(&result?)
    }

    fn refresh_legacy_reuse(&self, receipt: &mut Receipt) -> Result<bool, HubError> {
        let mut upgraded = false;
        for entry in &mut receipt.plan.entries {
            let DependencyState::ReuseLocal(path) = &entry.state else {
                continue;
            };
            let candidate = self.local_thunderstore_dependency(
                &entry.package,
                &entry.version,
                &receipt.after,
                &receipt.plan.plugins,
            );
            let Some(candidate) = candidate
                .as_ref()
                .and_then(DependencyState::local)
                .filter(|candidate| candidate.path == *path)
            else {
                return Err(plan_error(format!(
                    "Cannot recheck the pending local dependency at {}",
                    path.display()
                )));
            };
            entry.state = DependencyState::LocalCandidate(candidate.clone());
            upgraded = true;
        }
        Ok(upgraded)
    }

    fn verify_local_reuse(&self, receipt: &Receipt, plugins: &Path) -> Result<(), HubError> {
        for entry in &receipt.plan.entries {
            let Some(expected) = entry.state.local() else {
                continue;
            };
            let detected = self.local_thunderstore_dependency(
                &entry.package,
                &entry.version,
                &receipt.after,
                plugins,
            );
            let matches = match (&entry.state, detected.as_ref()) {
                (
                    DependencyState::VerifiedLocal(_),
                    Some(DependencyState::VerifiedLocal(local)),
                ) => local == expected,
                (DependencyState::LocalCandidate(_), Some(state)) => {
                    state.local() == Some(expected)
                }
                _ => false,
            };
            if !matches {
                return Err(plan_error(format!(
                    "Local dependency changed at {}",
                    expected.path.display()
                )));
            }
        }
        Ok(())
    }

    fn verify_selection(&self, plan: &DependencyPlan) -> Result<(), HubError> {
        let root = plan.entries.last().ok_or_else(changed)?;
        if !plan.installable()
            || self.plan_thunderstore_dependencies(&root.package, &root.version)? != *plan
        {
            return Err(changed());
        }
        Ok(())
    }

    pub(super) fn begin_install(
        &self,
        plan: &DependencyPlan,
        staged: &[PackagePlan],
    ) -> Result<Receipt, HubError> {
        self.verify_selection(plan)?;
        self.ensure_game_closed()?;
        let game = self.game_folder().ok_or(HubError::NoGameFolder)?;
        let mut after = packages::active_or_default(&self.profiles, &self.links)?;
        let mut prior = Vec::new();
        let mut targets = BTreeSet::new();
        let mut available: BTreeSet<_> = plan
            .entries
            .iter()
            .filter(|e| {
                e.state.satisfied() || matches!(e.state, DependencyState::LocalCandidate(_))
            })
            .map(|e| format!("{}-{}", e.package, e.version).to_ascii_lowercase())
            .collect();
        let mut pending = plan
            .entries
            .iter()
            .filter(|entry| entry.state == DependencyState::Install);
        for package in staged {
            let entry = pending.next().ok_or_else(changed)?;
            if !package
                .source
                .eq_ignore_ascii_case(&format!("{SOURCE_PREFIX}{}", entry.package))
                || package.version != entry.version
            {
                return Err(changed());
            }
            self.check_local_import(package, &plan.plugins, false)?;
            let target = package
                .target
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(changed)?;
            if !targets.insert(target.to_lowercase()) {
                return Err(profiles::ProfileError::DuplicateTarget(package.target.clone()).into());
            }
            let manifest = read_manifest(&package.staging)?;
            if manifest.version_number != entry.version
                || !manifest
                    .dependencies
                    .iter()
                    .all(|d| available.contains(&d.to_ascii_lowercase()))
            {
                return Err(changed());
            }
            self.check_thunderstore_plugins(package, &manifest.name, &prior)?;
            available.insert(format!("{}-{}", entry.package, entry.version).to_ascii_lowercase());
            prior.extend(
                package
                    .files
                    .iter()
                    .filter(|f| install::is_dll_file(&f.relative))
                    .filter_map(|f| crate::mods::build_mod(&f.staged, &[])),
            );
            after.entries.push(ProfileEntry {
                mod_id: package.mod_id.clone(),
                version: package.version.clone(),
                source: Some(package.source.clone()),
                automatic: Some(entry) != plan.entries.last(),
                dependencies: manifest.dependencies,
                ..ProfileEntry::default()
            });
        }
        if pending.next().is_some() {
            return Err(changed());
        }
        for package in staged {
            let stored = packages::import_package(package, &self.store)?;
            if !self
                .store
                .read_entry(&stored.mod_id, &stored.version)
                .is_some_and(|m| m.source == package.source)
            {
                return Err(changed());
            }
        }
        after.active = true;
        let ops = profiles::plan_apply(&after, &self.links, &self.store, &game, &plan.plugins)?;
        if ops.len() != staged.len() {
            return Err(changed());
        }
        for (op, package) in ops.iter().zip(staged) {
            let ApplyOp::Deploy {
                target,
                mod_id,
                version,
            } = op
            else {
                return Err(changed());
            };
            if target != &package.target
                || mod_id != &package.mod_id
                || version != &package.version
                || self.links.get(target).is_some()
            {
                return Err(changed());
            }
        }
        profiles::check_deployment_targets(&self.links, &ops, None)?;
        let parent = plan.plugins.parent().ok_or_else(changed)?;
        let workspace = unique_download_dir(parent)?;
        let receipt = Receipt {
            before: self.profiles.load(&after.name),
            active: self.profiles.active(),
            after,
            plan: plan.clone(),
            store: self.store.root().to_owned(),
            workspace: workspace
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        };
        let result = (|| {
            for (i, entry) in receipt.additions().enumerate() {
                let (_, link) = self.deployment_copy(entry, &plan.plugins)?;
                let source = Path::new(&link.store_path);
                let dest = workspace.join(i.to_string());
                if link.files.is_empty() {
                    crate::links::copy_file(source, &dest)
                } else {
                    crate::links::copy_tree(source, &dest).map(|_| ())
                }
                .map_err(profiles::ProfileError::from)?;
            }
            crate::atomic::replace(&self.receipt_path(), &serde_json::to_vec(&receipt)?)
                .map_err(HubError::Io)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&workspace);
        }
        result?;
        Ok(receipt)
    }

    fn deployment_copy(
        &self,
        entry: &ProfileEntry,
        plugins: &Path,
    ) -> Result<(PathBuf, LinkEntry), HubError> {
        let payload = self
            .store
            .resolve(&entry.mod_id, &entry.version)
            .ok_or_else(changed)?;
        let manifest = self
            .store
            .read_entry(&entry.mod_id, &entry.version)
            .ok_or_else(changed)?;
        if entry.source.as_ref() != Some(&manifest.source) {
            return Err(changed());
        }
        crate::store::verify_payload(&payload, &manifest)
            .map_err(|path| plan_error(format!("Store payload changed: {}", path.display())))?;
        let layout = profiles::deployment(&manifest.files);
        let target = layout.target(&entry.mod_id, plugins);
        let (source, files) = match layout {
            Deployment::File(name) => (payload.join(name), Vec::new()),
            Deployment::Directory => (
                payload,
                manifest.files.into_iter().map(|f| f.path).collect(),
            ),
        };
        Ok((
            target,
            LinkEntry {
                mod_id: entry.mod_id.clone(),
                version: entry.version.clone(),
                kind: LinkKind::Copy,
                store_path: source.to_string_lossy().into_owned(),
                files,
            },
        ))
    }

    fn finish_receipt(&mut self, receipt: &Receipt) -> Result<(), HubError> {
        let plugins = self.plugins_dir()?;
        let game = self.game_folder().ok_or(HubError::NoGameFolder)?;
        if self.game_running()? {
            return Err(HubError::GameRunning);
        }
        if receipt.store != self.store.root()
            || receipt.plan.plugins != plugins
            || receipt.workspace.is_empty()
            || !receipt
                .workspace
                .bytes()
                .all(|c| c.is_ascii_digit() || c == b'-')
        {
            return Err(changed());
        }
        let workspace = plugins
            .parent()
            .ok_or_else(changed)?
            .join(&receipt.workspace);
        if !crate::paths::is_inside(&workspace, &game)
            || workspace
                .symlink_metadata()
                .is_ok_and(|m| crate::paths::is_link(&m))
        {
            return Err(changed());
        }
        let current = self.profiles.load(&receipt.after.name);
        let mut after = receipt.after.clone();
        after.active = self.profiles.active().as_deref() == Some(&after.name);
        if (current != receipt.before && current.as_ref() != Some(&after))
            || (self.profiles.active() != receipt.active && !after.active)
        {
            return Err(changed());
        }
        let satisfied = receipt
            .plan
            .entries
            .iter()
            .filter(|entry| entry.state == DependencyState::Satisfied)
            .map(|e| format!("{}-{}", e.package, e.version))
            .collect::<Vec<_>>();
        self.links = crate::links::LinkIndex::load(&self.app_data);
        self.check_thunderstore_dependencies(&satisfied, &receipt.after, &plugins)?;
        self.verify_local_reuse(receipt, &plugins)?;
        let mut links = self.links.clone();
        let mut publications = Vec::new();
        for (i, entry) in receipt.additions().enumerate() {
            let (target, link) = self.deployment_copy(entry, &plugins)?;
            links
                .check_target(&game, &target)
                .map_err(profiles::ProfileError::from)?;
            let copy = workspace.join(i.to_string());
            if copy.try_exists().map_err(HubError::Io)? {
                if links.get(&target).is_some() {
                    return Err(changed());
                }
                crate::links::verify_copy(&copy, &link).map_err(profiles::ProfileError::from)?;
                let name = target
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(changed)?;
                if let Some(occupied) = crate::paths::find_child(&plugins, name) {
                    return Err(profiles::ProfileError::OccupiedTarget(occupied).into());
                }
                publications.push((copy, target, link));
            } else {
                links
                    .record_copy(&target, link)
                    .map_err(profiles::ProfileError::from)?;
            }
        }
        for (copy, target, link) in publications {
            super::publish::publish(&copy, &target)
                .map_err(|e| plan_error(format!("{}: {e}", target.display())))?;
            links
                .record_copy(&target, link)
                .map_err(profiles::ProfileError::from)?;
        }
        links.save().map_err(HubError::Io)?;
        self.links = links;
        self.profiles.save_deployed(&receipt.after)?;
        match fs::remove_dir(workspace) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(HubError::Io(e)),
            _ => {}
        }
        fs::remove_file(self.receipt_path()).map_err(HubError::Io)
    }
}

impl Receipt {
    fn additions(&self) -> impl Iterator<Item = &ProfileEntry> {
        let count = self
            .plan
            .entries
            .iter()
            .filter(|entry| entry.state == DependencyState::Install)
            .count();
        self.after
            .entries
            .iter()
            .skip(self.after.entries.len().saturating_sub(count))
    }
}
