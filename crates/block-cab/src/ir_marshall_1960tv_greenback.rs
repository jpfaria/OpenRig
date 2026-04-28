use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "marshall_1960tv_greenback";
pub const DISPLAY_NAME: &str = "1960TV Greenback";
const BRAND: &str = "marshall";

// Three-axis pack: mic position × distance × preamp.
// Only 8 of the 4×5×4 = 80 possible combinations were captured. The
// `resolve_capture` lookup rejects the holes so the UI can still expose
// all three knobs as independent controls.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (mic_pos, distance, preamp, file)
    ("ll", "1.50", "oa30_sa73",  "cabs/marshall_1960tv_greenback/m25_ll_1960tv_4x12_sm57_1_50in_0_0in_oa30_sa73.wav"),
    ("lr", "1.00", "neve_7603",  "cabs/marshall_1960tv_greenback/m25_lr_1960tv_4x12_sm57_1_00in_0_0in_7603.wav"),
    ("lr", "1.50", "oa30_7603",  "cabs/marshall_1960tv_greenback/m25_lr_1960tv_4x12_sm57_1_50in_0_0in_oa30_7603.wav"),
    ("ul", "1.25", "neve_7603",  "cabs/marshall_1960tv_greenback/m25_ul_1960tv_4x12_sm57_1_25in_0_0in_7603.wav"),
    ("ul", "2.00", "neve_7603",  "cabs/marshall_1960tv_greenback/m25_ul_1960tv_4x12_sm57_2_00in_0_0in_7603.wav"),
    ("ul", "2.25", "vp28",       "cabs/marshall_1960tv_greenback/m25_ul_1960tv_4x12_sm57_2_25in_0_0in_vp28.wav"),
    ("ur", "1.00", "vp28",       "cabs/marshall_1960tv_greenback/m25_ur_1960tv_4x12_sm57_1_00in_0_0in_vp28.wav"),
    ("ur", "2.00", "vp28",       "cabs/marshall_1960tv_greenback/m25_ur_1960tv_4x12_sm57_2_00in_0_0in_vp28.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            enum_parameter(
                "mic_position",
                "Mic Position",
                Some("Cab"),
                Some("ll"),
                &[
                    ("ll", "Lower Left"),
                    ("lr", "Lower Right"),
                    ("ul", "Upper Left"),
                    ("ur", "Upper Right"),
                ],
            ),
            enum_parameter(
                "distance",
                "Distance",
                Some("Cab"),
                Some("1.50"),
                &[
                    ("1.00", "1.00 in"),
                    ("1.25", "1.25 in"),
                    ("1.50", "1.50 in"),
                    ("2.00", "2.00 in"),
                    ("2.25", "2.25 in"),
                ],
            ),
            enum_parameter(
                "preamp",
                "Mic Preamp",
                Some("Cab"),
                Some("oa30_sa73"),
                &[
                    ("oa30_sa73", "OA30 + SA73"),
                    ("oa30_7603", "OA30 + Neve 7603"),
                    ("neve_7603", "Neve 7603"),
                    ("vp28",      "VP28"),
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
    let mic = required_string(params, "mic_position").map_err(anyhow::Error::msg)?;
    let dist = required_string(params, "distance").map_err(anyhow::Error::msg)?;
    let pre = required_string(params, "preamp").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(m, d, p, _)| *m == mic && *d == dist && *p == pre)
        .map(|(_, _, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "cab '{}' has no capture for mic_position={} distance={} preamp={}",
                MODEL_ID, mic, dist, pre
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
