//! #827, red-first: the DI picker sees a take the moment it is saved — from
//! the editor OR from any other transport — without the meter tick listing
//! the folder on every pass.

use super::*;

fn write_take(dir: &std::path::Path, name: &str) {
    application::looper_take_library::save_take(dir, name, &[0.1; 8], 48_000).expect("save");
}

#[test]
fn a_catalog_over_a_missing_folder_is_empty() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut catalog = TakeCatalog::new(tmp.path().join("looper-takes"));
    assert!(catalog.takes().is_empty());
}

#[test]
fn a_take_saved_after_the_first_read_shows_up_on_the_next() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("looper-takes");
    let mut catalog = TakeCatalog::new(dir.clone());
    assert!(catalog.takes().is_empty());

    // The folder is born with the first take.
    write_take(&dir, "first");
    assert_eq!(catalog.takes().to_vec(), vec![dir.join("first.wav")]);

    write_take(&dir, "second");
    assert_eq!(
        catalog.takes().to_vec(),
        vec![dir.join("first.wav"), dir.join("second.wav")]
    );
}

#[test]
fn a_take_deleted_by_hand_disappears() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().to_path_buf();
    write_take(&dir, "keep");
    write_take(&dir, "drop");
    let mut catalog = TakeCatalog::new(dir.clone());
    assert_eq!(catalog.takes().len(), 2);

    std::fs::remove_file(dir.join("drop.wav")).unwrap();
    assert_eq!(catalog.takes().to_vec(), vec![dir.join("keep.wav")]);
}
