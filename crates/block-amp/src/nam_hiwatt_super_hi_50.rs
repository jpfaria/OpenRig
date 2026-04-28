use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_hiwatt_super_hi_50";
pub const DISPLAY_NAME: &str = "Super-Hi 50";
const BRAND: &str = "hiwatt";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: voicing × mic.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, mic, file)
    ("noon",     "di",     "amps/hiwatt_super_hi_50/amp_hwat_superhi50_noon_04_di.nam"),
    ("noon",     "blend1", "amps/hiwatt_super_hi_50/amp_hwat_superhi50_noon_04_blend_1.nam"),
    ("noon",     "blend3", "amps/hiwatt_super_hi_50/amp_hwat_superhi50_noon_04_blend_3.nam"),
    ("bright_od","sm57",   "amps/hiwatt_super_hi_50/amp_hwat_superhi50_bright_overdrive_sm57.nam"),
    ("bright_od","di",     "amps/hiwatt_super_hi_50/amp_hwat_superhi50_bright_overdrive_di.nam"),
    ("bright_od","blend1", "amps/hiwatt_super_hi_50/amp_hwat_superhi50_bright_overdrive_blend_1.nam"),
    ("bright_od","blend2", "amps/hiwatt_super_hi_50/amp_hwat_superhi50_bright_overdrive_blend_2.nam"),
    ("bright_od","blend3", "amps/hiwatt_super_hi_50/amp_hwat_superhi50_bright_overdrive_blend_3.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("noon"),
            &[
                ("noon",      "Noon"),
                ("bright_od", "Bright Overdrive"),
            ],
        ),
        enum_parameter(
            "mic",
            "Mic",
            Some("Amp"),
            Some("blend1"),
            &[
                ("sm57",   "SM57"),
                ("di",     "DI (No Cab)"),
                ("blend1", "Blend #1"),
                ("blend2", "Blend #2"),
                ("blend3", "Blend #3"),
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
    let voicing = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    let mic = required_string(params, "mic").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, m, _)| *v == voicing && *m == mic)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for voicing={} mic={}",
                MODEL_ID, voicing, mic
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
