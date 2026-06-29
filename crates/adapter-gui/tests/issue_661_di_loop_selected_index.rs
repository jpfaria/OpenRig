//! Issue #661 — RED-FIRST tests for `di_loop_selected_index`.
//!
//! Maps the currently selected `DiLoopSource` to its row index inside the
//! ComboBox source list so the popup can highlight it on reopen. Pure (no
//! I/O, no AppWindow). Lives in `adapter_gui::di_loop_ui_sources`.
//!
//! - `Bundled(id)` present in the list → its index.
//! - `Bundled(id)` absent           → -1.
//! - `File(_)` (not in the bundled list) → -1.

use application::di_loader::DiLoopSource;

use adapter_gui::di_loop_ui_sources::{build_di_loop_sources, di_loop_selected_index};

#[test]
fn selected_index_returns_position_of_bundled_source() {
    let sources = build_di_loop_sources(&["dry_1", "dry_2", "dry_3"]);
    let idx = di_loop_selected_index(&sources, &DiLoopSource::Bundled("dry_2".to_string()));
    assert_eq!(idx, 1, "dry_2 is at index 1");
}

#[test]
fn selected_index_first_bundled_source_is_zero() {
    let sources = build_di_loop_sources(&["dry_1", "dry_2"]);
    let idx = di_loop_selected_index(&sources, &DiLoopSource::Bundled("dry_1".to_string()));
    assert_eq!(idx, 0);
}

#[test]
fn selected_index_unknown_bundled_returns_minus_one() {
    let sources = build_di_loop_sources(&["dry_1"]);
    let idx = di_loop_selected_index(&sources, &DiLoopSource::Bundled("ghost".to_string()));
    assert_eq!(idx, -1, "an id not in the list has no row to highlight");
}

#[test]
fn selected_index_file_source_returns_minus_one() {
    let sources = build_di_loop_sources(&["dry_1"]);
    let idx = di_loop_selected_index(
        &sources,
        &DiLoopSource::File(std::path::PathBuf::from("/tmp/whatever.wav")),
    );
    assert_eq!(
        idx, -1,
        "a File source is not represented in the bundled list"
    );
}
