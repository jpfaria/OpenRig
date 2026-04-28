use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "ampeg_svt_8x10";
pub const DISPLAY_NAME: &str = "SVT 4x10/8x10";
const BRAND: &str = "ampeg";

const CAPTURES: &[(&str, &str, &str)] = &[
    ("ampeg_8x10_d6_ah", "Ampeg 8x10 D6 AH", "cabs/ampeg_svt_8x10/ampeg_8x10_d6_ah.wav"),
    ("ampeg_8x10_4033_ah", "Ampeg 8x10 4033 AH", "cabs/ampeg_svt_8x10/ampeg_8x10_4033_ah.wav"),
    ("ampeg_svt_beta52", "Ampeg SVT Beta52", "cabs/ampeg_svt_8x10/ampeg_svt_beta52.wav"),
    ("ampeg_8x10_e602_a107", "Ampeg 8x10 e602 A107", "cabs/ampeg_svt_8x10/ampeg_8x10_e602_a107.wav"),
    ("ampeg_8x10_57_ah", "Ampeg 8x10 57 AH", "cabs/ampeg_svt_8x10/ampeg_8x10_57_ah.wav"),
    ("ampeg_svt_bright_neumann", "Ampeg SVT Bright Neumann", "cabs/ampeg_svt_8x10/ampeg_svt_bright_neumann.wav"),
    ("ampeg_8x10_4033_a107", "Ampeg 8x10 4033 A107", "cabs/ampeg_svt_8x10/ampeg_8x10_4033_a107.wav"),
    ("ampeg_svt_d_i_out", "Ampeg SVT D-I-Out", "cabs/ampeg_svt_8x10/ampeg_svt_d_i_out.wav"),
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
            Some("ampeg_8x10_d6_ah"),
            &[
            ("ampeg_8x10_d6_ah", "Ampeg 8x10 D6 AH"),
            ("ampeg_8x10_4033_ah", "Ampeg 8x10 4033 AH"),
            ("ampeg_svt_beta52", "Ampeg SVT Beta52"),
            ("ampeg_8x10_e602_a107", "Ampeg 8x10 e602 A107"),
            ("ampeg_8x10_57_ah", "Ampeg 8x10 57 AH"),
            ("ampeg_svt_bright_neumann", "Ampeg SVT Bright Neumann"),
            ("ampeg_8x10_4033_a107", "Ampeg 8x10 4033 A107"),
            ("ampeg_svt_d_i_out", "Ampeg SVT D-I-Out"),
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
