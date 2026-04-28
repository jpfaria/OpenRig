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

// Two-axis pack: drive × tone (level fixed at 50, voice fixed at mid).
// 6 × 5 = 30 captures, full grid.
const CAPTURES: &[(&str, &str, &str)] = &[
    // (drive, tone, file)
    ("10",  "0",   "pedals/suhr_riot/suhr_riot_drive10_level50_tone0_vmid.nam"),
    ("10",  "25",  "pedals/suhr_riot/suhr_riot_drive10_level50_tone25_vmid.nam"),
    ("10",  "50",  "pedals/suhr_riot/suhr_riot_drive10_level50_tone50_vmid.nam"),
    ("10",  "75",  "pedals/suhr_riot/suhr_riot_drive10_level50_tone75_vmid.nam"),
    ("10",  "100", "pedals/suhr_riot/suhr_riot_drive10_level50_tone100_vmid.nam"),
    ("25",  "0",   "pedals/suhr_riot/suhr_riot_drive25_level50_tone0_vmid.nam"),
    ("25",  "25",  "pedals/suhr_riot/suhr_riot_drive25_level50_tone25_vmid.nam"),
    ("25",  "50",  "pedals/suhr_riot/suhr_riot_drive25_level50_tone50_vmid.nam"),
    ("25",  "75",  "pedals/suhr_riot/suhr_riot_drive25_level50_tone75_vmid.nam"),
    ("25",  "100", "pedals/suhr_riot/suhr_riot_drive25_level50_tone100_vmid.nam"),
    ("50",  "0",   "pedals/suhr_riot/suhr_riot_drive50_level50_tone0_vmid.nam"),
    ("50",  "25",  "pedals/suhr_riot/suhr_riot_drive50_level50_tone25_vmid.nam"),
    ("50",  "50",  "pedals/suhr_riot/suhr_riot_drive50_level50_tone50_vmid.nam"),
    ("50",  "75",  "pedals/suhr_riot/suhr_riot_drive50_level50_tone75_vmid.nam"),
    ("50",  "100", "pedals/suhr_riot/suhr_riot_drive50_level50_tone100_vmid.nam"),
    ("75",  "0",   "pedals/suhr_riot/suhr_riot_drive75_level50_tone0_vmid.nam"),
    ("75",  "25",  "pedals/suhr_riot/suhr_riot_drive75_level50_tone25_vmid.nam"),
    ("75",  "50",  "pedals/suhr_riot/suhr_riot_drive75_level50_tone50_vmid.nam"),
    ("75",  "75",  "pedals/suhr_riot/suhr_riot_drive75_level50_tone75_vmid.nam"),
    ("75",  "100", "pedals/suhr_riot/suhr_riot_drive75_level50_tone100_vmid.nam"),
    ("85",  "0",   "pedals/suhr_riot/suhr_riot_drive85_level50_tone0_vmid.nam"),
    ("85",  "25",  "pedals/suhr_riot/suhr_riot_drive85_level50_tone25_vmid.nam"),
    ("85",  "50",  "pedals/suhr_riot/suhr_riot_drive85_level50_tone50_vmid.nam"),
    ("85",  "75",  "pedals/suhr_riot/suhr_riot_drive85_level50_tone75_vmid.nam"),
    ("85",  "100", "pedals/suhr_riot/suhr_riot_drive85_level50_tone100_vmid.nam"),
    ("100", "0",   "pedals/suhr_riot/suhr_riot_drive100_level50_tone0_vmid.nam"),
    ("100", "25",  "pedals/suhr_riot/suhr_riot_drive100_level50_tone25_vmid.nam"),
    ("100", "50",  "pedals/suhr_riot/suhr_riot_drive100_level50_tone50_vmid.nam"),
    ("100", "75",  "pedals/suhr_riot/suhr_riot_drive100_level50_tone75_vmid.nam"),
    ("100", "100", "pedals/suhr_riot/suhr_riot_drive100_level50_tone100_vmid.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "drive",
            "Distortion",
            Some("Pedal"),
            Some("50"),
            &[
                ("10",  "10%"),
                ("25",  "25%"),
                ("50",  "50%"),
                ("75",  "75%"),
                ("85",  "85%"),
                ("100", "100%"),
            ],
        ),
        enum_parameter(
            "tone",
            "Tone",
            Some("Pedal"),
            Some("50"),
            &[
                ("0",   "0%"),
                ("25",  "25%"),
                ("50",  "50%"),
                ("75",  "75%"),
                ("100", "100%"),
            ],
        ),
    ];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let path = resolve_capture(params)?;
    build_processor_with_assets_for_layout(
        &nam::resolve_nam_capture(path)?,
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
    let path = resolve_capture(params)?;
    Ok(format!("model='{}'", path))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let drive = required_string(params, "drive").map_err(anyhow::Error::msg)?;
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(d, t, _)| *d == drive && *t == tone)
        .map(|(_, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for drive={} tone={}",
                MODEL_ID, drive, tone
            )
        })
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
