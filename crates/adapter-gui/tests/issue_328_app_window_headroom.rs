//! #328 — app-window.slint sits at 498/500 lines; the chain graph needs two
//! globals, two root overlays and three test harnesses exported from it
//! (spec §5.4). The modal dialogs that only read `OverlayBridge` move to their
//! own component first, behaviour-preserving.

use std::path::PathBuf;

fn read(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

const MOVED: [&str; 5] = [
    "ConfirmDeleteBlockDialog {",
    "ConfirmDeleteChainDialog {",
    "ConfirmDeleteRecentProjectDialog {",
    "PresetPickerOverlay {",
    "Toast {",
];

#[test]
fn app_window_leaves_room_for_the_chain_graph_overlays() {
    // Task 1 leaves ~451 lines; Tasks 8, 11, 14 and 15 add ~11 import and
    // overlay lines, so the part ends near 462 — still 20 under the cap.
    let lines = read("ui/app-window.slint").lines().count();
    assert!(
        lines <= 480,
        "app-window.slint has {lines} lines; the chain graph needs ~20 more under the 500 cap"
    );
}

#[test]
fn the_modal_dialogs_render_from_their_own_component() {
    let app = read("ui/app-window.slint");
    assert!(
        app.contains("RootModalOverlays {"),
        "app-window must host RootModalOverlays"
    );
    for moved in MOVED {
        assert!(
            !app.contains(moved),
            "{moved} must render from root_modal_overlays.slint"
        );
    }
    let overlays = read("ui/components/root_modal_overlays.slint");
    for kept in MOVED {
        assert!(
            overlays.contains(kept),
            "{kept} missing from root_modal_overlays.slint"
        );
    }
}
