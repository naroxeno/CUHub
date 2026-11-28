//! Explicit equivalence between the Thunderstore CUCoreLib package and its BepInEx plugin.

use std::path::{Path, PathBuf};

use super::{DependencyState, Hub, Profile, dependencies::LocalDependency, profiles};
use crate::{mods, paths, store};

const CUCORELIB_PACKAGE: &str = "Jimmyking-CUCoreLib";
const CUCORELIB_GUID: &str = "net.cucorelib";

impl Hub {
    pub(super) fn local_thunderstore_dependency(
        &self,
        package: &str,
        version: &str,
        profile: &Profile,
        plugins: &Path,
    ) -> Option<DependencyState> {
        if !package.eq_ignore_ascii_case(CUCORELIB_PACKAGE) {
            return None;
        }
        let mut disabled = Vec::new();
        let mut candidates = Vec::new();
        let mut unverified_deployment = Vec::new();
        for path in mods::collect_dlls(plugins) {
            let evidence = mods::read_dll_evidence(&path);
            let plugin_version = evidence.as_ref().filter(|dll| {
                dll.plugin_guid
                    .as_deref()
                    .is_some_and(|guid| guid.eq_ignore_ascii_case(CUCORELIB_GUID))
            });
            let named = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.eq_ignore_ascii_case("CUCoreLib.dll")
                        || name.eq_ignore_ascii_case("CUCoreLib.dll.disabled")
                });
            if plugin_version.is_none() && !named {
                continue;
            }
            let managed = profile.entries.iter().find(|entry| {
                profiles::entry_target(entry, &self.store, plugins).is_some_and(|target| {
                    paths::same_target(&path, &target) || path.starts_with(&target)
                })
            });
            if mods::is_disabled_path(&path) || managed.is_some_and(|entry| !entry.enabled) {
                disabled.push(path);
            } else if managed.is_some_and(|entry| !self.thunderstore_entry_deployed(entry, plugins))
            {
                unverified_deployment.push(path);
            } else {
                candidates.push((
                    path,
                    plugin_version.and_then(|dll| dll.plugin_version.clone()),
                ));
            }
        }
        if !unverified_deployment.is_empty() {
            return Some(conflict(
                "Cannot verify the managed CUCoreLib deployment",
                unverified_deployment,
            ));
        }
        match candidates.as_slice() {
            [] if disabled.is_empty() => None,
            [] => Some(conflict(
                "Only disabled CUCoreLib files were found",
                disabled,
            )),
            [(path, Some(installed))] if matching_version(installed, version) => {
                Some(self.identify_local_dependency(path, installed, version, plugins))
            }
            [(path, Some(installed))] => Some(conflict(
                &format!("CUCoreLib plugin version {installed} does not match {version}"),
                [path.clone()],
            )),
            [(path, None)] => Some(conflict(
                "Cannot verify the CUCoreLib plugin GUID and version",
                [path.clone()],
            )),
            _ => Some(conflict(
                "Multiple enabled CUCoreLib candidates were found",
                candidates.into_iter().map(|(path, _)| path),
            )),
        }
    }

    fn identify_local_dependency(
        &self,
        path: &Path,
        installed: &str,
        version: &str,
        plugins: &Path,
    ) -> DependencyState {
        let Ok(sha256) = store::hash_file(path) else {
            return conflict(
                "Cannot read the CUCoreLib DLL for verification",
                [path.to_owned()],
            );
        };
        let local = LocalDependency {
            path: path.to_owned(),
            version: installed.into(),
            sha256,
        };
        if self.matches_cached_cucorelib(&local, version, plugins) {
            DependencyState::VerifiedLocal(local)
        } else {
            DependencyState::LocalCandidate(local)
        }
    }

    fn matches_cached_cucorelib(
        &self,
        local: &LocalDependency,
        version: &str,
        plugins: &Path,
    ) -> bool {
        const MOD_ID: &str = "thunderstore.jimmyking-cucorelib";
        const SOURCE: &str = "thunderstore:Jimmyking-CUCoreLib";
        let Some(entry) = self.store.read_entry(MOD_ID, version) else {
            return false;
        };
        let Some(payload) = self.store.resolve(MOD_ID, version) else {
            return false;
        };
        if !entry.source.eq_ignore_ascii_case(SOURCE)
            || store::verify_payload(&payload, &entry).is_err()
        {
            return false;
        }
        let Ok(relative) = local.path.strip_prefix(plugins) else {
            return false;
        };
        let Some(first) = relative.components().next() else {
            return false;
        };
        let root = plugins.join(first.as_os_str());
        if root == local.path {
            return entry.files.len() == 1
                && entry
                    .files
                    .iter()
                    .any(|file| Path::new(&file.path) == relative && file.sha256 == local.sha256);
        }
        local.path.strip_prefix(&root).is_ok_and(|dll| {
            entry
                .files
                .iter()
                .any(|file| Path::new(&file.path) == dll && file.sha256 == local.sha256)
        }) && store::verify_payload(&root, &entry).is_ok()
    }
}

fn matching_version(installed: &str, expected: &str) -> bool {
    installed == expected || (installed == "1.1" && expected == "1.1.0")
}

fn conflict(reason: &str, paths: impl IntoIterator<Item = PathBuf>) -> DependencyState {
    let files = paths
        .into_iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    DependencyState::LocalConflict(format!("{reason} ({})", files.join(", ")))
}
