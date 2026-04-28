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

// Two-axis pack: EQ setting × mic+speaker.
// EQ encodes Volume/Treble/Bass front-panel settings.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (eq, mic_speaker, file)
    ("v3_t8_b2",   "sm57_orig",       "amps/fender_princeton_reverb_1972/princ_v3_t8_b2_sm57_orig_spkr.nam"),
    ("v3_5_t6_b3", "sm57_orig",       "amps/fender_princeton_reverb_1972/princ_v3_5_t6_b3_sm57_orig_spkr.nam"),
    ("v4_t7_b2",   "sm57_off_jensen", "amps/fender_princeton_reverb_1972/princ_v4_t7_b2_sm57offcntr_jensen_c10q.nam"),
    ("v4_t7_b2",   "heil_pr30_jensen","amps/fender_princeton_reverb_1972/princ_v4_t7_b2_heil_pr30center_jensen_c10q.nam"),
    ("v5_t7_b2",   "sm57_jensen",     "amps/fender_princeton_reverb_1972/princ_v5_t7_b2_sm57_jensen_c10q.nam"),
    ("v5_t7_b2",   "heil_pr30_jensen","amps/fender_princeton_reverb_1972/princ_v5_t7_b2_heil_pr_30_jensen_c10q.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "eq",
            "EQ Setting",
            Some("Amp"),
            Some("v5_t7_b2"),
            &[
                ("v3_t8_b2",   "V3 / T8 / B2"),
                ("v3_5_t6_b3", "V3.5 / T6 / B3"),
                ("v4_t7_b2",   "V4 / T7 / B2"),
                ("v5_t7_b2",   "V5 / T7 / B2"),
            ],
        ),
        enum_parameter(
            "mic_speaker",
            "Mic + Speaker",
            Some("Amp"),
            Some("sm57_jensen"),
            &[
                ("sm57_orig",        "SM57 (Orig)"),
                ("sm57_jensen",      "SM57 (Jensen)"),
                ("sm57_off_jensen",  "SM57 Off (Jensen)"),
                ("heil_pr30_jensen", "Heil PR30 (Jensen)"),
            ],
        ),
    ];
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
    let eq = required_string(params, "eq").map_err(anyhow::Error::msg)?;
    let mic_speaker = required_string(params, "mic_speaker").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(e, m, _)| *e == eq && *m == mic_speaker)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for eq={} mic_speaker={}",
                MODEL_ID, eq, mic_speaker
            )
        })
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
