use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, KnobLayoutEntry};

pub const MODEL_ID: &str = "nam_ibanez_ts9";
pub const DISPLAY_NAME: &str = "TS9 Tube Screamer (NAM)";
const BRAND: &str = "ibanez";

pub const NAM_PLUGIN_DEFAULTS: NamPluginParams = NamPluginParams {
    input_level_db: 0.0,
    output_level_db: 0.0,
    noise_gate_threshold_db: -80.0,
    noise_gate_enabled: true,
    eq_enabled: false,
    bass: 5.0,
    middle: 5.0,
    treble: 5.0,
};

struct Ts9Capture {
    drive: i32,
    tone: i32,
    level: i32,
    model_path: &'static str,
}

const CAPTURES: &[Ts9Capture] = &[
    Ts9Capture { drive: 0,  tone: 6, level: 6,  model_path: "pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 0 Tone 6 Level 6.nam" },
    Ts9Capture { drive: 0,  tone: 7, level: 7,  model_path: "pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 0 Tone 7 Level 7.nam" },
    Ts9Capture { drive: 0,  tone: 9, level: 9,  model_path: "pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 0 Tone 9 Level 9.nam" },
    Ts9Capture { drive: 2,  tone: 7, level: 10, model_path: "pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 2 Tone 7 Level 10.nam" },
    Ts9Capture { drive: 7,  tone: 7, level: 7,  model_path: "pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 7 Tone 7 Level 7.nam" },
    Ts9Capture { drive: 7,  tone: 7, level: 9,  model_path: "pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 7 Tone 7 Level 9.nam" },
    Ts9Capture { drive: 8,  tone: 4, level: 5,  model_path: "pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 8 Tone 4 Level 5.nam" },
    Ts9Capture { drive: 8,  tone: 8, level: 8,  model_path: "pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 8 Tone 8 Level 8.nam" },
    Ts9Capture { drive: 10, tone: 9, level: 7,  model_path: "pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 10 Tone 9 Level 7.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        float_parameter("drive", "Drive", Some("Gain"), Some(7.0), 0.0, 10.0, 1.0, ParameterUnit::None),
        float_parameter("tone", "Tone", Some("EQ"), Some(7.0), 0.0, 10.0, 1.0, ParameterUnit::None),
        float_parameter("level", "Level", Some("Output"), Some(7.0), 0.0, 10.0, 1.0, ParameterUnit::None),
    ];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    log::info!("NAM TS9: selected capture D{} T{} L{}", capture.drive, capture.tone, capture.level);
    let plugin_params = plugin_params_from_set_with_defaults(params, NAM_PLUGIN_DEFAULTS)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(capture.model_path)?,
        None,
        plugin_params,
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

/// Find the capture closest to the requested (drive, tone, level).
/// Priority: drive first, then tone, then level.
fn resolve_capture(params: &ParameterSet) -> Result<&'static Ts9Capture> {
    let drive = required_f32(params, "drive").map_err(anyhow::Error::msg)?.round() as i32;
    let tone = required_f32(params, "tone").map_err(anyhow::Error::msg)?.round() as i32;
    let level = required_f32(params, "level").map_err(anyhow::Error::msg)?.round() as i32;

    CAPTURES
        .iter()
        .min_by_key(|c| {
            let dd = (c.drive - drive).abs() * 100; // drive has highest weight
            let dt = (c.tone - tone).abs() * 10;    // tone has medium weight
            let dl = (c.level - level).abs();        // level has lowest weight
            dd + dt + dl
        })
        .ok_or_else(|| anyhow!("no captures available for model '{}'", MODEL_ID))
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
    knob_layout: &[
        KnobLayoutEntry { param_key: "drive", svg_cx: 130.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0, max: 10.0, step: 1.0 },
        KnobLayoutEntry { param_key: "tone",  svg_cx: 302.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0, max: 10.0, step: 1.0 },
        KnobLayoutEntry { param_key: "level", svg_cx: 470.0, svg_cy: 90.0, svg_r: 22.0, min: 0.0, max: 10.0, step: 1.0 },
    ],
};
