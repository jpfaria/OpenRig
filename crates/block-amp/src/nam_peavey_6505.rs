use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_peavey_6505";
pub const DISPLAY_NAME: &str = "6505";
const BRAND: &str = "peavey";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Two-axis pack: voicing × gain step. All captures from APP 6505+ pack.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (voicing, gain, file)
    ("clean",       "g02", "amps/peavey_6505/app_6505plus_clean_gain_02.nam"),
    ("clean",       "g04", "amps/peavey_6505/app_6505plus_clean_gain_04.nam"),
    ("clean",       "g06", "amps/peavey_6505/app_6505plus_clean_gain_06.nam"),
    ("clean",       "g07", "amps/peavey_6505/app_6505plus_clean_gain_07.nam"),
    ("clean",       "g08", "amps/peavey_6505/app_6505plus_clean_gain_08.nam"),
    ("clean",       "g09", "amps/peavey_6505/app_6505plus_clean_gain_09.nam"),
    ("mid_forward", "g07", "amps/peavey_6505/app_6505plus_midforward_gain_07.nam"),
    ("scooped",     "g06", "amps/peavey_6505/app_6505plus_scooped_gain_06.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Amp"),
            Some("clean"),
            &[
                ("clean",       "Clean"),
                ("mid_forward", "Mid Forward"),
                ("scooped",     "Scooped"),
            ],
        ),
        enum_parameter(
            "gain",
            "Gain",
            Some("Amp"),
            Some("g06"),
            &[
                ("g02", "G02"),
                ("g04", "G04"),
                ("g06", "G06"),
                ("g07", "G07"),
                ("g08", "G08"),
                ("g09", "G09"),
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
    let gain = required_string(params, "gain").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, g, _)| *v == voicing && *g == gain)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for voicing={} gain={}",
                MODEL_ID, voicing, gain
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
