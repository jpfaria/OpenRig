use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_marshall_jmp_2203";
pub const DISPLAY_NAME: &str = "JMP 2203";
const BRAND: &str = "marshall";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Three-axis pack: voicing × master_volume × boost. Sparse — only 5 of the
// 5×5×2 = 50 combinations were captured (each voicing was sampled at one MV).
// resolve_capture rejects invalid combos so the UI keeps the knobs independent.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (voicing, master_volume, boost, file)
    ("noon",    "3", "off", "amps/marshall_jmp_2203/slammin_marshall_2203_noon_mv3_48k_standard.nam"),
    ("scooped", "4", "off", "amps/marshall_jmp_2203/slammin_marshall_2203_scooped_mv4_48k_standard.nam"),
    ("dark",    "5", "off", "amps/marshall_jmp_2203/slammin_marshall_2203_dark_mv5_48k_standard.nam"),
    ("wylde",   "6", "sd1", "amps/marshall_jmp_2203/slammin_marshall_2203_wylde_sd1_mv6_48k.nam"),
    ("rock",    "7", "off", "amps/marshall_jmp_2203/slammin_marshall_2203_rock_mv7_48k.nam"),
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
                ("noon",    "Noon"),
                ("scooped", "Scooped"),
                ("dark",    "Dark"),
                ("wylde",   "Wylde"),
                ("rock",    "Rock"),
            ],
        ),
        enum_parameter(
            "master_volume",
            "Master Volume",
            Some("Amp"),
            Some("3"),
            &[
                ("3", "3"),
                ("4", "4"),
                ("5", "5"),
                ("6", "6"),
                ("7", "7"),
            ],
        ),
        enum_parameter(
            "boost",
            "Boost",
            Some("Amp"),
            Some("off"),
            &[
                ("off", "Off"),
                ("sd1", "SD1"),
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
    let mv = required_string(params, "master_volume").map_err(anyhow::Error::msg)?;
    let boost = required_string(params, "boost").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(v, m, b, _)| *v == voicing && *m == mv && *b == boost)
        .map(|(_, _, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "amp '{}' has no capture for voicing={} master_volume={} boost={}",
                MODEL_ID, voicing, mv, boost
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
