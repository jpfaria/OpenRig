use anyhow::{anyhow, Result};
use asset_runtime::{materialize, EmbeddedAsset};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "marshall_jcm_800_2203";
pub const DISPLAY_NAME: &str = "JCM 800 2203";

macro_rules! capture {
    ($volume:literal, $gain:literal, $asset_id:literal, $relative_path:literal) => {
        MarshallJcm800Capture {
            params: MarshallJcm800Params {
                volume: $volume,
                gain: $gain,
            },
            asset: EmbeddedAsset::new(
                $asset_id,
                $relative_path,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../",
                    $relative_path
                )),
            ),
        }
    };
}

pub const NAM_PLUGIN_DEFAULTS: NamPluginParams = NamPluginParams {
    input_level_db: 0.0,
    output_level_db: 0.0,
    noise_gate_threshold_db: -80.0,
    noise_gate_enabled: true,
    eq_enabled: true,
    bass: 5.0,
    middle: 5.0,
    treble: 5.0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarshallJcm800Params {
    pub volume: i32,
    pub gain: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarshallJcm800Capture {
    pub params: MarshallJcm800Params,
    pub asset: EmbeddedAsset,
}

pub const CAPTURES: &[MarshallJcm800Capture] = &[
    capture!(
        50,
        10,
        "preamp.marshall_jcm_800_2203.mv50.g10",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv5_g1_azg_700.nam"
    ),
    capture!(
        50,
        20,
        "preamp.marshall_jcm_800_2203.mv50.g20",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv5_g2_azg_700.nam"
    ),
    capture!(
        50,
        30,
        "preamp.marshall_jcm_800_2203.mv50.g30",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv5_g3_azg_700.nam"
    ),
    capture!(
        50,
        40,
        "preamp.marshall_jcm_800_2203.mv50.g40",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv5_g4_azg_700.nam"
    ),
    capture!(
        50,
        50,
        "preamp.marshall_jcm_800_2203.mv50.g50",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv5_g5_azg_700.nam"
    ),
    capture!(
        50,
        60,
        "preamp.marshall_jcm_800_2203.mv50.g60",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv5_g6_azg_700.nam"
    ),
    capture!(
        50,
        70,
        "preamp.marshall_jcm_800_2203.mv50.g70",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv5_g7_azg_700.nam"
    ),
    capture!(
        50,
        80,
        "preamp.marshall_jcm_800_2203.mv50.g80",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv5_g8_azg_700.nam"
    ),
    capture!(
        50,
        90,
        "preamp.marshall_jcm_800_2203.mv50.g90",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv5_g9_azg_700.nam"
    ),
    capture!(
        50,
        100,
        "preamp.marshall_jcm_800_2203.mv50.g100",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv5_g10_azg_700.nam"
    ),
    capture!(
        60,
        10,
        "preamp.marshall_jcm_800_2203.mv60.g10",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv6_g1_azg_700.nam"
    ),
    capture!(
        60,
        20,
        "preamp.marshall_jcm_800_2203.mv60.g20",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv6_g2_azg_700.nam"
    ),
    capture!(
        60,
        30,
        "preamp.marshall_jcm_800_2203.mv60.g30",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv6_g3_azg_700.nam"
    ),
    capture!(
        60,
        40,
        "preamp.marshall_jcm_800_2203.mv60.g40",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv6_g4_azg_700.nam"
    ),
    capture!(
        60,
        50,
        "preamp.marshall_jcm_800_2203.mv60.g50",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv6_g5_azg_700.nam"
    ),
    capture!(
        60,
        60,
        "preamp.marshall_jcm_800_2203.mv60.g60",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv6_g6_azg_700.nam"
    ),
    capture!(
        60,
        70,
        "preamp.marshall_jcm_800_2203.mv60.g70",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv6_g7_azg_700.nam"
    ),
    capture!(
        60,
        80,
        "preamp.marshall_jcm_800_2203.mv60.g80",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv6_g8_azg_700.nam"
    ),
    capture!(
        60,
        90,
        "preamp.marshall_jcm_800_2203.mv60.g90",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv6_g9_azg_700.nam"
    ),
    capture!(
        60,
        100,
        "preamp.marshall_jcm_800_2203.mv60.g100",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv6_g10_azg_700.nam"
    ),
    capture!(
        70,
        10,
        "preamp.marshall_jcm_800_2203.mv70.g10",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv7_g1_azg_700.nam"
    ),
    capture!(
        70,
        20,
        "preamp.marshall_jcm_800_2203.mv70.g20",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv7_g2_azg_700.nam"
    ),
    capture!(
        70,
        30,
        "preamp.marshall_jcm_800_2203.mv70.g30",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv7_g3_azg_700.nam"
    ),
    capture!(
        70,
        40,
        "preamp.marshall_jcm_800_2203.mv70.g40",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv7_g4_azg_700.nam"
    ),
    capture!(
        70,
        50,
        "preamp.marshall_jcm_800_2203.mv70.g50",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv7_g5_azg_700.nam"
    ),
    capture!(
        70,
        60,
        "preamp.marshall_jcm_800_2203.mv70.g60",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv7_g6_azg_700.nam"
    ),
    capture!(
        70,
        70,
        "preamp.marshall_jcm_800_2203.mv70.g70",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv7_g7_azg_700.nam"
    ),
    capture!(
        70,
        80,
        "preamp.marshall_jcm_800_2203.mv70.g80",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv7_g8_azg_700.nam"
    ),
    capture!(
        70,
        90,
        "preamp.marshall_jcm_800_2203.mv70.g90",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv7_g9_azg_700.nam"
    ),
    capture!(
        70,
        100,
        "preamp.marshall_jcm_800_2203.mv70.g100",
        "captures/nam/preamp/marshall_jcm_800_2203/jcm800_2203_p5_b5_m5_t5_mv7_g10_azg_700.nam"
    ),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for("preamp", MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        float_parameter(
            "volume",
            "Volume",
            Some("Amp"),
            Some(50.0),
            50.0,
            70.0,
            10.0,
            ParameterUnit::Percent,
        ),
        float_parameter(
            "gain",
            "Gain",
            Some("Amp"),
            Some(40.0),
            10.0,
            100.0,
            10.0,
            ParameterUnit::Percent,
        ),
    ];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    let plugin_params = plugin_params_from_set_with_defaults(params, NAM_PLUGIN_DEFAULTS)?;
    let model_path = materialize(&capture.asset)?;
    build_processor_with_assets_for_layout(
        &model_path.to_string_lossy(),
        None,
        plugin_params,
        sample_rate,
        layout,
    )
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

pub const MODEL_DEFINITION: PreampModelDefinition = PreampModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "marshall",
    backend_kind: PreampBackendKind::Nam,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[
        block_core::KnobLayoutEntry { param_key: "volume",  svg_cx: 500.0, svg_cy: 140.0, svg_r: 22.0, min: 50.0, max: 70.0,  step: 10.0 },
        block_core::KnobLayoutEntry { param_key: "gain",    svg_cx: 650.0, svg_cy: 140.0, svg_r: 22.0, min: 10.0, max: 100.0, step: 10.0 },
    ],
};

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("asset_id='{}'", capture.asset.id))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static MarshallJcm800Capture> {
    let requested = MarshallJcm800Params {
        volume: read_percent(params, "volume")?,
        gain: read_percent(params, "gain")?,
    };

    CAPTURES
        .iter()
        .find(|capture| capture.params == requested)
        .ok_or_else(|| {
            anyhow!(
                "amp model '{}' does not support volume={} gain={}",
                MODEL_ID,
                requested.volume,
                requested.gain
            )
        })
}

fn read_percent(params: &ParameterSet, path: &str) -> Result<i32> {
    let value = required_f32(params, path).map_err(anyhow::Error::msg)?;
    let rounded = value.round();
    if (value - rounded).abs() > 1e-4 {
        return Err(anyhow!(
            "amp model '{}' requires '{}' to be a whole-number percentage, got {}",
            MODEL_ID,
            path,
            value
        ));
    }
    Ok(rounded as i32)
}
