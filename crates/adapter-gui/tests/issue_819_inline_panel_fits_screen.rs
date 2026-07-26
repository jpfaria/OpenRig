//! #819 — in fullscreen the inline block editor must stay within the screen.
//!
//! Intended behaviour: a param-heavy block (e.g. the Dragonfly Room reverb,
//! ~20 knobs) computes a #500 panel taller than the screen. `BlockPanelEditor`
//! is designed to *grow to fit* and never scroll (see its own comment: "the
//! panel window grows to fit every row ... never needs to scroll") — fine for
//! the detached OS window that grows with it, but the inline editor is an
//! overlay bounded by the fullscreen app window. When the panel is taller than
//! the screen its lower knobs spill past the bottom edge with no way to reach
//! them.
//!
//! So the inline host must bound the panel to the available page height and
//! make the overflow scrollable — every knob reachable no matter the count,
//! without touching the detached path.
//!
//! Source-presence check (the crate's `no_native_dialogs.rs` convention),
//! scoped to the inline panel-editor region so unrelated ScrollViews elsewhere
//! in the file (e.g. the block-type picker) can't produce a false pass.

/// Slice of `project_chains.slint` covering just the inline panel editor.
fn inline_panel_region() -> &'static str {
    let page = include_str!("../ui/pages/project_chains.slint");
    let start = page
        .find("Panel editor (preamp/amp with knobs)")
        .expect("inline panel-editor marker comment");
    let rest = &page[start..];
    let end = rest
        .find("Form editor (other block types)")
        .expect("form-editor marker follows the panel editor");
    &rest[..end]
}

#[test]
fn inline_editor_scrolls_when_the_panel_exceeds_the_screen() {
    let region = inline_panel_region();

    assert!(
        region.contains("Flickable") || region.contains("ScrollView"),
        "the inline BlockPanelEditor must be hosted in a Flickable/ScrollView so \
         a panel taller than the fullscreen screen can be scrolled to its lower \
         knobs (BlockPanelEditor itself never scrolls — it grows to fit)."
    );

    assert!(
        region.contains("Math.min") && region.contains("parent.height"),
        "the inline editor's visible height must be clamped to the page \
         (Math.min against parent.height), so a param-heavy block cannot overflow \
         the fullscreen screen."
    );
}
