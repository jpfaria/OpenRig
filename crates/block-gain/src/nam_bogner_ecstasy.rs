use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_bogner_ecstasy";
pub const DISPLAY_NAME: &str = "Bogner Ecstasy";
const BRAND: &str = "bogner";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "bogner_red_mna_preset_1",        model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_1.nam" },
    NamCapture { tone: "bogner_red_mna_preset_10",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_10.nam" },
    NamCapture { tone: "bogner_red_mna_preset_11",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_11.nam" },
    NamCapture { tone: "bogner_red_mna_preset_12",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_12.nam" },
    NamCapture { tone: "bogner_red_mna_preset_13",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_13.nam" },
    NamCapture { tone: "bogner_red_mna_preset_14",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_14.nam" },
    NamCapture { tone: "bogner_red_mna_preset_15",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_15.nam" },
    NamCapture { tone: "bogner_red_mna_preset_16",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_16.nam" },
    NamCapture { tone: "bogner_red_mna_preset_17",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_17.nam" },
    NamCapture { tone: "bogner_red_mna_preset_18",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_18.nam" },
    NamCapture { tone: "bogner_red_mna_preset_19",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_19.nam" },
    NamCapture { tone: "bogner_red_mna_preset_2",        model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_2.nam" },
    NamCapture { tone: "bogner_red_mna_preset_20",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_20.nam" },
    NamCapture { tone: "bogner_red_mna_preset_21",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_21.nam" },
    NamCapture { tone: "bogner_red_mna_preset_22",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_22.nam" },
    NamCapture { tone: "bogner_red_mna_preset_23",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_23.nam" },
    NamCapture { tone: "bogner_red_mna_preset_24",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_24.nam" },
    NamCapture { tone: "bogner_red_mna_preset_25",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_25.nam" },
    NamCapture { tone: "bogner_red_mna_preset_26",       model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_26.nam" },
    NamCapture { tone: "bogner_red_mna_preset_3",        model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_3.nam" },
    NamCapture { tone: "bogner_red_mna_preset_4",        model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_4.nam" },
    NamCapture { tone: "bogner_red_mna_preset_5",        model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_5.nam" },
    NamCapture { tone: "bogner_red_mna_preset_6",        model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_6.nam" },
    NamCapture { tone: "bogner_red_mna_preset_7",        model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_7.nam" },
    NamCapture { tone: "bogner_red_mna_preset_8",        model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_8.nam" },
    NamCapture { tone: "bogner_red_mna_preset_9",        model_path: "pedals/bogner_ecstasy/bogner_red_mna_preset_9.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_1",  model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_1.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_10", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_10.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_11", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_11.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_12", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_12.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_13", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_13.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_14", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_14.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_15", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_15.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_16", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_16.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_17", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_17.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_18", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_18.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_19", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_19.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_2",  model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_2.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_20", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_20.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_21", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_21.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_22", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_22.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_23", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_23.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_24", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_24.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_25", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_25.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_26", model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_26.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_3",  model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_3.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_4",  model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_4.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_5",  model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_5.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_6",  model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_6.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_7",  model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_7.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_8",  model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_8.nam" },
    NamCapture { tone: "mna_bogner_red_pedal_preset_9",  model_path: "pedals/bogner_ecstasy/mna_bogner_red_pedal_preset_9.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("bogner_red_mna_preset_1"),
        &[
            ("bogner_red_mna_preset_1",        "Bogner Red Mna Preset 1"),
            ("bogner_red_mna_preset_10",       "Bogner Red Mna Preset 10"),
            ("bogner_red_mna_preset_11",       "Bogner Red Mna Preset 11"),
            ("bogner_red_mna_preset_12",       "Bogner Red Mna Preset 12"),
            ("bogner_red_mna_preset_13",       "Bogner Red Mna Preset 13"),
            ("bogner_red_mna_preset_14",       "Bogner Red Mna Preset 14"),
            ("bogner_red_mna_preset_15",       "Bogner Red Mna Preset 15"),
            ("bogner_red_mna_preset_16",       "Bogner Red Mna Preset 16"),
            ("bogner_red_mna_preset_17",       "Bogner Red Mna Preset 17"),
            ("bogner_red_mna_preset_18",       "Bogner Red Mna Preset 18"),
            ("bogner_red_mna_preset_19",       "Bogner Red Mna Preset 19"),
            ("bogner_red_mna_preset_2",        "Bogner Red Mna Preset 2"),
            ("bogner_red_mna_preset_20",       "Bogner Red Mna Preset 20"),
            ("bogner_red_mna_preset_21",       "Bogner Red Mna Preset 21"),
            ("bogner_red_mna_preset_22",       "Bogner Red Mna Preset 22"),
            ("bogner_red_mna_preset_23",       "Bogner Red Mna Preset 23"),
            ("bogner_red_mna_preset_24",       "Bogner Red Mna Preset 24"),
            ("bogner_red_mna_preset_25",       "Bogner Red Mna Preset 25"),
            ("bogner_red_mna_preset_26",       "Bogner Red Mna Preset 26"),
            ("bogner_red_mna_preset_3",        "Bogner Red Mna Preset 3"),
            ("bogner_red_mna_preset_4",        "Bogner Red Mna Preset 4"),
            ("bogner_red_mna_preset_5",        "Bogner Red Mna Preset 5"),
            ("bogner_red_mna_preset_6",        "Bogner Red Mna Preset 6"),
            ("bogner_red_mna_preset_7",        "Bogner Red Mna Preset 7"),
            ("bogner_red_mna_preset_8",        "Bogner Red Mna Preset 8"),
            ("bogner_red_mna_preset_9",        "Bogner Red Mna Preset 9"),
            ("mna_bogner_red_pedal_preset_1",  "Mna Bogner Red Pedal Preset 1"),
            ("mna_bogner_red_pedal_preset_10", "Mna Bogner Red Pedal Preset 10"),
            ("mna_bogner_red_pedal_preset_11", "Mna Bogner Red Pedal Preset 11"),
            ("mna_bogner_red_pedal_preset_12", "Mna Bogner Red Pedal Preset 12"),
            ("mna_bogner_red_pedal_preset_13", "Mna Bogner Red Pedal Preset 13"),
            ("mna_bogner_red_pedal_preset_14", "Mna Bogner Red Pedal Preset 14"),
            ("mna_bogner_red_pedal_preset_15", "Mna Bogner Red Pedal Preset 15"),
            ("mna_bogner_red_pedal_preset_16", "Mna Bogner Red Pedal Preset 16"),
            ("mna_bogner_red_pedal_preset_17", "Mna Bogner Red Pedal Preset 17"),
            ("mna_bogner_red_pedal_preset_18", "Mna Bogner Red Pedal Preset 18"),
            ("mna_bogner_red_pedal_preset_19", "Mna Bogner Red Pedal Preset 19"),
            ("mna_bogner_red_pedal_preset_2",  "Mna Bogner Red Pedal Preset 2"),
            ("mna_bogner_red_pedal_preset_20", "Mna Bogner Red Pedal Preset 20"),
            ("mna_bogner_red_pedal_preset_21", "Mna Bogner Red Pedal Preset 21"),
            ("mna_bogner_red_pedal_preset_22", "Mna Bogner Red Pedal Preset 22"),
            ("mna_bogner_red_pedal_preset_23", "Mna Bogner Red Pedal Preset 23"),
            ("mna_bogner_red_pedal_preset_24", "Mna Bogner Red Pedal Preset 24"),
            ("mna_bogner_red_pedal_preset_25", "Mna Bogner Red Pedal Preset 25"),
            ("mna_bogner_red_pedal_preset_26", "Mna Bogner Red Pedal Preset 26"),
            ("mna_bogner_red_pedal_preset_3",  "Mna Bogner Red Pedal Preset 3"),
            ("mna_bogner_red_pedal_preset_4",  "Mna Bogner Red Pedal Preset 4"),
            ("mna_bogner_red_pedal_preset_5",  "Mna Bogner Red Pedal Preset 5"),
            ("mna_bogner_red_pedal_preset_6",  "Mna Bogner Red Pedal Preset 6"),
            ("mna_bogner_red_pedal_preset_7",  "Mna Bogner Red Pedal Preset 7"),
            ("mna_bogner_red_pedal_preset_8",  "Mna Bogner Red Pedal Preset 8"),
            ("mna_bogner_red_pedal_preset_9",  "Mna Bogner Red Pedal Preset 9"),
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
