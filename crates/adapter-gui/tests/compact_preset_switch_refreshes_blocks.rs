//! Issue #667: switching the chain preset from the **compact view** updates
//! the audio (the project state changes, the tone changes) but leaves the
//! compact block list showing the PREVIOUS preset's blocks.
//!
//! The `on_switch_chain_preset` wiring in `compact_chain_callbacks::wire`
//! forwards to the main window via `invoke_switch_chain_preset` and stops —
//! it never rebuilds this window's own `compact_blocks` model. The
//! block-CRUD handlers (`compact_chain_block_handlers`,
//! `compact_chain_param_handlers`) DO call `build_compact_blocks` +
//! `set_compact_blocks` after every mutation, which is why insert/delete/
//! reorder refresh the compact list but a preset switch does not.
//!
//! Same class as #614 ("dispatch alone is dead"): the command runs, the
//! state changes, but the compact UI model is never re-projected.
//!
//! Source-presence wiring test, mirroring
//! `compact_block_search_wiring_tests::compact_chain_callbacks_wires_on_search_block_model_to_refilter`.
//! `compact_chain_callbacks.rs` already uses `set_compact_blocks` elsewhere
//! (the model-search wiring), so the assertion is scoped to the
//! `on_switch_chain_preset` closure body only — a whole-file `contains`
//! would pass falsely.

use std::path::PathBuf;

/// The body of the `on_switch_chain_preset` closure: bounded from the handler
/// registration to the next sibling header callback (`on_switch_chain_scene`),
/// so the assertion can never bleed into the model-search wiring further down
/// the file (which legitimately uses `set_compact_blocks`). A whole-file
/// `contains` would therefore pass falsely.
fn on_switch_chain_preset_closure() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/compact_chain_callbacks.rs");
    let src =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let needle = "on_switch_chain_preset";
    let start = src
        .find(needle)
        .unwrap_or_else(|| panic!("compact_chain_callbacks.rs has no `{needle}` handler"));
    // The next header callback wired after the preset one; everything between
    // is the preset closure (+ the #659 search-wiring line, which uses neither
    // `build_compact_blocks` nor `set_compact_blocks`).
    let rest = &src[start..];
    let end = rest
        .find("on_switch_chain_scene")
        .unwrap_or_else(|| panic!("expected `on_switch_chain_scene` to follow the preset handler"));
    rest[..end].to_string()
}

#[test]
fn switching_preset_in_compact_view_rebuilds_the_compact_blocks_model() {
    let closure = on_switch_chain_preset_closure();
    assert!(
        closure.contains("set_compact_blocks"),
        "the `on_switch_chain_preset` wiring must rebuild this window's \
         `compact_blocks` model after the preset switch (mirror the block \
         handlers: `build_compact_blocks` + `set_compact_blocks`) — otherwise \
         the audio changes but the compact block list stays on the previous \
         preset (#667). Closure read:\n{closure}"
    );
}

#[test]
fn switching_preset_in_compact_view_reprojects_blocks_via_build_compact_blocks() {
    let closure = on_switch_chain_preset_closure();
    assert!(
        closure.contains("build_compact_blocks"),
        "the `on_switch_chain_preset` wiring must re-project the chain's blocks \
         via `build_compact_blocks(project, chain_index)` before setting them \
         on the compact window (#667). Closure read:\n{closure}"
    );
}
