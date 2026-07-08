//! #251: the native editor must reuse the engine's plugin instance and never
//! load a standalone one. A standalone editor instance creates a second copy of
//! the plugin whose GUI lifecycle corrupts the module (Valhalla) and then
//! breaks the engine's audio instance — the "VST3 não entra na chain" symptom.
//!
//! So the open policy is: engine context present → open (reuse); absent →
//! refuse with a clear reason, do NOT fall back to standalone loading.

use project::vst3_editor::{has_engine_context, require_engine_context};
use std::path::PathBuf;

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

// #780: the editor-open lookup resolves the engine instance by a per-block key,
// not by model_id, so two blocks of the same plugin address their own instance.
// Env-gated on OPENRIG_TEST_VST3_DIR; run with --test-threads=1.
#[test]
fn has_engine_context_resolves_by_block_instance_key() {
    let Some(dir) = std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from) else {
        return;
    };
    vst3_host::init_vst3_catalog(48_000.0, &[dir]);
    let Some(entry) = vst3_host::vst3_catalog().iter().find(|e| {
        e.info
            .bundle_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("ChowCentaur.vst3"))
            .unwrap_or(false)
    }) else {
        return;
    };
    let uid = vst3_host::resolve_uid_for_model(&entry.model_id).unwrap();
    let plugin =
        vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, 48_000.0, 2, 512, &[]).unwrap();
    let _ = vst3_host::register_vst3_gui_context(
        "rig:gtr:block:3",
        &entry.model_id,
        plugin.controller_clone(),
        plugin.library_arc(),
    );

    assert!(
        has_engine_context("rig:gtr:block:3"),
        "the block instance key must resolve the registered context"
    );
    assert!(
        !has_engine_context(&entry.model_id),
        "model_id must NOT resolve a context — keying is per block instance (#780)"
    );

    // The open path takes the block instance key and recovers the plugin model
    // from the registered context (needed to resolve the catalog entry), rather
    // than being handed the model id. Window-free proof of that resolution.
    assert_eq!(
        project::vst3_editor::editor_model_for("rig:gtr:block:3").as_deref(),
        Some(entry.model_id),
        "open path must recover the model from the block's context"
    );
    assert!(
        project::vst3_editor::editor_model_for(&entry.model_id).is_none(),
        "a bare model id is not a registered instance key"
    );
    drop(plugin);
}
