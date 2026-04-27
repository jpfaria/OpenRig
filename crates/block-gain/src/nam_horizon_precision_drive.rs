use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string, 
    ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "nam_horizon_precision_drive";
pub const DISPLAY_NAME: &str = "Horizon Precision Drive";
const BRAND: &str = "horizon_devices";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

#[derive(Clone, Copy)]
struct GridCapture {
    bass: f32,
    drive: f32,
    presence: f32,
    volume: f32,
    size: NamSize,
    model_path: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NamSize {
    Lite,
    Standard,
}

const BASS_MIN: f32 = 0.0;
const BASS_MAX: f32 = 3.0;
const DRIVE_MIN: f32 = 0.0;
const DRIVE_MAX: f32 = 1.8;
const PRESENCE_MIN: f32 = 1.0;
const PRESENCE_MAX: f32 = 8.0;
const VOLUME_MIN: f32 = 6.0;
const VOLUME_MAX: f32 = 8.0;

const CAPTURES: &[GridCapture] = &[
    GridCapture { bass: 3.0, drive: 1.8, presence: 5.0, volume: 6.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_06_d_1_8_b_03_a_p5_lite.nam" },
    GridCapture { bass: 3.0, drive: 1.8, presence: 5.0, volume: 6.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_06_d_1_8_b_03_a_p5_std.nam" },
    GridCapture { bass: 3.0, drive: 1.8, presence: 5.0, volume: 6.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_06_d_1_8_b_03_a_p5_xstd.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 1.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p1_lite.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 1.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p1_std.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 1.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p1_xstd.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 2.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p2_lite.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 2.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p2_std.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 2.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p2_xstd.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 3.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p3_lite.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 3.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p3_std.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 3.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p3_xstd.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 4.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p4_lite.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 4.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p4_std.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 4.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p4_xstd.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 5.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p5_lite.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 5.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p5_std.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 5.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p5_xstd.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 6.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p6_lite.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 6.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p6_std.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 6.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p6_xstd.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 7.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p7_lite.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 7.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p7_std.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 7.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p7_xstd.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 8.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p8_lite.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 8.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p8_std.nam" },
    GridCapture { bass: 0.0, drive: 0.0, presence: 8.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p8_xstd.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 1.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p1_lite.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 1.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p1_std.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 1.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p1_xstd.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 2.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p2_lite.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 2.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p2_std.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 2.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p2_xstd.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 3.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p3_lite.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 3.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p3_std.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 3.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p3_xstd.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 4.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p4_lite.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 4.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p4_std.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 4.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p4_xstd.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 5.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p5_lite.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 5.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p5_std.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 5.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p5_xstd.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 6.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p6_lite.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 6.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p6_std.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 6.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p6_xstd.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 7.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p7_lite.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 7.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p7_std.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 7.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p7_xstd.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 8.0, volume: 8.0, size: NamSize::Lite, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p8_lite.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 8.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p8_std.nam" },
    GridCapture { bass: 2.0, drive: 0.0, presence: 8.0, volume: 8.0, size: NamSize::Standard, model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p8_xstd.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.audio_mode = ModelAudioMode::DualMono;
    schema.parameters = vec![
        float_parameter("bass", "Bass", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("drive", "Drive", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("presence", "Presence", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        float_parameter("volume", "Volume", Some("Pedal"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        enum_parameter("size", "Model Size", Some("Capture"), Some("standard"), &[("lite", "Lite"), ("standard", "Standard")]),
    ];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(capture.model_path)?,
        None,
        NAM_PLUGIN_FIXED_PARAMS,
        sample_rate,
        layout,
    )
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("model='{}'", capture.model_path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static GridCapture> {
    let bass_pct = required_f32(params, "bass").map_err(anyhow::Error::msg)?;
    let drive_pct = required_f32(params, "drive").map_err(anyhow::Error::msg)?;
    let presence_pct = required_f32(params, "presence").map_err(anyhow::Error::msg)?;
    let volume_pct = required_f32(params, "volume").map_err(anyhow::Error::msg)?;
    let bass = BASS_MIN + (bass_pct / 100.0) * (BASS_MAX - BASS_MIN);
    let drive = DRIVE_MIN + (drive_pct / 100.0) * (DRIVE_MAX - DRIVE_MIN);
    let presence = PRESENCE_MIN + (presence_pct / 100.0) * (PRESENCE_MAX - PRESENCE_MIN);
    let volume = VOLUME_MIN + (volume_pct / 100.0) * (VOLUME_MAX - VOLUME_MIN);
    let size_str = required_string(params, "size").map_err(anyhow::Error::msg)?;
    let size = match size_str.as_str() {
        "lite" => NamSize::Lite,
        "standard" => NamSize::Standard,
        other => return Err(anyhow!("unknown size '{}'", other)),
    };
    let candidates = CAPTURES.iter().filter(|c| c.size == size);
    candidates
        .min_by(|a, b| {
            let da = (a.bass - bass).powi(2) + (a.drive - drive).powi(2) + (a.presence - presence).powi(2) + (a.volume - volume).powi(2);
            let db = (b.bass - bass).powi(2) + (b.drive - drive).powi(2) + (b.presence - presence).powi(2) + (b.volume - volume).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .ok_or_else(|| anyhow!("no capture matches"))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};

