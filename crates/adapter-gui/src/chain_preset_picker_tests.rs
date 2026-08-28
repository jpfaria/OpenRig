//! #913 — the bank helpers the LOAD PICKER and the save dialog use.
//!
//! `chain_preset_bank_tests` covers what the bank answers about a rig; these
//! are the file-facing helpers around it: what a loaded file renames the active
//! preset to, what the picker's search keeps, and when saving would overwrite.

use super::{
    filter_preset_names, preset_overwrite_required, preset_rename_target_from_path,
    preset_save_path,
};

#[test]
fn a_loaded_file_renames_the_preset_to_its_stem_untouched() {
    assert_eq!(
        preset_rename_target_from_path(std::path::Path::new("/p/my_lead-tone.openrig-preset"))
            .as_deref(),
        Some("my_lead-tone"),
        "#510: dashes and underscores are the user's choice, not ours to humanize"
    );
}

#[test]
fn a_path_with_no_stem_renames_nothing() {
    assert_eq!(
        preset_rename_target_from_path(std::path::Path::new("/")),
        None
    );
    assert_eq!(
        preset_rename_target_from_path(std::path::Path::new("")),
        None
    );
}

#[test]
fn the_picker_search_is_case_insensitive_and_empty_passes_everything() {
    let names = vec![
        "Studio Clean".to_string(),
        "Lead Boost".to_string(),
        "lead rhythm".to_string(),
    ];
    assert_eq!(filter_preset_names(&names, "LEAD").len(), 2);
    assert_eq!(filter_preset_names(&names, "   ").len(), 3);
    assert!(filter_preset_names(&names, "bass").is_empty());
}

#[test]
fn saving_over_an_existing_file_asks_for_confirmation() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(
        !preset_overwrite_required(dir.path(), "Lead Boost"),
        "nothing saved yet"
    );
    let path = preset_save_path(dir.path(), "Lead Boost");
    std::fs::write(&path, b"preset:\n").expect("write");
    assert!(preset_overwrite_required(dir.path(), "Lead Boost"));
}
