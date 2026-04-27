use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_way_huge_swollen_pickle";
pub const DISPLAY_NAME: &str = "Way Huge Swollen Pickle";
const BRAND: &str = "way_huge";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "boss_os_2",        model_path: "pedals/way_huge_swollen_pickle/boss_os_2.nam" },
    NamCapture { tone: "buxom_boost_1",    model_path: "pedals/way_huge_swollen_pickle/buxom_boost_1.nam" },
    NamCapture { tone: "buxom_boost_2",    model_path: "pedals/way_huge_swollen_pickle/buxom_boost_2.nam" },
    NamCapture { tone: "buxom_boost_3",    model_path: "pedals/way_huge_swollen_pickle/buxom_boost_3.nam" },
    NamCapture { tone: "gci_jugendstil",   model_path: "pedals/way_huge_swollen_pickle/gci_jugendstil.nam" },
    NamCapture { tone: "pp_rot_mid_boost", model_path: "pedals/way_huge_swollen_pickle/pp_rot_mid_boost.nam" },
    NamCapture { tone: "pp_rot_mid_scoop", model_path: "pedals/way_huge_swollen_pickle/pp_rot_mid_scoop.nam" },
    NamCapture { tone: "pp_wendigo",       model_path: "pedals/way_huge_swollen_pickle/pp_wendigo.nam" },
    NamCapture { tone: "swollen_pickle",   model_path: "pedals/way_huge_swollen_pickle/swollen_pickle.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("boss_os_2"),
        &[
            ("boss_os_2",        "Boss Os 2"),
            ("buxom_boost_1",    "Buxom Boost 1"),
            ("buxom_boost_2",    "Buxom Boost 2"),
            ("buxom_boost_3",    "Buxom Boost 3"),
            ("gci_jugendstil",   "Gci Jugendstil"),
            ("pp_rot_mid_boost", "Pp Rot Mid Boost"),
            ("pp_rot_mid_scoop", "Pp Rot Mid Scoop"),
            ("pp_wendigo",       "Pp Wendigo"),
            ("swollen_pickle",   "Swollen Pickle"),
        ],
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(capture.model_path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("model='{}'", capture.model_path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static NamCapture> {
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.tone == tone)
        .ok_or_else(|| anyhow!("gain model '{}' does not support tone='{}'", MODEL_ID, tone))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
