use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "fender_bassman_2x15";
pub const DISPLAY_NAME: &str = "Bassman 2x15 CTS";
const BRAND: &str = "fender";

// Three-axis pack: mic × speaker × position (1970 Bassman CTS).
// 121 = Royer R-121 ribbon. Holes return Err.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (mic, speaker, position, file)
    ("r121", "lower",   "cap",       "cabs/fender_bassman_2x15/1970_bassman_cabinet_cts_121_lower_cap_3.wav"),
    ("r121", "lower",   "cap_edge",  "cabs/fender_bassman_2x15/1970_bassman_cabinet_cts_121_lower_cap_edge_3.wav"),
    ("r121", "lower",   "cone",      "cabs/fender_bassman_2x15/1970_bassman_cabinet_cts_121_lower_cone_3.wav"),
    ("r121", "lower",   "cone_edge", "cabs/fender_bassman_2x15/1970_bassman_cabinet_cts_121_lower_cone_edge_3.wav"),
    ("r121", "upper",   "cap_edge",  "cabs/fender_bassman_2x15/1970_bassman_cabinet_cts_121_upper_cap_edge_3.wav"),
    ("c414", "distant", "12_inch",   "cabs/fender_bassman_2x15/1970_bassman_cabinet_cts_c414_distant_12in_3.wav"),
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
                Some("r121"),
                &[
                    ("r121", "Royer R-121"),
                    ("c414", "AKG C414"),
                ],
            ),
            enum_parameter(
                "speaker",
                "Speaker",
                Some("Cab"),
                Some("lower"),
                &[
                    ("lower",   "Lower 15\""),
                    ("upper",   "Upper 15\""),
                    ("distant", "Distant"),
                ],
            ),
            enum_parameter(
                "position",
                "Position",
                Some("Cab"),
                Some("cap"),
                &[
                    ("cap",       "Cap"),
                    ("cap_edge",  "Cap Edge"),
                    ("cone",      "Cone"),
                    ("cone_edge", "Cone Edge"),
                    ("12_inch",   "12 inch (Distant)"),
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
    let speaker = required_string(params, "speaker").map_err(anyhow::Error::msg)?;
    let position = required_string(params, "position").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(m, s, p, _)| *m == mic && *s == speaker && *p == position)
        .map(|(_, _, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "cab '{}' has no capture for mic={} speaker={} position={}",
                MODEL_ID, mic, speaker, position
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
