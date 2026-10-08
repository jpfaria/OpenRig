//! Moving to the dot folder must not lose what older releases saved: every
//! entry of the old folder the new one lacks is copied over, nothing the new
//! one already holds is overwritten, the old folder stays as it was, and the
//! copy runs once.

use std::fs;
use std::path::Path;

use crate::legacy_user_data::{copy_legacy_into, MIGRATION_MARKER};

fn write(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

#[test]
fn entries_missing_from_the_user_folder_are_copied_with_their_contents() {
    let tmp = tempfile::tempdir().unwrap();
    let legacy = tmp.path().join("legacy");
    let root = tmp.path().join(".openrig");
    write(&legacy.join("midi-profile.yaml"), "input: x\n");
    write(&legacy.join("tone3000/amp/model.nam"), "nam");

    copy_legacy_into(&root, &[legacy.clone()]).unwrap();

    assert_eq!(
        fs::read_to_string(root.join("midi-profile.yaml")).unwrap(),
        "input: x\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("tone3000/amp/model.nam")).unwrap(),
        "nam"
    );
}

#[test]
fn an_entry_the_user_folder_already_has_is_never_overwritten() {
    let tmp = tempfile::tempdir().unwrap();
    let legacy = tmp.path().join("legacy");
    let root = tmp.path().join(".openrig");
    write(&legacy.join("config.yaml"), "old: true\n");
    write(&root.join("config.yaml"), "new: true\n");

    copy_legacy_into(&root, &[legacy]).unwrap();

    assert_eq!(
        fs::read_to_string(root.join("config.yaml")).unwrap(),
        "new: true\n"
    );
}

#[test]
fn the_old_folder_is_left_as_it_was() {
    let tmp = tempfile::tempdir().unwrap();
    let legacy = tmp.path().join("legacy");
    let root = tmp.path().join(".openrig");
    write(&legacy.join("looper-takes/take.wav"), "wav");

    copy_legacy_into(&root, &[legacy.clone()]).unwrap();

    assert_eq!(
        fs::read_to_string(legacy.join("looper-takes/take.wav")).unwrap(),
        "wav"
    );
}

#[test]
fn the_copy_runs_once() {
    let tmp = tempfile::tempdir().unwrap();
    let legacy = tmp.path().join("legacy");
    let root = tmp.path().join(".openrig");
    write(&legacy.join("a.yaml"), "a");

    copy_legacy_into(&root, &[legacy.clone()]).unwrap();
    assert!(root.join(MIGRATION_MARKER).exists());
    write(&legacy.join("b.yaml"), "b");
    copy_legacy_into(&root, &[legacy]).unwrap();

    assert!(!root.join("b.yaml").exists());
}

#[test]
fn a_missing_old_folder_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join(".openrig");

    copy_legacy_into(&root, &[tmp.path().join("never-existed")]).unwrap();

    assert!(root.join(MIGRATION_MARKER).exists());
}

#[test]
fn every_old_folder_contributes_what_the_earlier_ones_did_not() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("share/openrig");
    let config = tmp.path().join("config/OpenRig");
    let root = tmp.path().join(".openrig");
    write(&data.join("looper-takes/t.wav"), "wav");
    write(&config.join("config.yaml"), "cfg");

    copy_legacy_into(&root, &[data, config]).unwrap();

    assert!(root.join("looper-takes/t.wav").exists());
    assert_eq!(fs::read_to_string(root.join("config.yaml")).unwrap(), "cfg");
}

#[cfg(target_os = "macos")]
#[test]
fn on_macos_the_old_folder_is_application_support() {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    assert_eq!(
        crate::legacy_user_data::legacy_user_data_roots(),
        vec![home.join("Library/Application Support/OpenRig")]
    );
}

#[cfg(target_os = "windows")]
#[test]
fn on_windows_nothing_moved() {
    assert!(crate::legacy_user_data::legacy_user_data_roots().is_empty());
}
