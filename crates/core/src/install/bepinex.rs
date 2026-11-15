//! Detect installed BepInEx and plan installs from pinned upstream releases. No UI, no writes.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::paths;

use super::archive::StagedArchive;
use super::plan::{FileAction, Layout, PlanError, PlannedFile, UpgradePlan};

pub const INCOMPATIBLE_FILE: &str = "GameAssembly.dll";

const ROOT_FILES: [&str; 4] = [
    ".doorstop_version",
    "doorstop_config.ini",
    "changelog.txt",
    "winhttp.dll",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Lts,
    Experimental,
}

impl Flavor {
    pub fn major(self) -> u8 {
        match self {
            Self::Lts => 5,
            Self::Experimental => 6,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Lts => "BepInEx 5 LTS",
            Self::Experimental => "BepInEx 6 (Mono)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Release {
    pub flavor: Flavor,
    pub version: &'static str,
    pub file_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
}

impl Release {
    pub fn label(&self) -> String {
        format!("{} {}", self.flavor.label(), self.version)
    }
}

pub const LTS: Release = Release {
    flavor: Flavor::Lts,
    version: "5.4.23.6",
    file_name: "BepInEx_win_x64_5.4.23.6.zip",
    url: "https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.6/BepInEx_win_x64_5.4.23.6.zip",
    sha256: "6c57a714a892efb4e9e1fef63d608a3391f30ec984031c465404e4ee381a0755",
};

pub const EXPERIMENTAL: Release = Release {
    flavor: Flavor::Experimental,
    version: "6.0.0-be.793",
    file_name: "BepInEx-Unity.Mono-win-x64-6.0.0-be.793+dbc82f0.zip",
    url: "https://builds.bepinex.dev/projects/bepinex_be/793/BepInEx-Unity.Mono-win-x64-6.0.0-be.793%2Bdbc82f0.zip",
    sha256: "22d06f2d74b5ed6949b8d591406bc91b4a3cf0d8c347d548316ece0095e08b43",
};

pub fn releases() -> [Release; 2] {
    [LTS, EXPERIMENTAL]
}

pub fn release_for(flavor: Flavor) -> Release {
    match flavor {
        Flavor::Lts => LTS,
        Flavor::Experimental => EXPERIMENTAL,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub root: PathBuf,
    pub installed: bool,
    pub complete: bool,
    pub major: Option<u8>,
    pub version: Option<String>,
    pub plugins: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub il2cpp: bool,
}

pub fn detect(game_root: &Path) -> Status {
    let bepinex = paths::find_child(game_root, "BepInEx");
    let core = bepinex
        .as_ref()
        .and_then(|directory| paths::find_child(directory, "core"));
    let mono = core
        .as_ref()
        .and_then(|directory| paths::find_child(directory, "BepInEx.dll"));
    let il2cpp = core
        .as_ref()
        .and_then(|directory| paths::find_child(directory, "BepInEx.Core.dll"));
    let loader = paths::find_child(game_root, "winhttp.dll");
    let doorstop = paths::find_child(game_root, "doorstop_config.ini");
    let core_dll = mono.clone().or_else(|| il2cpp.clone());
    let major = if mono.is_some() {
        Some(5)
    } else if il2cpp.is_some() {
        Some(6)
    } else {
        None
    };
    Status {
        root: game_root.to_path_buf(),
        installed: bepinex.is_some() || loader.is_some(),
        complete: core_dll.is_some() && loader.is_some() && doorstop.is_some(),
        major,
        version: core_dll.as_ref().and_then(|path| read_version(path)),
        plugins: bepinex
            .as_ref()
            .and_then(|directory| paths::find_child(directory, "plugins")),
        config: bepinex
            .as_ref()
            .and_then(|directory| paths::find_child(directory, "config")),
        il2cpp: paths::find_child(game_root, INCOMPATIBLE_FILE).is_some(),
    }
}

fn read_version(core_dll: &Path) -> Option<String> {
    crate::mods::read_assembly_version(core_dll)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Fresh,
    Repair,
}

pub fn plan_install(
    game_root: &Path,
    staged: &StagedArchive,
    mode: Mode,
) -> Result<UpgradePlan, PlanError> {
    if !game_root.is_dir() {
        return Err(PlanError::NotADirectory(game_root.to_path_buf()));
    }
    if paths::find_child(game_root, INCOMPATIBLE_FILE).is_some() {
        return Err(PlanError::Unsupported(format!(
            "{INCOMPATIBLE_FILE} is present, so this game uses IL2CPP. BepInEx 5 and BepInEx 6 Mono do not load there."
        )));
    }
    let mut files = Vec::new();
    let mut conflicts = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut ignored = 0usize;
    for file in &staged.files {
        let relative = safe_relative(&file.relative)?;
        if !allowed(&relative) {
            ignored += 1;
            continue;
        }
        let target = game_root.join(&relative);
        if !paths::is_inside(&target, game_root) {
            return Err(PlanError::Outside(target));
        }
        let key = relative.to_string_lossy().replace('\\', "/").to_lowercase();
        if !seen.insert(key) {
            return Err(PlanError::Duplicate(target));
        }
        let action = classify(&target, game_root, mode, &mut conflicts)?;
        files.push(PlannedFile {
            staged: file.path.clone(),
            target,
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
            "{ignored} archive file(s) that are not part of BepInEx were skipped."
        ));
    }
    if paths::find_child(game_root, "BepInEx").is_some() {
        if mode == Mode::Fresh {
            warnings.push(String::from(
                "BepInEx is already installed; use Repair to replace its files.",
            ));
        }
        if let Some(bepinex) = paths::find_child(game_root, "BepInEx")
            && (paths::find_child(&bepinex, "config").is_some()
                || paths::find_child(&bepinex, "plugins").is_some())
        {
            warnings.push(String::from(
                "Existing BepInEx/config and BepInEx/plugins are kept.",
            ));
        }
    }
    Ok(UpgradePlan {
        layout: Layout::BepInEx,
        target_root: game_root.to_path_buf(),
        files,
        conflicts: conflicts.into_iter().collect(),
        warnings,
    })
}

fn safe_relative(path: &Path) -> Result<PathBuf, PlanError> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            _ => return Err(PlanError::UnsafePath(path.display().to_string())),
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(PlanError::UnsafePath(path.display().to_string()));
    }
    Ok(normalized)
}

fn allowed(relative: &Path) -> bool {
    let text = relative
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    if !text.starts_with("bepinex/") {
        return ROOT_FILES.contains(&text.as_str());
    }
    let rest = &text["bepinex/".len()..];
    !(rest == "config"
        || rest == "plugins"
        || rest == "patchers"
        || rest.starts_with("config/")
        || rest.starts_with("plugins/")
        || rest.starts_with("patchers/"))
}

fn classify(
    target: &Path,
    root: &Path,
    mode: Mode,
    conflicts: &mut BTreeSet<PathBuf>,
) -> Result<FileAction, PlanError> {
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if mode == Mode::Fresh || metadata.file_type().is_symlink() || metadata.is_dir() {
                conflicts.insert(target.to_path_buf());
            }
            Ok(FileAction::Replace)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if parent_is_blocked(target, root) {
                conflicts.insert(target.to_path_buf());
            }
            Ok(FileAction::Create)
        }
        Err(source) => Err(PlanError::Io {
            path: target.to_path_buf(),
            source,
        }),
    }
}

fn parent_is_blocked(target: &Path, root: &Path) -> bool {
    let mut current = target.parent();
    while let Some(directory) = current {
        if directory == root || !paths::is_inside(directory, root) {
            break;
        }
        if let Ok(metadata) = fs::symlink_metadata(directory)
            && (!metadata.is_dir() || metadata.file_type().is_symlink())
        {
            return true;
        }
        current = directory.parent();
    }
    false
}

pub fn ensure_plugins(game_root: &Path) -> io::Result<PathBuf> {
    let bepinex = paths::resolve_child(game_root, "BepInEx");
    let plugins = paths::resolve_child(&bepinex, "plugins");
    fs::create_dir_all(&plugins)?;
    Ok(plugins)
}

#[derive(Debug)]
pub enum HashError {
    Read { path: PathBuf, source: io::Error },
    Mismatch { expected: String, actual: String },
}

impl std::fmt::Display for HashError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::Mismatch { expected, actual } => write!(
                formatter,
                "the download does not match the release checksum (expected {expected}, got {actual})"
            ),
        }
    }
}

impl std::error::Error for HashError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Mismatch { .. } => None,
        }
    }
}

pub fn verify_sha256(path: &Path, expected: &str) -> Result<(), HashError> {
    let mut input = fs::File::open(path).map_err(|source| HashError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|source| HashError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual == expected.to_ascii_lowercase() {
        Ok(())
    } else {
        Err(HashError::Mismatch {
            expected: expected.to_ascii_lowercase(),
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::archive::StagedFile;
    use crate::test_support::temp_dir;

    fn staged(entries: &[(&str, u64)]) -> StagedArchive {
        StagedArchive {
            staging_root: PathBuf::from("staging"),
            files: entries
                .iter()
                .map(|(relative, size)| StagedFile {
                    relative: PathBuf::from(relative),
                    path: PathBuf::from("staging").join(relative),
                    size: *size,
                })
                .collect(),
        }
    }

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, text).expect("write file");
    }

    #[test]
    fn releases_are_pinned_and_distinct() {
        let releases = releases();
        for release in releases {
            assert!(release.url.starts_with("https://"));
            assert_eq!(release.sha256.len(), 64);
            assert!(release.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        }
        assert_eq!(releases[0].flavor, Flavor::Lts);
        assert_eq!(releases[1].flavor, Flavor::Experimental);
        assert_ne!(releases[0].sha256, releases[1].sha256);
    }

    #[test]
    fn allowlist_keeps_core_and_refuses_user_data() {
        for allowed_path in [
            "BepInEx/core/BepInEx.dll",
            "BepInEx/core/BepInEx.Core.dll",
            "BepInEx/plugins",
            "winhttp.dll",
            ".doorstop_version",
            "doorstop_config.ini",
            "changelog.txt",
            "BepInEx\\core\\0Harmony.dll",
        ] {
            if allowed_path.ends_with("plugins") {
                continue;
            }
            assert!(
                allowed(Path::new(allowed_path)),
                "{allowed_path} should be allowed"
            );
        }
        for refused in [
            "BepInEx/config/BepInEx.cfg",
            "BepInEx/plugins/Mod.dll",
            "BepInEx/patchers/Patcher.dll",
            "BepInEx/patchers",
            "BepInEx/config",
            "readme.txt",
            "../winhttp.dll",
        ] {
            assert!(!allowed(Path::new(refused)), "{refused} should be refused");
        }
    }

    #[test]
    fn repair_replaces_owned_files_and_creates_missing_ones() {
        let dir = temp_dir("bepinex-repair");
        write(&dir.join("winhttp.dll"), "old");
        write(&dir.join("BepInEx/core/BepInEx.dll"), "old");
        let archive = staged(&[
            ("winhttp.dll", 3),
            ("BepInEx/core/BepInEx.dll", 3),
            ("BepInEx/core/0Harmony.dll", 3),
        ]);
        let plan = plan_install(&dir, &archive, Mode::Repair).expect("plan");
        assert_eq!(plan.layout, Layout::BepInEx);
        assert_eq!(plan.target_root, dir);
        assert!(plan.can_apply());
        assert_eq!(plan.files.len(), 3);
        let loader = plan
            .files
            .iter()
            .find(|file| file.target.ends_with("winhttp.dll"))
            .expect("loader");
        assert_eq!(loader.action, FileAction::Replace);
        let harmony = plan
            .files
            .iter()
            .find(|file| file.target.ends_with("0Harmony.dll"))
            .expect("harmony");
        assert_eq!(harmony.action, FileAction::Create);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn fresh_refuses_existing_files() {
        let dir = temp_dir("bepinex-fresh");
        write(&dir.join("winhttp.dll"), "old");
        let archive = staged(&[("winhttp.dll", 3), ("BepInEx/core/BepInEx.dll", 3)]);
        let plan = plan_install(&dir, &archive, Mode::Fresh).expect("plan");
        assert!(!plan.can_apply());
        assert_eq!(plan.conflicts, vec![dir.join("winhttp.dll")]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn il2cpp_games_are_refused() {
        let dir = temp_dir("bepinex-il2cpp");
        write(&dir.join("GameAssembly.dll"), "native");
        let archive = staged(&[("winhttp.dll", 3)]);
        assert!(matches!(
            plan_install(&dir, &archive, Mode::Fresh),
            Err(PlanError::Unsupported(_))
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn traversal_entries_are_refused() {
        let dir = temp_dir("bepinex-traversal");
        let archive = staged(&[("../winhttp.dll", 3)]);
        assert!(matches!(
            plan_install(&dir, &archive, Mode::Fresh),
            Err(PlanError::UnsafePath(_))
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn detect_reports_complete_install() {
        let dir = temp_dir("bepinex-detect");
        write(&dir.join("winhttp.dll"), "loader");
        write(&dir.join("doorstop_config.ini"), "config");
        write(&dir.join("BepInEx/core/BepInEx.dll"), "core");
        let status = detect(&dir);
        assert!(status.installed);
        assert!(status.complete);
        assert_eq!(status.major, Some(5));
        assert!(status.plugins.is_none());
        assert!(!status.il2cpp);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn checksum_detects_tampering() {
        let dir = temp_dir("bepinex-hash");
        let file = dir.join("asset.zip");
        write(&file, "abc");
        assert!(
            verify_sha256(
                &file,
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            )
            .is_ok()
        );
        assert!(matches!(
            verify_sha256(&file, &"0".repeat(64)),
            Err(HashError::Mismatch { .. })
        ));
        let _ = fs::remove_dir_all(dir);
    }
}
