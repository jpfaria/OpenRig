use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "mesa_traditional_4x12_v30";
pub const DISPLAY_NAME: &str = "Traditional 4x12 V30";
const BRAND: &str = "mesa";

// Three-axis pack: mic position × distance × preamp.
// Only 10 of the 3×7×4 = 84 possible combinations were captured. The
// `resolve_capture` lookup rejects the holes so the UI can still expose
// all three knobs as independent controls.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (mic_pos, distance, preamp, file)
    ("ll", "0.50", "neve_7603",  "cabs/mesa_traditional_4x12_v30/v30_ll_4fb_4x12_sm57_0_50in_0_0in_7603_3.wav"),
    ("ll", "0.75", "vp28",       "cabs/mesa_traditional_4x12_v30/v30_ll_4fb_4x12_sm57_0_75in_0_0in_vp28_3.wav"),
    ("ll", "1.25", "sa73",       "cabs/mesa_traditional_4x12_v30/v30_ll_4fb_4x12_sm57_1_25in_0_0in_sa73_3.wav"),
    ("ll", "2.50", "neve_7603",  "cabs/mesa_traditional_4x12_v30/v30_ll_4fb_4x12_sm57_2_50in_0_0in_7603_3.wav"),
    ("lr", "0.25", "neve_7603",  "cabs/mesa_traditional_4x12_v30/v30_lr_4fb_4x12_sm57_0_25in_0_0in_7603_3.wav"),
    ("lr", "0.75", "vp28",       "cabs/mesa_traditional_4x12_v30/v30_lr_4fb_4x12_sm57_0_75in_0_0in_vp28_3.wav"),
    ("lr", "2.00", "vp28",       "cabs/mesa_traditional_4x12_v30/v30_lr_4fb_4x12_sm57_2_00in_0_0in_vp28_3.wav"),
    ("ul", "0.50", "oa30_sa73",  "cabs/mesa_traditional_4x12_v30/v30_ul_4fb_4x12_sm57_0_50in_0_0in_oa30_sa73_3.wav"),
    ("ul", "1.25", "neve_7603",  "cabs/mesa_traditional_4x12_v30/v30_ul_4fb_4x12_sm57_1_25in_0_0in_7603_3.wav"),
    ("ul", "1.50", "sa73",       "cabs/mesa_traditional_4x12_v30/v30_ul_4fb_4x12_sm57_1_50in_0_0in_sa73_3.wav"),
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
                ],
            ),
            enum_parameter(
                "distance",
                "Distance",
                Some("Cab"),
                Some("0.50"),
                &[
                    ("0.25", "0.25 in"),
                    ("0.50", "0.50 in"),
                    ("0.75", "0.75 in"),
                    ("1.25", "1.25 in"),
                    ("1.50", "1.50 in"),
                    ("2.00", "2.00 in"),
                    ("2.50", "2.50 in"),
                ],
            ),
            enum_parameter(
                "preamp",
                "Mic Preamp",
                Some("Cab"),
                Some("neve_7603"),
                &[
                    ("neve_7603", "Neve 7603"),
                    ("vp28",      "VP28"),
                    ("sa73",      "SA73"),
                    ("oa30_sa73", "OA30 + SA73"),
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
