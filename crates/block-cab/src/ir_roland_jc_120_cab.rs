use anyhow::{bail, Result};
use ir::build_mono_ir_processor_from_wav;
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "roland_jc_120_cab";
pub const DISPLAY_NAME: &str = "JC-120 Cab";
const BRAND: &str = "roland";

const IR_FILE: &str = "cabs/roland_jc_120_cab/sm57_md421_mix.wav";

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![],
    }
}

pub fn build_processor_for_model(
    _params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => {
            let wav_path = ir::resolve_ir_capture(IR_FILE)?;
            Ok(BlockProcessor::Mono(build_mono_ir_processor_from_wav(&wav_path, sample_rate)?))
        }
        AudioChannelLayout::Stereo => bail!("cab model '{}' currently expects mono processor layout", MODEL_ID),
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
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
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

pub fn validate_params(_params: &ParameterSet) -> Result<()> { Ok(()) }

pub fn asset_summary(_params: &ParameterSet) -> Result<String> {
    Ok(format!("asset_id='{}'", IR_FILE))
}
