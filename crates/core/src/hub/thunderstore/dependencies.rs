//! Read-only expansion of exact dependencies from the Thunderstore catalogue.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::{
    BEPINEX_PACK, BEPINEX_PACK_VERSION, Hub, HubError, Package, Profile, SOURCE_PREFIX, packages,
    plan_error, profiles, satisfies_bepinex_pack, thunderstore,
};
use crate::profiles::ProfileEntry;

const MAX_DEPTH: usize = 128;
const MAX_PACKAGES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DependencyPlan {
    pub profile: String,
    pub plugins: std::path::PathBuf,
    pub entries: Vec<PlannedDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PlannedDependency {
    pub package: String,
    pub version: String,
    pub state: DependencyState,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DependencyState {
    Satisfied,
    ReuseLocal(std::path::PathBuf),
    LocalCandidate(LocalDependency),
    VerifiedLocal(LocalDependency),
    LocalConflict(String),
    Install,
    Unavailable,
    Cycle,
    VersionConflict(String),
    InstalledVersion(String),
    DeploymentUnavailable,
    RuntimeRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LocalDependency {
    pub path: std::path::PathBuf,
    pub version: String,
    pub sha256: String,
}

impl DependencyState {
    pub(super) fn local(&self) -> Option<&LocalDependency> {
        match self {
            Self::LocalCandidate(local) | Self::VerifiedLocal(local) => Some(local),
            _ => None,
        }
    }

    pub(super) fn satisfied(&self) -> bool {
        matches!(self, Self::Satisfied | Self::VerifiedLocal(_))
    }
}

impl DependencyPlan {
    pub fn requires_local_confirmation(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| matches!(entry.state, DependencyState::LocalCandidate(_)))
    }

    pub fn installable(&self) -> bool {
        self.entries
            .last()
            .is_some_and(|entry| entry.state == DependencyState::Install)
            && self.entries.iter().all(|entry| {
                entry.state == DependencyState::Install
                    || entry.state.satisfied()
                    || matches!(entry.state, DependencyState::LocalCandidate(_))
            })
    }

    pub fn ready_for_single_install(&self) -> bool {
        let Some((target, dependencies)) = self.entries.split_last() else {
            return false;
        };
        target.state == DependencyState::Install
            && dependencies
                .iter()
                .all(|entry| entry.state == DependencyState::Satisfied)
    }
}

impl Hub {
    pub fn plan_thunderstore_dependencies(
        &self,
        package: &str,
        version: &str,
    ) -> Result<DependencyPlan, HubError> {
        let catalogue = self
            .thunderstore_catalogue()
            .cached()
            .map_err(HubError::Io)?;
        let plugins = self.plugins_dir()?;
        let profile = packages::active_or_default(&self.profiles, &self.links)?;
        let runtime_ready = satisfies_bepinex_pack(&self.bepinex_status()?);
        let mut resolver = Resolver {
            catalogue: catalogue
                .iter()
                .map(|item| (item.full_name.to_ascii_lowercase(), item))
                .collect(),
            selected: BTreeMap::new(),
            visiting: BTreeSet::new(),
            reported_conflicts: BTreeSet::new(),
            entries: Vec::new(),
            hub: self,
            profile: &profile,
            plugins: &plugins,
            runtime_ready,
        };
        resolver.visit(package, version)?;
        let entries = resolver.entries;
        Ok(DependencyPlan {
            profile: profile.name,
            plugins,
            entries,
        })
    }

    pub(super) fn installed_thunderstore_dependency(
        &self,
        package: &str,
        version: &str,
        profile: &Profile,
        plugins: &Path,
    ) -> Option<DependencyState> {
        let mod_id = format!("thunderstore.{}", package.to_ascii_lowercase());
        let entry = profile.entries.iter().find(|entry| {
            self.thunderstore_entry_matches(entry, package)
                || entry.mod_id.eq_ignore_ascii_case(&mod_id)
        });
        Some(match entry {
            Some(entry) if entry.version != version => {
                DependencyState::InstalledVersion(entry.version.clone())
            }
            Some(entry)
                if self.thunderstore_entry_matches(entry, package)
                    && self.thunderstore_entry_deployed(entry, plugins) =>
            {
                DependencyState::Satisfied
            }
            Some(_) => DependencyState::DeploymentUnavailable,
            None => return self.local_thunderstore_dependency(package, version, profile, plugins),
        })
    }

    pub(super) fn thunderstore_entry_matches(&self, entry: &ProfileEntry, id: &str) -> bool {
        entry
            .source
            .as_deref()
            .and_then(|source| source.strip_prefix(SOURCE_PREFIX))
            .is_some_and(|source| source.eq_ignore_ascii_case(id))
    }

    pub(super) fn thunderstore_entry_deployed(&self, entry: &ProfileEntry, plugins: &Path) -> bool {
        entry.enabled
            && profiles::entry_target(entry, &self.store, plugins).is_some_and(|target| {
                self.links.get(&target).is_some_and(|link| {
                    link.mod_id == entry.mod_id && link.version == entry.version
                }) && self
                    .links
                    .target_current(&target)
                    .is_ok_and(|current| current)
            })
    }
}

struct Resolver<'a> {
    catalogue: BTreeMap<String, &'a Package>,
    selected: BTreeMap<String, String>,
    visiting: BTreeSet<String>,
    reported_conflicts: BTreeSet<(String, String)>,
    entries: Vec<PlannedDependency>,
    hub: &'a Hub,
    profile: &'a Profile,
    plugins: &'a Path,
    runtime_ready: bool,
}

impl Resolver<'_> {
    fn visit(&mut self, package: &str, version: &str) -> Result<(), HubError> {
        let key = package.to_ascii_lowercase();
        if let Some(previous) = self.selected.get(&key) {
            let conflict = if previous != version {
                Some(DependencyState::VersionConflict(previous.clone()))
            } else if self.visiting.contains(&key) {
                Some(DependencyState::Cycle)
            } else {
                None
            };
            if let Some(state) = conflict
                && self.reported_conflicts.insert((key, version.to_owned()))
            {
                self.push(package, version, state);
            }
            return Ok(());
        }
        if self.visiting.len() >= MAX_DEPTH || self.selected.len() >= MAX_PACKAGES {
            return Err(plan_error(format!(
                "Thunderstore dependency graph limit exceeded at {package}@{version}"
            )));
        }
        self.selected.insert(key.clone(), version.to_owned());
        if package.eq_ignore_ascii_case(BEPINEX_PACK) && version == BEPINEX_PACK_VERSION {
            self.push(
                package,
                version,
                if self.runtime_ready {
                    DependencyState::Satisfied
                } else {
                    DependencyState::RuntimeRequired
                },
            );
            return Ok(());
        }
        let release = self.catalogue.get(&key).copied().and_then(|item| {
            item.versions
                .iter()
                .find(|item| item.version_number == version)
        });
        let state = self.state(package, version, release.is_some());
        if let Some(release) = release {
            self.visiting.insert(key.clone());
            for dependency in &release.dependencies {
                let (id, number) = thunderstore::dependency(dependency).ok_or_else(|| {
                    plan_error(format!("invalid Thunderstore dependency: {dependency}"))
                })?;
                self.visit(id, number)?;
            }
            self.visiting.remove(&key);
        }
        self.push(package, version, state);
        Ok(())
    }

    fn state(&self, package: &str, version: &str, available: bool) -> DependencyState {
        self.hub
            .installed_thunderstore_dependency(package, version, self.profile, self.plugins)
            .unwrap_or(if available {
                DependencyState::Install
            } else {
                DependencyState::Unavailable
            })
    }

    fn push(&mut self, package: &str, version: &str, state: DependencyState) {
        self.entries.push(PlannedDependency {
            package: package.to_owned(),
            version: version.to_owned(),
            state,
        });
    }
}
