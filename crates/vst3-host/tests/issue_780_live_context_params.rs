//! #780 repro: with the chain ON, a live VST3 instance is registered in the
//! param registry (engine's `register_vst3_gui_context`). The compact view then
//! resolves params through the LIVE context (`live_params_for_model`) instead of
//! a throw-away load. This drives exactly that path and asserts params are read
//! from a registered live instance — the case where the compact view showed a
//! VST3 block with zero knobs.
//!
//! Env-gated on OPENRIG_TEST_VST3_DIR; run with --test-threads=1.

use std::path::PathBuf;

const SR: f64 = 48_000.0;

#[test]
fn catalog_params_reads_a_registered_live_instance() {
    let Some(dir) = std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from) else {
        return;
    };
    vst3_host::init_vst3_catalog(SR, &[dir]);
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
    let model = entry.model_id.to_string();
    let uid = vst3_host::resolve_uid_for_model(&model).expect("resolve uid");

    // Load an instance and register it as the engine does for a streaming block.
    let plugin = vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, 512, &[])
        .expect("load vst3 instance");
    let _channel = vst3_host::register_vst3_gui_context(
        "live-block-1",
        &model,
        plugin.controller_clone(),
        plugin.library_arc(),
    );

    // Now the compact view's path: catalog_params → live_params_for_model.
    let params = vst3_host::catalog_params(&model);
    assert!(
        !params.is_empty(),
        "catalog_params returned ZERO params from a REGISTERED live instance (model {})",
        model
    );
}
