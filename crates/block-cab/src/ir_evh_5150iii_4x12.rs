use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::CabModelDefinition;
use crate::CabBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "evh_5150iii_4x12";
pub const DISPLAY_NAME: &str = "5150III 4x12 G12-EVH";
const BRAND: &str = "evh";

const CAPTURES: &[(&str, &str, &str)] = &[
    ("ll_1_50in_vp28", "G12-EVH LL 5150III 4x12 SM57 1.50in 0.0in VP28", "cabs/evh_5150iii_4x12/g12_evh_ll_5150iii_4x12_sm57_1_50in_0_0in_vp28_3.wav"),
    ("lr_1_00in_oa30_cl7603", "G12-EVH LR 5150III 4x12 SM57 1.00in 0.0in OA30 CL7603", "cabs/evh_5150iii_4x12/g12_evh_lr_5150iii_4x12_sm57_1_00in_0_0in_oa30_cl7603_3.wav"),
    ("lr_2_25in_vp28", "G12-EVH LR 5150III 4x12 SM57 2.25in 0.0in VP28", "cabs/evh_5150iii_4x12/g12_evh_lr_5150iii_4x12_sm57_2_25in_0_0in_vp28_3.wav"),
    ("ul_2_00in_oa30_cl7603", "G12-EVH UL 5150III 4x12 SM57 2.00in 0.0in OA30 CL7603", "cabs/evh_5150iii_4x12/g12_evh_ul_5150iii_4x12_sm57_2_00in_0_0in_oa30_cl7603_3.wav"),
    ("ur_1_50in_vp28", "G12-EVH UR 5150III 4x12 SM57 1.50in 0.0in VP28", "cabs/evh_5150iii_4x12/g12_evh_ur_5150iii_4x12_sm57_1_50in_0_0in_vp28_3.wav"),
    ("ur_2_25in_vp28", "G12-EVH UR 5150III 4x12 SM57 2.25in 0.0in VP28", "cabs/evh_5150iii_4x12/g12_evh_ur_5150iii_4x12_sm57_2_25in_0_0in_vp28_3.wav"),
    ("ll_1_00in_oa30_cl7603", "G12-EVH LL 5150III 4x12 SM57 1.00in 0.0in OA30 CL7603", "cabs/evh_5150iii_4x12/g12_evh_ll_5150iii_4x12_sm57_1_00in_0_0in_oa30_cl7603_3.wav"),
    ("ll_1_00in_cl7603", "G12-EVH LL 5150III 4x12 SM57 1.00in 0.0in CL7603", "cabs/evh_5150iii_4x12/g12_evh_ll_5150iii_4x12_sm57_1_00in_0_0in_cl7603_3.wav"),
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
            Some("ll_1_50in_vp28"),
            &[
            ("ll_1_50in_vp28", "G12-EVH LL 5150III 4x12 SM57 1.50in 0.0in VP28"),
            ("lr_1_00in_oa30_cl7603", "G12-EVH LR 5150III 4x12 SM57 1.00in 0.0in OA30 CL7603"),
            ("lr_2_25in_vp28", "G12-EVH LR 5150III 4x12 SM57 2.25in 0.0in VP28"),
            ("ul_2_00in_oa30_cl7603", "G12-EVH UL 5150III 4x12 SM57 2.00in 0.0in OA30 CL7603"),
            ("ur_1_50in_vp28", "G12-EVH UR 5150III 4x12 SM57 1.50in 0.0in VP28"),
            ("ur_2_25in_vp28", "G12-EVH UR 5150III 4x12 SM57 2.25in 0.0in VP28"),
            ("ll_1_00in_oa30_cl7603", "G12-EVH LL 5150III 4x12 SM57 1.00in 0.0in OA30 CL7603"),
            ("ll_1_00in_cl7603", "G12-EVH LL 5150III 4x12 SM57 1.00in 0.0in CL7603"),
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
