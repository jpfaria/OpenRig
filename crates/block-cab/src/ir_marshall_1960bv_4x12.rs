use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "marshall_1960bv_4x12";
pub const DISPLAY_NAME: &str = "1960BV 4x12";
const BRAND: &str = "marshall";

const CAPTURES: &[(&str, &str, &str)] = &[
    ("g12_4_sm57_3", "Marshall G12 4 SM57 3", "cabs/marshall_1960bv_4x12/marshall_g12_4_sm57_3_3.wav"),
    ("g12_1_sm57_2", "Marshall G12 1 SM57 2", "cabs/marshall_1960bv_4x12/marshall_g12_1_sm57_2_3.wav"),
    ("g12_1_sm58_5", "Marshall G12 1 SM58 5", "cabs/marshall_1960bv_4x12/marshall_g12_1_sm58_5_3.wav"),
    ("g12_4_sm58_4", "Marshall G12 4 SM58 4", "cabs/marshall_1960bv_4x12/marshall_g12_4_sm58_4_3.wav"),
    ("v30_3_sm57_1", "Marshall V30 3 SM57 1", "cabs/marshall_1960bv_4x12/marshall_v30_3_sm57_1_3.wav"),
    ("v30_2_sm58_6", "Marshall V30 2 SM58 6", "cabs/marshall_1960bv_4x12/marshall_v30_2_sm58_6_3.wav"),
    ("g12_4_sm57_8", "Marshall G12 4 SM57 8", "cabs/marshall_1960bv_4x12/marshall_g12_4_sm57_8_3.wav"),
    ("v30_2_sm58_1", "Marshall V30 2 SM58 1", "cabs/marshall_1960bv_4x12/marshall_v30_2_sm58_1_3.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "capture",
            "Capture",
            Some("Cab"),
            Some("g12_4_sm57_3"),
            &[
            ("g12_4_sm57_3", "Marshall G12 4 SM57 3"),
            ("g12_1_sm57_2", "Marshall G12 1 SM57 2"),
            ("g12_1_sm58_5", "Marshall G12 1 SM58 5"),
            ("g12_4_sm58_4", "Marshall G12 4 SM58 4"),
            ("v30_3_sm57_1", "Marshall V30 3 SM57 1"),
            ("v30_2_sm58_6", "Marshall V30 2 SM58 6"),
            ("g12_4_sm57_8", "Marshall G12 4 SM57 8"),
            ("v30_2_sm58_1", "Marshall V30 2 SM58 1"),
            ],
        )],
    }
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => {
            let path = resolve_capture(params)?;
            let wav_path = ir::resolve_ir_capture(path)?;
            let ir = IrAsset::load_from_wav(&wav_path)?;
            if ir.channel_count() != 1 {
                bail!(
                    "cab model '{}' capture must be mono, got {} channels",
                    MODEL_ID,
                    ir.channel_count()
                );
            }
            let processor = build_mono_ir_processor_from_wav(&wav_path, sample_rate)?;
            Ok(BlockProcessor::Mono(processor))
        }
        AudioChannelLayout::Stereo => bail!(
            "cab model '{}' currently expects mono processor layout",
            MODEL_ID
        ),
    }
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let key = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("cab '{}' has no capture '{}'", MODEL_ID, key))
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

pub const MODEL_DEFINITION: CabModelDefinition = CabModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: CabBackendKind::Ir,
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
    Ok(format!("asset_id='{}'", path))
}
