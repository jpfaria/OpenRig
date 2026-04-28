use anyhow::{anyhow, Result};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_diezel_zerrer";
pub const DISPLAY_NAME: &str = "Zerrer";
const BRAND: &str = "diezel";

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

const CAPTURES: &[(&str, &str, &str)] = &[
    ("04_od_g12_t2_m10_b11_v12_p3_d11", "04 Zerrer OD G12 T2 M10 B11 V12 P3 D11 48hz 24Bit", "preamp/diezel_zerrer/04_zerrer_od_g12_t2_m10_b11_v12_p3_d11_48hz_24bit.nam"),
    ("05_od_g3_t2_m10_b11_v12_p3_d11", "05 Zerrer OD G3 T2 M10 B11 V12 P3 D11 48hz 24Bit", "preamp/diezel_zerrer/05_zerrer_od_g3_t2_m10_b11_v12_p3_d11_48hz_24bit.nam"),
    ("28_od_g10_t4_m12_b3_v2_p3_d11", "28 Zerrer OD G10 T4 M12 B3 V2 P3 D11 48hz 24Bit", "preamp/diezel_zerrer/28_zerrer_od_g10_t4_m12_b3_v2_p3_d11_48hz_24bit.nam"),
    ("06_od_g9_t2_m10_b11_v12_p3_d11", "06 Zerrer OD G9 T2 M10 B11 V12 P3 D11 48hz 24Bit", "preamp/diezel_zerrer/06_zerrer_od_g9_t2_m10_b11_v12_p3_d11_48hz_24bit.nam"),
    ("26_od_g3_t3_m12_b3_v1_p3_d11", "26 Zerrer OD G3 T3 M12 B3 V1 P3 D11 48hz 24Bit", "preamp/diezel_zerrer/26_zerrer_od_g3_t3_m12_b3_v1_p3_d11_48hz_24bit.nam"),
    ("27_od_g11_t4_m3_b1_v1_p3_d11", "27 Zerrer OD G11 T4 M3 B1 V1 P3 D11 48hz 24Bit", "preamp/diezel_zerrer/27_zerrer_od_g11_t4_m3_b1_v1_p3_d11_48hz_24bit.nam"),
    ("25_od_g4_t3_m7_b3_v1_p3_d11", "25 Zerrer OD G4 T3 M7 B3 V1 P3 D11 48hz 24Bit", "preamp/diezel_zerrer/25_zerrer_od_g4_t3_m7_b3_v1_p3_d11_48hz_24bit.nam"),
    ("12_ch1_g10_t12_m2_b3", "12 Zerrer CH1 G10 T12 M2 B3 48hz 24Bit", "preamp/diezel_zerrer/12_zerrer_ch1_g10_t12_m2_b3_48hz_24bit.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema =
        model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "capture",
        "Capture",
        Some("Amp"),
        Some("04_od_g12_t2_m10_b11_v12_p3_d11"),
        &[
            ("04_od_g12_t2_m10_b11_v12_p3_d11", "04 Zerrer OD G12 T2 M10 B11 V12 P3 D11 48hz 24Bit"),
            ("05_od_g3_t2_m10_b11_v12_p3_d11", "05 Zerrer OD G3 T2 M10 B11 V12 P3 D11 48hz 24Bit"),
            ("28_od_g10_t4_m12_b3_v2_p3_d11", "28 Zerrer OD G10 T4 M12 B3 V2 P3 D11 48hz 24Bit"),
            ("06_od_g9_t2_m10_b11_v12_p3_d11", "06 Zerrer OD G9 T2 M10 B11 V12 P3 D11 48hz 24Bit"),
            ("26_od_g3_t3_m12_b3_v1_p3_d11", "26 Zerrer OD G3 T3 M12 B3 V1 P3 D11 48hz 24Bit"),
            ("27_od_g11_t4_m3_b1_v1_p3_d11", "27 Zerrer OD G11 T4 M3 B1 V1 P3 D11 48hz 24Bit"),
            ("25_od_g4_t3_m7_b3_v1_p3_d11", "25 Zerrer OD G4 T3 M7 B3 V1 P3 D11 48hz 24Bit"),
            ("12_ch1_g10_t12_m2_b3", "12 Zerrer CH1 G10 T12 M2 B3 48hz 24Bit"),
        ],
    )];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let path = resolve_capture(params)?;
    let plugin_params = plugin_params_from_set_with_defaults(params, NAM_PLUGIN_DEFAULTS)?;
    let model_path = nam::resolve_nam_capture(path)?;
    build_processor_with_assets_for_layout(&model_path, None, plugin_params, sample_rate, layout)
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let key = required_string(params, "capture").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| anyhow!("preamp '{}' has no capture '{}'", MODEL_ID, key))
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
    brand: BRAND,
    backend_kind: PreampBackendKind::Nam,
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
