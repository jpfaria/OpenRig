use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "engl_e412";
pub const DISPLAY_NAME: &str = "E412 Karnivore";
const BRAND: &str = "engl";

// Two-axis pack: mic × position.
// Holes (e.g. SM57 only at default position) return Err.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (mic, position, file)
    ("sm57",  "default",       "cabs/engl_e412/engl_sm57_3.wav"),
    ("md421", "center",        "cabs/engl_e412/engl_md421_center_3.wav"),
    ("md421", "cone",          "cabs/engl_e412/engl_md421_cone_3.wav"),
    ("md421", "edge_dust_cap", "cabs/engl_e412/engl_md421_edgedustcap_3.wav"),
    ("m160",  "center",        "cabs/engl_e412/engl_m160_center_3.wav"),
    ("m160",  "cone",          "cabs/engl_e412/engl_m160_cone_3.wav"),
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
                Some("sm57"),
                &[
                    ("sm57",  "SM57"),
                    ("md421", "MD 421"),
                    ("m160",  "Beyer M160"),
                ],
            ),
            enum_parameter(
                "position",
                "Position",
                Some("Cab"),
                Some("default"),
                &[
                    ("default",       "Default"),
                    ("center",        "Center"),
                    ("cone",          "Cone"),
                    ("edge_dust_cap", "Edge / Dust Cap"),
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
