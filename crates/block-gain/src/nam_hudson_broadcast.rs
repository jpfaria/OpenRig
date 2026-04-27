use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_hudson_broadcast";
pub const DISPLAY_NAME: &str = "Hudson Broadcast";
const BRAND: &str = "hudson";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "level1200_g",   model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level1200_g.nam" },
    NamCapture { tone: "level1200_g_1", model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level1200_g_1.nam" },
    NamCapture { tone: "level1200_g_2", model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level1200_g_2.nam" },
    NamCapture { tone: "level1200_g_3", model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level1200_g_3.nam" },
    NamCapture { tone: "level1200_g_4", model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level1200_g_4.nam" },
    NamCapture { tone: "level130_ga",   model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level130_ga.nam" },
    NamCapture { tone: "level130_ga_1", model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level130_ga_1.nam" },
    NamCapture { tone: "level130_ga_2", model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level130_ga_2.nam" },
    NamCapture { tone: "level130_ga_3", model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level130_ga_3.nam" },
    NamCapture { tone: "level130_ga_4", model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level130_ga_4.nam" },
    NamCapture { tone: "level130_ga_5", model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level130_ga_5.nam" },
    NamCapture { tone: "level130_ga_6", model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level130_ga_6.nam" },
    NamCapture { tone: "level130_ga_7", model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level130_ga_7.nam" },
    NamCapture { tone: "level130_ga_8", model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level130_ga_8.nam" },
    NamCapture { tone: "level130_ga_9", model_path: "pedals/hudson_broadcast/hudson_broadcast_bc_24v_ltd_low_cut900_level130_ga_9.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("level1200_g"),
        &[
            ("level1200_g",   "Level1200 G"),
            ("level1200_g_1", "Level1200 G 1"),
            ("level1200_g_2", "Level1200 G 2"),
            ("level1200_g_3", "Level1200 G 3"),
            ("level1200_g_4", "Level1200 G 4"),
            ("level130_ga",   "Level130 Ga"),
            ("level130_ga_1", "Level130 Ga 1"),
            ("level130_ga_2", "Level130 Ga 2"),
            ("level130_ga_3", "Level130 Ga 3"),
            ("level130_ga_4", "Level130 Ga 4"),
            ("level130_ga_5", "Level130 Ga 5"),
            ("level130_ga_6", "Level130 Ga 6"),
            ("level130_ga_7", "Level130 Ga 7"),
            ("level130_ga_8", "Level130 Ga 8"),
            ("level130_ga_9", "Level130 Ga 9"),
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
