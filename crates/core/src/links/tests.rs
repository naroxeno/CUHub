use super::*;

#[test]
fn a_replacement_failure_restores_the_previous_target() {
    let dir = crate::test_support::temp_dir("links-replacement");
    let target = dir.join("plugin.dll");
    let source = dir.join("source.dll");
    fs::write(&source, b"old").unwrap();
    let mut index = LinkIndex::load(&dir);
    index.deploy(&dir, &target, &source, "plugin", "1").unwrap();
    let entry = index.get(&target).unwrap();
    let failure = replace_target(&target, &dir.join("missing.dll"), entry).unwrap_err();
    assert!(matches!(failure, LinkError::MissingStore(_)));
    assert_eq!(fs::read(&target).unwrap(), b"old");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_legacy_hardlink_replaced_by_equal_bytes_is_not_owned() {
    let dir = crate::test_support::temp_dir("links-replaced");
    let target = dir.join("plugin.dll");
    let source = dir.join("stored.dll");
    fs::write(&source, b"original").unwrap();
    let mut index = LinkIndex::load(&dir);
    fs::hard_link(&source, &target).unwrap();
    index.entries.insert(
        key(&target),
        LinkEntry {
            mod_id: "plugin".to_owned(),
            version: "1".to_owned(),
            kind: LinkKind::Hardlink,
            store_path: key(&source),
            files: Vec::new(),
        },
    );
    fs::remove_file(&target).unwrap();
    fs::write(&target, b"original").unwrap();
    assert!(matches!(
        index.remove(&dir, &target),
        Err(LinkError::ChangedTarget(_))
    ));
    assert!(target.is_file());
    assert!(index.get(&target).is_some());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn copied_file_is_removed_without_changing_its_source() {
    let dir = crate::test_support::temp_dir("links-copied-file");
    let source = store_file(&dir);
    let target = dir.join("copied.dll");
    copy_file(&source, &target).unwrap();
    let mut index = LinkIndex::load(&dir);
    index.entries.insert(
        key(&target),
        LinkEntry {
            mod_id: "plugin".to_owned(),
            version: "1".to_owned(),
            kind: LinkKind::Copy,
            store_path: key(&source),
            files: Vec::new(),
        },
    );
    index.remove(&dir, &target).unwrap();
    assert!(!target.exists());
    assert!(source.is_file());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_copy_cleans_up_files_created_before_the_error() {
    let dir = crate::test_support::temp_dir("links-copy-failure");
    let source = dir.join("source");
    let nested = source.join("nested");
    let external = dir.join("external");
    let target = dir.join("target");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir(&external).unwrap();
    fs::write(source.join("copied.dll"), b"old").unwrap();
    fs::write(external.join("keep.dll"), b"keep").unwrap();
    dir_link(&nested.join("foreign"), &external).unwrap();
    assert!(matches!(
        copy_tree(&source, &target),
        Err(LinkError::ChangedTarget(_))
    ));
    assert!(!target.exists());
    assert_eq!(fs::read(external.join("keep.dll")).unwrap(), b"keep");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn malformed_index_is_preserved() {
    let dir = crate::test_support::temp_dir("links-malformed");
    let index_path = dir.join(INDEX_FILE);
    fs::write(&index_path, b"{bad").unwrap();
    let mut index = LinkIndex::load(&dir);
    let target = dir.join("plugin.dll");
    assert!(
        index
            .deploy(&dir, &target, &store_file(&dir), "plugin", "1")
            .is_err()
    );
    assert!(index.save().is_err());
    assert!(!target.exists());
    assert_eq!(fs::read(&index_path).unwrap(), b"{bad");
    fs::remove_dir_all(dir).unwrap();
}
use crate::test_support::temp_dir;

fn store_file(root: &Path) -> PathBuf {
    let path = root.join("Store/mods/mod/1.0/payload/plugin.dll");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"MZ").unwrap();
    path
}

#[test]
fn deploy_file_then_remove_keeps_store() {
    let dir = temp_dir("links-file");
    let game = dir.join("game");
    fs::create_dir_all(game.join("BepInEx/Plugins")).unwrap();
    let source = store_file(&dir);
    let mut index = LinkIndex::load(&dir);
    let target = game.join("BepInEx/Plugins/plugin.dll");
    let kind = index.deploy(&game, &target, &source, "mod", "1.0").unwrap();
    assert_eq!(kind, LinkKind::Copy);
    assert!(target.is_file());
    index.remove(&game, &target).unwrap();
    assert!(!target.exists());
    assert!(source.is_file());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn deploy_dir_then_remove_keeps_store() {
    let dir = temp_dir("links-dir");
    let game = dir.join("game");
    fs::create_dir_all(game.join("BepInEx/Plugins")).unwrap();
    let source = dir.join("Store/mods/big/1.0/payload");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("a.dll"), b"MZ").unwrap();
    let mut index = LinkIndex::load(&dir);
    let target = game.join("BepInEx/Plugins/big");
    index.deploy(&game, &target, &source, "big", "1.0").unwrap();
    index.remove(&game, &target).unwrap();
    assert!(target.symlink_metadata().is_err());
    assert!(source.join("a.dll").is_file());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn remove_refuses_unmanaged_target() {
    let dir = temp_dir("links-unmanaged");
    let game = dir.join("game");
    fs::create_dir_all(game.join("BepInEx/Plugins")).unwrap();
    let mut index = LinkIndex::load(&dir);
    let target = game.join("BepInEx/Plugins/user.dll");
    assert!(matches!(
        index.remove(&game, &target),
        Err(LinkError::NotManaged(_))
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn deploy_rejects_targets_outside_game() {
    let dir = temp_dir("links-outside");
    let game = dir.join("game");
    fs::create_dir_all(&game).unwrap();
    let source = store_file(&dir);
    let mut index = LinkIndex::load(&dir);
    let outside = dir.join("elsewhere/plugin.dll");
    assert!(matches!(
        index.deploy(&game, &outside, &source, "mod", "1.0"),
        Err(LinkError::OutsideGame(_))
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn index_round_trips() {
    let dir = temp_dir("links-index");
    let game = dir.join("game");
    fs::create_dir_all(game.join("BepInEx/Plugins")).unwrap();
    let source = store_file(&dir);
    let mut index = LinkIndex::load(&dir);
    let target = game.join("BepInEx/Plugins/plugin.dll");
    index.deploy(&game, &target, &source, "mod", "1.0").unwrap();
    index.save().unwrap();
    let loaded = LinkIndex::load(&dir);
    assert!(loaded.get(&target).is_some());
    assert_eq!(loaded.deployed_for("mod", "1.0").len(), 1);
    let _ = fs::remove_dir_all(dir);
}

#[cfg(windows)]
fn dir_link(target: &Path, source: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let target = target.components().collect::<PathBuf>();
    let source = source.components().collect::<PathBuf>();
    let (Some(target), Some(source)) = (target.to_str(), source.to_str()) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid junction path",
        ));
    };
    // cmd still expands percent expressions inside quotes.
    if [target, source]
        .iter()
        .any(|path| path.contains(['%', '"', '\r', '\n']))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsupported junction path",
        ));
    }
    let output = std::process::Command::new("cmd")
        .creation_flags(CREATE_NO_WINDOW)
        .arg("/D")
        .arg("/V:OFF")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .raw_arg(format!("\"{target}\""))
        .raw_arg(format!("\"{source}\""))
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other("mklink /J failed"))
    }
}

#[cfg(not(windows))]
fn dir_link(target: &Path, source: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}
