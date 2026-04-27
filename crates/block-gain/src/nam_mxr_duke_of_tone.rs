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

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "dist_color_vol_330_tone_1030_drive_700",   model_path: "pedals/mxr_duke_of_tone/dot_dist_color_vol_330_tone_1030_drive_700.nam" },
    NamCapture { tone: "dist_color_vol_330_tone_1200_drive_700",   model_path: "pedals/mxr_duke_of_tone/dot_dist_color_vol_330_tone_1200_drive_700.nam" },
    NamCapture { tone: "dist_color_vol_330_tone_230_drive_700",    model_path: "pedals/mxr_duke_of_tone/dot_dist_color_vol_330_tone_230_drive_700.nam" },
    NamCapture { tone: "dist_cranked_vol_330_tone_1030_drive_500", model_path: "pedals/mxr_duke_of_tone/dot_dist_cranked_vol_330_tone_1030_drive_500.nam" },
    NamCapture { tone: "dist_cranked_vol_330_tone_1200_drive_500", model_path: "pedals/mxr_duke_of_tone/dot_dist_cranked_vol_330_tone_1200_drive_500.nam" },
    NamCapture { tone: "dist_cranked_vol_330_tone_230_drive_500",  model_path: "pedals/mxr_duke_of_tone/dot_dist_cranked_vol_330_tone_230_drive_500.nam" },
    NamCapture { tone: "dist_high_vol_330_tone_1030_drive_300",    model_path: "pedals/mxr_duke_of_tone/dot_dist_high_vol_330_tone_1030_drive_300.nam" },
    NamCapture { tone: "dist_high_vol_330_tone_1200_drive_300",    model_path: "pedals/mxr_duke_of_tone/dot_dist_high_vol_330_tone_1200_drive_300.nam" },
    NamCapture { tone: "dist_high_vol_330_tone_230_drive_300",     model_path: "pedals/mxr_duke_of_tone/dot_dist_high_vol_330_tone_230_drive_300.nam" },
    NamCapture { tone: "dist_low_vol_330_tone_1030_drive_900",     model_path: "pedals/mxr_duke_of_tone/dot_dist_low_vol_330_tone_1030_drive_900.nam" },
    NamCapture { tone: "dist_low_vol_330_tone_1200_drive_900",     model_path: "pedals/mxr_duke_of_tone/dot_dist_low_vol_330_tone_1200_drive_900.nam" },
    NamCapture { tone: "dist_low_vol_330_tone_230_drive_900",      model_path: "pedals/mxr_duke_of_tone/dot_dist_low_vol_330_tone_230_drive_900.nam" },
    NamCapture { tone: "dist_med_vol_330_tone_1030_drive_1200",    model_path: "pedals/mxr_duke_of_tone/dot_dist_med_vol_330_tone_1030_drive_1200.nam" },
    NamCapture { tone: "dist_med_vol_330_tone_1200_drive_1200",    model_path: "pedals/mxr_duke_of_tone/dot_dist_med_vol_330_tone_1200_drive_1200.nam" },
    NamCapture { tone: "dist_med_vol_330_tone_230_drive_1200",     model_path: "pedals/mxr_duke_of_tone/dot_dist_med_vol_330_tone_230_drive_1200.nam" },
    NamCapture { tone: "od_color_vol_330_tone_1030_drive_700",     model_path: "pedals/mxr_duke_of_tone/dot_od_color_vol_330_tone_1030_drive_700.nam" },
    NamCapture { tone: "od_color_vol_330_tone_1200_drive_700",     model_path: "pedals/mxr_duke_of_tone/dot_od_color_vol_330_tone_1200_drive_700.nam" },
    NamCapture { tone: "od_color_vol_330_tone_230_drive_700",      model_path: "pedals/mxr_duke_of_tone/dot_od_color_vol_330_tone_230_drive_700.nam" },
    NamCapture { tone: "od_cranked_vol_330_tone_1030_drive_500",   model_path: "pedals/mxr_duke_of_tone/dot_od_cranked_vol_330_tone_1030_drive_500.nam" },
    NamCapture { tone: "od_cranked_vol_330_tone_1200_drive_500",   model_path: "pedals/mxr_duke_of_tone/dot_od_cranked_vol_330_tone_1200_drive_500.nam" },
    NamCapture { tone: "od_cranked_vol_330_tone_230_drive_500",    model_path: "pedals/mxr_duke_of_tone/dot_od_cranked_vol_330_tone_230_drive_500.nam" },
    NamCapture { tone: "od_high_vol_330_tone_1030_drive_300",      model_path: "pedals/mxr_duke_of_tone/dot_od_high_vol_330_tone_1030_drive_300.nam" },
    NamCapture { tone: "od_high_vol_330_tone_1200_drive_300",      model_path: "pedals/mxr_duke_of_tone/dot_od_high_vol_330_tone_1200_drive_300.nam" },
    NamCapture { tone: "od_high_vol_330_tone_230_drive_300",       model_path: "pedals/mxr_duke_of_tone/dot_od_high_vol_330_tone_230_drive_300.nam" },
    NamCapture { tone: "od_low_vol_330_tone_1030_drive_900",       model_path: "pedals/mxr_duke_of_tone/dot_od_low_vol_330_tone_1030_drive_900.nam" },
    NamCapture { tone: "od_low_vol_330_tone_1200_drive_900",       model_path: "pedals/mxr_duke_of_tone/dot_od_low_vol_330_tone_1200_drive_900.nam" },
    NamCapture { tone: "od_low_vol_330_tone_230_drive_900",        model_path: "pedals/mxr_duke_of_tone/dot_od_low_vol_330_tone_230_drive_900.nam" },
    NamCapture { tone: "od_med_vol_330_tone_1030_drive_1200",      model_path: "pedals/mxr_duke_of_tone/dot_od_med_vol_330_tone_1030_drive_1200.nam" },
    NamCapture { tone: "od_med_vol_330_tone_1200_drive_1200",      model_path: "pedals/mxr_duke_of_tone/dot_od_med_vol_330_tone_1200_drive_1200.nam" },
    NamCapture { tone: "od_med_vol_330_tone_230_drive_1200",       model_path: "pedals/mxr_duke_of_tone/dot_od_med_vol_330_tone_230_drive_1200.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("dist_color_vol_330_tone_1030_drive_700"),
        &[
            ("dist_color_vol_330_tone_1030_drive_700",   "Dist Color Vol 330 Tone 1030 Drive 700"),
            ("dist_color_vol_330_tone_1200_drive_700",   "Dist Color Vol 330 Tone 1200 Drive 700"),
            ("dist_color_vol_330_tone_230_drive_700",    "Dist Color Vol 330 Tone 230 Drive 700"),
            ("dist_cranked_vol_330_tone_1030_drive_500", "Dist Cranked Vol 330 Tone 1030 Drive 500"),
            ("dist_cranked_vol_330_tone_1200_drive_500", "Dist Cranked Vol 330 Tone 1200 Drive 500"),
            ("dist_cranked_vol_330_tone_230_drive_500",  "Dist Cranked Vol 330 Tone 230 Drive 500"),
            ("dist_high_vol_330_tone_1030_drive_300",    "Dist High Vol 330 Tone 1030 Drive 300"),
            ("dist_high_vol_330_tone_1200_drive_300",    "Dist High Vol 330 Tone 1200 Drive 300"),
            ("dist_high_vol_330_tone_230_drive_300",     "Dist High Vol 330 Tone 230 Drive 300"),
            ("dist_low_vol_330_tone_1030_drive_900",     "Dist Low Vol 330 Tone 1030 Drive 900"),
            ("dist_low_vol_330_tone_1200_drive_900",     "Dist Low Vol 330 Tone 1200 Drive 900"),
            ("dist_low_vol_330_tone_230_drive_900",      "Dist Low Vol 330 Tone 230 Drive 900"),
            ("dist_med_vol_330_tone_1030_drive_1200",    "Dist Med Vol 330 Tone 1030 Drive 1200"),
            ("dist_med_vol_330_tone_1200_drive_1200",    "Dist Med Vol 330 Tone 1200 Drive 1200"),
            ("dist_med_vol_330_tone_230_drive_1200",     "Dist Med Vol 330 Tone 230 Drive 1200"),
            ("od_color_vol_330_tone_1030_drive_700",     "Od Color Vol 330 Tone 1030 Drive 700"),
            ("od_color_vol_330_tone_1200_drive_700",     "Od Color Vol 330 Tone 1200 Drive 700"),
            ("od_color_vol_330_tone_230_drive_700",      "Od Color Vol 330 Tone 230 Drive 700"),
            ("od_cranked_vol_330_tone_1030_drive_500",   "Od Cranked Vol 330 Tone 1030 Drive 500"),
            ("od_cranked_vol_330_tone_1200_drive_500",   "Od Cranked Vol 330 Tone 1200 Drive 500"),
            ("od_cranked_vol_330_tone_230_drive_500",    "Od Cranked Vol 330 Tone 230 Drive 500"),
            ("od_high_vol_330_tone_1030_drive_300",      "Od High Vol 330 Tone 1030 Drive 300"),
            ("od_high_vol_330_tone_1200_drive_300",      "Od High Vol 330 Tone 1200 Drive 300"),
            ("od_high_vol_330_tone_230_drive_300",       "Od High Vol 330 Tone 230 Drive 300"),
            ("od_low_vol_330_tone_1030_drive_900",       "Od Low Vol 330 Tone 1030 Drive 900"),
            ("od_low_vol_330_tone_1200_drive_900",       "Od Low Vol 330 Tone 1200 Drive 900"),
            ("od_low_vol_330_tone_230_drive_900",        "Od Low Vol 330 Tone 230 Drive 900"),
            ("od_med_vol_330_tone_1030_drive_1200",      "Od Med Vol 330 Tone 1030 Drive 1200"),
            ("od_med_vol_330_tone_1200_drive_1200",      "Od Med Vol 330 Tone 1200 Drive 1200"),
            ("od_med_vol_330_tone_230_drive_1200",       "Od Med Vol 330 Tone 230 Drive 1200"),
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
