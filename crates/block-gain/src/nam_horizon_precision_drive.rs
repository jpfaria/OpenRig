use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_horizon_precision_drive";
pub const DISPLAY_NAME: &str = "Horizon Precision Drive";
const BRAND: &str = "horizon";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "06_d_1_8_b_03_a_p5_lite", model_path: "pedals/horizon_precision_drive/pdrive_v_06_d_1_8_b_03_a_p5_lite.nam" },
    NamCapture { tone: "06_d_1_8_b_03_a_p5_std",  model_path: "pedals/horizon_precision_drive/pdrive_v_06_d_1_8_b_03_a_p5_std.nam" },
    NamCapture { tone: "06_d_1_8_b_03_a_p5_xstd", model_path: "pedals/horizon_precision_drive/pdrive_v_06_d_1_8_b_03_a_p5_xstd.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p1_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p1_lite.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p1_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p1_std.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p1_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p1_xstd.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p2_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p2_lite.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p2_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p2_std.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p2_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p2_xstd.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p3_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p3_lite.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p3_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p3_std.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p3_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p3_xstd.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p4_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p4_lite.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p4_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p4_std.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p4_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p4_xstd.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p5_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p5_lite.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p5_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p5_std.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p5_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p5_xstd.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p6_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p6_lite.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p6_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p6_std.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p6_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p6_xstd.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p7_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p7_lite.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p7_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p7_std.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p7_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p7_xstd.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p8_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p8_lite.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p8_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p8_std.nam" },
    NamCapture { tone: "08_d_00_b_00_a_p8_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_00_a_p8_xstd.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p1_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p1_lite.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p1_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p1_std.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p1_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p1_xstd.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p2_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p2_lite.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p2_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p2_std.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p2_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p2_xstd.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p3_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p3_lite.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p3_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p3_std.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p3_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p3_xstd.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p4_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p4_lite.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p4_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p4_std.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p4_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p4_xstd.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p5_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p5_lite.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p5_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p5_std.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p5_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p5_xstd.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p6_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p6_lite.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p6_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p6_std.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p6_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p6_xstd.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p7_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p7_lite.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p7_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p7_std.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p7_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p7_xstd.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p8_lite",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p8_lite.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p8_std",   model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p8_std.nam" },
    NamCapture { tone: "08_d_00_b_02_a_p8_xstd",  model_path: "pedals/horizon_precision_drive/pdrive_v_08_d_00_b_02_a_p8_xstd.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("06_d_1_8_b_03_a_p5_lite"),
        &[
            ("06_d_1_8_b_03_a_p5_lite", "06 D 1 8 B 03 A P5 Lite"),
            ("06_d_1_8_b_03_a_p5_std",  "06 D 1 8 B 03 A P5 Std"),
            ("06_d_1_8_b_03_a_p5_xstd", "06 D 1 8 B 03 A P5 Xstd"),
            ("08_d_00_b_00_a_p1_lite",  "08 D 00 B 00 A P1 Lite"),
            ("08_d_00_b_00_a_p1_std",   "08 D 00 B 00 A P1 Std"),
            ("08_d_00_b_00_a_p1_xstd",  "08 D 00 B 00 A P1 Xstd"),
            ("08_d_00_b_00_a_p2_lite",  "08 D 00 B 00 A P2 Lite"),
            ("08_d_00_b_00_a_p2_std",   "08 D 00 B 00 A P2 Std"),
            ("08_d_00_b_00_a_p2_xstd",  "08 D 00 B 00 A P2 Xstd"),
            ("08_d_00_b_00_a_p3_lite",  "08 D 00 B 00 A P3 Lite"),
            ("08_d_00_b_00_a_p3_std",   "08 D 00 B 00 A P3 Std"),
            ("08_d_00_b_00_a_p3_xstd",  "08 D 00 B 00 A P3 Xstd"),
            ("08_d_00_b_00_a_p4_lite",  "08 D 00 B 00 A P4 Lite"),
            ("08_d_00_b_00_a_p4_std",   "08 D 00 B 00 A P4 Std"),
            ("08_d_00_b_00_a_p4_xstd",  "08 D 00 B 00 A P4 Xstd"),
            ("08_d_00_b_00_a_p5_lite",  "08 D 00 B 00 A P5 Lite"),
            ("08_d_00_b_00_a_p5_std",   "08 D 00 B 00 A P5 Std"),
            ("08_d_00_b_00_a_p5_xstd",  "08 D 00 B 00 A P5 Xstd"),
            ("08_d_00_b_00_a_p6_lite",  "08 D 00 B 00 A P6 Lite"),
            ("08_d_00_b_00_a_p6_std",   "08 D 00 B 00 A P6 Std"),
            ("08_d_00_b_00_a_p6_xstd",  "08 D 00 B 00 A P6 Xstd"),
            ("08_d_00_b_00_a_p7_lite",  "08 D 00 B 00 A P7 Lite"),
            ("08_d_00_b_00_a_p7_std",   "08 D 00 B 00 A P7 Std"),
            ("08_d_00_b_00_a_p7_xstd",  "08 D 00 B 00 A P7 Xstd"),
            ("08_d_00_b_00_a_p8_lite",  "08 D 00 B 00 A P8 Lite"),
            ("08_d_00_b_00_a_p8_std",   "08 D 00 B 00 A P8 Std"),
            ("08_d_00_b_00_a_p8_xstd",  "08 D 00 B 00 A P8 Xstd"),
            ("08_d_00_b_02_a_p1_lite",  "08 D 00 B 02 A P1 Lite"),
            ("08_d_00_b_02_a_p1_std",   "08 D 00 B 02 A P1 Std"),
            ("08_d_00_b_02_a_p1_xstd",  "08 D 00 B 02 A P1 Xstd"),
            ("08_d_00_b_02_a_p2_lite",  "08 D 00 B 02 A P2 Lite"),
            ("08_d_00_b_02_a_p2_std",   "08 D 00 B 02 A P2 Std"),
            ("08_d_00_b_02_a_p2_xstd",  "08 D 00 B 02 A P2 Xstd"),
            ("08_d_00_b_02_a_p3_lite",  "08 D 00 B 02 A P3 Lite"),
            ("08_d_00_b_02_a_p3_std",   "08 D 00 B 02 A P3 Std"),
            ("08_d_00_b_02_a_p3_xstd",  "08 D 00 B 02 A P3 Xstd"),
            ("08_d_00_b_02_a_p4_lite",  "08 D 00 B 02 A P4 Lite"),
            ("08_d_00_b_02_a_p4_std",   "08 D 00 B 02 A P4 Std"),
            ("08_d_00_b_02_a_p4_xstd",  "08 D 00 B 02 A P4 Xstd"),
            ("08_d_00_b_02_a_p5_lite",  "08 D 00 B 02 A P5 Lite"),
            ("08_d_00_b_02_a_p5_std",   "08 D 00 B 02 A P5 Std"),
            ("08_d_00_b_02_a_p5_xstd",  "08 D 00 B 02 A P5 Xstd"),
            ("08_d_00_b_02_a_p6_lite",  "08 D 00 B 02 A P6 Lite"),
            ("08_d_00_b_02_a_p6_std",   "08 D 00 B 02 A P6 Std"),
            ("08_d_00_b_02_a_p6_xstd",  "08 D 00 B 02 A P6 Xstd"),
            ("08_d_00_b_02_a_p7_lite",  "08 D 00 B 02 A P7 Lite"),
            ("08_d_00_b_02_a_p7_std",   "08 D 00 B 02 A P7 Std"),
            ("08_d_00_b_02_a_p7_xstd",  "08 D 00 B 02 A P7 Xstd"),
            ("08_d_00_b_02_a_p8_lite",  "08 D 00 B 02 A P8 Lite"),
            ("08_d_00_b_02_a_p8_std",   "08 D 00 B 02 A P8 Std"),
            ("08_d_00_b_02_a_p8_xstd",  "08 D 00 B 02 A P8 Xstd"),
        ],
    )];
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static NamCapture> {
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.tone == tone)
        .ok_or_else(|| anyhow!("gain model '{}' does not support tone='{}'", MODEL_ID, tone))
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
