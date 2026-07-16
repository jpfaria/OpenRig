//! Issue #511: replace all native OS dialogs with in-app Slint dialogs.
//!
//! `rfd` (the native dialog crate) is a recurring source of cross-platform
//! bugs (macOS focus stealing, KDE Wayland glitches, Orange Pi touch sessions
//! that don't fit the touchscreen). Acceptance criterion: no `rfd::` symbol
//! left in `crates/adapter-gui/src/`.
//!
//! These source-presence tests are intentionally cheap and obvious: they
//! flip GREEN the moment each migrated site drops its `rfd::` usage, and
//! flip RED again on any regression that re-introduces a native dialog.
//! UI rendering is hard to assert without an `AppWindow`; the dispatch
//! contract is covered separately in each wiring's own callback tests.

use std::path::PathBuf;

fn read_src(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn chain_row_wiring_uses_no_native_dialog() {
    let src = read_src("chain_row_wiring.rs");
    assert!(
        !src.contains("rfd::"),
        "issue #511: chain_row_wiring.rs must not use `rfd::` (native \
         dialog steals focus on macOS and does not fit Orange Pi touch \
         sessions). Use the in-app `ConfirmDelete*Dialog` overlay pattern \
         instead — see `confirm_delete_dialog.slint`."
    );
}

#[test]
fn block_editor_window_lifecycle_uses_no_native_dialog() {
    let src = read_src("block_editor_window_lifecycle.rs");
    assert!(
        !src.contains("rfd::"),
        "issue #360: block_editor_window_lifecycle.rs must not use `rfd::` \
         (delete-block confirmation is the last native MessageDialog in this \
         crate). Use an in-window overlay on `BlockEditorWindow` mirroring \
         the `ConfirmDeleteBlockDialog` pattern already in use on AppWindow."
    );
}

// ── Issue #360: every destructive action must raise its overlay before
// dispatching the removal Command. Source-presence pins ensure the gate
// stays in place — if someone re-inlines the dispatch without the
// confirm step, these flip RED.

#[test]
fn compact_view_block_delete_is_gated_by_overlay() {
    let src = read_src("compact_chain_block_handlers.rs");
    assert!(
        src.contains("set_show_confirm_delete_block"),
        "issue #360: compact view must raise the in-window block-delete \
         overlay (set_show_confirm_delete_block) before dispatching \
         Command::RemoveBlock — the previous wiring removed the block \
         silently on click."
    );
}

#[test]
fn compact_view_chain_delete_is_gated_by_overlay() {
    // The wiring moved to its own module (#787, file-size cap).
    let src = read_src("compact_chain_delete_wiring.rs");
    assert!(
        src.contains("set_show_confirm_delete_chain"),
        "issue #360: compact view must raise its OWN in-window chain-delete \
         overlay (set_show_confirm_delete_chain) — delegating to \
         AppWindow.invoke_remove_chain surfaces the modal on the wrong \
         window."
    );
}

#[test]
fn recent_project_remove_is_gated_by_overlay() {
    let src = read_src("recent_projects_wiring.rs");
    assert!(
        src.contains("set_show_confirm_delete_recent_project"),
        "issue #360: removing a recent-project entry must raise the \
         confirmation overlay (set_show_confirm_delete_recent_project) \
         first; on_remove_recent_project used to call \
         Command::RemoveRecentProject with no confirmation at all."
    );
}
