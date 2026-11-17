use super::*;
use crate::links::LinkIndex;
use crate::mods::ModStatus;
use crate::store::Store;
use crate::test_support::temp_dir;
use std::fs;

fn installed(name: &str, source: PathBuf, version: Option<&str>, disabled: bool) -> InstalledMod {
    InstalledMod {
        name: name.to_owned(),
        installed_version: version.map(str::to_owned),
        assembly_version: None,
        bepinex_version: None,
        expected_version: None,
        author: None,
        description: None,
        guid: None,
        nexus_url: None,
        metadata_id: None,
        source_path: source,
        dll_paths: Vec::new(),
        modified_at: None,
        expected_modified_at: None,
        disabled,
        status: ModStatus::UpToDate,
        declared_dependencies: Vec::new(),
        missing_dependencies: Vec::new(),
        incompatible_with: Vec::new(),
    }
}

fn game(directory: &Path) -> (PathBuf, PathBuf) {
    let game_root = directory.join("game");
    let plugins = game_root.join("Plugins");
    fs::create_dir_all(&plugins).unwrap();
    (game_root, plugins)
}

#[test]
fn adopts_a_folder_mod_into_the_store_and_links_it() {
    let directory = temp_dir("adopt-folder");
    let (game_root, plugins) = game(&directory);
    let source = plugins.join("Better Mod");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("BetterMod.dll"), b"MZ plugin").unwrap();
    fs::write(source.join("readme.txt"), b"docs").unwrap();
    let mods = [installed(
        "Better Mod",
        source.clone(),
        Some("1.2.3"),
        false,
    )];
    let store = Store::new(directory.join("Store"));
    let mut links = LinkIndex::load(&directory);
    let plan = plan_adopt(&mods, &plugins, &store, &links);
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    let report = apply_adopt(
        &plan,
        &store,
        &mut links,
        &game_root,
        &directory.join("Backups"),
    )
    .unwrap();
    assert_eq!(report.adopted.len(), 1);
    let item = &report.adopted[0];
    assert_eq!(item.mod_id, "better-mod");
    assert_eq!(item.version, "1.2.3");
    let entry = store.read_entry("better-mod", "1.2.3").unwrap();
    assert_eq!(entry.files.len(), 2);
    assert!(links.get(&plugins.join("Better Mod")).is_some());
    assert!(plugins.join("Better Mod/BetterMod.dll").exists());
    assert!(report.backup_dir.join("Better Mod/BetterMod.dll").exists());
    assert_eq!(item.source, item.target);
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn adopts_a_single_dll_as_a_file_unit() {
    let directory = temp_dir("adopt-file");
    let (game_root, plugins) = game(&directory);
    let source = plugins.join("Solo.dll");
    fs::write(&source, b"MZ solo").unwrap();
    let mods = [installed("Solo", source.clone(), Some("1.0"), false)];
    let mut links = LinkIndex::load(&directory);
    let store = Store::new(directory.join("Store"));
    let plan = plan_adopt(&mods, &plugins, &store, &links);
    assert_eq!(plan.items.len(), 1, "{plan:?}");
    let report = apply_adopt(
        &plan,
        &store,
        &mut links,
        &game_root,
        &directory.join("Backups"),
    )
    .unwrap();
    let item = &report.adopted[0];
    assert_eq!(item.mod_id, "solo");
    let entry = store.read_entry("solo", "1.0").unwrap();
    assert_eq!(entry.files.len(), 1);
    assert_eq!(entry.files[0].path, "Solo.dll");
    let target = plugins.join("Solo.dll");
    assert!(target.exists());
    assert!(links.get(&target).is_some());
    assert!(report.backup_dir.join("Solo.dll").exists());
    assert_eq!(item.source, item.target);
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn skips_disabled_and_unversioned_mods() {
    let directory = temp_dir("adopt-skip");
    let (_game_root, plugins) = game(&directory);
    let off = plugins.join("Off.dll");
    fs::write(&off, b"MZ").unwrap();
    let unknown = plugins.join("Unknown.dll");
    fs::write(&unknown, b"MZ").unwrap();
    let mods = [
        installed("Off", off, Some("1.0"), true),
        installed("Unknown", unknown, None, false),
    ];
    let store = Store::new(directory.join("Store"));
    let links = LinkIndex::load(&directory);
    let plan = plan_adopt(&mods, &plugins, &store, &links);
    assert!(plan.items.is_empty());
    let reasons: Vec<AdoptSkip> = plan.skipped.iter().map(|(_, reason)| *reason).collect();
    assert!(reasons.contains(&AdoptSkip::Disabled));
    assert!(reasons.contains(&AdoptSkip::MissingVersion));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn skips_mods_that_already_have_a_link() {
    let directory = temp_dir("adopt-managed");
    let (game_root, plugins) = game(&directory);
    let staging = directory.join("staging");
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("Mine.dll"), b"MZ").unwrap();
    let store = Store::new(directory.join("Store"));
    let stored = store.import_dir(&staging, "mine", "1.0", "test").unwrap();
    let mut links = LinkIndex::load(&directory);
    let source = plugins.join("Mine");
    links
        .deploy(&game_root, &source, &stored.path, "mine", "1.0")
        .unwrap();
    let mods = [installed("Mine", source, Some("1.0"), false)];
    let plan = plan_adopt(&mods, &plugins, &store, &links);
    assert!(plan.items.is_empty());
    assert_eq!(plan.skipped[0].1, AdoptSkip::AlreadyManaged);
    let _ = fs::remove_dir_all(directory);
}
