use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_mxr_duke_of_tone";
pub const DISPLAY_NAME: &str = "MXR Duke of Tone";
const BRAND: &str = "mxr";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

// Three-axis pack: mode × voicing × tone (volume fixed at 3:30).
// 2 modes (OD / Dist) × 5 voicings × 3 tone steps = 30 captures, full grid.
// Each voicing label corresponds to a fixed drive position that can't be
// changed independently of the voicing in the original capture set.
const CAPTURES: &[(&str, &str, &str, &str)] = &[
    // (mode, voicing, tone, file)
    ("od",   "high",    "230",  "pedals/mxr_duke_of_tone/dot_od_high_vol_330_tone_230_drive_300.nam"),
    ("od",   "high",    "1030", "pedals/mxr_duke_of_tone/dot_od_high_vol_330_tone_1030_drive_300.nam"),
    ("od",   "high",    "1200", "pedals/mxr_duke_of_tone/dot_od_high_vol_330_tone_1200_drive_300.nam"),
    ("od",   "cranked", "230",  "pedals/mxr_duke_of_tone/dot_od_cranked_vol_330_tone_230_drive_500.nam"),
    ("od",   "cranked", "1030", "pedals/mxr_duke_of_tone/dot_od_cranked_vol_330_tone_1030_drive_500.nam"),
    ("od",   "cranked", "1200", "pedals/mxr_duke_of_tone/dot_od_cranked_vol_330_tone_1200_drive_500.nam"),
    ("od",   "color",   "230",  "pedals/mxr_duke_of_tone/dot_od_color_vol_330_tone_230_drive_700.nam"),
    ("od",   "color",   "1030", "pedals/mxr_duke_of_tone/dot_od_color_vol_330_tone_1030_drive_700.nam"),
    ("od",   "color",   "1200", "pedals/mxr_duke_of_tone/dot_od_color_vol_330_tone_1200_drive_700.nam"),
    ("od",   "low",     "230",  "pedals/mxr_duke_of_tone/dot_od_low_vol_330_tone_230_drive_900.nam"),
    ("od",   "low",     "1030", "pedals/mxr_duke_of_tone/dot_od_low_vol_330_tone_1030_drive_900.nam"),
    ("od",   "low",     "1200", "pedals/mxr_duke_of_tone/dot_od_low_vol_330_tone_1200_drive_900.nam"),
    ("od",   "med",     "230",  "pedals/mxr_duke_of_tone/dot_od_med_vol_330_tone_230_drive_1200.nam"),
    ("od",   "med",     "1030", "pedals/mxr_duke_of_tone/dot_od_med_vol_330_tone_1030_drive_1200.nam"),
    ("od",   "med",     "1200", "pedals/mxr_duke_of_tone/dot_od_med_vol_330_tone_1200_drive_1200.nam"),
    ("dist", "high",    "230",  "pedals/mxr_duke_of_tone/dot_dist_high_vol_330_tone_230_drive_300.nam"),
    ("dist", "high",    "1030", "pedals/mxr_duke_of_tone/dot_dist_high_vol_330_tone_1030_drive_300.nam"),
    ("dist", "high",    "1200", "pedals/mxr_duke_of_tone/dot_dist_high_vol_330_tone_1200_drive_300.nam"),
    ("dist", "cranked", "230",  "pedals/mxr_duke_of_tone/dot_dist_cranked_vol_330_tone_230_drive_500.nam"),
    ("dist", "cranked", "1030", "pedals/mxr_duke_of_tone/dot_dist_cranked_vol_330_tone_1030_drive_500.nam"),
    ("dist", "cranked", "1200", "pedals/mxr_duke_of_tone/dot_dist_cranked_vol_330_tone_1200_drive_500.nam"),
    ("dist", "color",   "230",  "pedals/mxr_duke_of_tone/dot_dist_color_vol_330_tone_230_drive_700.nam"),
    ("dist", "color",   "1030", "pedals/mxr_duke_of_tone/dot_dist_color_vol_330_tone_1030_drive_700.nam"),
    ("dist", "color",   "1200", "pedals/mxr_duke_of_tone/dot_dist_color_vol_330_tone_1200_drive_700.nam"),
    ("dist", "low",     "230",  "pedals/mxr_duke_of_tone/dot_dist_low_vol_330_tone_230_drive_900.nam"),
    ("dist", "low",     "1030", "pedals/mxr_duke_of_tone/dot_dist_low_vol_330_tone_1030_drive_900.nam"),
    ("dist", "low",     "1200", "pedals/mxr_duke_of_tone/dot_dist_low_vol_330_tone_1200_drive_900.nam"),
    ("dist", "med",     "230",  "pedals/mxr_duke_of_tone/dot_dist_med_vol_330_tone_230_drive_1200.nam"),
    ("dist", "med",     "1030", "pedals/mxr_duke_of_tone/dot_dist_med_vol_330_tone_1030_drive_1200.nam"),
    ("dist", "med",     "1200", "pedals/mxr_duke_of_tone/dot_dist_med_vol_330_tone_1200_drive_1200.nam"),
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter(
            "mode",
            "Mode",
            Some("Pedal"),
            Some("od"),
            &[
                ("od",   "Overdrive"),
                ("dist", "Distortion"),
            ],
        ),
        enum_parameter(
            "voicing",
            "Voicing",
            Some("Pedal"),
            Some("color"),
            &[
                ("high",    "High Gain"),
                ("cranked", "Cranked"),
                ("color",   "Colour"),
                ("low",     "Low Gain"),
                ("med",     "Medium"),
            ],
        ),
        enum_parameter(
            "tone",
            "Tone",
            Some("Pedal"),
            Some("1030"),
            &[
                ("230",  "2:30"),
                ("1030", "10:30"),
                ("1200", "12:00"),
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
    let mode = required_string(params, "mode").map_err(anyhow::Error::msg)?;
    let voicing = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    let tone = required_string(params, "tone").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|(m, v, t, _)| *m == mode && *v == voicing && *t == tone)
        .map(|(_, _, _, path)| *path)
        .ok_or_else(|| {
            anyhow!(
                "gain '{}' has no capture for mode={} voicing={} tone={}",
                MODEL_ID, mode, voicing, tone
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
