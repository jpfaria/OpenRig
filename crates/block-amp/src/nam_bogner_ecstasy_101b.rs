use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_bogner_ecstasy_101b";
pub const DISPLAY_NAME: &str = "Ecstasy 101B";
const BRAND: &str = "bogner";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

const CAPTURES: &[(&str, &str, &str)] = &[
    ("bogner_ecstasy_channel_2_warm_ro", "Bogner Ecstasy - Channel 2 Warm Rock Tone", "amps/bogner_ecstasy_101b/bogner_ecstasy_channel_2_warm_rock_tone.nam"),
    ("bogner_ecstasy_vai_ftlog_tone_b_", "Bogner Ecstasy - Vai FTLOG Tone (B off)", "amps/bogner_ecstasy_101b/bogner_ecstasy_vai_ftlog_tone_b_off.nam"),
    ("bogner_ecstasy_fender_clean", "Bogner Ecstasy - Fender Clean", "amps/bogner_ecstasy_101b/bogner_ecstasy_fender_clean.nam"),
    ("bogner_ecstasy_dumble_crunch", "Bogner Ecstasy - Dumble Crunch", "amps/bogner_ecstasy_101b/bogner_ecstasy_dumble_crunch.nam"),
    ("bogner_ecstasy_plexi_crunch", "Bogner Ecstasy - Plexi Crunch", "amps/bogner_ecstasy_101b/bogner_ecstasy_plexi_crunch.nam"),
    ("bogner_ecstasy_basic_clean", "Bogner Ecstasy - Basic Clean", "amps/bogner_ecstasy_101b/bogner_ecstasy_basic_clean.nam"),
    ("bogner_ecstasy_crunchier_rock_to", "Bogner Ecstasy - Crunchier Rock Tone", "amps/bogner_ecstasy_101b/bogner_ecstasy_crunchier_rock_tone.nam"),
    ("bogner_ecstasy_bright_crunchy_ro", "Bogner Ecstasy - Bright Crunchy Rock", "amps/bogner_ecstasy_101b/bogner_ecstasy_bright_crunchy_rock.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("bogner_ecstasy_channel_2_warm_ro"),
        &[
            ("bogner_ecstasy_channel_2_warm_ro", "Bogner Ecstasy - Channel 2 Warm Rock Tone"),
            ("bogner_ecstasy_vai_ftlog_tone_b_", "Bogner Ecstasy - Vai FTLOG Tone (B off)"),
            ("bogner_ecstasy_fender_clean", "Bogner Ecstasy - Fender Clean"),
            ("bogner_ecstasy_dumble_crunch", "Bogner Ecstasy - Dumble Crunch"),
            ("bogner_ecstasy_plexi_crunch", "Bogner Ecstasy - Plexi Crunch"),
            ("bogner_ecstasy_basic_clean", "Bogner Ecstasy - Basic Clean"),
            ("bogner_ecstasy_crunchier_rock_to", "Bogner Ecstasy - Crunchier Rock Tone"),
            ("bogner_ecstasy_bright_crunchy_ro", "Bogner Ecstasy - Bright Crunchy Rock"),
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
