use super::*;
use dotnetdll::prelude::{Assembly, Module, Resolution, WriteOptions};
use std::io::Write;

const DLL: &[u8] = b"MZfixture";
const MANIFEST: &[u8] = br#"{"name":"Sample","version_number":"2.0.0","dependencies":[]}"#;
const BEPINEX_DEPENDENCY: &str = "BepInEx-BepInExPack-5.4.2305";

struct Fixture {
    root: PathBuf,
    plugins: PathBuf,
    hub: Hub,
}

impl Fixture {
    fn new() -> Self {
        let root = crate::test_support::temp_dir("thunderstore-install");
        let game = root.join("game");
        let plugins = game.join("BepInEx/Plugins");
        fs::create_dir_all(&plugins).unwrap();
        let mut hub = Hub::open_at(root.join("app"));
        hub.settings.game_path = game.to_string_lossy().into_owned();
        Self { root, plugins, hub }
    }

    fn plan(
        &self,
        extra: Option<&str>,
        dependencies: Vec<String>,
    ) -> Result<PackagePlan, HubError> {
        let archive = self.root.join("test.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        let manifest = if dependencies.is_empty() {
            MANIFEST.to_vec()
        } else {
            serde_json::to_vec(&serde_json::json!({
                "name": "Sample", "version_number": "2.0.0", "dependencies": dependencies,
            }))
            .unwrap()
        };
        for (name, bytes) in [
            ("manifest.json", manifest.as_slice()),
            ("BepInEx/plugins/Sample.dll", DLL),
        ]
        .into_iter()
        .chain(extra.into_iter().map(|name| (name, DLL)))
        {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        let staged = self
            .hub
            .stage_download(&archive, &self.hub.staging_root())
            .unwrap();
        let version = Version {
            version_number: String::from("2.0.0"),
            description: String::new(),
            dependencies,
            download_url: String::new(),
            file_size: 0,
        };
        let package = Package {
            full_name: String::from("Test-Sample"),
            owner: String::from("Test"),
            name: String::from("Sample"),
            has_nsfw_content: false,
            versions: vec![version.clone()],
        };
        let plan = self.hub.plan_thunderstore(&package, &version, &staged)?;
        self.hub
            .check_thunderstore_plan(&plan, &self.plugins, false)?;
        Ok(plan)
    }

    fn write_bepinex(&self, [major, minor, build, revision]: [u16; 4]) {
        let game = self.root.join("game");
        let core = game.join("BepInEx/core");
        fs::create_dir_all(&core).unwrap();
        let name = if major == 5 {
            "BepInEx"
        } else {
            "BepInEx.Core"
        };
        let file = format!("{name}.dll");
        let mut assembly = Assembly::new(name);
        assembly.version.major = major;
        assembly.version.minor = minor;
        assembly.version.build = build;
        assembly.version.revision = revision;
        let mut resolution = Resolution::new(Module::new(file.clone()));
        resolution.assembly = Some(assembly);
        fs::write(
            core.join(file),
            resolution.write(WriteOptions::default()).unwrap(),
        )
        .unwrap();
        fs::write(game.join("winhttp.dll"), DLL).unwrap();
        fs::write(game.join("doorstop_config.ini"), b"[General]").unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn preview_then_import_and_install_preserves_the_store_copy() {
    let mut fixture = Fixture::new();
    let plan = fixture.plan(None, Vec::new()).unwrap();
    assert_eq!(plan.version, "2.0.0");
    assert_eq!(plan.target, fixture.plugins.join("Sample.dll"));
    assert!(!plan.target.exists());
    assert!(fixture.hub.store.list().unwrap().is_empty());

    let report = fixture.hub.apply_package(&plan).unwrap();
    let stored = report.stored.path.join("Sample.dll");
    assert_eq!(fs::read(&plan.target).unwrap(), DLL);
    assert_eq!(
        fixture.hub.thunderstore_installed(),
        vec![(String::from("Test-Sample"), String::from("2.0.0"))]
    );
    assert_eq!(
        fixture.hub.links.get(&plan.target).unwrap().kind,
        crate::links::LinkKind::Copy
    );
    fs::write(&plan.target, b"changed by the game").unwrap();
    assert_eq!(fs::read(stored).unwrap(), DLL);
}

#[test]
fn missing_dependency_and_unsupported_paths_leave_plugins_untouched() {
    let fixture = Fixture::new();
    let error = fixture
        .plan(None, vec![String::from("Other-Library-1.0.0")])
        .unwrap_err();
    assert!(error.to_string().contains("Other-Library-1.0.0"));
    let error = fixture
        .plan(Some("BepInEx/patchers/patch.dll"), Vec::new())
        .unwrap_err();
    assert!(error.to_string().contains("patchers"));
    assert_eq!(fs::read_dir(&fixture.plugins).unwrap().count(), 0);
    assert!(fixture.hub.store.list().unwrap().is_empty());
}

#[test]
fn rechecks_the_target_when_applying_a_preview() {
    let mut fixture = Fixture::new();
    let plan = fixture.plan(None, Vec::new()).unwrap();
    fs::write(&plan.target, b"user file").unwrap();
    assert!(fixture.hub.apply_package(&plan).is_err());
    assert_eq!(fs::read(&plan.target).unwrap(), b"user file");
    assert!(fixture.hub.store.list().unwrap().is_empty());
}

#[test]
fn the_detected_bepinex_five_runtime_satisfies_the_pack_at_or_above_the_minimum() {
    for revision in [5, 6] {
        let mut fixture = Fixture::new();
        fixture.write_bepinex([5, 4, 23, revision]);
        let plan = fixture
            .plan(None, vec![BEPINEX_DEPENDENCY.to_owned()])
            .unwrap();
        let report = fixture.hub.apply_package(&plan).unwrap();
        let profile = fixture.hub.profiles.load(&report.profile).unwrap();
        assert_eq!(profile.entries.len(), 1);
        assert_eq!(profile.entries[0].mod_id, "thunderstore.test-sample");
        assert_eq!(
            fixture.hub.bepinex_status().unwrap().version.as_deref(),
            Some(format!("5.4.23.{revision}").as_str())
        );
    }
}

#[test]
fn the_pack_rejects_older_new_major_or_incomplete_runtime_installations() {
    for version in [[5, 4, 23, 4], [6, 0, 0, 0]] {
        let fixture = Fixture::new();
        fixture.write_bepinex(version);
        let error = fixture
            .plan(None, vec![BEPINEX_DEPENDENCY.to_owned()])
            .unwrap_err();
        assert!(error.to_string().contains(BEPINEX_DEPENDENCY));
    }
    let mut fixture = Fixture::new();
    fixture.write_bepinex([5, 4, 23, 6]);
    let plan = fixture
        .plan(None, vec![BEPINEX_DEPENDENCY.to_owned()])
        .unwrap();
    fs::remove_file(fixture.root.join("game/doorstop_config.ini")).unwrap();
    let error = fixture.hub.apply_package(&plan).unwrap_err();
    assert!(error.to_string().contains(BEPINEX_DEPENDENCY));
    assert!(!plan.target.exists());
    assert!(fixture.hub.store.list().unwrap().is_empty());
}

fn catalogue_package(owner: &str, name: &str, version: &str, dependencies: &[&str]) -> Package {
    Package {
        name: name.to_owned(),
        full_name: format!("{owner}-{name}"),
        owner: owner.to_owned(),
        versions: vec![Version {
            version_number: version.to_owned(),
            description: String::new(),
            dependencies: dependencies.iter().map(|id| (*id).to_owned()).collect(),
            download_url: format!(
                "https://thunderstore.io/package/download/{owner}/{name}/{version}/"
            ),
            file_size: 1,
        }],
        has_nsfw_content: false,
    }
}

fn save_catalogue(fixture: &Fixture, packages: &[Package]) {
    let path = fixture
        .hub
        .app_data()
        .join("Cache/Thunderstore/packages.json");
    crate::atomic::replace(&path, &serde_json::to_vec(packages).unwrap()).unwrap();
}

#[test]
fn expands_prosthetics_dependencies_in_order_without_downloading_or_deploying() {
    let fixture = Fixture::new();
    fixture.write_bepinex([5, 4, 23, 6]);
    save_catalogue(
        &fixture,
        &[
            catalogue_package(
                "Herobrinee",
                "Prosthetics_mod",
                "1.1.7",
                &["Jimmyking-CUCoreLib-1.1.0", BEPINEX_DEPENDENCY],
            ),
            catalogue_package("Jimmyking", "CUCoreLib", "1.1.0", &[BEPINEX_DEPENDENCY]),
        ],
    );

    let plan = fixture
        .hub
        .plan_thunderstore_dependencies("Herobrinee-Prosthetics_mod", "1.1.7")
        .unwrap();
    assert_eq!(
        plan.entries
            .iter()
            .map(|entry| (entry.package.as_str(), entry.version.as_str(), &entry.state))
            .collect::<Vec<_>>(),
        vec![
            (
                "BepInEx-BepInExPack",
                "5.4.2305",
                &DependencyState::Satisfied
            ),
            ("Jimmyking-CUCoreLib", "1.1.0", &DependencyState::Install),
            (
                "Herobrinee-Prosthetics_mod",
                "1.1.7",
                &DependencyState::Install
            ),
        ]
    );
    assert!(!plan.ready_for_single_install());
    assert!(!fixture.hub.download_cache().exists());
    assert_eq!(fs::read_dir(&fixture.plugins).unwrap().count(), 0);
    assert!(fixture.hub.store.list().unwrap().is_empty());
}

#[test]
fn lists_cycles_conflicting_pins_and_unavailable_versions() {
    let fixture = Fixture::new();
    save_catalogue(
        &fixture,
        &[
            catalogue_package(
                "Team",
                "Root",
                "1.0.0",
                &["Team-Child-1.0.0", "Team-Child-2.0.0", "Team-Missing-1.0.0"],
            ),
            catalogue_package("Team", "Child", "1.0.0", &["Team-Root-1.0.0"]),
        ],
    );
    let plan = fixture
        .hub
        .plan_thunderstore_dependencies("Team-Root", "1.0.0")
        .unwrap();
    assert!(
        plan.entries
            .iter()
            .any(|entry| entry.package == "Team-Root" && entry.state == DependencyState::Cycle)
    );
    assert!(
        plan.entries
            .iter()
            .any(|entry| entry.package == "Team-Child"
                && entry.state == DependencyState::VersionConflict("1.0.0".to_owned()))
    );
    assert!(plan.entries.iter().any(
        |entry| entry.package == "Team-Missing" && entry.state == DependencyState::Unavailable
    ));
    assert!(!plan.ready_for_single_install());
}

fn staged_pair(fixture: &Fixture) -> (DependencyPlan, Vec<PackagePlan>) {
    let packages = [
        catalogue_package("Jimmyking", "CUCoreLib", "1.1.0", &[]),
        catalogue_package(
            "Herobrinee",
            "Prosthetics_mod",
            "1.1.7",
            &["Jimmyking-CUCoreLib-1.1.0"],
        ),
    ];
    save_catalogue(fixture, &packages);
    let mut staged = Vec::new();
    for package in &packages {
        let version = &package.versions[0];
        let archive = fixture.root.join(format!("{}.zip", package.name));
        let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("manifest.json", options).unwrap();
        serde_json::to_writer(&mut zip, &serde_json::json!({
            "name": package.name, "version_number": version.version_number, "dependencies": version.dependencies
        })).unwrap();
        zip.start_file(format!("plugins/{}.dll", package.name), options)
            .unwrap();
        zip.write_all(DLL).unwrap();
        if package.name == "Prosthetics_mod" {
            zip.start_file("plugins/assets/test", options).unwrap();
            zip.write_all(b"asset").unwrap();
        }
        zip.finish().unwrap();
        let extracted = fixture
            .hub
            .stage_download(&archive, &fixture.hub.staging_root())
            .unwrap();
        staged.push(
            fixture
                .hub
                .plan_thunderstore(package, version, &extracted)
                .unwrap(),
        );
    }
    let plan = fixture
        .hub
        .plan_thunderstore_dependencies("Herobrinee-Prosthetics_mod", "1.1.7")
        .unwrap();
    (plan, staged)
}

#[test]
fn resumes_every_publication_boundary_without_the_catalogue_or_downloads() {
    for published in 0..=2 {
        let fixture = Fixture::new();
        let (plan, packages) = staged_pair(&fixture);
        let receipt = fixture.hub.begin_install(&plan, &packages).unwrap();
        let workspace = fixture.plugins.parent().unwrap().join(receipt.workspace);
        for (index, package) in packages.iter().take(published).enumerate() {
            super::publish::publish(&workspace.join(index.to_string()), &package.target).unwrap();
        }
        fs::remove_file(
            fixture
                .hub
                .app_data
                .join("Cache/Thunderstore/packages.json"),
        )
        .unwrap();
        let mut restarted = Hub::open_at(fixture.hub.app_data.clone());
        restarted
            .settings
            .game_path
            .clone_from(&fixture.hub.settings.game_path);
        let pending = restarted.interrupted_thunderstore_plan().unwrap().unwrap();
        assert_eq!(pending, plan);
        restarted
            .apply_thunderstore_dependencies(&pending, false, &mut |_| {
                panic!("unexpected download")
            })
            .unwrap();
        assert!(restarted.interrupted_thunderstore_plan().unwrap().is_none());
        let mut profile = restarted.profiles.load(&plan.profile).unwrap();
        assert_eq!(profile.entries.len(), 2);
        assert!(profile.entries[0].automatic);
        assert!(!profile.entries[1].automatic);
        assert!(profile.orphaned_dependencies().is_empty());
        profile.entries.pop();
        assert_eq!(profile.orphaned_dependencies(), ["Jimmyking-CUCoreLib"]);
        assert_eq!(fs::read(&packages[0].target).unwrap(), DLL);
        assert_eq!(
            fs::read(packages[1].target.join("assets/test")).unwrap(),
            b"asset"
        );
        for p in packages {
            assert!(restarted.links.target_current(&p.target).unwrap());
        }
    }
}

#[test]
fn replay_keeps_foreign_and_modified_files_and_retries_from_the_receipt() {
    let mut fixture = Fixture::new();
    let (plan, packages) = staged_pair(&fixture);
    let receipt = fixture.hub.begin_install(&plan, &packages).unwrap();
    let workspace = fixture.plugins.parent().unwrap().join(receipt.workspace);
    super::publish::publish(&workspace.join("0"), &packages[0].target).unwrap();
    fs::write(&packages[1].target, b"player").unwrap();
    assert!(
        fixture
            .hub
            .apply_thunderstore_dependencies(&plan, false, &mut |_| {})
            .is_err()
    );
    assert_eq!(fs::read(&packages[1].target).unwrap(), b"player");
    assert!(fixture.hub.profiles.active().is_none());
    fs::remove_file(&packages[1].target).unwrap();
    fs::write(&packages[0].target, b"modified").unwrap();
    assert!(
        fixture
            .hub
            .apply_thunderstore_dependencies(&plan, false, &mut |_| {})
            .is_err()
    );
    assert_eq!(fs::read(&packages[0].target).unwrap(), b"modified");
    fs::write(&packages[0].target, DLL).unwrap();
    fixture
        .hub
        .apply_thunderstore_dependencies(&plan, false, &mut |_| {})
        .unwrap();
    assert_eq!(fixture.hub.thunderstore_installed().len(), 2);
}

#[test]
fn resumes_after_links_save_but_before_the_active_profile_marker() {
    let mut fixture = Fixture::new();
    let (plan, packages) = staged_pair(&fixture);
    fixture.hub.begin_install(&plan, &packages).unwrap();
    let marker = fixture.hub.app_data.join("Profiles/active.txt");
    fs::create_dir_all(&marker).unwrap();
    assert!(
        fixture
            .hub
            .apply_thunderstore_dependencies(&plan, false, &mut |_| {})
            .is_err()
    );
    assert!(
        packages
            .iter()
            .all(|p| fixture.hub.links.target_current(&p.target).unwrap())
    );
    assert!(
        fixture
            .hub
            .interrupted_thunderstore_plan()
            .unwrap()
            .is_some()
    );
    fs::remove_dir(marker).unwrap();
    let mut restarted = Hub::open_at(fixture.hub.app_data.clone());
    restarted
        .settings
        .game_path
        .clone_from(&fixture.hub.settings.game_path);
    restarted
        .apply_thunderstore_dependencies(&plan, false, &mut |_| panic!("unexpected download"))
        .unwrap();
    assert_eq!(
        restarted.profiles.active().as_deref(),
        Some(plan.profile.as_str())
    );
    assert!(restarted.interrupted_thunderstore_plan().unwrap().is_none());
}

#[test]
fn refuses_to_confirm_if_a_target_became_occupied() {
    let fixture = Fixture::new();
    let (plan, packages) = staged_pair(&fixture);
    let blocker = fixture
        .plugins
        .join("THUNDERSTORE.HEROBRINEE-PROSTHETICS_MOD");
    fs::write(&blocker, b"user").unwrap();
    assert!(fixture.hub.begin_install(&plan, &packages).is_err());
    assert_eq!(fs::read(blocker).unwrap(), b"user");
    assert!(!packages[0].target.exists());
    assert!(fixture.hub.store.list().unwrap().is_empty());
    assert!(
        fixture
            .hub
            .interrupted_thunderstore_plan()
            .unwrap()
            .is_none()
    );
}

fn local_cucorelib(version: &str) -> Vec<u8> {
    use dotnetdll::prelude::*;

    let mut assembly = Resolution::new(Module::new("CUCoreLib.dll"));
    assembly.assembly = Some(Assembly::new("CUCoreLib"));
    let bepinex = assembly.push_assembly_reference(ExternalAssemblyReference::new("BepInEx"));
    let attribute: MethodType = BaseType::class(
        assembly.push_type_reference(type_ref! { BepInEx.BepInPlugin in #bepinex }),
    )
    .into();
    let constructor = assembly
        .push_method_reference(method_ref! { void #attribute::.ctor(string, string, string) });
    let plugin = assembly.push_type_definition(TypeDefinition::new(None, "CUCoreLibPlugin"));
    assembly[plugin].attributes.push(Attribute::new(
        constructor.into(),
        CustomAttributeData {
            constructor_args: ["net.cucorelib", "CUCoreLib", version]
                .into_iter()
                .map(|value| FixedArg::String(Some(value.into())))
                .collect(),
            named_args: Vec::new(),
        },
    ));
    assembly.write(WriteOptions::default()).unwrap()
}

#[test]
fn reuses_a_matching_local_cucorelib_in_the_batch_and_rechecks_before_replay() {
    let mut fixture = Fixture::new();
    let (_, mut packages) = staged_pair(&fixture);
    packages.remove(0);
    let path = fixture.plugins.join("CUCoreLib.dll");
    let original = local_cucorelib("1.1");
    fs::write(&path, &original).unwrap();
    let plan = fixture
        .hub
        .plan_thunderstore_dependencies("Herobrinee-Prosthetics_mod", "1.1.7")
        .unwrap();
    assert!(plan.installable());
    assert!(!plan.ready_for_single_install());
    assert!(plan.requires_local_confirmation());
    assert!(
        matches!(&plan.entries[0].state, DependencyState::LocalCandidate(local) if local.path == path && local.version == "1.1")
    );
    let rejected = fixture
        .hub
        .apply_thunderstore_dependencies(&plan, false, &mut |_| panic!("unexpected download"))
        .unwrap_err();
    assert!(rejected.to_string().contains("Confirm"));
    assert!(
        fixture
            .hub
            .interrupted_thunderstore_plan()
            .unwrap()
            .is_none()
    );
    fixture.hub.begin_install(&plan, &packages).unwrap();
    let mut replacement = original.clone();
    replacement.push(0);
    fs::write(&path, replacement).unwrap();
    let error = fixture
        .hub
        .apply_thunderstore_dependencies(&plan, true, &mut |_| panic!("unexpected download"))
        .unwrap_err();
    assert!(error.to_string().contains("changed"));
    assert!(error.to_string().contains(&path.display().to_string()));
    assert!(!packages[0].target.exists());
    fs::write(&path, &original).unwrap();
    fixture
        .hub
        .apply_thunderstore_dependencies(&plan, true, &mut |_| panic!("unexpected download"))
        .unwrap();
    assert_eq!(fixture.hub.thunderstore_installed().len(), 1);
    assert_eq!(fs::read(path).unwrap(), original);
    assert!(packages[0].target.is_dir());
}

#[test]
fn rejects_unverified_wrong_version_disabled_and_duplicate_local_cucorelib() {
    let fixture = Fixture::new();
    let (_, _) = staged_pair(&fixture);
    let path = fixture.plugins.join("CUCoreLib.dll");
    let disabled = fixture.plugins.join("CUCoreLib.dll.disabled");
    let other = fixture.plugins.join("Bundled.dll");
    let check = |expected: &str| {
        let plan = fixture
            .hub
            .plan_thunderstore_dependencies("Herobrinee-Prosthetics_mod", "1.1.7")
            .unwrap();
        assert!(!plan.installable());
        assert!(
            matches!(&plan.entries[0].state, DependencyState::LocalConflict(reason) if reason.contains(expected))
        );
    };
    fs::write(&path, DLL).unwrap();
    check("Cannot verify");
    fs::write(&path, local_cucorelib("1.2.0")).unwrap();
    check("1.2.0");
    fs::rename(&path, &disabled).unwrap();
    check("disabled");
    fs::write(&path, local_cucorelib("1.1")).unwrap();
    fs::write(&other, local_cucorelib("1.1.0")).unwrap();
    check("Multiple");
}

#[test]
fn reuses_the_deployed_dll_from_a_github_profile_entry() {
    let mut fixture = Fixture::new();
    staged_pair(&fixture);
    let archive = fixture.root.join("CUCoreLib.dll");
    fs::write(&archive, local_cucorelib("1.1.0")).unwrap();
    let mut package = fixture.hub.prepare_local_import(&archive).unwrap();
    package.source = "github:jimmyking9999999/CUCoreLib".into();
    fixture.hub.apply_package(&package).unwrap();
    let plan = fixture
        .hub
        .plan_thunderstore_dependencies("Herobrinee-Prosthetics_mod", "1.1.7")
        .unwrap();
    assert!(plan.installable());
    assert!(
        matches!(&plan.entries[0].state, DependencyState::LocalCandidate(local) if local.path == package.target)
    );
}

#[test]
fn verifies_reuse_only_when_the_complete_thunderstore_payload_matches() {
    let fixture = Fixture::new();
    staged_pair(&fixture);
    let reference = fixture.root.join("reference");
    fs::create_dir(&reference).unwrap();
    let bytes = local_cucorelib("1.1.0");
    fs::write(reference.join("CUCoreLib.dll"), &bytes).unwrap();
    fs::write(reference.join("library.data"), b"published resource").unwrap();
    fixture
        .hub
        .store
        .import_dir(
            &reference,
            "thunderstore.jimmyking-cucorelib",
            "1.1.0",
            "thunderstore:Jimmyking-CUCoreLib",
        )
        .unwrap();
    let path = fixture.plugins.join("CUCoreLib");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("CUCoreLib.dll"), &bytes).unwrap();

    let plan = || {
        fixture
            .hub
            .plan_thunderstore_dependencies("Herobrinee-Prosthetics_mod", "1.1.7")
            .unwrap()
    };
    assert!(plan().requires_local_confirmation());
    fs::write(path.join("library.data"), b"published resource").unwrap();
    let verified = plan();
    assert!(verified.installable());
    assert!(!verified.requires_local_confirmation());
    assert!(matches!(
        verified.entries[0].state,
        DependencyState::VerifiedLocal(_)
    ));
    fs::write(path.join("library.data"), b"different").unwrap();
    assert!(plan().requires_local_confirmation());
}

#[test]
fn requires_confirmation_when_resuming_an_older_local_reuse_receipt() {
    let mut fixture = Fixture::new();
    let (_, mut packages) = staged_pair(&fixture);
    packages.remove(0);
    let dll = fixture.plugins.join("CUCoreLib.dll");
    fs::write(&dll, local_cucorelib("1.1")).unwrap();
    let plan = fixture
        .hub
        .plan_thunderstore_dependencies("Herobrinee-Prosthetics_mod", "1.1.7")
        .unwrap();
    fixture.hub.begin_install(&plan, &packages).unwrap();

    let path = fixture.hub.app_data.join("Thunderstore-install.json");
    let mut legacy: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    legacy["plan"]["entries"][0]["state"] = serde_json::json!({"ReuseLocal": dll});
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

    let pending = fixture
        .hub
        .interrupted_thunderstore_plan()
        .unwrap()
        .unwrap();
    assert_eq!(pending, plan);
    let error = fixture
        .hub
        .apply_thunderstore_dependencies(&pending, false, &mut |_| panic!("unexpected download"))
        .unwrap_err();
    assert!(error.to_string().contains("Confirm"));
    assert!(!packages[0].target.exists());
    fixture
        .hub
        .apply_thunderstore_dependencies(&pending, true, &mut |_| panic!("unexpected download"))
        .unwrap();
    assert!(packages[0].target.is_dir());
}

#[test]
fn deploys_root_dll_with_its_sibling_assets_as_one_unit() {
    let mut fixture = Fixture::new();
    let archive = fixture.root.join("prosthetics.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
    for (path, bytes) in [
        ("manifest.json", MANIFEST),
        ("prosthetics.dll", DLL),
        ("prosthetics/augments/example.png", b"sprite".as_slice()),
    ] {
        zip.start_file(path, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    let staged = fixture
        .hub
        .stage_download(&archive, &fixture.hub.staging_root())
        .unwrap();
    let package = catalogue_package("Test", "Sample", "2.0.0", &[]);
    let plan = fixture
        .hub
        .plan_thunderstore(&package, &package.versions[0], &staged)
        .unwrap();
    assert!(!plan.single);
    fixture.hub.apply_package(&plan).unwrap();
    assert_eq!(fs::read(plan.target.join("prosthetics.dll")).unwrap(), DLL);
    assert_eq!(
        fs::read(plan.target.join("prosthetics/augments/example.png")).unwrap(),
        b"sprite"
    );

    let mut mismatched = staged;
    mismatched
        .files
        .iter_mut()
        .find(|file| file.relative.ends_with("example.png"))
        .unwrap()
        .relative = PathBuf::from("config/example.png");
    assert!(
        fixture
            .hub
            .plan_thunderstore(&package, &package.versions[0], &mismatched)
            .unwrap_err()
            .to_string()
            .contains("config/example.png")
    );
}
