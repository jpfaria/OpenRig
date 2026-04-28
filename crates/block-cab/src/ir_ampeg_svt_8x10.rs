use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "ampeg_svt_8x10";
pub const DISPLAY_NAME: &str = "SVT 4x10/8x10";
const BRAND: &str = "ampeg";

// Two-axis pack: mic × position.
// 8x10 captures vary mic and position (AH = ah_3, A107 = a107_3).
// "svt_di" position covers D-I and on-axis SVT-only captures.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (mic, position, file)
    ("d6",          "ah",     "cabs/ampeg_svt_8x10/ampeg_8x10_d6_ah_3.wav"),
    ("57",          "ah",     "cabs/ampeg_svt_8x10/ampeg_8x10_57_ah_3.wav"),
    ("4033",        "ah",     "cabs/ampeg_svt_8x10/ampeg_8x10_4033_ah_3.wav"),
    ("4033",        "a107",   "cabs/ampeg_svt_8x10/ampeg_8x10_4033_a107_3.wav"),
    ("e602",        "a107",   "cabs/ampeg_svt_8x10/ampeg_8x10_e602_a107_3.wav"),
    ("beta52",      "svt_di", "cabs/ampeg_svt_8x10/ampeg_svt_beta52_3.wav"),
    ("neumann",     "svt_di", "cabs/ampeg_svt_8x10/ampeg_svt_bright_neumann_3.wav"),
    ("di_out",      "svt_di", "cabs/ampeg_svt_8x10/ampeg_svt_d_i_out_3.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            enum_parameter(
                "mic",
                "Mic",
                Some("Cab"),
                Some("57"),
                &[
                    ("d6",      "AKG D6"),
                    ("57",      "SM57"),
                    ("4033",    "Audix 4033"),
                    ("e602",    "Sennheiser e602"),
                    ("beta52",  "Beta 52"),
                    ("neumann", "Neumann (Bright)"),
                    ("di_out",  "D-I Out"),
                ],
            ),
            enum_parameter(
                "position",
                "Position",
                Some("Cab"),
                Some("ah"),
                &[
                    ("ah",     "AH (Cap Edge)"),
                    ("a107",   "A107"),
                    ("svt_di", "SVT (Direct)"),
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
    let mic = required_string(params, "mic").map_err(anyhow::Error::msg)?;
    let position = required_string(params, "position").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(m, p, _)| *m == mic && *p == position)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "cab '{}' has no capture for mic={} position={}",
                MODEL_ID, mic, position
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
