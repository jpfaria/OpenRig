//! Issue #780 — the catalog must expose a VST3's real parameters so OpenRig can
//! render knobs for it (routing param changes through the tested in-place update
//! path instead of the native editor alone). The light discovery scan leaves
//! `entry.info.params` empty; `catalog_params` fills it from the controller.
//!
//! Env-gated on OPENRIG_TEST_VST3_DIR; run with --test-threads=1.

use std::path::PathBuf;

const SR: f64 = 48_000.0;

fn chow_model() -> Option<String> {
    let dir = std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from)?;
    vst3_host::init_vst3_catalog(SR, &[dir]);
    vst3_host::vst3_catalog()
        .iter()
        .find(|e| {
            e.info
                .bundle_path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("ChowCentaur.vst3"))
                .unwrap_or(false)
        })
        .map(|e| e.model_id.to_string())
}

#[test]
fn catalog_params_reads_real_vst3_parameters() {
    let Some(model) = chow_model() else { return };

    // The light-scan catalog entry has no params...
    let entry = vst3_host::find_vst3_plugin(&model).unwrap();
    assert!(
        entry.info.params.is_empty(),
        "precondition: light scan leaves entry.info.params empty"
    );

    // ...but catalog_params reads them from the controller.
    let params = vst3_host::catalog_params(&model);
    assert!(
        !params.is_empty(),
        "catalog_params must expose the plugin's real parameters (got none)"
    );
    assert!(
        params.iter().any(|p| !p.title.is_empty()),
        "parameters should carry human titles for the knob labels"
    );

    // A second call is served from the cache and returns the same set.
    let again = vst3_host::catalog_params(&model);
    assert_eq!(
        params.len(),
        again.len(),
        "cached call returns the same parameter set"
    );
}
