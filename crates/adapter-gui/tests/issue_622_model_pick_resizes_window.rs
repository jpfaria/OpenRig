//! Issue #622: choosing a plugin/model in the block editor must recalculate
//! the editor window size, exactly like the parameter-update path does.
//!
//! The picker funnels every model choice through
//! `block_model_search_wiring`'s `on_choose_block_model_by_id` handler. That
//! handler rebuilt the parameter list but never resized the window, so a
//! plugin with more parameters than the previous one overflowed a window
//! left at the old size (the user had to see a too-small window / cut-off
//! params). The fix mirrors the update path: call `apply_panel_dimensions`
//! after the model is applied.
//!
//! UI rendering is hard to assert without an `AppWindow`, so this follows
//! the crate's source-presence convention (see `no_native_dialogs.rs`): it
//! flips GREEN the moment the recalc is wired in and RED again on regression.

use std::path::PathBuf;

fn read_src(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn choosing_a_model_by_id_recalculates_block_editor_window_size() {
    let src = read_src("block_model_search_wiring.rs");
    assert!(
        src.contains("apply_panel_dimensions"),
        "issue #622: block_model_search_wiring must recalculate the block \
         editor window size after a model pick (call `apply_panel_dimensions`, \
         the same recalc the parameter-update path runs). Without it, picking \
         a plugin with more params leaves the window at the previous size and \
         the new params don't fit."
    );
}
