//! Bridge package rules: the fixed Plugins/CuhubBridge target and the two-DLL payload check. No writes.

use std::path::{Path, PathBuf};

use crate::install::PackagePayload;

pub const BRIDGE_DIR: &str = "CuhubBridge";
pub const PLUGIN_FILE: &str = "CuhubBridge.Plugin.dll";
pub const CORE_FILE: &str = "CuhubBridge.Core.dll";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgePayload {
    pub plugin: PathBuf,
    pub core: PathBuf,
}

pub fn target_dir(plugins: &Path) -> PathBuf {
    plugins.join(BRIDGE_DIR)
}

pub fn installed_plugin(target: &Path) -> Option<PathBuf> {
    for suffix in ["", ".disabled"] {
        let plugin = target.join(format!("{PLUGIN_FILE}{suffix}"));
        let core = target.join(format!("{CORE_FILE}{suffix}"));
        if plugin.is_file() && core.is_file() {
            return Some(plugin);
        }
    }
    None
}

pub fn validate_payload(payload: &PackagePayload) -> Result<BridgePayload, String> {
    let mut plugin = None;
    let mut core = None;
    for file in &payload.files {
        let components = file.relative.components().count();
        let name = file
            .relative
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                format!(
                    "The bridge package contains a file name that is not valid UTF-8: {}",
                    file.relative.display()
                )
            })?;
        if components != 1 {
            return Err(format!(
                "The bridge package nests {name} in a folder; put both DLLs at the package root."
            ));
        }
        if name == PLUGIN_FILE {
            if plugin.replace(file.staged.clone()).is_some() {
                return Err(format!(
                    "The bridge package contains {PLUGIN_FILE} more than once."
                ));
            }
        } else if name == CORE_FILE {
            if core.replace(file.staged.clone()).is_some() {
                return Err(format!(
                    "The bridge package contains {CORE_FILE} more than once."
                ));
            }
        } else {
            return Err(format!(
                "The bridge package contains an unexpected file: {}",
                file.relative.display()
            ));
        }
    }
    if payload.files.len() != 2 {
        return Err(format!(
            "The bridge package must contain exactly {PLUGIN_FILE} and {CORE_FILE}; found {} file(s).",
            payload.files.len()
        ));
    }
    let Some(plugin) = plugin else {
        return Err(format!("The bridge package is missing {PLUGIN_FILE}."));
    };
    let Some(core) = core else {
        return Err(format!("The bridge package is missing {CORE_FILE}."));
    };
    Ok(BridgePayload { plugin, core })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::{PackageFile, PackagePayload};
    use std::path::PathBuf;

    fn package_file(name: &str) -> PackageFile {
        PackageFile {
            staged: PathBuf::from("/staging").join(name),
            relative: PathBuf::from(name),
            size: 1,
        }
    }

    fn payload(names: &[&str]) -> PackagePayload {
        PackagePayload {
            files: names.iter().map(|name| package_file(name)).collect(),
            single: names.len() == 1,
        }
    }

    #[test]
    fn target_is_fixed_under_plugins() {
        let target = target_dir(Path::new("/game/BepInEx/plugins"));
        assert_eq!(target, PathBuf::from("/game/BepInEx/plugins/CuhubBridge"));
    }

    #[test]
    fn accepts_the_bridge_pair_in_any_order() {
        let forward = validate_payload(&payload(&[PLUGIN_FILE, CORE_FILE])).unwrap();
        let reverse = validate_payload(&payload(&[CORE_FILE, PLUGIN_FILE])).unwrap();
        assert_eq!(forward.plugin, reverse.plugin);
        assert_eq!(forward.core, reverse.core);
        assert_eq!(forward.plugin, package_file(PLUGIN_FILE).staged);
        assert_eq!(forward.core, package_file(CORE_FILE).staged);
    }

    #[test]
    fn rejects_the_bridge_pair_with_the_wrong_case() {
        let error = validate_payload(&payload(&[
            "cuhubbridge.plugin.DLL",
            "CuhubBridge.core.dll",
        ]))
        .unwrap_err();
        assert!(error.contains("unexpected file"), "{error}");
    }

    #[test]
    fn rejects_a_single_dll() {
        let error = validate_payload(&payload(&[PLUGIN_FILE])).unwrap_err();
        assert!(error.contains("exactly"), "{error}");
    }

    #[test]
    fn rejects_a_missing_core_dll() {
        let error = validate_payload(&payload(&[PLUGIN_FILE, "Other.dll"])).unwrap_err();
        assert!(error.contains("Other.dll"), "{error}");
    }

    #[test]
    fn rejects_an_extra_file() {
        let error = validate_payload(&payload(&[PLUGIN_FILE, CORE_FILE, "README.md"])).unwrap_err();
        assert!(error.contains("README.md"), "{error}");
    }

    #[test]
    fn rejects_a_nested_dll() {
        let error =
            validate_payload(&payload(&["CuhubBridge/CuhubBridge.Plugin.dll"])).unwrap_err();
        assert!(error.contains("nests"), "{error}");
    }

    #[test]
    fn rejects_a_duplicate_dll() {
        let error = validate_payload(&payload(&[PLUGIN_FILE, PLUGIN_FILE, CORE_FILE])).unwrap_err();
        assert!(error.contains("more than once"), "{error}");
    }

    #[test]
    fn installed_plugin_finds_the_enabled_dll() {
        let root = std::env::temp_dir().join(format!(
            "cuhub-bridge-target-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(installed_plugin(&root), None);
        std::fs::write(root.join(PLUGIN_FILE), b"test").unwrap();
        assert_eq!(installed_plugin(&root), None);
        std::fs::write(root.join(CORE_FILE), b"test").unwrap();
        assert_eq!(installed_plugin(&root), Some(root.join(PLUGIN_FILE)));
        std::fs::rename(
            root.join(PLUGIN_FILE),
            root.join(format!("{PLUGIN_FILE}.disabled")),
        )
        .unwrap();
        assert_eq!(installed_plugin(&root), None);
        std::fs::rename(
            root.join(CORE_FILE),
            root.join(format!("{CORE_FILE}.disabled")),
        )
        .unwrap();
        assert_eq!(
            installed_plugin(&root),
            Some(root.join(format!("{PLUGIN_FILE}.disabled")))
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn payload_plans_both_dlls_inside_the_bridge_target() {
        let root = std::env::temp_dir().join(format!(
            "cuhub-bridge-plan-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        ));
        let staging = root.join("staging");
        std::fs::create_dir_all(staging.join("CuhubBridge")).unwrap();
        let mut files = Vec::new();
        for name in [PLUGIN_FILE, CORE_FILE] {
            let path = staging.join("CuhubBridge").join(name);
            std::fs::write(&path, b"test").unwrap();
            files.push(crate::install::StagedFile {
                relative: PathBuf::from("CuhubBridge").join(name),
                path,
                size: 4,
            });
        }
        let staged = crate::install::StagedArchive {
            staging_root: staging.clone(),
            files,
        };
        let payload = crate::install::plan_package(&staged).unwrap();
        validate_payload(&payload).unwrap();
        let plugins = root.join("BepInEx").join("plugins");
        let target = target_dir(&plugins);
        let plan = crate::install::plan_payload(
            &staged,
            crate::install::PlanTarget {
                root: &target,
                plugins: &plugins,
                owned_root: &target,
                owned_files: &[],
            },
        )
        .unwrap();
        assert_eq!(plan.files.len(), 2);
        for file in &plan.files {
            assert_eq!(file.target.parent(), Some(target.as_path()));
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn planned_files_apply_into_the_target_with_a_backup() {
        let root = std::env::temp_dir().join(format!(
            "cuhub-bridge-apply-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        ));
        let staging = root.join("staging");
        std::fs::create_dir_all(staging.join("CuhubBridge")).unwrap();
        let mut files = Vec::new();
        for (name, body) in [
            (PLUGIN_FILE, b"new-plugin".as_slice()),
            (CORE_FILE, b"new-core"),
        ] {
            let path = staging.join("CuhubBridge").join(name);
            std::fs::write(&path, body).unwrap();
            files.push(crate::install::StagedFile {
                relative: PathBuf::from("CuhubBridge").join(name),
                path,
                size: body.len() as u64,
            });
        }
        let staged = crate::install::StagedArchive {
            staging_root: staging.clone(),
            files,
        };
        let plugins = root.join("plugins");
        let target = target_dir(&plugins);
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join(PLUGIN_FILE), b"old-plugin").unwrap();
        let plan = crate::install::plan_payload(
            &staged,
            crate::install::PlanTarget {
                root: &target,
                plugins: &plugins,
                owned_root: &target,
                owned_files: &[],
            },
        )
        .unwrap();
        let report = crate::install::apply_upgrade(&plan, &root.join("backups")).unwrap();
        assert!(report.backup.is_some());
        assert_eq!(
            std::fs::read(target.join(PLUGIN_FILE)).unwrap(),
            b"new-plugin"
        );
        assert_eq!(std::fs::read(target.join(CORE_FILE)).unwrap(), b"new-core");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_wrapped_zip_installs_into_the_bridge_target() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "cuhub-bridge-zip-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let archive = root.join("CuhubBridge.zip");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        for name in [PLUGIN_FILE, CORE_FILE] {
            writer
                .start_file(
                    format!("CuhubBridge/{name}"),
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(b"MZbridge").unwrap();
        }
        writer.finish().unwrap();
        let staged = crate::install::stage_archive(&archive, &root.join("staging")).unwrap();
        let payload = crate::install::plan_package(&staged).unwrap();
        validate_payload(&payload).unwrap();
        let plugins = root.join("BepInEx").join("plugins");
        let target = target_dir(&plugins);
        let plan = crate::install::plan_payload(
            &staged,
            crate::install::PlanTarget {
                root: &target,
                plugins: &plugins,
                owned_root: &target,
                owned_files: &[],
            },
        )
        .unwrap();
        crate::install::apply_upgrade(&plan, &root.join("backups")).unwrap();
        assert_eq!(
            std::fs::read(target.join(PLUGIN_FILE)).unwrap(),
            b"MZbridge"
        );
        assert_eq!(std::fs::read(target.join(CORE_FILE)).unwrap(), b"MZbridge");
        let _ = std::fs::remove_dir_all(&root);
    }
}
