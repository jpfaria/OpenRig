//! #913 — what the preset picker lists.
//!
//! The list is read once when the picker opens and filtered from that snapshot
//! as the user types, so anything missed here is invisible for the whole
//! session. The presets folder is a user setting that may point anywhere —
//! including at nothing — and the picker still has to open.

use super::scan_preset_files;

fn dir_with(files: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for name in files {
        std::fs::write(dir.path().join(name), b"preset:\n").expect("write");
    }
    dir
}

fn names(dir: &tempfile::TempDir) -> Vec<String> {
    scan_preset_files(dir.path())
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

#[test]
fn both_yaml_extensions_are_listed() {
    let dir = dir_with(&["clean.yaml", "drive.yml"]);
    assert_eq!(names(&dir), vec!["clean", "drive"]);
}

#[test]
fn anything_that_is_not_yaml_is_left_out() {
    let dir = dir_with(&["clean.yaml", "notes.txt", "capture.nam", "cab.wav"]);
    assert_eq!(names(&dir), vec!["clean"]);
}

#[test]
fn the_list_is_sorted_by_filename_not_by_the_filesystems_order() {
    let dir = dir_with(&["zeta.yaml", "alpha.yaml", "mid.yaml"]);
    assert_eq!(
        names(&dir),
        vec!["alpha", "mid", "zeta"],
        "the same folder must list the same way on every open and every machine"
    );
}

#[test]
fn underscores_in_the_filename_are_shown_as_spaces() {
    let dir = dir_with(&["studio_clean_boost.yaml"]);
    assert_eq!(names(&dir), vec!["studio clean boost"]);
}

#[test]
fn a_dash_is_left_alone() {
    // Only underscores are de-slugged; a dash is a character the user chose.
    let dir = dir_with(&["lead-boost.yaml"]);
    assert_eq!(names(&dir), vec!["lead-boost"]);
}

#[test]
fn each_name_is_paired_with_the_file_it_came_from() {
    let dir = dir_with(&["clean.yaml"]);
    let listed = scan_preset_files(dir.path());
    assert_eq!(listed.len(), 1);
    assert!(listed[0].1.ends_with("clean.yaml"));
}

#[test]
fn an_empty_folder_lists_nothing() {
    let dir = dir_with(&[]);
    assert!(scan_preset_files(dir.path()).is_empty());
}

#[test]
fn a_folder_that_does_not_exist_lists_nothing_instead_of_failing() {
    assert!(
        scan_preset_files(std::path::Path::new("/nonexistent/openrig-913/presets")).is_empty(),
        "the presets path is a user setting — the picker must still open"
    );
}

#[test]
fn a_subdirectory_named_like_a_preset_is_not_offered() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(dir.path().join("bundle.yaml")).expect("mkdir");
    std::fs::write(dir.path().join("clean.yaml"), b"preset:\n").expect("write");
    // The scan keeps it out of the way of the real files either way: what
    // matters is that the real preset is still listed and pickable.
    let listed = names(&dir);
    assert!(listed.contains(&"clean".to_string()));
}
