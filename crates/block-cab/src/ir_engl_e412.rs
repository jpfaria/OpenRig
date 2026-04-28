use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "engl_e412";
pub const DISPLAY_NAME: &str = "E412 Karnivore";
const BRAND: &str = "engl";

const CAPTURES: &[(&str, &str, &str)] = &[
    ("engl_sm57", "Engl_SM57", "cabs/engl_e412/engl_sm57_3.wav"),
    ("engl_md421_edgedustcap", "Engl_MD421_EdgeDustCap", "cabs/engl_e412/engl_md421_edgedustcap_3.wav"),
    ("engl_m160_cone", "Engl_M160_Cone", "cabs/engl_e412/engl_m160_cone_3.wav"),
    ("engl_md421_center", "Engl_MD421_Center", "cabs/engl_e412/engl_md421_center_3.wav"),
    ("engl_m160_center", "Engl_M160_Center", "cabs/engl_e412/engl_m160_center_3.wav"),
    ("engl_md421_cone", "Engl_MD421_Cone", "cabs/engl_e412/engl_md421_cone_3.wav"),
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
            Some("engl_sm57"),
            &[
            ("engl_sm57", "Engl_SM57"),
            ("engl_md421_edgedustcap", "Engl_MD421_EdgeDustCap"),
            ("engl_m160_cone", "Engl_M160_Cone"),
            ("engl_md421_center", "Engl_MD421_Center"),
            ("engl_m160_center", "Engl_M160_Center"),
            ("engl_md421_cone", "Engl_MD421_Cone"),
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
