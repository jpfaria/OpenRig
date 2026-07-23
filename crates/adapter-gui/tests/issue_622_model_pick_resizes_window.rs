//! Issue #622: choosing a plugin/model in the block editor must recalculate
//! the editor size, exactly like the parameter-update path does. A plugin with
//! more parameters than the previous one must not overflow an editor left at
//! the old size.
//!
//! The invariant is unchanged; only its location moved. Originally the picker
//! funnelled every model choice through `block_model_search_wiring`, which
//! called `apply_panel_dimensions` for the always-open persistent
//! `BlockEditorWindow`. #819 retired that window, so the recalc now lives in
//! the handler each editor actually owns:
//!
//! * detached — `block_editor_window_lifecycle`'s `on_choose_block_model`
//!   calls `apply_panel_dimensions` (the model-by-id picker reaches it via
//!   `invoke_choose_block_model`).
//! * inline (fullscreen/touch) — the main window's model paths publish the
//!   #500-computed height through `publish_inline_panel_height`.
//!
//! UI rendering is hard to assert without an `AppWindow`, so this follows the
//! crate's source-presence convention (see `no_native_dialogs.rs`): it flips
//! GREEN the moment the recalc is wired in and RED again on regression.

use std::path::PathBuf;

fn read_src(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn detached_editor_model_pick_recalculates_window_size() {
    let src = read_src("block_editor_window_lifecycle.rs");
    let handler = src
        .split_once("on_choose_block_model")
        .expect("block_editor_window_lifecycle must wire on_choose_block_model")
        .1;
    assert!(
        handler.contains("apply_panel_dimensions"),
        "issue #622: picking a model in the detached editor must recalculate \
         the window size (call `apply_panel_dimensions`, the same recalc the \
         parameter-update path runs). Without it, a plugin with more params \
         leaves the window at the previous size and the new params don't fit."
    );
}

#[test]
fn inline_editor_model_pick_recalculates_panel_height() {
    for (file, what) in [
        ("block_choose_type_callback.rs", "picking a block type"),
        ("block_insert_callbacks.rs", "changing the model"),
        ("select_chain_block_callback.rs", "opening an existing block"),
    ] {
        let src = read_src(file);
        assert!(
            src.contains("publish_inline_panel_height"),
            "issue #622 (inline editor, #819): {what} must republish the \
             #500-computed panel height from {file}, or the inline panel keeps \
             the previous size and clips the new block's knobs."
        );
    }
}
