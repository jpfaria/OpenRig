use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "fender_super_reverb_4x10";
pub const DISPLAY_NAME: &str = "Super Reverb 4x10";
const BRAND: &str = "fender";

// Single-axis pack: P10R speakers + Brauner BU87IC mic, varying mic distance.
// All captures share offset 1.0in. Only the on-axis distance varies.
const CAPTURES: &[(&str, &str, &str)] = &[
    ("1_75", "1.75 in", "cabs/fender_super_reverb_4x10/p10r_ur_4x10_bu87ic_1_75in_1_0in_3.wav"),
    ("2_00", "2.00 in", "cabs/fender_super_reverb_4x10/p10r_ur_4x10_bu87ic_2_00in_1_0in_3.wav"),
    ("2_25", "2.25 in", "cabs/fender_super_reverb_4x10/p10r_ur_4x10_bu87ic_2_25in_1_0in_3.wav"),
    ("2_50", "2.50 in", "cabs/fender_super_reverb_4x10/p10r_ur_4x10_bu87ic_2_50in_1_0in_3.wav"),
    ("2_75", "2.75 in", "cabs/fender_super_reverb_4x10/p10r_ur_4x10_bu87ic_2_75in_1_0in_3.wav"),
    ("3_50", "3.50 in", "cabs/fender_super_reverb_4x10/p10r_ur_4x10_bu87ic_3_50in_1_0in_3.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "distance",
            "Mic Distance",
            Some("Cab"),
            Some("2_00"),
            &[
                ("1_75", "1.75 in"),
                ("2_00", "2.00 in"),
                ("2_25", "2.25 in"),
                ("2_50", "2.50 in"),
                ("2_75", "2.75 in"),
                ("3_50", "3.50 in"),
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
    let key = required_string(params, "distance").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("cab '{}' has no capture for distance '{}'", MODEL_ID, key))
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
