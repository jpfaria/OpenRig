use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_digitech_bad_monkey";
pub const DISPLAY_NAME: &str = "DigiTech Bad Monkey";
const BRAND: &str = "digitech";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "boost1_v10_b4_t6_g0_feather", model_path: "pedals/digitech_bad_monkey/badmonkey_boost1_v10_b4_t6_g0_feather.nam" },
    NamCapture { tone: "boost1_v10_b4_t6_g0_lite", model_path: "pedals/digitech_bad_monkey/badmonkey_boost1_v10_b4_t6_g0_lite.nam" },
    NamCapture { tone: "boost1_v10_b4_t6_g0_standard", model_path: "pedals/digitech_bad_monkey/badmonkey_boost1_v10_b4_t6_g0_standard.nam" },
    NamCapture { tone: "boost2_v10_b4_t6_g4_feather", model_path: "pedals/digitech_bad_monkey/badmonkey_boost2_v10_b4_t6_g4_feather.nam" },
    NamCapture { tone: "boost2_v10_b4_t6_g4_lite", model_path: "pedals/digitech_bad_monkey/badmonkey_boost2_v10_b4_t6_g4_lite.nam" },
    NamCapture { tone: "boost2_v10_b4_t6_g4_standard", model_path: "pedals/digitech_bad_monkey/badmonkey_boost2_v10_b4_t6_g4_standard.nam" },
    NamCapture { tone: "boost3_v10_b7_t6_g0_feather", model_path: "pedals/digitech_bad_monkey/badmonkey_boost3_v10_b7_t6_g0_feather.nam" },
    NamCapture { tone: "boost3_v10_b7_t6_g0_lite", model_path: "pedals/digitech_bad_monkey/badmonkey_boost3_v10_b7_t6_g0_lite.nam" },
    NamCapture { tone: "boost3_v10_b7_t6_g0_standard", model_path: "pedals/digitech_bad_monkey/badmonkey_boost3_v10_b7_t6_g0_standard.nam" },
    NamCapture { tone: "boost3_v10_b7_t6_g4_feather", model_path: "pedals/digitech_bad_monkey/badmonkey_boost3_v10_b7_t6_g4_feather.nam" },
    NamCapture { tone: "boost3_v10_b7_t6_g4_lite", model_path: "pedals/digitech_bad_monkey/badmonkey_boost3_v10_b7_t6_g4_lite.nam" },
    NamCapture { tone: "boost3_v10_b7_t6_g4_standard", model_path: "pedals/digitech_bad_monkey/badmonkey_boost3_v10_b7_t6_g4_standard.nam" },
    NamCapture { tone: "fulltone_feather", model_path: "pedals/digitech_bad_monkey/badmonkey_fulltone_feather.nam" },
    NamCapture { tone: "fulltone_lite", model_path: "pedals/digitech_bad_monkey/badmonkey_fulltone_lite.nam" },
    NamCapture { tone: "fulltone_standard", model_path: "pedals/digitech_bad_monkey/badmonkey_fulltone_standard.nam" },
    NamCapture { tone: "glory_feather", model_path: "pedals/digitech_bad_monkey/badmonkey_glory_feather.nam" },
    NamCapture { tone: "glory_lite", model_path: "pedals/digitech_bad_monkey/badmonkey_glory_lite.nam" },
    NamCapture { tone: "glory_standard", model_path: "pedals/digitech_bad_monkey/badmonkey_glory_standard.nam" },
    NamCapture { tone: "klon_feather", model_path: "pedals/digitech_bad_monkey/badmonkey_klon_feather.nam" },
    NamCapture { tone: "klon_lite", model_path: "pedals/digitech_bad_monkey/badmonkey_klon_lite.nam" },
    NamCapture { tone: "klon_standard", model_path: "pedals/digitech_bad_monkey/badmonkey_klon_standard.nam" },
    NamCapture { tone: "noble_odr_1_feather", model_path: "pedals/digitech_bad_monkey/badmonkey_noble_odr_1_feather.nam" },
    NamCapture { tone: "noble_odr_1_lite", model_path: "pedals/digitech_bad_monkey/badmonkey_noble_odr_1_lite.nam" },
    NamCapture { tone: "noble_odr_1_standard", model_path: "pedals/digitech_bad_monkey/badmonkey_noble_odr_1_standard.nam" },
    NamCapture { tone: "ts10_feather", model_path: "pedals/digitech_bad_monkey/badmonkey_ts10_feather.nam" },
    NamCapture { tone: "ts10_lite", model_path: "pedals/digitech_bad_monkey/badmonkey_ts10_lite.nam" },
    NamCapture { tone: "ts10_standard", model_path: "pedals/digitech_bad_monkey/badmonkey_ts10_standard.nam" },
    NamCapture { tone: "zen_feather", model_path: "pedals/digitech_bad_monkey/badmonkey_zen_feather.nam" },
    NamCapture { tone: "zen_lite", model_path: "pedals/digitech_bad_monkey/badmonkey_zen_lite.nam" },
    NamCapture { tone: "zen_standard", model_path: "pedals/digitech_bad_monkey/badmonkey_zen_standard.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("boost1_v10_b4_t6_g0_feather"),
        &[
            ("boost1_v10_b4_t6_g0_feather", "Boost1 V10 B4 T6 G0 Feather"),
            ("boost1_v10_b4_t6_g0_lite", "Boost1 V10 B4 T6 G0 Lite"),
            ("boost1_v10_b4_t6_g0_standard", "Boost1 V10 B4 T6 G0 Standard"),
            ("boost2_v10_b4_t6_g4_feather", "Boost2 V10 B4 T6 G4 Feather"),
            ("boost2_v10_b4_t6_g4_lite", "Boost2 V10 B4 T6 G4 Lite"),
            ("boost2_v10_b4_t6_g4_standard", "Boost2 V10 B4 T6 G4 Standard"),
            ("boost3_v10_b7_t6_g0_feather", "Boost3 V10 B7 T6 G0 Feather"),
            ("boost3_v10_b7_t6_g0_lite", "Boost3 V10 B7 T6 G0 Lite"),
            ("boost3_v10_b7_t6_g0_standard", "Boost3 V10 B7 T6 G0 Standard"),
            ("boost3_v10_b7_t6_g4_feather", "Boost3 V10 B7 T6 G4 Feather"),
            ("boost3_v10_b7_t6_g4_lite", "Boost3 V10 B7 T6 G4 Lite"),
            ("boost3_v10_b7_t6_g4_standard", "Boost3 V10 B7 T6 G4 Standard"),
            ("fulltone_feather", "Fulltone Feather"),
            ("fulltone_lite", "Fulltone Lite"),
            ("fulltone_standard", "Fulltone Standard"),
            ("glory_feather", "Glory Feather"),
            ("glory_lite", "Glory Lite"),
            ("glory_standard", "Glory Standard"),
            ("klon_feather", "Klon Feather"),
            ("klon_lite", "Klon Lite"),
            ("klon_standard", "Klon Standard"),
            ("noble_odr_1_feather", "Noble Odr 1 Feather"),
            ("noble_odr_1_lite", "Noble Odr 1 Lite"),
            ("noble_odr_1_standard", "Noble Odr 1 Standard"),
            ("ts10_feather", "Ts10 Feather"),
            ("ts10_lite", "Ts10 Lite"),
            ("ts10_standard", "Ts10 Standard"),
            ("zen_feather", "Zen Feather"),
            ("zen_lite", "Zen Lite"),
            ("zen_standard", "Zen Standard"),
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
