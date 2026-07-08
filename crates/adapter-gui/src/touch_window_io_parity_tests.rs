//! Parity guard: the touch layout and secondary chain-editor windows must
//! forward `io-binding-names` from the root I/O binding list into the inline
//! `ChainEndpointEditorPage` overlays.  Without this the endpoint picker
//! renders an empty dropdown on touch / fullscreen mode.
//!
//! These are source-presence tests (reading the Slint source) so they catch
//! regressions without needing a live Slint runtime.  They mirror the
//! convention in `i18n_tests.rs` and `block_editor_window_lifecycle.rs`.

use std::fs;
use std::path::Path;

fn ui_dir() -> std::path::PathBuf {
    // Resolve relative to this file at compile time so the tests work from
    // any working directory (CI or local solver).
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ui")
}

fn read_slint(name: &str) -> String {
    let path = ui_dir().join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e))
}

// ── touch_main: inline endpoint editor must bind io-binding-names from root ───

/// The inline `ChainEndpointEditorPage` overlay in `touch_main.slint` that
/// appears in fullscreen mode must forward `root.input-io-binding-names` (or
/// equivalent root property) rather than the hard-coded empty literal `[]`.
///
/// When this test is RED the Slint file contains `io-binding-names: []` inside
/// the fullscreen chain-io-editor block, which means the endpoint picker has no
/// bindings to show on touch / fullscreen builds.
#[test]
fn touch_main_inline_input_endpoint_editor_binds_io_binding_names() {
    let src = read_slint("touch_main.slint");

    // The guard: if we find the hard-coded empty literal inside the
    // ChainEndpointEditorPage block in touch_main, the parity is broken.
    //
    // We look for the pattern `io-binding-names: []` which is what the desktop
    // inline editor uses when it intentionally passes an empty list (the old
    // non-binding path).  The touch layout must instead forward the root
    // property.
    assert!(
        !src.contains("show-chain-io-editor") || !src.contains("io-binding-names: []"),
        "touch_main.slint: the inline ChainEndpointEditorPage overlay must \
         forward io-binding-names from the root property, not pass `[]`. \
         Found `io-binding-names: []` inside a show-chain-io-editor block."
    );
}

/// The inline `ChainEndpointEditorPage` overlay in `touch_main.slint` must
/// declare `input-io-binding-names` (or `io-binding-names`) as a root
/// property so the Rust wiring layer can populate it from `AppConfig`.
#[test]
fn touch_main_exposes_io_binding_names_root_property() {
    let src = read_slint("touch_main.slint");

    // The root TouchMain component must declare an io-binding-names property
    // (same as DesktopMain) so the Rust init layer can bind it.
    assert!(
        src.contains("io-binding-names"),
        "touch_main.slint: root TouchMain component must expose \
         `io-binding-names` property (same as DesktopMain / AppWindow) \
         so the Rust wiring can populate the endpoint picker. Not found."
    );
}

// #716: the inline per-endpoint editor overlays (`show-input-editor` /
// `show-output-editor` `ChainEndpointEditorPage`) were removed from
// `ChainEditorWindow` — the chain now selects I/O via the binding checklist,
// so the two parity guards for those overlays no longer apply.

// ── desktop_app_init: io_bindings::wire is called for both AppWindow and ──────
// ── ProjectSettingsWindow.  Since we cannot call AppWindow::new() in tests ─────
// ── without a display, we guard via source-presence on the Rust side.      ─────

/// The `settings::io_bindings::wire` function must accept both the `AppWindow`
/// and the `ProjectSettingsWindow` as parameters and seed / install callbacks
/// on both so the settings panel inside the secondary window has live I/O
/// binding CRUD.  Guard that the function signature in `io_bindings.rs` takes
/// `project_settings_window` as the second argument.
#[test]
fn io_bindings_wire_function_takes_project_settings_window() {
    let src = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("settings")
            .join("io_bindings.rs"),
    )
    .expect("cannot read settings/io_bindings.rs");

    // The wire function must accept a ProjectSettingsWindow parameter so
    // it can seed and install callbacks on the secondary settings window.
    assert!(
        src.contains("project_settings_window: &ProjectSettingsWindow")
            || src.contains("psw: &ProjectSettingsWindow"),
        "settings/io_bindings.rs: `wire()` must accept a \
         `ProjectSettingsWindow` parameter to achieve parity between \
         the main AppWindow and the secondary settings window.  Not found."
    );
}
