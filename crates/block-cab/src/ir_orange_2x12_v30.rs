use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "orange_2x12_v30";
pub const DISPLAY_NAME: &str = "Orange 2x12 V30";
const BRAND: &str = "orange";

const CAPTURES: &[(&str, &str, &str)] = &[
    ("orange_2x12_v30_c414_e", "Orange 2x12 V30 C414 E", "cabs/orange_2x12_v30/orange_2x12_v30_c414_e.wav"),
    ("orange_2x12_v30_e906_c", "Orange 2x12 V30 e906 C", "cabs/orange_2x12_v30/orange_2x12_v30_e906_c.wav"),
    ("orange_2x12_v30_sm57_a", "Orange 2x12 V30 SM57 A", "cabs/orange_2x12_v30/orange_2x12_v30_sm57_a.wav"),
    ("orange_2x12_v30_rm700_c", "Orange 2x12 V30 RM700 C", "cabs/orange_2x12_v30/orange_2x12_v30_rm700_c.wav"),
    ("orange_2x12_v30_rm700_e", "Orange 2x12 V30 RM700 E", "cabs/orange_2x12_v30/orange_2x12_v30_rm700_e.wav"),
    ("orange_2x12_v30_sm57_c", "Orange 2x12 V30 SM57 C", "cabs/orange_2x12_v30/orange_2x12_v30_sm57_c.wav"),
    ("orange_2x12_v30_sm57_e", "Orange 2x12 V30 SM57 E", "cabs/orange_2x12_v30/orange_2x12_v30_sm57_e.wav"),
    ("orange_2x12_v30_c414_d", "Orange 2x12 V30 C414 D", "cabs/orange_2x12_v30/orange_2x12_v30_c414_d.wav"),
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
            Some("orange_2x12_v30_c414_e"),
            &[
            ("orange_2x12_v30_c414_e", "Orange 2x12 V30 C414 E"),
            ("orange_2x12_v30_e906_c", "Orange 2x12 V30 e906 C"),
            ("orange_2x12_v30_sm57_a", "Orange 2x12 V30 SM57 A"),
            ("orange_2x12_v30_rm700_c", "Orange 2x12 V30 RM700 C"),
            ("orange_2x12_v30_rm700_e", "Orange 2x12 V30 RM700 E"),
            ("orange_2x12_v30_sm57_c", "Orange 2x12 V30 SM57 C"),
            ("orange_2x12_v30_sm57_e", "Orange 2x12 V30 SM57 E"),
            ("orange_2x12_v30_c414_d", "Orange 2x12 V30 C414 D"),
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
