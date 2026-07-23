//! #819 — the single persistent `BlockEditorWindow` was retired. Since #815
//! every detached editor is built per-block via `create_and_wire` (add and
//! edit alike), so the startup-created persistent window and its duplicated
//! wiring are dead. This guard pins that they stay gone.

/// `desktop_app.rs` must not construct a persistent `BlockEditorWindow`.
#[test]
fn desktop_app_builds_no_persistent_block_editor_window() {
    let src = include_str!("desktop_app.rs");
    assert_eq!(
        src.matches("BlockEditorWindow::new").count(),
        0,
        "the persistent BlockEditorWindow is retired; editors are built per-block via create_and_wire"
    );
}

/// The dead sync helper for the persistent window must be gone.
#[test]
fn sync_block_editor_window_helper_is_removed() {
    let src = include_str!("helpers.rs");
    assert!(
        !src.contains("fn sync_block_editor_window"),
        "sync_block_editor_window synced the retired persistent window; it must be deleted"
    );
}
