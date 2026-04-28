use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_fender_princeton_reverb_1972";
pub const DISPLAY_NAME: &str = "Princeton Reverb 1972";
const BRAND: &str = "fender";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("princ_v5_t7_b2_sm57_jensen_c10q", "Princ_v5-t7-b2 SM57 Jensen-C10Q", "amps/fender_princeton_reverb_1972/princ_v5_t7_b2_sm57_jensen_c10q.nam"),
    ("princ_v5_t7_b2_heil_pr_30_jensen", "Princ_v5-t7-b2 Heil-PR-30 Jensen-C10Q", "amps/fender_princeton_reverb_1972/princ_v5_t7_b2_heil_pr_30_jensen_c10q.nam"),
    ("princ_v3_t8_b2_sm57_orig_spkr", "Princ_v3_t8_b2 SM57 orig spkr", "amps/fender_princeton_reverb_1972/princ_v3_t8_b2_sm57_orig_spkr.nam"),
    ("princ_v3_5_t6_b3_sm57_orig_spkr", "Princ_v3.5_t6_b3 SM57 orig spkr", "amps/fender_princeton_reverb_1972/princ_v3_5_t6_b3_sm57_orig_spkr.nam"),
    ("princ_v4_t7_b2_sm57offcntr_jense", "Princ_v4_t7_b2 SM57offcntr_Jensen C10Q", "amps/fender_princeton_reverb_1972/princ_v4_t7_b2_sm57offcntr_jensen_c10q.nam"),
    ("princ_v4_t7_b2_heil_pr30center_j", "Princ_v4_t7_b2 Heil-PR30center_Jensen C10Q", "amps/fender_princeton_reverb_1972/princ_v4_t7_b2_heil_pr30center_jensen_c10q.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("princ_v5_t7_b2_sm57_jensen_c10q"),
        &[
            ("princ_v5_t7_b2_sm57_jensen_c10q", "Princ_v5-t7-b2 SM57 Jensen-C10Q"),
            ("princ_v5_t7_b2_heil_pr_30_jensen", "Princ_v5-t7-b2 Heil-PR-30 Jensen-C10Q"),
            ("princ_v3_t8_b2_sm57_orig_spkr", "Princ_v3_t8_b2 SM57 orig spkr"),
            ("princ_v3_5_t6_b3_sm57_orig_spkr", "Princ_v3.5_t6_b3 SM57 orig spkr"),
            ("princ_v4_t7_b2_sm57offcntr_jense", "Princ_v4_t7_b2 SM57offcntr_Jensen C10Q"),
            ("princ_v4_t7_b2_heil_pr30center_j", "Princ_v4_t7_b2 Heil-PR30center_Jensen C10Q"),
        ],
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let path = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let key = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("amp '{}' has no capture '{}'", MODEL_ID, key))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: AmpModelDefinition = AmpModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: AmpBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let path = resolve_capture(params)?;
    Ok(format!("model='{}'", path))
}
