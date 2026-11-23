use super::*;
use crate::catalogs::DependencyRequirement;
use crate::sources::SourceManifest;
use crate::test_support::temp_dir;
use dotnetdll::prelude::*;

fn dll(path: &Path, major: u16) -> Vec<u8> {
    let name = path.file_stem().unwrap().to_str().unwrap();
    let module = path.file_name().unwrap().to_str().unwrap();
    let mut resolution = Resolution::new(Module::new(module));
    resolution.assembly = Some(Assembly::new(name));
    let bepinex = resolution.push_assembly_reference(ExternalAssemblyReference::new("BepInEx"));
    let attribute: MethodType = BaseType::class(
        resolution.push_type_reference(type_ref! { BepInEx.BepInPlugin in #bepinex }),
    )
    .into();
    let ctor = resolution
        .push_method_reference(method_ref! { void #attribute::.ctor(string, string, string) });
    let plugin = resolution.push_type_definition(TypeDefinition::new(None, name));
    resolution[plugin].attributes.push(Attribute::new(
        ctor.into(),
        CustomAttributeData {
            constructor_args: vec![
                FixedArg::String(Some(format!("migration.{name}").into())),
                FixedArg::String(Some(name.into())),
                FixedArg::String(Some(format!("{major}.0.0").into())),
            ],
            named_args: Vec::new(),
        },
    ));
    let data = resolution.write(WriteOptions::default()).unwrap();
    fs::write(path, &data).unwrap();
    data
}

fn fixture() -> (PathBuf, Hub, PathBuf) {
    let directory = temp_dir("migration");
    let game = directory.join("game");
    let plugins = game.join("BepInEx").join("Plugins");
    fs::create_dir_all(&plugins).unwrap();
    let mut hub = Hub::open_at(directory.join("app"));
    hub.settings.game_path = game.to_string_lossy().into_owned();
    hub.network = None;
    (directory, hub, plugins)
}

#[test]
fn rejects_a_changed_local_scope_before_backup() {
    let (directory, mut hub, plugins) = fixture();
    let source = plugins.join("LocalPack");
    fs::create_dir(&source).unwrap();
    dll(&source.join("Local.dll"), 1);
    let (mut session, rows) = hub.scan_migration().unwrap();
    fs::write(source.join("edited.txt"), "new resource").unwrap();
    assert!(matches!(
        hub.apply_migration_local(&mut session, &rows),
        Err(HubError::Profile(ProfileError::PlanChanged))
    ));
    assert!(source.join("edited.txt").is_file());
    assert!(hub.profiles.active().is_none());
    assert!(!hub.backup_root().exists());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn github_and_local_migration_share_a_new_profile_and_keep_original_backups() {
    let (directory, mut hub, plugins) = fixture();
    let remote = plugins.join("Remote");
    fs::create_dir(&remote).unwrap();
    let original = dll(&remote.join("Remote.dll"), 2);
    fs::write(remote.join("edited.txt"), "player edits").unwrap();
    let local = dll(&plugins.join("Local.dll"), 1);
    let config = plugins.parent().unwrap().join("config");
    fs::create_dir(&config).unwrap();
    fs::write(config.join("player.cfg"), "keep").unwrap();
    let mut old = Profile::new("Before");
    old.config_overrides.push("opaque".into());
    old.active = true;
    hub.profiles.save(&old).unwrap();

    hub.sources = Some(
        SourceManifest::parse(
            r#"{
        "schemaVersion": 1,
        "generatedAt": "2026-11-23T00:00:00Z",
        "mods": [{"name": "Remote", "repo": "example/remote", "match": {"dll": ["Remote.dll"]}}]
    }"#,
        )
        .unwrap(),
    );
    let log = fs::read(hub.logs.session_path()).unwrap();
    let (mut session, rows) = hub.scan_migration().unwrap();
    assert_eq!(fs::read(hub.logs.session_path()).unwrap(), log);
    let remote_row = rows.iter().find(|row| row.path == remote).unwrap();
    let entry = &remote_row.sources[0].1;
    let installed = mods::build_mod(&remote, &[]).unwrap();
    assert_eq!(installed.installed_version.as_deref(), Some("2.0.0"));
    assert_eq!(installed.guid.as_deref(), Some("migration.Remote"));
    let download = directory.join("Remote.dll");

    dll(&download, 1);
    let lower = hub.stage_download(&download, &hub.staging_root()).unwrap();
    let downgrade = hub.migration_package(remote_row, entry, &installed, &lower);
    assert!(
        matches!(downgrade, Err(HubError::Update(_))),
        "{downgrade:?}"
    );
    hub.discard_staging(&lower.staging_root);
    dll(&download, 2);
    let same = hub.stage_download(&download, &hub.staging_root()).unwrap();
    assert!(
        hub.migration_package(remote_row, entry, &installed, &same)
            .is_ok()
    );
    hub.discard_staging(&same.staging_root);

    let upgraded = dll(&download, 3);
    let staged = hub.stage_download(&download, &hub.staging_root()).unwrap();
    let mut blocked = entry.clone();
    blocked.depends.push(DependencyRequirement {
        name: "MissingDependency".into(),
        ..Default::default()
    });
    assert!(matches!(
        hub.migration_package(remote_row, &blocked, &installed, &staged),
        Err(HubError::Dependencies(_))
    ));
    assert_eq!(fs::read(remote.join("Remote.dll")).unwrap(), original);
    assert_eq!(hub.profiles.active().as_deref(), Some("Before"));
    let (package, local_version) = hub
        .migration_package(remote_row, entry, &installed, &staged)
        .unwrap();
    let preview = MigrationPreview {
        row: remote_row.clone(),
        entry: entry.clone(),
        staged,
        package,
        local_version,
        backup_root: session.backup_root.clone(),
    };
    let reinstalled = hub
        .apply_migration_reinstall(&mut session, &preview)
        .unwrap();
    let backup = reinstalled.backup.unwrap().join("Remote");
    assert_eq!(fs::read(backup.join("Remote.dll")).unwrap(), original);
    assert_eq!(
        fs::read(backup.join("edited.txt")).unwrap(),
        b"player edits"
    );
    assert_eq!(fs::read(plugins.join("Remote.dll")).unwrap(), upgraded);
    assert!(!preview.staged.staging_root.exists());

    let local_row = rows
        .into_iter()
        .find(|row| row.path == plugins.join("Local.dll"))
        .unwrap();
    let mut imported = hub
        .apply_migration_local(&mut session, &[local_row])
        .unwrap();
    let imported = imported.pop().unwrap().unwrap();
    assert_eq!(imported.profile, reinstalled.profile);
    assert!(imported.profile.starts_with("adopt_"));
    assert_eq!(fs::read(plugins.join("Local.dll")).unwrap(), local);
    assert_eq!(
        fs::read(imported.backup.unwrap().join("Local.dll")).unwrap(),
        local
    );
    assert_eq!(fs::read(config.join("player.cfg")).unwrap(), b"keep");
    let migrated = hub.profiles.load(&imported.profile).unwrap();
    assert!(migrated.active);
    assert_eq!(migrated.entries.len(), 2);
    assert_eq!(
        migrated.entries[0].source.as_deref(),
        Some("github:example/remote")
    );
    assert!(migrated.entries[1].source.is_none());
    old.active = false;
    assert_eq!(hub.profiles.load("Before").unwrap(), old);
    fs::remove_dir_all(directory).unwrap();
}
