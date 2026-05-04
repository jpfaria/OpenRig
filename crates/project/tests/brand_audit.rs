//! Cross-crate brand audit — every model in every block-* registry must
//! declare a non-empty `brand` (or be on the legitimately-empty whitelist).
//!
//! Acceptance criterion of issue #194 Phase 4c: lint that fails if any
//! `MODEL_DEFINITION` carries `brand: ""` outside the whitelist. Runs as a
//! workspace-level integration test in the `project` crate, which already
//! depends on every block-* crate.

/// Models that are LEGITIMATELY brand-less:
///   - `classical`           — generic acoustic body (no manufacturer)
///   - `generic_ir`          — user-loaded IR file loader
///   - `neural_amp_modeler`  — user-loaded NAM file loader (GENERIC_NAM_MODEL_ID)
const BRAND_WHITELIST: &[&str] = &["classical", "generic_ir", "neural_amp_modeler"];

fn assert_models_have_brand<F>(crate_name: &str, models: &[&'static str], get_visual_brand: F)
where
    F: Fn(&str) -> Option<&'static str>,
{
    let mut offenders: Vec<String> = Vec::new();
    for model in models {
        if BRAND_WHITELIST.contains(model) {
            continue;
        }
        match get_visual_brand(model) {
            Some(brand) if !brand.is_empty() => {}
            Some(_) | None => offenders.push((*model).to_string()),
        }
    }
    assert!(
        offenders.is_empty(),
        "[{crate}] {n} model(s) have empty brand outside the whitelist: {offenders:?}",
        crate = crate_name,
        n = offenders.len(),
        offenders = offenders
    );
}

#[test]
fn all_block_models_declare_non_empty_brand() {
    assert_models_have_brand("block-cab", block_cab::supported_models(), |m| {
        block_cab::cab_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand("block-amp", block_amp::supported_models(), |m| {
        block_amp::amp_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand("block-preamp", block_preamp::supported_models(), |m| {
        block_preamp::preamp_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand("block-body", block_body::supported_models(), |m| {
        block_body::body_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand(
        "block-full-rig",
        block_full_rig::supported_models(),
        |m| block_full_rig::full_rig_model_visual(m).map(|v| v.brand),
    );
    assert_models_have_brand("block-delay", block_delay::supported_models(), |m| {
        block_delay::delay_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand("block-reverb", block_reverb::supported_models(), |m| {
        block_reverb::reverb_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand("block-dyn", block_dyn::supported_models(), |m| {
        block_dyn::dyn_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand("block-filter", block_filter::supported_models(), |m| {
        block_filter::filter_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand("block-mod", block_mod::supported_models(), |m| {
        block_mod::mod_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand("block-nam", block_nam::supported_models(), |m| {
        block_nam::nam_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand("block-pitch", block_pitch::supported_models(), |m| {
        block_pitch::pitch_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand("block-wah", block_wah::supported_models(), |m| {
        block_wah::wah_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand("block-util", block_util::supported_models(), |m| {
        block_util::util_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand("block-gain", block_gain::supported_models(), |m| {
        block_gain::gain_model_visual(m).map(|v| v.brand)
    });
    assert_models_have_brand("block-ir", block_ir::supported_models(), |m| {
        block_ir::ir_model_visual(m).map(|v| v.brand)
    });
}
