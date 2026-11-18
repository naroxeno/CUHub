//! Maps a staged archive to a single deployable package payload. No writes.

use std::collections::BTreeSet;
use std::path::{Component, PathBuf};

use super::archive::{StagedArchive, StagedFile, is_dll_file};
use super::plan::{
    Layout, PlanError, common_root, detect_layout, has_plugins_prefix, relative_for, usable_files,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageFile {
    pub staged: PathBuf,
    pub relative: PathBuf,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackagePayload {
    pub files: Vec<PackageFile>,
    pub single: bool,
}

pub fn plan_package(staged: &StagedArchive) -> Result<PackagePayload, PlanError> {
    let usable = usable_files(staged);
    if usable.is_empty() {
        return Err(PlanError::NoPayload);
    }
    match detect_layout(&usable) {
        Layout::Dll => {
            let file = usable[0];
            let relative = file
                .relative
                .file_name()
                .map(PathBuf::from)
                .ok_or_else(|| PlanError::UnsafePath(file.relative.display().to_string()))?;
            Ok(PackagePayload {
                files: vec![package_file(file, relative)],
                single: true,
            })
        }
        layout => {
            let plugins_layout = layout == Layout::Mod && has_plugins_prefix(&usable);
            let wrapper = if layout == Layout::Mod && !plugins_layout {
                common_root(&usable)
            } else {
                None
            };
            let mut files = Vec::new();
            let mut seen = BTreeSet::new();
            for file in &usable {
                let Some(relative) = relative_for(file, layout, wrapper.as_deref(), plugins_layout)
                else {
                    continue;
                };
                if relative.as_os_str().is_empty() || !is_plain_relative(&relative) {
                    return Err(PlanError::UnsafePath(relative.display().to_string()));
                }
                if !seen.insert(relative.to_string_lossy().to_lowercase()) {
                    return Err(PlanError::Duplicate(relative));
                }
                files.push(package_file(file, relative));
            }
            if files.is_empty() {
                return Err(PlanError::NoPayload);
            }
            let single = files.len() == 1
                && files[0].relative.components().count() == 1
                && is_dll_file(&files[0].relative);
            Ok(PackagePayload { files, single })
        }
    }
}

fn package_file(file: &StagedFile, relative: PathBuf) -> PackageFile {
    PackageFile {
        staged: file.path.clone(),
        relative,
        size: file.size,
    }
}

fn is_plain_relative(path: &std::path::Path) -> bool {
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::plan_package;
    use crate::install::archive::{StagedArchive, StagedFile};

    fn archive(paths: &[&str]) -> StagedArchive {
        StagedArchive {
            staging_root: PathBuf::from("/tmp/staging"),
            files: paths
                .iter()
                .map(|path| StagedFile {
                    relative: PathBuf::from(path),
                    path: PathBuf::from("/tmp/staging").join(path),
                    size: 1,
                })
                .collect(),
        }
    }

    #[test]
    fn maps_bepinex_plugin_archives_to_their_plugins_folder() {
        let payload = plan_package(&archive(&[
            "BepInEx/core/BepInEx.dll",
            "BepInEx/plugins/QoL/QoL.dll",
        ]))
        .unwrap();

        assert_eq!(payload.files.len(), 1);
        assert_eq!(payload.files[0].relative, Path::new("QoL/QoL.dll"));
        assert!(!payload.single);
    }

    #[test]
    fn maps_archives_with_a_plugins_prefix() {
        let payload = plan_package(&archive(&["plugins/QoL/QoL.dll", "readme.txt"])).unwrap();

        assert_eq!(payload.files.len(), 1);
        assert_eq!(payload.files[0].relative, Path::new("QoL/QoL.dll"));
        assert!(!payload.single);
    }
}
