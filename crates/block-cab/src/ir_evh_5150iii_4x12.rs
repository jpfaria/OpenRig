use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "evh_5150iii_4x12";
pub const DISPLAY_NAME: &str = "5150III 4x12 G12-EVH";
const BRAND: &str = "evh";

// Three-axis pack: mic position × distance × preamp.
// Only 8 of the 4×4×3 = 48 possible combinations were captured. The
// `resolve_capture` lookup rejects the holes so the UI can still expose
// all three knobs as independent controls.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (mic_pos, distance, preamp, file)
    ("ll", "1.00", "cl7603",      "cabs/evh_5150iii_4x12/g12_evh_ll_5150iii_4x12_sm57_1_00in_0_0in_cl7603_3.wav"),
    ("ll", "1.00", "oa30_cl7603", "cabs/evh_5150iii_4x12/g12_evh_ll_5150iii_4x12_sm57_1_00in_0_0in_oa30_cl7603_3.wav"),
    ("ll", "1.50", "vp28",        "cabs/evh_5150iii_4x12/g12_evh_ll_5150iii_4x12_sm57_1_50in_0_0in_vp28_3.wav"),
    ("lr", "1.00", "oa30_cl7603", "cabs/evh_5150iii_4x12/g12_evh_lr_5150iii_4x12_sm57_1_00in_0_0in_oa30_cl7603_3.wav"),
    ("lr", "2.25", "vp28",        "cabs/evh_5150iii_4x12/g12_evh_lr_5150iii_4x12_sm57_2_25in_0_0in_vp28_3.wav"),
    ("ul", "2.00", "oa30_cl7603", "cabs/evh_5150iii_4x12/g12_evh_ul_5150iii_4x12_sm57_2_00in_0_0in_oa30_cl7603_3.wav"),
    ("ur", "1.50", "vp28",        "cabs/evh_5150iii_4x12/g12_evh_ur_5150iii_4x12_sm57_1_50in_0_0in_vp28_3.wav"),
    ("ur", "2.25", "vp28",        "cabs/evh_5150iii_4x12/g12_evh_ur_5150iii_4x12_sm57_2_25in_0_0in_vp28_3.wav"),
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
                Some("1.00"),
                &[
                    ("1.00", "1.00 in"),
                    ("1.50", "1.50 in"),
                    ("2.00", "2.00 in"),
                    ("2.25", "2.25 in"),
                ],
            ),
            enum_parameter(
                "preamp",
                "Mic Preamp",
                Some("Cab"),
                Some("cl7603"),
                &[
                    ("cl7603",      "CL7603"),
                    ("oa30_cl7603", "OA30 + CL7603"),
                    ("vp28",        "VP28"),
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
