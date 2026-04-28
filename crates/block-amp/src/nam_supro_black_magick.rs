use anyhow::{anyhow, Result};
use crate::registry::{AmpBackendKind, AmpModelDefinition};
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_supro_black_magick";
pub const DISPLAY_NAME: &str = "Supro Black Magick";
const BRAND: &str = "supro";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// 2-axis: tone setting (3-6) × speaker (SRL stock vs P12Q upgrade). Sparse:
// only T5 has both speakers; other tone steps only with stock SRL. Knob layout
// V1=5, V2=0, In 1-2 fixed.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (tone, speaker, _, file)
    ("3", "srl",  "", "amps/supro_black_magick/di_supro_1695tj_in1_2_v1_5_v2_0_t_3_srl.nam"),
    ("4", "srl",  "", "amps/supro_black_magick/di_supro_1695tj_in1_2_v1_5_v2_0_t_4_srl.nam"),
    ("5", "srl",  "", "amps/supro_black_magick/di_supro_1695tj_in1_2_v1_5_v2_0_t_5_srl.nam"),
    ("5", "p12q", "", "amps/supro_black_magick/di_supro_1695tj_in1_2_v1_5_v2_0_t_5_p12q.nam"),
    ("6", "srl",  "", "amps/supro_black_magick/di_supro_1695tj_in1_2_v1_5_v2_0_t_6_srl.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("amp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "tone",
            "Tone",
            Some("Amp"),
            Some("5"),
            &[
                ("3", "3"),
                ("4", "4"),
                ("5", "5"),
                ("6", "6"),
            ],
        ),
        enum_parameter(
            "speaker",
            "Speaker",
            Some("Amp"),
            Some("srl"),
            &[
                ("srl",  "SRL (stock)"),
                ("p12q", "P12Q"),
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
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    let speaker = required_string(params, "speaker").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(t, s, _, _)| *t == tone && *s == speaker)
        .map(|(_, _, _, path)| *path)
        .ok_or_else(|| anyhow!(
            "amp '{}' has no capture for tone='{}' speaker='{}'",
            MODEL_ID, tone, speaker
        ))
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
