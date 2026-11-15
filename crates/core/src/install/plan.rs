//! Builds a safe file plan from a staged archive. No writes.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::paths;

use super::archive::{StagedArchive, StagedFile};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    BepInEx,
    Mod,
    Dll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAction {
    Create,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub staged: PathBuf,
    pub target: PathBuf,
    pub action: FileAction,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradePlan {
    pub layout: Layout,
    pub target_root: PathBuf,
    pub files: Vec<PlannedFile>,
    pub conflicts: Vec<PathBuf>,
    pub warnings: Vec<String>,
}

impl UpgradePlan {
    pub fn can_apply(&self) -> bool {
        self.conflicts.is_empty() && !self.files.is_empty()
    }
}

pub struct PlanTarget<'a> {
    pub root: &'a Path,
    pub plugins: &'a Path,
    pub owned_root: &'a Path,
    pub owned_files: &'a [PathBuf],
}

#[derive(Debug)]
pub enum PlanError {
    NoPayload,
    SingleDllOnly,
    NotDll(String),
    NoMatchingDll(String),
    AmbiguousDll(String),
    UnsafePath(String),
    Outside(PathBuf),
    Duplicate(PathBuf),
    Io { path: PathBuf, source: io::Error },
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPayload => formatter.write_str("archive has no installable files"),
            Self::SingleDllOnly => {
                formatter.write_str("a bare DLL update must contain exactly one file")
            }
            Self::NotDll(name) => write!(formatter, "{name} is not a DLL"),
            Self::NoMatchingDll(name) => {
                write!(
                    formatter,
                    "release DLL {name} does not match an installed DLL"
                )
            }
            Self::AmbiguousDll(name) => {
                write!(
                    formatter,
                    "release DLL {name} matches more than one installed DLL"
                )
            }
            Self::UnsafePath(path) => write!(formatter, "archive path is not safe: {path}"),
            Self::Outside(path) => write!(
                formatter,
                "{} would be written outside the target",
                path.display()
            ),
            Self::Duplicate(path) => {
                write!(formatter, "archive has duplicate path {}", path.display())
            }
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for PlanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub fn plan_upgrade(
    staged: &StagedArchive,
    target: PlanTarget<'_>,
) -> Result<UpgradePlan, PlanError> {
    let usable: Vec<&StagedFile> = staged
        .files
        .iter()
        .filter(|file| !is_skipped(&file.relative))
        .collect();
    if usable.is_empty() {
        return Err(PlanError::NoPayload);
    }
    let layout = detect_layout(&usable);
    let plugins_layout = layout == Layout::Mod && has_plugins_prefix(&usable);
    let target_root = if layout == Layout::BepInEx || plugins_layout {
        target.plugins.to_path_buf()
    } else {
        target.root.to_path_buf()
    };
    let wrapper = if layout == Layout::Mod && !plugins_layout {
        common_root(&usable)
    } else {
        None
    };
    let mut files = Vec::new();
    let mut conflicts = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut ignored = 0;
    for file in &usable {
        let Some(relative) = relative_for(file, layout, wrapper.as_deref(), plugins_layout) else {
            ignored += 1;
            continue;
        };
        if relative.as_os_str().is_empty() {
            ignored += 1;
            continue;
        }
        let target_path = target_root.join(&relative);
        if !paths::is_inside(&target_path, &target_root) {
            return Err(PlanError::Outside(target_path));
        }
        let key = target_path.to_string_lossy().to_lowercase();
        if !seen.insert(key) {
            return Err(PlanError::Duplicate(target_path));
        }
        let action = match fs::symlink_metadata(&target_path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || metadata.is_dir()
                    || !is_owned(&target_path, &target)
                {
                    conflicts.insert(target_path.clone());
                }
                FileAction::Replace
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if parent_is_blocked(&target_path, &target_root)? {
                    conflicts.insert(target_path.clone());
                }
                FileAction::Create
            }
            Err(source) => {
                return Err(PlanError::Io {
                    path: target_path,
                    source,
                });
            }
        };
        files.push(PlannedFile {
            staged: file.path.clone(),
            target: target_path,
            action,
            size: file.size,
        });
    }
    if files.is_empty() {
        return Err(PlanError::NoPayload);
    }
    let mut warnings = Vec::new();
    if ignored > 0 {
        warnings.push(format!(
            "{ignored} archive file(s) outside the install root were ignored."
        ));
    }
    if !conflicts.is_empty() {
        warnings.push(format!(
            "{} existing file(s) belong to another mod or BepInEx and will not be overwritten.",
            conflicts.len()
        ));
    }
    Ok(UpgradePlan {
        layout,
        target_root,
        files,
        conflicts: conflicts.into_iter().collect(),
        warnings,
    })
}

pub fn plan_payload(
    staged: &StagedArchive,
    target: PlanTarget<'_>,
) -> Result<UpgradePlan, PlanError> {
    let usable: Vec<&StagedFile> = staged
        .files
        .iter()
        .filter(|file| !is_skipped(&file.relative))
        .collect();
    let single_dll = usable.len() == 1
        && usable[0]
            .relative
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"));
    if single_dll {
        plan_bare_dll(&usable, target)
    } else {
        plan_upgrade(staged, target)
    }
}

fn plan_bare_dll(files: &[&StagedFile], target: PlanTarget<'_>) -> Result<UpgradePlan, PlanError> {
    let [file] = files else {
        return Err(PlanError::SingleDllOnly);
    };
    let name = file
        .relative
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| PlanError::UnsafePath(file.relative.display().to_string()))?;
    if !name.to_ascii_lowercase().ends_with(".dll") {
        return Err(PlanError::NotDll(name.to_string()));
    }
    let mut matches = target.owned_files.iter().filter(|owned| {
        owned
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|owned| owned.eq_ignore_ascii_case(name))
    });
    let (owned, renamed) = match (matches.next(), matches.next()) {
        (Some(owned), None) => (owned.clone(), false),
        (None, _) if target.owned_files.len() == 1 => (target.owned_files[0].clone(), true),
        (Some(_), Some(_)) => return Err(PlanError::AmbiguousDll(name.to_string())),
        (None, _) => return Err(PlanError::NoMatchingDll(name.to_string())),
    };
    if !paths::is_inside(&owned, target.root) {
        return Err(PlanError::Outside(owned));
    }
    let mut warnings = Vec::new();
    if renamed {
        warnings.push(format!(
            "release DLL {name} differs from the installed {}; replacing it because the mod has a single DLL.",
            owned.display()
        ));
    }
    let mut conflicts = Vec::new();
    let action = match fs::symlink_metadata(&owned) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            FileAction::Replace
        }
        Ok(_) => {
            conflicts.push(owned.clone());
            FileAction::Replace
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => FileAction::Create,
        Err(source) => {
            return Err(PlanError::Io {
                path: owned,
                source,
            });
        }
    };
    let target_root = owned
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| target.root.to_path_buf());
    Ok(UpgradePlan {
        layout: Layout::Dll,
        target_root,
        files: vec![PlannedFile {
            staged: file.path.clone(),
            target: owned,
            action,
            size: file.size,
        }],
        conflicts,
        warnings,
    })
}

fn detect_layout(files: &[&StagedFile]) -> Layout {
    if files
        .iter()
        .any(|file| has_component(&file.relative, "BepInEx"))
    {
        Layout::BepInEx
    } else {
        Layout::Mod
    }
}

fn has_plugins_prefix(files: &[&StagedFile]) -> bool {
    files.iter().any(|file| {
        first_component(&file.relative).is_some_and(|name| name.eq_ignore_ascii_case("plugins"))
    })
}

fn has_component(path: &Path, wanted: &str) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(wanted)
    })
}

fn first_component(path: &Path) -> Option<String> {
    path.components()
        .next()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
}

fn common_root(files: &[&StagedFile]) -> Option<String> {
    let first = first_component(&files.first()?.relative)?;
    let all_share = files.iter().all(|file| {
        file.relative.components().count() > 1
            && first_component(&file.relative).is_some_and(|name| name == first)
    });
    all_share.then_some(first)
}

fn relative_for(
    file: &StagedFile,
    layout: Layout,
    wrapper: Option<&str>,
    plugins_layout: bool,
) -> Option<PathBuf> {
    match layout {
        Layout::BepInEx => strip_after_plugins(&file.relative),
        Layout::Mod if plugins_layout => {
            let has_prefix = first_component(&file.relative)
                .is_some_and(|name| name.eq_ignore_ascii_case("plugins"));
            has_prefix.then(|| strip_first(&file.relative)).flatten()
        }
        Layout::Dll => None,
        Layout::Mod => match wrapper {
            Some(wrapper) if first_component(&file.relative).as_deref() == Some(wrapper) => {
                strip_first(&file.relative)
            }
            _ => Some(file.relative.clone()),
        },
    }
}

fn strip_after_plugins(path: &Path) -> Option<PathBuf> {
    let components: Vec<Component<'_>> = path.components().collect();
    let bepinex = components.iter().position(|component| {
        component
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("BepInEx")
    })?;
    let plugins = components.get(bepinex + 1)?;
    if !plugins
        .as_os_str()
        .to_string_lossy()
        .eq_ignore_ascii_case("plugins")
    {
        return None;
    }
    collect_after(&components, bepinex + 2)
}

fn strip_first(path: &Path) -> Option<PathBuf> {
    let components: Vec<Component<'_>> = path.components().collect();
    collect_after(&components, 1)
}

fn collect_after(components: &[Component<'_>], start: usize) -> Option<PathBuf> {
    if start >= components.len() {
        return None;
    }
    let mut relative = PathBuf::new();
    for component in &components[start..] {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

fn is_skipped(path: &Path) -> bool {
    if path.components().any(|component| {
        component
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case("__MACOSX")
    }) {
        return true;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    if name.eq_ignore_ascii_case(".DS_Store")
        || name.eq_ignore_ascii_case("Thumbs.db")
        || name.eq_ignore_ascii_case("desktop.ini")
    {
        return true;
    }
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
}

fn is_owned(target: &Path, plan_target: &PlanTarget<'_>) -> bool {
    if plan_target
        .owned_files
        .iter()
        .any(|file| paths::is_inside(target, file))
    {
        return true;
    }
    plan_target.owned_root.is_dir() && paths::is_inside(target, plan_target.owned_root)
}

fn parent_is_blocked(target: &Path, root: &Path) -> Result<bool, PlanError> {
    let mut current = target.parent();
    while let Some(directory) = current {
        if !paths::is_inside(directory, root) {
            return Ok(true);
        }
        match fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.is_dir() => return Ok(false),
            Ok(_) => return Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if directory == root {
                    return Ok(false);
                }
                current = directory.parent();
            }
            Err(source) => {
                return Err(PlanError::Io {
                    path: directory.to_path_buf(),
                    source,
                });
            }
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::test_support::temp_dir;

    use super::*;
    use crate::install::archive::StagedArchive;

    fn staged(root: &Path, names: &[&str]) -> StagedArchive {
        let files = names
            .iter()
            .map(|name| {
                let path = root.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, b"x").unwrap();
                StagedFile {
                    relative: PathBuf::from(name),
                    path,
                    size: 1,
                }
            })
            .collect();
        StagedArchive {
            staging_root: root.to_path_buf(),
            files,
        }
    }

    fn target<'a>(root: &'a Path, plugins: &'a Path, owned: &'a Path) -> PlanTarget<'a> {
        PlanTarget {
            root,
            plugins,
            owned_root: owned,
            owned_files: &[],
        }
    }

    #[test]
    fn bepinex_layout_strips_wrapper_and_keeps_plugins() {
        let dir = temp_dir("plan-bepinex");
        let staging = staged(&dir, &["Pack/BepInEx/plugins/Mod/Mod.dll"]);
        let game = dir.join("game");
        let plugins = game.join("BepInEx/plugins");
        fs::create_dir_all(&plugins).unwrap();
        let plan = plan_upgrade(&staging, target(&plugins, &plugins, &plugins)).unwrap();
        assert_eq!(plan.layout, Layout::BepInEx);
        assert_eq!(plan.files[0].target, plugins.join("Mod/Mod.dll"));
        assert!(plan.can_apply());
    }

    #[test]
    fn a_new_file_is_allowed_but_another_mods_file_is_not_overwritten() {
        let dir = temp_dir("plan-ownership");
        let staging = staged(&dir, &["Mod.dll", "Extra.dll"]);
        let plugins = dir.join("plugins");
        let root = plugins.join("Mod");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Mod.dll"), b"old").unwrap();
        let plan = plan_upgrade(&staging, target(&root, &plugins, &root)).unwrap();
        assert_eq!(plan.files[0].target, root.join("Mod.dll"));
        assert_eq!(plan.files[0].action, FileAction::Replace);
        assert_eq!(plan.files[1].target, root.join("Extra.dll"));
        assert_eq!(plan.files[1].action, FileAction::Create);
        assert!(plan.can_apply());
    }

    #[test]
    fn overwriting_an_unowned_file_is_a_conflict() {
        let dir = temp_dir("plan-conflict");
        let staging = staged(&dir, &["Other.dll"]);
        let plugins = dir.join("plugins");
        let root = plugins.join("Mod");
        fs::create_dir_all(&root).unwrap();
        fs::write(plugins.join("Other.dll"), b"other").unwrap();
        let plan = plan_upgrade(&staging, target(&plugins, &plugins, &root)).unwrap();
        assert_eq!(plan.conflicts, vec![plugins.join("Other.dll")]);
        assert!(!plan.can_apply());
    }

    #[test]
    fn text_files_are_skipped() {
        let dir = temp_dir("plan-text");
        let staging = staged(&dir, &["README.txt", "Mod.dll"]);
        let root = dir.join("plugins/Mod");
        fs::create_dir_all(&root).unwrap();
        let plan = plan_upgrade(&staging, target(&root, &root, &root)).unwrap();
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].target, root.join("Mod.dll"));
    }

    #[test]
    fn bare_dll_replaces_the_matching_owned_file() {
        let dir = temp_dir("plan-dll");
        let staging = staged(&dir, &["Mod.dll"]);
        let plugins = dir.join("plugins");
        let root = plugins.join("Mod");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Mod.dll"), b"old").unwrap();
        let owned = vec![root.join("Mod.dll")];
        let plan = plan_payload(
            &staging,
            PlanTarget {
                root: &root,
                plugins: &plugins,
                owned_root: &root,
                owned_files: &owned,
            },
        )
        .unwrap();
        assert_eq!(plan.layout, Layout::Dll);
        assert_eq!(plan.files[0].target, root.join("Mod.dll"));
        assert_eq!(plan.files[0].action, FileAction::Replace);
        assert!(plan.can_apply());
    }

    #[test]
    fn bare_dll_replaces_the_only_owned_file_when_the_name_differs() {
        let dir = temp_dir("plan-dll-renamed");
        let staging = staged(&dir, &["QoL.Unknown.dll"]);
        let plugins = dir.join("plugins");
        let root = plugins.join("Mod");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("QoL Unknown.dll"), b"old").unwrap();
        let owned = vec![root.join("QoL Unknown.dll")];
        let plan = plan_payload(
            &staging,
            PlanTarget {
                root: &root,
                plugins: &plugins,
                owned_root: &root,
                owned_files: &owned,
            },
        )
        .unwrap();
        assert_eq!(plan.files[0].target, root.join("QoL Unknown.dll"));
        assert_eq!(plan.files[0].action, FileAction::Replace);
        assert_eq!(plan.warnings.len(), 1);
        assert!(plan.warnings[0].contains("single DLL"));
        assert!(plan.can_apply());
    }

    #[test]
    fn bare_dll_with_a_new_name_still_fails_when_several_dlls_are_owned() {
        let dir = temp_dir("plan-dll-mismatch");
        let staging = staged(&dir, &["Other.dll"]);
        let plugins = dir.join("plugins");
        let root = plugins.join("Mod");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Mod.dll"), b"old").unwrap();
        fs::write(root.join("Extra.dll"), b"old").unwrap();
        let owned = vec![root.join("Mod.dll"), root.join("Extra.dll")];
        let error = plan_payload(
            &staging,
            PlanTarget {
                root: &root,
                plugins: &plugins,
                owned_root: &root,
                owned_files: &owned,
            },
        )
        .unwrap_err();
        assert!(matches!(error, PlanError::NoMatchingDll(_)));
    }

    #[test]
    fn bare_dll_prefers_a_matching_name_among_several_owned_files() {
        let dir = temp_dir("plan-dll-pick");
        let staging = staged(&dir, &["Extra.dll"]);
        let plugins = dir.join("plugins");
        let root = plugins.join("Mod");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Mod.dll"), b"old").unwrap();
        fs::write(root.join("Extra.dll"), b"old").unwrap();
        let owned = vec![root.join("Mod.dll"), root.join("Extra.dll")];
        let plan = plan_payload(
            &staging,
            PlanTarget {
                root: &root,
                plugins: &plugins,
                owned_root: &root,
                owned_files: &owned,
            },
        )
        .unwrap();
        assert_eq!(plan.files[0].target, root.join("Extra.dll"));
        assert!(plan.warnings.is_empty());
    }

    #[test]
    fn a_single_dll_archive_targets_the_owned_path() {
        let dir = temp_dir("plan-dll-archive");
        let staging = staged(&dir, &["Pack/BepInEx/plugins/Mod/Mod.dll"]);
        let game = dir.join("game");
        let plugins = game.join("BepInEx/plugins");
        let root = plugins.join("Mod");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Mod.dll"), b"old").unwrap();
        let owned = vec![root.join("Mod.dll")];
        let plan = plan_payload(
            &staging,
            PlanTarget {
                root: &root,
                plugins: &plugins,
                owned_root: &root,
                owned_files: &owned,
            },
        )
        .unwrap();
        assert_eq!(plan.files[0].target, root.join("Mod.dll"));
        assert_eq!(plan.files[0].action, FileAction::Replace);
    }
}
