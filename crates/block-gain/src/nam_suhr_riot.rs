use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_suhr_riot";
pub const DISPLAY_NAME: &str = "Suhr Riot";
const BRAND: &str = "suhr";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "drive100_level50_tone0_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive100_level50_tone0_vmid.nam" },
    NamCapture { tone: "drive100_level50_tone100_vmid", model_path: "pedals/suhr_riot/suhr_riot_drive100_level50_tone100_vmid.nam" },
    NamCapture { tone: "drive100_level50_tone25_vmid",  model_path: "pedals/suhr_riot/suhr_riot_drive100_level50_tone25_vmid.nam" },
    NamCapture { tone: "drive100_level50_tone50_vmid",  model_path: "pedals/suhr_riot/suhr_riot_drive100_level50_tone50_vmid.nam" },
    NamCapture { tone: "drive100_level50_tone75_vmid",  model_path: "pedals/suhr_riot/suhr_riot_drive100_level50_tone75_vmid.nam" },
    NamCapture { tone: "drive10_level50_tone0_vmid",    model_path: "pedals/suhr_riot/suhr_riot_drive10_level50_tone0_vmid.nam" },
    NamCapture { tone: "drive10_level50_tone100_vmid",  model_path: "pedals/suhr_riot/suhr_riot_drive10_level50_tone100_vmid.nam" },
    NamCapture { tone: "drive10_level50_tone25_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive10_level50_tone25_vmid.nam" },
    NamCapture { tone: "drive10_level50_tone50_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive10_level50_tone50_vmid.nam" },
    NamCapture { tone: "drive10_level50_tone75_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive10_level50_tone75_vmid.nam" },
    NamCapture { tone: "drive25_level50_tone0_vmid",    model_path: "pedals/suhr_riot/suhr_riot_drive25_level50_tone0_vmid.nam" },
    NamCapture { tone: "drive25_level50_tone100_vmid",  model_path: "pedals/suhr_riot/suhr_riot_drive25_level50_tone100_vmid.nam" },
    NamCapture { tone: "drive25_level50_tone25_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive25_level50_tone25_vmid.nam" },
    NamCapture { tone: "drive25_level50_tone50_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive25_level50_tone50_vmid.nam" },
    NamCapture { tone: "drive25_level50_tone75_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive25_level50_tone75_vmid.nam" },
    NamCapture { tone: "drive50_level50_tone0_vmid",    model_path: "pedals/suhr_riot/suhr_riot_drive50_level50_tone0_vmid.nam" },
    NamCapture { tone: "drive50_level50_tone100_vmid",  model_path: "pedals/suhr_riot/suhr_riot_drive50_level50_tone100_vmid.nam" },
    NamCapture { tone: "drive50_level50_tone25_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive50_level50_tone25_vmid.nam" },
    NamCapture { tone: "drive50_level50_tone50_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive50_level50_tone50_vmid.nam" },
    NamCapture { tone: "drive50_level50_tone75_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive50_level50_tone75_vmid.nam" },
    NamCapture { tone: "drive75_level50_tone0_vmid",    model_path: "pedals/suhr_riot/suhr_riot_drive75_level50_tone0_vmid.nam" },
    NamCapture { tone: "drive75_level50_tone100_vmid",  model_path: "pedals/suhr_riot/suhr_riot_drive75_level50_tone100_vmid.nam" },
    NamCapture { tone: "drive75_level50_tone25_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive75_level50_tone25_vmid.nam" },
    NamCapture { tone: "drive75_level50_tone50_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive75_level50_tone50_vmid.nam" },
    NamCapture { tone: "drive75_level50_tone75_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive75_level50_tone75_vmid.nam" },
    NamCapture { tone: "drive85_level50_tone0_vmid",    model_path: "pedals/suhr_riot/suhr_riot_drive85_level50_tone0_vmid.nam" },
    NamCapture { tone: "drive85_level50_tone100_vmid",  model_path: "pedals/suhr_riot/suhr_riot_drive85_level50_tone100_vmid.nam" },
    NamCapture { tone: "drive85_level50_tone25_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive85_level50_tone25_vmid.nam" },
    NamCapture { tone: "drive85_level50_tone50_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive85_level50_tone50_vmid.nam" },
    NamCapture { tone: "drive85_level50_tone75_vmid",   model_path: "pedals/suhr_riot/suhr_riot_drive85_level50_tone75_vmid.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("drive100_level50_tone0_vmid"),
        &[
            ("drive100_level50_tone0_vmid",   "Drive100 Level50 Tone0 Vmid"),
            ("drive100_level50_tone100_vmid", "Drive100 Level50 Tone100 Vmid"),
            ("drive100_level50_tone25_vmid",  "Drive100 Level50 Tone25 Vmid"),
            ("drive100_level50_tone50_vmid",  "Drive100 Level50 Tone50 Vmid"),
            ("drive100_level50_tone75_vmid",  "Drive100 Level50 Tone75 Vmid"),
            ("drive10_level50_tone0_vmid",    "Drive10 Level50 Tone0 Vmid"),
            ("drive10_level50_tone100_vmid",  "Drive10 Level50 Tone100 Vmid"),
            ("drive10_level50_tone25_vmid",   "Drive10 Level50 Tone25 Vmid"),
            ("drive10_level50_tone50_vmid",   "Drive10 Level50 Tone50 Vmid"),
            ("drive10_level50_tone75_vmid",   "Drive10 Level50 Tone75 Vmid"),
            ("drive25_level50_tone0_vmid",    "Drive25 Level50 Tone0 Vmid"),
            ("drive25_level50_tone100_vmid",  "Drive25 Level50 Tone100 Vmid"),
            ("drive25_level50_tone25_vmid",   "Drive25 Level50 Tone25 Vmid"),
            ("drive25_level50_tone50_vmid",   "Drive25 Level50 Tone50 Vmid"),
            ("drive25_level50_tone75_vmid",   "Drive25 Level50 Tone75 Vmid"),
            ("drive50_level50_tone0_vmid",    "Drive50 Level50 Tone0 Vmid"),
            ("drive50_level50_tone100_vmid",  "Drive50 Level50 Tone100 Vmid"),
            ("drive50_level50_tone25_vmid",   "Drive50 Level50 Tone25 Vmid"),
            ("drive50_level50_tone50_vmid",   "Drive50 Level50 Tone50 Vmid"),
            ("drive50_level50_tone75_vmid",   "Drive50 Level50 Tone75 Vmid"),
            ("drive75_level50_tone0_vmid",    "Drive75 Level50 Tone0 Vmid"),
            ("drive75_level50_tone100_vmid",  "Drive75 Level50 Tone100 Vmid"),
            ("drive75_level50_tone25_vmid",   "Drive75 Level50 Tone25 Vmid"),
            ("drive75_level50_tone50_vmid",   "Drive75 Level50 Tone50 Vmid"),
            ("drive75_level50_tone75_vmid",   "Drive75 Level50 Tone75 Vmid"),
            ("drive85_level50_tone0_vmid",    "Drive85 Level50 Tone0 Vmid"),
            ("drive85_level50_tone100_vmid",  "Drive85 Level50 Tone100 Vmid"),
            ("drive85_level50_tone25_vmid",   "Drive85 Level50 Tone25 Vmid"),
            ("drive85_level50_tone50_vmid",   "Drive85 Level50 Tone50 Vmid"),
            ("drive85_level50_tone75_vmid",   "Drive85 Level50 Tone75 Vmid"),
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
