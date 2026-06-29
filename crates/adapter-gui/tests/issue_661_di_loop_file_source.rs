//! Issue #661 (follow-up) — RED-FIRST tests for representing a user-chosen
//! `DiLoopSource::File` in the chain-tile ComboBox.
//!
//! A File source was previously unrepresentable in the source list, so after
//! "Choose file…" the ComboBox kept showing the sentinel and `selected_index`
//! was -1 (nothing highlighted). These helpers give the loaded File a labelled
//! entry so the popup shows the chosen file and it can be (re)selected.

use std::path::PathBuf;

use application::di_loader::DiLoopSource;

use adapter_gui::di_loop_ui_sources::{
    build_di_loop_sources_with_loaded, di_loop_file_label, di_loop_selected_index,
    CHOOSE_FILE_SENTINEL,
};

#[test]
fn file_label_is_the_file_name_with_extension() {
    let label = di_loop_file_label(&PathBuf::from("/Users/me/Desktop/ambience take.wav"));
    assert_eq!(label, "ambience take.wav");
}

#[test]
fn sources_with_loaded_file_inserts_label_before_sentinel() {
    let loaded = DiLoopSource::File(PathBuf::from("/x/my_loop.wav"));
    let sources = build_di_loop_sources_with_loaded(&["dry_1", "dry_2"], Some(&loaded));

    assert_eq!(sources.len(), 4, "2 bundled + file label + sentinel");
    assert_eq!(sources[0], "dry_1");
    assert_eq!(sources[1], "dry_2");
    assert_eq!(sources[2], "my_loop.wav");
    assert_eq!(sources[3], CHOOSE_FILE_SENTINEL);
}

#[test]
fn sources_with_no_file_loaded_is_just_bundled_plus_sentinel() {
    let sources = build_di_loop_sources_with_loaded(&["dry_1"], None);
    assert_eq!(
        sources,
        vec!["dry_1".to_string(), CHOOSE_FILE_SENTINEL.to_string()]
    );
}

#[test]
fn sources_with_bundled_loaded_does_not_add_a_file_label() {
    let loaded = DiLoopSource::Bundled("dry_1".to_string());
    let sources = build_di_loop_sources_with_loaded(&["dry_1"], Some(&loaded));
    assert_eq!(
        sources,
        vec!["dry_1".to_string(), CHOOSE_FILE_SENTINEL.to_string()]
    );
}

#[test]
fn selected_index_points_at_the_loaded_file_label() {
    let loaded = DiLoopSource::File(PathBuf::from("/x/my_loop.wav"));
    let sources = build_di_loop_sources_with_loaded(&["dry_1", "dry_2"], Some(&loaded));
    let idx = di_loop_selected_index(&sources, &loaded);
    assert_eq!(
        idx, 2,
        "the file label sits at index 2 (after the 2 bundled ids)"
    );
}
