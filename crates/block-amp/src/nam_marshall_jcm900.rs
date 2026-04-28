use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_jcm900";
pub const DISPLAY_NAME: &str = "JCM900";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: amp model × tone.
// JCM900 + JTM45 + Vox AC15 + Dumble ODS captures.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (amp_model, tone, file)
    ("vox_ac15", "clean",   "amps/marshall_jcm900/vox_ac15_clean.nam"),
    ("vox_ac15", "crunch",  "amps/marshall_jcm900/vox_ac15_crunsh.nam"),
    ("vox_ac15", "crsh_2",  "amps/marshall_jcm900/vox_ac15_crsh_2.nam"),
    ("jtm_45",   "clean",   "amps/marshall_jcm900/marshall_jtm_45_clean.nam"),
    ("jtm_45",   "crsh",    "amps/marshall_jcm900/jtm_45_crsh.nam"),
    ("jcm_900",  "higain",  "amps/marshall_jcm900/marshall_jcm_900_higain.nam"),
    ("dumble",   "clean",   "amps/marshall_jcm900/ods_dumble_clean.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "amp_model",
            "Amp Model",
            Some("Amp"),
            Some("jcm_900"),
            &[
                ("vox_ac15", "Vox AC15"),
                ("jtm_45",   "JTM 45"),
                ("jcm_900",  "JCM 900"),
                ("dumble",   "Dumble ODS"),
            ],
        ),
        enum_parameter(
            "tone",
            "Tone",
            Some("Amp"),
            Some("higain"),
            &[
                ("clean",   "Clean"),
                ("crunch",  "Crunch"),
                ("crsh_2",  "Crunch (Take 2)"),
                ("crsh",    "Crsh"),
                ("higain",  "Hi-Gain"),
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
    let amp_model = required_string(params, "amp_model").map_err(anyhow::Error::msg)?;
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(a, t, _)| *a == amp_model && *t == tone)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for amp_model={} tone={}",
                MODEL_ID, amp_model, tone
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
