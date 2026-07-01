//! #251: the native editor must reuse the engine's plugin instance and never
//! load a standalone one. A standalone editor instance creates a second copy of
//! the plugin whose GUI lifecycle corrupts the module (Valhalla) and then
//! breaks the engine's audio instance — the "VST3 não entra na chain" symptom.
//!
//! So the open policy is: engine context present → open (reuse); absent →
//! refuse with a clear reason, do NOT fall back to standalone loading.

use project::vst3_editor::require_engine_context;

#[test]
fn refuses_to_open_without_an_engine_instance() {
    assert!(
        require_engine_context(false).is_err(),
        "without an engine instance the editor must refuse (no standalone load)"
    );
    assert!(
        require_engine_context(true).is_ok(),
        "with an engine instance the editor opens by reusing it"
    );
}
