use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_eden_e300t";
pub const DISPLAY_NAME: &str = "E300T";
const BRAND: &str = "eden";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("glowplug", "Glowplug", "amps/eden_e300t/glowplug.nam"),
    ("e300t_flat_gain_6", "E300T Flat (Gain 6)", "amps/eden_e300t/e300t_flat_gain_6.nam"),
    ("wtdi", "WTDI", "amps/eden_e300t/wtdi.nam"),
    ("wtdi_2", "WTDI 2", "amps/eden_e300t/wtdi_2.nam"),
    ("e300t_eq_1_gain_6_bass_2_mid_3_s", "E300T EQ 1 (Gain 6, Bass +2, Mid +3, Shift ON, Treb 0)", "amps/eden_e300t/e300t_eq_1_gain_6_bass_2_mid_3_shift_on_treb_0.nam"),
    ("e300t_eq_2_gain_6_bass_3_mid_2_s", "E300T EQ 2 (Gain 6, Bass +3, Mid +2, Shift ON, Treb 0)", "amps/eden_e300t/e300t_eq_2_gain_6_bass_3_mid_2_shift_on_treb_0.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("glowplug"),
        &[
            ("glowplug", "Glowplug"),
            ("e300t_flat_gain_6", "E300T Flat (Gain 6)"),
            ("wtdi", "WTDI"),
            ("wtdi_2", "WTDI 2"),
            ("e300t_eq_1_gain_6_bass_2_mid_3_s", "E300T EQ 1 (Gain 6, Bass +2, Mid +3, Shift ON, Treb 0)"),
            ("e300t_eq_2_gain_6_bass_3_mid_2_s", "E300T EQ 2 (Gain 6, Bass +3, Mid +2, Shift ON, Treb 0)"),
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
