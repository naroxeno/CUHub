//! Real-filesystem tests for the profile store and the deployment plan.

use super::*;
use crate::test_support::temp_dir;

fn game_dir(dir: &Path) -> PathBuf {
    let root = dir.join("game");
    fs::create_dir_all(root.join("BepInEx/Plugins")).unwrap();
    root
}

fn import(dir: &Path, store: &Store, mod_id: &str, version: &str, files: &[(&str, &[u8])]) {
    let source = dir.join("sources").join(format!("{mod_id}-{version}"));
    for (relative, bytes) in files {
        let path = source.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    store.import_dir(&source, mod_id, version, "test").unwrap();
}

fn entry(mod_id: &str, version: &str) -> ProfileEntry {
    ProfileEntry {
        mod_id: mod_id.to_owned(),
        version: version.to_owned(),
        ..Default::default()
    }
}

fn deploy_op(target: &Path, mod_id: &str, version: &str) -> ApplyOp {
    ApplyOp::Deploy {
        target: target.to_path_buf(),
        mod_id: mod_id.to_owned(),
        version: version.to_owned(),
    }
}

fn remove_op(target: &Path) -> ApplyOp {
    ApplyOp::Remove {
        target: target.to_path_buf(),
    }
}

#[test]
fn saves_lists_loads_and_removes_profiles() {
    let dir = temp_dir("profiles-store");
    let profiles = ProfileStore::new(dir.clone());
    assert!(profiles.list().is_empty());
    assert!(profiles.load("Pack").is_none());

    let mut profile = Profile::new("Pack");
    profile.entries.push(entry("com.author.qol", "1.0.0"));
    profiles.save(&profile).unwrap();

    assert_eq!(profiles.list(), vec!["Pack".to_owned()]);
    assert_eq!(profiles.active(), None);
    assert_eq!(profiles.load("Pack").as_ref(), Some(&profile));

    profiles.set_active(Some("Pack")).unwrap();
    assert_eq!(profiles.active().as_deref(), Some("Pack"));
    assert!(profiles.load("Pack").unwrap().active);

    let raw = fs::read_to_string(dir.join("Profiles/Pack.json")).unwrap();
    assert!(raw.contains("\"schemaVersion\": 1"));
    assert!(raw.contains("\"modId\": \"com.author.qol\""));

    profiles.set_active(None).unwrap();
    assert_eq!(profiles.active(), None);
    assert!(!dir.join("Profiles/active.txt").exists());

    profiles.remove("Pack").unwrap();
    assert!(profiles.list().is_empty());
    assert!(profiles.load("Pack").is_none());
    assert_eq!(profiles.active(), None);

    assert_eq!(
        profiles.save(&Profile::new("..")).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(profiles.load("../Pack").is_none());
    assert!(!dir.join("Profiles").join("..json").exists());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn active_marker_tracks_the_saved_profile() {
    let dir = temp_dir("profiles-active");
    let profiles = ProfileStore::new(dir.clone());
    let mut first = Profile::new("First");
    profiles.save(&first).unwrap();
    let second = Profile::new("Second");
    profiles.save(&second).unwrap();
    assert_eq!(profiles.active(), None);

    profiles.set_active(Some("First")).unwrap();
    assert_eq!(profiles.active().as_deref(), Some("First"));
    assert!(profiles.load("First").unwrap().active);
    assert!(!profiles.load("Second").unwrap().active);

    assert_eq!(
        profiles.set_active(Some("Missing")).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    assert_eq!(
        profiles.set_active(Some("../First")).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );

    first.active = true;
    profiles.save(&first).unwrap();
    assert_eq!(profiles.active().as_deref(), Some("First"));
    first.active = false;
    profiles.save(&first).unwrap();
    assert_eq!(profiles.active(), None);

    profiles.set_active(Some("Second")).unwrap();
    profiles.remove("Second").unwrap();
    assert_eq!(profiles.active(), None);
    assert_eq!(profiles.list(), vec!["First".to_owned()]);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn plans_a_deploy_for_an_empty_deployment() {
    let dir = temp_dir("profiles-plan-deploy");
    let game = game_dir(&dir);
    let plugins = game.join("BepInEx/Plugins");
    let store = Store::new(dir.clone());
    import(&dir, &store, "com.author.qol", "1.0.0", &[("QoL", b"q")]);
    import(
        &dir,
        &store,
        "com.author.solo",
        "2.0.0",
        &[("Solo.dll", b"MZ")],
    );
    let profile = Profile {
        name: "Pack".to_owned(),
        entries: vec![
            entry("com.author.qol", "1.0.0"),
            entry("com.author.solo", "2.0.0"),
        ],
        ..Default::default()
    };
    let index = LinkIndex::load(&dir);
    let ops = plan_apply(&profile, &index, &store, &game, &plugins).unwrap();
    assert_eq!(
        ops,
        vec![
            deploy_op(&plugins.join("com.author.qol"), "com.author.qol", "1.0.0"),
            deploy_op(&plugins.join("Solo.dll"), "com.author.solo", "2.0.0"),
        ]
    );
    assert!(index.entries().next().is_none());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn applying_links_targets_and_keeps_the_store_untouched() {
    let dir = temp_dir("profiles-apply");
    let game = game_dir(&dir);
    let plugins = game.join("BepInEx/Plugins");
    let store = Store::new(dir.clone());
    import(
        &dir,
        &store,
        "com.author.solo",
        "2.0.0",
        &[("Solo.dll", b"MZ")],
    );
    let profile = Profile {
        name: "Pack".to_owned(),
        entries: vec![entry("com.author.solo", "2.0.0")],
        ..Default::default()
    };
    let mut index = LinkIndex::load(&dir);
    let ops = plan_apply(&profile, &index, &store, &game, &plugins).unwrap();
    let report = apply_plan(&mut index, &store, &game, &ops).unwrap();
    assert_eq!(report.applied, ops);
    assert!(report.remaining.is_empty());

    let target = plugins.join("Solo.dll");
    assert!(target.is_file());
    assert!(dir.join("Links.json").is_file());
    let reloaded = LinkIndex::load(&dir);
    assert_eq!(
        reloaded.get(&target).map(|entry| entry.version.as_str()),
        Some("2.0.0")
    );
    assert_eq!(
        index.get(&target).map(|entry| entry.version.as_str()),
        Some("2.0.0")
    );

    let payload = store.resolve("com.author.solo", "2.0.0").unwrap();
    assert!(payload.join("Solo.dll").is_file());
    assert_eq!(store.total_bytes(), store.total_bytes());

    let again = plan_apply(&profile, &reloaded, &store, &game, &plugins).unwrap();
    assert!(again.is_empty());
    let report = apply_plan(&mut index, &store, &game, &again).unwrap();
    assert!(report.applied.is_empty());
    assert!(report.remaining.is_empty());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn replaces_an_older_version_remove_then_deploy() {
    let dir = temp_dir("profiles-change");
    let game = game_dir(&dir);
    let plugins = game.join("BepInEx/Plugins");
    let store = Store::new(dir.clone());
    import(
        &dir,
        &store,
        "com.author.solo",
        "1.0.0",
        &[("Solo.dll", b"MZ-v1")],
    );
    import(
        &dir,
        &store,
        "com.author.solo",
        "2.0.0",
        &[("Solo.dll", b"MZ-v2")],
    );
    let target = plugins.join("Solo.dll");
    let source = store
        .resolve("com.author.solo", "1.0.0")
        .unwrap()
        .join("Solo.dll");
    let mut index = LinkIndex::load(&dir);
    index
        .deploy(&game, &target, &source, "com.author.solo", "1.0.0")
        .unwrap();
    index.save().unwrap();

    let profile = Profile {
        name: "Pack".to_owned(),
        entries: vec![entry("com.author.solo", "2.0.0")],
        ..Default::default()
    };
    let ops = plan_apply(&profile, &index, &store, &game, &plugins).unwrap();
    assert_eq!(
        ops,
        vec![
            remove_op(&target),
            deploy_op(&target, "com.author.solo", "2.0.0"),
        ]
    );
    let report = apply_plan(&mut index, &store, &game, &ops).unwrap();
    assert_eq!(report.applied.len(), 2);
    assert_eq!(fs::read(&target).unwrap(), b"MZ-v2");
    assert_eq!(index.get(&target).unwrap().version, "2.0.0");
    let reloaded = LinkIndex::load(&dir);
    assert!(
        plan_apply(&profile, &reloaded, &store, &game, &plugins)
            .unwrap()
            .is_empty()
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn removes_stale_links_under_plugins_only() {
    let dir = temp_dir("profiles-prune");
    let game = game_dir(&dir);
    let plugins = game.join("BepInEx/Plugins");
    let store = Store::new(dir.clone());
    import(&dir, &store, "com.author.qol", "1.0.0", &[("QoL", b"q")]);
    import(
        &dir,
        &store,
        "com.author.extra",
        "1.0.0",
        &[("Extra/a.dll", b"MZ")],
    );
    let both = Profile {
        name: "Both".to_owned(),
        entries: vec![
            entry("com.author.qol", "1.0.0"),
            entry("com.author.extra", "1.0.0"),
        ],
        ..Default::default()
    };
    let mut index = LinkIndex::load(&dir);
    let ops = plan_apply(&both, &index, &store, &game, &plugins).unwrap();
    apply_plan(&mut index, &store, &game, &ops).unwrap();

    let config = game.join("BepInEx/config/qol.cfg");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    let config_source = dir.join("qol.cfg");
    fs::write(&config_source, b"cfg").unwrap();
    index
        .deploy(&game, &config, &config_source, "com.author.qol", "1.0.0")
        .unwrap();

    let only = Profile {
        name: "Only".to_owned(),
        entries: vec![entry("com.author.qol", "1.0.0")],
        ..Default::default()
    };
    let ops = plan_apply(&only, &index, &store, &game, &plugins).unwrap();
    assert_eq!(ops, vec![remove_op(&plugins.join("com.author.extra"))]);
    let report = apply_plan(&mut index, &store, &game, &ops).unwrap();
    assert_eq!(report.applied, ops);
    assert!(plugins.join("com.author.extra").symlink_metadata().is_err());
    assert!(config.is_file());
    assert!(index.get(&config).is_some());
    assert!(index.get(&plugins.join("com.author.qol")).is_some());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn stops_on_missing_version_and_reports_the_finished_part() {
    let dir = temp_dir("profiles-partial");
    let game = game_dir(&dir);
    let plugins = game.join("BepInEx/Plugins");
    let store = Store::new(dir.clone());
    import(&dir, &store, "com.author.qol", "1.0.0", &[("QoL", b"q")]);
    let target = plugins.join("com.author.qol");
    let ops = vec![
        deploy_op(&target, "com.author.qol", "1.0.0"),
        remove_op(&target),
        deploy_op(&target, "com.author.qol", "9.9.9"),
    ];
    let mut index = LinkIndex::load(&dir);
    let error = apply_plan(&mut index, &store, &game, &ops).unwrap_err();
    let ProfileError::Apply { report, source } = error else {
        panic!("expected an apply failure");
    };
    assert_eq!(report.applied, ops[..2].to_vec());
    assert_eq!(report.remaining, ops[2..].to_vec());
    assert!(matches!(
        source.as_ref(),
        ProfileError::MissingVersion { .. }
    ));
    assert!(target.symlink_metadata().is_err());
    let reloaded = LinkIndex::load(&dir);
    assert!(reloaded.entries().next().is_none());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn ignores_disabled_entries_and_rejects_invalid_plans() {
    let dir = temp_dir("profiles-rules");
    let game = game_dir(&dir);
    let plugins = game.join("BepInEx/Plugins");
    let store = Store::new(dir.clone());
    import(&dir, &store, "com.author.qol", "1.0.0", &[("QoL", b"q")]);
    import(
        &dir,
        &store,
        "com.author.solo",
        "2.0.0",
        &[("Same.dll", b"MZ")],
    );
    import(
        &dir,
        &store,
        "com.author.other",
        "2.0.0",
        &[("Same.dll", b"MZ")],
    );
    let index = LinkIndex::load(&dir);

    let mut disabled = entry("com.author.qol", "1.0.0");
    disabled.enabled = false;
    let profile = Profile {
        name: "Pack".to_owned(),
        entries: vec![disabled],
        ..Default::default()
    };
    assert!(
        plan_apply(&profile, &index, &store, &game, &plugins)
            .unwrap()
            .is_empty()
    );

    let unknown = Profile {
        name: "Pack".to_owned(),
        entries: vec![entry("com.author.qol", "9.9.9")],
        ..Default::default()
    };
    assert!(matches!(
        plan_apply(&unknown, &index, &store, &game, &plugins),
        Err(ProfileError::MissingVersion { .. })
    ));

    let duplicate_mod = Profile {
        name: "Pack".to_owned(),
        entries: vec![
            entry("com.author.qol", "1.0.0"),
            entry("com.author.qol", "1.0.0"),
        ],
        ..Default::default()
    };
    assert!(matches!(
        plan_apply(&duplicate_mod, &index, &store, &game, &plugins),
        Err(ProfileError::DuplicateMod(_))
    ));

    let duplicate_target = Profile {
        name: "Pack".to_owned(),
        entries: vec![
            entry("com.author.solo", "2.0.0"),
            entry("com.author.other", "2.0.0"),
        ],
        ..Default::default()
    };
    assert!(matches!(
        plan_apply(&duplicate_target, &index, &store, &game, &plugins),
        Err(ProfileError::DuplicateTarget(_))
    ));

    assert!(matches!(
        plan_apply(
            &Profile::default(),
            &index,
            &store,
            &game,
            &dir.join("outside")
        ),
        Err(ProfileError::OutsideGame(_))
    ));
    let _ = fs::remove_dir_all(dir);
}
