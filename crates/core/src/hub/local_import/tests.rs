use std::io::Write;
use std::path::PathBuf;

use dotnetdll::prelude::*;

use super::*;
use crate::profiles::{Profile, ProfileEntry};
use crate::test_support::temp_dir;

const DLL: &[u8] = b"MZfixture";

fn plugin_dll() -> Vec<u8> {
    let mut assembly = Resolution::new(Module::new("Fixture.dll"));
    assembly.assembly = Some(Assembly::new("Fixture"));
    let bepinex = assembly.push_assembly_reference(ExternalAssemblyReference::new("BepInEx"));
    let attribute: MethodType = BaseType::class(
        assembly.push_type_reference(type_ref! { BepInEx.BepInPlugin in #bepinex }),
    )
    .into();
    let constructor = assembly
        .push_method_reference(method_ref! { void #attribute::.ctor(string, string, string) });
    let plugin = assembly.push_type_definition(TypeDefinition::new(None, "LocalImportFixture"));
    assembly[plugin].attributes.push(Attribute::new(
        constructor.into(),
        CustomAttributeData {
            constructor_args: ["test.local-import", "Fixture", "1.2.3"]
                .into_iter()
                .map(|value| FixedArg::String(Some(value.into())))
                .collect(),
            named_args: Vec::new(),
        },
    ));
    assembly.write(WriteOptions::default()).unwrap()
}

struct Fixture {
    root: PathBuf,
    plugins: PathBuf,
    hub: Hub,
}

impl Fixture {
    fn new() -> Self {
        let root = temp_dir("local-import");
        let game = root.join("game");
        let plugins = game.join("BepInEx/Plugins");
        fs::create_dir_all(&plugins).unwrap();
        let mut hub = Hub::open_at(root.join("app"));
        hub.settings.game_path = game.to_string_lossy().into_owned();
        hub.network = None;
        Self { root, plugins, hub }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn previews_local_zip_and_deploys_after_confirmation() {
    let mut fixture = Fixture::new();
    let plugin = plugin_dll();
    let archive = fixture.root.join("PlayerMod.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
    zip.start_file(
        "BepInEx/plugins/PlayerMod.dll",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(&plugin).unwrap();
    zip.start_file(
        "BepInEx/config/PlayerMod.cfg",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(b"ignored").unwrap();
    zip.finish().unwrap();
    let original = fs::read(&archive).unwrap();
    let plan = fixture.hub.prepare_local_import(&archive).unwrap();
    assert_eq!(plan.version, "1.2.3");
    assert_eq!(plan.target, fixture.plugins.join("PlayerMod.dll"));
    assert_eq!(plan.files.len(), 1);
    assert!(!plan.target.exists());
    assert!(fixture.hub.store.list().unwrap().is_empty());

    let report = fixture.hub.apply_package(&plan).unwrap();
    let profile = fixture.hub.profiles.load(&report.profile).unwrap();
    assert_eq!(report.profile, "Default");
    assert_eq!(fixture.hub.profiles.active().as_deref(), Some("Default"));
    assert_eq!(profile.entries[0].source.as_deref(), Some(SOURCE));
    assert_eq!(fs::read(&plan.target).unwrap(), plugin);
    assert_eq!(fs::read(&archive).unwrap(), original);
    fixture.hub.discard_staging(&plan.staging);
    assert!(!plan.staging.exists());
}

#[test]
fn cancel_local_dll_keeps_the_original_and_plugins_untouched() {
    let fixture = Fixture::new();
    let dll = fixture.root.join("PlayerMod.dll");
    fs::write(&dll, DLL).unwrap();
    let plan = fixture.hub.prepare_local_import(&dll).unwrap();
    fixture.hub.discard_staging(&plan.staging);
    assert!(!plan.staging.exists());
    assert!(!plan.target.exists());
    assert_eq!(fs::read(&dll).unwrap(), DLL);
    assert!(fixture.hub.profiles.active().is_none());
}

#[test]
fn stages_and_deploys_local_sevenz_and_rar_archives() {
    let plugin = plugin_dll();
    for extension in ["7z", "rar"] {
        let mut fixture = Fixture::new();
        let archive = fixture.root.join(format!("PlayerMod.{extension}"));
        if extension == "7z" {
            let dll = fixture.root.join("PlayerMod.dll");
            fs::write(&dll, &plugin).unwrap();
            sevenz_rust::compress_to_path(dll, &archive).unwrap();
        } else {
            let mut rar = rars::Builder::new(rars::ArchiveVersion::Rar50).store(true);
            rar.add_bytes(b"PlayerMod.dll".to_vec(), plugin.clone(), None, None)
                .unwrap();
            fs::write(&archive, rar.to_bytes().unwrap()).unwrap();
        }
        let plan = fixture.hub.prepare_local_import(&archive).unwrap();
        assert!(!plan.target.exists());
        fixture.hub.apply_package(&plan).unwrap();
        assert_eq!(fs::read(&plan.target).unwrap(), plugin);
        assert!(archive.is_file());
    }
}

#[test]
fn a_disabled_profile_entry_blocks_a_local_import_without_a_live_target() {
    let fixture = Fixture::new();
    let dll = fixture.root.join("PlayerMod.dll");
    fs::write(&dll, DLL).unwrap();
    let mut profile = Profile::new("Existing");
    profile.active = true;
    profile.entries.push(ProfileEntry {
        mod_id: String::from("PLAYERMOD"),
        version: String::from("1.0.0"),
        enabled: false,
        ..ProfileEntry::default()
    });
    fixture.hub.profiles.save(&profile).unwrap();
    let error = fixture.hub.prepare_local_import(&dll).unwrap_err();
    assert!(error.to_string().contains("Existing"), "{error}");
    assert!(
        fixture
            .root
            .join("app/Staging")
            .read_dir()
            .unwrap()
            .next()
            .is_none()
    );
    assert!(fixture.plugins.read_dir().unwrap().next().is_none());
}

#[test]
fn a_case_insensitive_target_collision_after_preview_stops_apply_before_importing() {
    let mut fixture = Fixture::new();
    let dll = fixture.root.join("PlayerMod.dll");
    fs::write(&dll, DLL).unwrap();
    let plan = fixture.hub.prepare_local_import(&dll).unwrap();
    let conflict = fixture.plugins.join("playermod.DLL.disabled");
    fs::write(&conflict, b"keep").unwrap();
    let error = fixture.hub.apply_package(&plan).unwrap_err();
    assert!(error.to_string().contains("already exists"), "{error}");
    assert_eq!(fs::read(&conflict).unwrap(), b"keep");
    assert!(!plan.target.exists());
    assert!(fixture.hub.store.list().unwrap().is_empty());
}

#[test]
fn a_renamed_local_dll_cannot_duplicate_an_installed_plugin_guid() {
    let fixture = Fixture::new();
    let plugin = plugin_dll();
    let installed = fixture.plugins.join("Original.dll");
    fs::write(&installed, &plugin).unwrap();
    let renamed = fixture.root.join("Renamed.dll");
    fs::write(&renamed, &plugin).unwrap();

    let error = fixture.hub.prepare_local_import(&renamed).unwrap_err();
    assert!(error.to_string().contains("Original.dll"), "{error}");
    assert!(!fixture.plugins.join("Renamed.dll").exists());
    assert_eq!(fs::read(installed).unwrap(), plugin);
}
