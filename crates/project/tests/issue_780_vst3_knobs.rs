//! Issue #780 — a catalog VST3 must produce an OpenRig parameter schema (knobs),
//! so its params are editable through the standard, tested SetBlockParameter →
//! in-place update path rather than only the native editor. The schema paths are
//! `p{id}`, matching what the engine's in-place VST3 update expects.
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
fn vst3_block_gets_openrig_knobs() {
    let Some(model) = chow_model() else { return };

    let schema = project::block::schema_for_block_model(block_core::EFFECT_TYPE_VST3, &model)
        .expect("VST3 schema builds");
    assert!(
        !schema.parameters.is_empty(),
        "a catalog VST3 must expose OpenRig knobs (schema had no parameters)"
    );
    assert!(
        schema
            .parameters
            .iter()
            .all(|p| p.path.starts_with('p') && p.path[1..].chars().all(|c| c.is_ascii_digit())),
        "every VST3 knob path must be p{{id}} so the engine's in-place update applies it"
    );
}
