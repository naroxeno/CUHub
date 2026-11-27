use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::install::PackageFile;
use crate::links::LinkIndex;
use crate::profiles::ProfileStore;

struct Case {
    root: PathBuf,
    appdata: PathBuf,
    game: PathBuf,
    plugins: PathBuf,
    backup: PathBuf,
}

impl Case {
    fn new(name: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "cuprum-packages-{name}-{}-{stamp}",
            std::process::id()
        ));
        let appdata = root.join("appdata");
        let game = root.join("game");
        let plugins = game.join("Plugins");
        let backup = appdata.join("Backups");
        fs::create_dir_all(&plugins).unwrap();
        fs::create_dir_all(&backup).unwrap();
        Self {
            root,
            appdata,
            game,
            plugins,
            backup,
        }
    }

    fn plan(
        &self,
        mod_id: &str,
        version: &str,
        target: PathBuf,
        previous: Option<PathBuf>,
        files: Vec<(&str, &[u8])>,
        single: bool,
    ) -> PackagePlan {
        let staging = self.root.join("staging").join(mod_id);
        let mut packaged = Vec::new();
        for (relative, bytes) in files {
            let staged = staging.join(relative);
            fs::create_dir_all(staged.parent().unwrap()).unwrap();
            fs::write(&staged, bytes).unwrap();
            packaged.push(PackageFile {
                staged,
                relative: PathBuf::from(relative),
                size: bytes.len() as u64,
            });
        }
        PackagePlan {
            mod_id: mod_id.to_owned(),
            version: version.to_owned(),
            source: "test".to_owned(),
            target,
            previous,
            managed_previous: None,
            staging,
            files: packaged,
            single,
            warnings: Vec::new(),
        }
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn run_apply(case: &Case, plan: &PackagePlan) -> Result<PackageReport, PackageError> {
    let store = Store::new(case.appdata.join("Store"));
    let profiles = ProfileStore::new(case.appdata.clone());
    let mut links = LinkIndex::load(&case.appdata);
    apply_with(
        plan,
        &store,
        &profiles,
        &mut links,
        &case.game,
        &case.plugins,
        &case.backup,
    )
}

fn apply_with(
    plan: &PackagePlan,
    store: &Store,
    profiles: &ProfileStore,
    links: &mut LinkIndex,
    game: &Path,
    plugins: &Path,
    backup: &Path,
) -> Result<PackageReport, PackageError> {
    super::apply(
        plan,
        PackageContext {
            store,
            links,
            profiles,
            game_root: game,
            plugins_root: plugins,
            backup_root: backup,
        },
    )
}

#[test]
fn single_dll_lands_in_the_store_and_plugins() {
    let case = Case::new("single");
    let plan = case.plan(
        "goodmod",
        "1.2.3",
        case.plugins.join("GoodMod.dll"),
        None,
        vec![("GoodMod.dll", b"dll payload")],
        true,
    );
    let report = run_apply(&case, &plan).unwrap();
    assert_eq!(report.files, 1);
    assert_eq!(report.stored.mod_id, "goodmod");
    assert_eq!(report.stored.version, "1.2.3");
    assert!(report.stored.path.join("GoodMod.dll").is_file());
    assert!(case.plugins.join("GoodMod.dll").exists());
    assert!(report.backup.is_none());
    let profile = ProfileStore::new(case.appdata.clone())
        .load("Default")
        .unwrap();
    assert!(profile.active);
    assert_eq!(profile.entries.len(), 1);
    assert_eq!(profile.entries[0].mod_id, "goodmod");
    let links = LinkIndex::load(&case.appdata);
    assert_eq!(
        links.get(&case.plugins.join("GoodMod.dll")).unwrap().mod_id,
        "goodmod"
    );
}

#[test]
fn refuses_to_reuse_a_modified_store_payload() {
    let case = Case::new("modified-store");
    let target = case.plugins.join("GoodMod.dll");
    let plan = case.plan(
        "goodmod",
        "1",
        target,
        None,
        vec![("GoodMod.dll", b"original")],
        true,
    );
    let report = run_apply(&case, &plan).unwrap();
    let payload = report.stored.path.join("GoodMod.dll");
    fs::write(&payload, b"external").unwrap();
    assert!(matches!(
        run_apply(&case, &plan),
        Err(PackageError::StoreChanged(path)) if path == payload
    ));
}

#[test]
fn a_conflict_at_apply_time_does_not_move_the_unmanaged_previous_target() {
    let case = Case::new("late-conflict");
    let previous = case.plugins.join("Old.dll");
    let target = case.plugins.join("New.dll");
    fs::write(&previous, b"old").unwrap();
    let plan = case.plan(
        "goodmod",
        "1",
        target.clone(),
        Some(previous.clone()),
        vec![("New.dll", b"new")],
        true,
    );
    fs::write(&target, b"external").unwrap();
    assert!(matches!(
        run_apply(&case, &plan),
        Err(PackageError::Profile(ProfileError::OccupiedTarget(path))) if path == target
    ));
    assert_eq!(fs::read(previous).unwrap(), b"old");
    assert_eq!(fs::read(target).unwrap(), b"external");
    assert_eq!(fs::read_dir(&case.backup).unwrap().count(), 0);
}

#[test]
fn a_package_cannot_redeploy_an_unrelated_profile_target() {
    let case = Case::new("unrelated-repair");
    let other_target = case.plugins.join("Other.dll");
    let other = case.plan(
        "other",
        "1",
        other_target.clone(),
        None,
        vec![("Other.dll", b"other")],
        true,
    );
    run_apply(&case, &other).unwrap();
    fs::remove_file(&other_target).unwrap();
    let target = case.plugins.join("New.dll");
    let plan = case.plan(
        "new",
        "1",
        target.clone(),
        None,
        vec![("New.dll", b"new")],
        true,
    );
    assert!(matches!(
        run_apply(&case, &plan),
        Err(PackageError::Profile(ProfileError::AdditionalTarget(path))) if path == other_target
    ));
    assert!(!target.exists());
    assert!(!other_target.exists());
}

#[test]
fn replacing_an_unmanaged_directory_backs_it_up_first() {
    let case = Case::new("replace");
    let previous = case.plugins.join("OldMod");
    fs::create_dir_all(&previous).unwrap();
    fs::write(previous.join("old.dll"), b"old bytes").unwrap();
    let target = case.plugins.join("oldmod");
    let plan = case.plan(
        "oldmod",
        "2.0.0",
        target.clone(),
        Some(previous),
        vec![("main.dll", b"new bytes"), ("config.ini", b"[mod]\n")],
        false,
    );
    let report = run_apply(&case, &plan).unwrap();
    let backup = report.backup.expect("unmanaged files are moved to Backups");
    assert!(backup.join("OldMod").join("old.dll").is_file());
    assert!(target.join("main.dll").is_file());
    assert!(!target.join("old.dll").exists());
}

#[test]
fn reapplying_the_same_version_reuses_the_stored_payload() {
    let case = Case::new("repeat");
    let plan = case.plan(
        "goodmod",
        "1.2.3",
        case.plugins.join("GoodMod.dll"),
        None,
        vec![("GoodMod.dll", b"dll payload")],
        true,
    );
    run_apply(&case, &plan).unwrap();
    let report = run_apply(&case, &plan).unwrap();
    assert_eq!(report.files, 1);
    assert!(case.plugins.join("GoodMod.dll").exists());
    let profile = ProfileStore::new(case.appdata.clone())
        .load("Default")
        .unwrap();
    assert_eq!(profile.entries.len(), 1);
}

#[test]
fn updating_a_managed_mod_replaces_the_profile_entry() {
    let case = Case::new("managed-update");
    let target = case.plugins.join("GoodMod.dll");
    let first = case.plan(
        "goodmod",
        "1.2.3",
        target.clone(),
        None,
        vec![("GoodMod.dll", b"old payload")],
        true,
    );
    run_apply(&case, &first).unwrap();
    let mut second = case.plan(
        "goodmod",
        "2.0.0",
        target.clone(),
        None,
        vec![("GoodMod.dll", b"new payload")],
        true,
    );
    second.source = "nexus:67".to_owned();
    let report = run_apply(&case, &second).unwrap();

    assert!(report.backup.is_none());
    assert_eq!(fs::read(&target).unwrap(), b"new payload");
    let store = Store::new(case.appdata.join("Store"));
    assert!(store.resolve("goodmod", "1.2.3").is_some());
    assert!(store.resolve("goodmod", "2.0.0").is_some());
    let profile = ProfileStore::new(case.appdata.clone())
        .load("Default")
        .unwrap();
    assert_eq!(profile.entries.len(), 1);
    assert_eq!(profile.entries[0].version, "2.0.0");
    assert_eq!(profile.entries[0].source.as_deref(), Some("nexus:67"));
}

#[test]
fn rejects_reusing_a_version_with_different_bytes() {
    let case = Case::new("conflict");
    let target = case.plugins.join("GoodMod.dll");
    let first = case.plan(
        "goodmod",
        "1.2.3",
        target.clone(),
        None,
        vec![("GoodMod.dll", b"old payload")],
        true,
    );
    run_apply(&case, &first).unwrap();
    let second = case.plan(
        "goodmod",
        "1.2.3",
        target,
        None,
        vec![("GoodMod.dll", b"different payload")],
        true,
    );
    let error = run_apply(&case, &second).unwrap_err();
    assert!(matches!(
        error,
        PackageError::PayloadConflict { mod_id, version }
            if mod_id == "goodmod" && version == "1.2.3"
    ));
}
