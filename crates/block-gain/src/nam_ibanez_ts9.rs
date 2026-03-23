use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, ModelParameterSchema, ParameterSet, required_string};
use block_core::{AudioChannelLayout, BlockProcessor};

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
    id: &'static str,
    label: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[Ts9Capture] = &[
    Ts9Capture {
        id: "clean_warm",
        label: "Clean Warm (D0 T6 L6)",
        model_path: "captures/nam/pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 0 Tone 6 Level 6.nam",
    },
    Ts9Capture {
        id: "clean_bright",
        label: "Clean Bright (D0 T7 L7)",
        model_path: "captures/nam/pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 0 Tone 7 Level 7.nam",
    },
    Ts9Capture {
        id: "clean_hot",
        label: "Clean Hot (D0 T9 L9)",
        model_path: "captures/nam/pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 0 Tone 9 Level 9.nam",
    },
    Ts9Capture {
        id: "light_crunch",
        label: "Light Crunch (D2 T7 L10)",
        model_path: "captures/nam/pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 2 Tone 7 Level 10.nam",
    },
    Ts9Capture {
        id: "mid_drive",
        label: "Mid Drive (D7 T7 L7)",
        model_path: "captures/nam/pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 7 Tone 7 Level 7.nam",
    },
    Ts9Capture {
        id: "mid_drive_hot",
        label: "Mid Drive Hot (D7 T7 L9)",
        model_path: "captures/nam/pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 7 Tone 7 Level 9.nam",
    },
    Ts9Capture {
        id: "heavy_dark",
        label: "Heavy Dark (D8 T4 L5)",
        model_path: "captures/nam/pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 8 Tone 4 Level 5.nam",
    },
    Ts9Capture {
        id: "heavy_bright",
        label: "Heavy Bright (D8 T8 L8)",
        model_path: "captures/nam/pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 8 Tone 8 Level 8.nam",
    },
    Ts9Capture {
        id: "max_drive",
        label: "Max Drive (D10 T9 L7)",
        model_path: "captures/nam/pedals/ibanez_ts9_tube_screamer/Ibanez TS9 Tube Screamer Drive 10 Tone 9 Level 7.nam",
    },
];

pub fn model_schema() -> ModelParameterSchema {
    let options: Vec<(&str, &str)> = CAPTURES.iter().map(|c| (c.id, c.label)).collect();
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "preset",
        "Preset",
        Some("Drive"),
        Some("mid_drive"),
        &options,
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let capture = resolve_capture(params)?;
    let plugin_params = plugin_params_from_set_with_defaults(params, NAM_PLUGIN_DEFAULTS)?;
    build_processor_with_assets_for_layout(
        capture.model_path,
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static Ts9Capture> {
    let requested = required_string(params, "preset").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.id == requested)
        .ok_or_else(|| anyhow!("gain model '{}' does not support preset '{}'", MODEL_ID, requested))
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
    knob_layout: &[],
};
