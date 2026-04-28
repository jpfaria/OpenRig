use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "marshall_1960bv_4x12";
pub const DISPLAY_NAME: &str = "1960BV 4x12";
const BRAND: &str = "marshall";

// Three-axis pack: speaker × mic × take.
// Speaker = which of the 4 cones in the 1960BV is being captured.
// Holes return Err so the UI flags missing combinations.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (speaker, mic, take, file)
    ("g12_1", "sm57", "take_2", "cabs/marshall_1960bv_4x12/marshall_g12_1_sm57_2_3.wav"),
    ("g12_1", "sm58", "take_5", "cabs/marshall_1960bv_4x12/marshall_g12_1_sm58_5_3.wav"),
    ("g12_4", "sm57", "take_3", "cabs/marshall_1960bv_4x12/marshall_g12_4_sm57_3_3.wav"),
    ("g12_4", "sm57", "take_8", "cabs/marshall_1960bv_4x12/marshall_g12_4_sm57_8_3.wav"),
    ("g12_4", "sm58", "take_4", "cabs/marshall_1960bv_4x12/marshall_g12_4_sm58_4_3.wav"),
    ("v30_2", "sm58", "take_1", "cabs/marshall_1960bv_4x12/marshall_v30_2_sm58_1_3.wav"),
    ("v30_2", "sm58", "take_6", "cabs/marshall_1960bv_4x12/marshall_v30_2_sm58_6_3.wav"),
    ("v30_3", "sm57", "take_1", "cabs/marshall_1960bv_4x12/marshall_v30_3_sm57_1_3.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            enum_parameter(
                "speaker",
                "Speaker",
                Some("Cab"),
                Some("g12_4"),
                &[
                    ("g12_1", "G12 (Speaker 1)"),
                    ("g12_4", "G12 (Speaker 4)"),
                    ("v30_2", "V30 (Speaker 2)"),
                    ("v30_3", "V30 (Speaker 3)"),
                ],
            ),
            enum_parameter(
                "mic",
                "Mic",
                Some("Cab"),
                Some("sm57"),
                &[
                    ("sm57", "SM57"),
                    ("sm58", "SM58"),
                ],
            ),
            enum_parameter(
                "take",
                "Take",
                Some("Cab"),
                Some("take_3"),
                &[
                    ("take_1", "Take 1"),
                    ("take_2", "Take 2"),
                    ("take_3", "Take 3"),
                    ("take_4", "Take 4"),
                    ("take_5", "Take 5"),
                    ("take_6", "Take 6"),
                    ("take_8", "Take 8"),
                ],
            ),
        ],
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
    let speaker = required_string(params, "speaker").map_err(anyhow::Error::msg)?;
    let mic = required_string(params, "mic").map_err(anyhow::Error::msg)?;
    let take = required_string(params, "take").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(s, m, t, _)| *s == speaker && *m == mic && *t == take)
        .map(|(_, _, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "cab '{}' has no capture for speaker={} mic={} take={}",
                MODEL_ID, speaker, mic, take
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
