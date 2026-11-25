use std::fs;
use std::path::PathBuf;

use super::*;
use crate::links::LinkIndex;
use crate::test_support::temp_dir;

struct Fixture {
    root: PathBuf,
    plugins: PathBuf,
    hub: Hub,
}

impl Fixture {
    fn new() -> Self {
        let root = temp_dir("profile-toggle");
        let game = root.join("game");
        let plugins = game.join("BepInEx/Plugins");
        fs::create_dir_all(&plugins).unwrap();
        let mut hub = Hub::open_at(root.join("app"));
        hub.settings.game_path = game.to_string_lossy().into_owned();
        hub.network = None;
        let mut profile = Profile::new("Test");
        profile.active = true;
        for name in ["target", "unrelated"] {
            let dll = root.join(format!("{name}.dll"));
            fs::write(&dll, b"MZfixture").unwrap();
            hub.store.import_file(&dll, name, "1.0.0", "local").unwrap();
            profile.entries.push(ProfileEntry {
                mod_id: name.to_owned(),
                version: String::from("1.0.0"),
                enabled: true,
                ..ProfileEntry::default()
            });
        }
        hub.profiles.save(&profile).unwrap();
        hub.apply_profile("Test").unwrap();
        Self { root, plugins, hub }
    }

    fn modify_deployed_copy(&mut self, target: &Path) {
        let path = self.hub.links.path();
        let mut data: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        data["entries"][target.to_str().unwrap()]["kind"] = "copy".into();
        fs::remove_file(target).unwrap();
        fs::write(target, b"local edit").unwrap();
        fs::write(path, serde_json::to_vec(&data).unwrap()).unwrap();
        self.hub.links = LinkIndex::load(&self.root.join("app"));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn single_toggle_ignores_unrelated_copy_drift_and_returns_the_updated_row() {
    let mut fixture = Fixture::new();
    let target = fixture.plugins.join("target.dll");
    let unrelated = fixture.plugins.join("unrelated.dll");
    fixture.modify_deployed_copy(&unrelated);
    assert!(fixture.hub.plan_profile("Test").is_err());
    let item = mods::build_mod(&target, &[]).unwrap();

    let (count, row) = fixture.hub.set_managed_mod_enabled(&item, false).unwrap();
    let row = row.unwrap();
    assert_eq!(count, 1);
    assert!(row.disabled);
    assert_eq!(row.source_path, target);
    assert!(!target.exists());
    assert!(fixture.hub.links.get(&target).is_none());
    assert!(!fixture.hub.profiles.load("Test").unwrap().entries[0].enabled);

    let (count, row) = fixture.hub.set_managed_mod_enabled(&row, true).unwrap();
    assert_eq!(count, 1);
    assert!(!row.unwrap().disabled);
    assert!(fixture.hub.links.get(&target).is_some());
    assert!(fixture.hub.profiles.load("Test").unwrap().entries[0].enabled);
    assert_eq!(fs::read(target).unwrap(), b"MZfixture");
    assert_eq!(fs::read(unrelated).unwrap(), b"local edit");
}

#[test]
fn single_toggle_rejects_a_modified_target_and_preserves_the_profile() {
    let mut fixture = Fixture::new();
    let target = fixture.plugins.join("target.dll");
    let item = mods::build_mod(&target, &[]).unwrap();
    fixture.modify_deployed_copy(&target);

    let error = fixture
        .hub
        .set_managed_mod_enabled(&item, false)
        .unwrap_err();
    assert!(error.to_string().contains("target.dll"));
    assert!(fixture.hub.profiles.load("Test").unwrap().entries[0].enabled);
    assert!(fixture.hub.links.get(&target).is_some());
    assert_eq!(fs::read(target).unwrap(), b"local edit");
}

#[test]
fn a_new_unmanaged_file_blocks_reenabling_a_profile_mod() {
    let mut fixture = Fixture::new();
    let target = fixture.plugins.join("target.dll");
    let item = mods::build_mod(&target, &[]).unwrap();
    let (_, row) = fixture.hub.set_managed_mod_enabled(&item, false).unwrap();
    fs::write(&target, b"unmanaged").unwrap();

    assert!(
        fixture
            .hub
            .set_managed_mod_enabled(&row.unwrap(), true)
            .is_err()
    );
    assert!(!fixture.hub.profiles.load("Test").unwrap().entries[0].enabled);
    assert!(fixture.hub.links.get(&target).is_none());
    assert_eq!(fs::read(target).unwrap(), b"unmanaged");
}
