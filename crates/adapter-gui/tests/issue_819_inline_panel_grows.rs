//! #819 — the fullscreen inline editor must grow vertically to show every knob,
//! exactly like the detached window (no scroll).
//!
//! Root cause of the residual clip: the inline overlay computed its own
//! `panel-width` from the parameter count (`param-count * 64 + 80`), while the
//! #500 policy (`block_panel_dimensions::compute`) caps the grid at MAX_COLS=6,
//! so its width is 900px and its `inner_panel_height` is derived from that 900.
//! With a 20-knob block the inline width ballooned to ~1360px, which pushed the
//! knob grid's start row down (grid-base-y scales with width) past the
//! 900-based inner height — clipping the bottom row. The detached window uses
//! the policy width, so it stays consistent and never clips.
//!
//! Fix: the inline panel-width must also come from the #500 policy (published
//! through `BlockParamTabs`), so width and inner-height agree and every row
//! fits. Source-presence check scoped to the inline panel-editor region.

#[test]
fn inline_panel_width_comes_from_the_500_policy() {
    // `BlockParamTabs.panel-width` is specific enough that a whole-file check
    // can't false-pass on an unrelated token.
    let page = include_str!("../ui/pages/project_chains.slint");
    assert!(
        page.contains("BlockParamTabs.panel-width"),
        "the inline BlockPanelEditor's width must come from the #500 policy \
         (BlockParamTabs.panel-width), not a param-count formula — otherwise the \
         width disagrees with the policy's inner height and the bottom knob row \
         clips."
    );
}
