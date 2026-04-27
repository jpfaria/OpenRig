use anyhow::{anyhow, Result};
use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{NamPluginParams, DEFAULT_PLUGIN_PARAMS},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_boss_od_3";
pub const DISPLAY_NAME: &str = "Boss OD-3";
const BRAND: &str = "boss";

pub const NAM_PLUGIN_FIXED_PARAMS: NamPluginParams = DEFAULT_PLUGIN_PARAMS;

struct NamCapture {
    tone: &'static str,
    model_path: &'static str,
}

const CAPTURES: &[NamCapture] = &[
    NamCapture { tone: "hidrive_centertone", model_path: "pedals/boss_od_3/bossod3_hidrive_centertone.nam" },
    NamCapture { tone: "hidrive_centertone_nano", model_path: "pedals/boss_od_3/bossod3_hidrive_centertone_nano.nam" },
    NamCapture { tone: "hidrive_hightone", model_path: "pedals/boss_od_3/bossod3_hidrive_hightone.nam" },
    NamCapture { tone: "hidrive_hightone_nano", model_path: "pedals/boss_od_3/bossod3_hidrive_hightone_nano.nam" },
    NamCapture { tone: "hidrive_lowtone", model_path: "pedals/boss_od_3/bossod3_hidrive_lowtone.nam" },
    NamCapture { tone: "hidrive_lowtone_nano", model_path: "pedals/boss_od_3/bossod3_hidrive_lowtone_nano.nam" },
    NamCapture { tone: "lowdrive_centertone", model_path: "pedals/boss_od_3/bossod3_lowdrive_centertone.nam" },
    NamCapture { tone: "lowdrive_centertone_nano", model_path: "pedals/boss_od_3/bossod3_lowdrive_centertone_nano.nam" },
    NamCapture { tone: "lowdrive_hightone", model_path: "pedals/boss_od_3/bossod3_lowdrive_hightone.nam" },
    NamCapture { tone: "lowdrive_hightone_nano", model_path: "pedals/boss_od_3/bossod3_lowdrive_hightone_nano.nam" },
    NamCapture { tone: "lowdrive_lowtone", model_path: "pedals/boss_od_3/bossod3_lowdrive_lowtone.nam" },
    NamCapture { tone: "lowdrive_lowtone_nano", model_path: "pedals/boss_od_3/bossod3_lowdrive_lowtone_nano.nam" },
    NamCapture { tone: "middrive_centertone", model_path: "pedals/boss_od_3/bossod3_middrive_centertone.nam" },
    NamCapture { tone: "middrive_centertone_nano", model_path: "pedals/boss_od_3/bossod3_middrive_centertone_nano.nam" },
    NamCapture { tone: "middrive_hightone", model_path: "pedals/boss_od_3/bossod3_middrive_hightone.nam" },
    NamCapture { tone: "middrive_hightone_nano", model_path: "pedals/boss_od_3/bossod3_middrive_hightone_nano.nam" },
    NamCapture { tone: "middrive_lowtone", model_path: "pedals/boss_od_3/bossod3_middrive_lowtone.nam" },
    NamCapture { tone: "middrive_lowtone_nano", model_path: "pedals/boss_od_3/bossod3_middrive_lowtone_nano.nam" },
    NamCapture { tone: "nodrive_centertone", model_path: "pedals/boss_od_3/bossod3_nodrive_centertone.nam" },
    NamCapture { tone: "nodrive_centertone_nano", model_path: "pedals/boss_od_3/bossod3_nodrive_centertone_nano.nam" },
    NamCapture { tone: "nodrive_hightone", model_path: "pedals/boss_od_3/bossod3_nodrive_hightone.nam" },
    NamCapture { tone: "nodrive_hightone_nano", model_path: "pedals/boss_od_3/bossod3_nodrive_hightone_nano.nam" },
    NamCapture { tone: "nodrive_lowtone", model_path: "pedals/boss_od_3/bossod3_nodrive_lowtone.nam" },
    NamCapture { tone: "nodrive_lowtone_nano", model_path: "pedals/boss_od_3/bossod3_nodrive_lowtone_nano.nam" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_GAIN, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![enum_parameter(
        "tone",
        "Tone",
        Some("Pedal"),
        Some("hidrive_centertone"),
        &[
            ("hidrive_centertone", "Hidrive Centertone"),
            ("hidrive_centertone_nano", "Hidrive Centertone Nano"),
            ("hidrive_hightone", "Hidrive Hightone"),
            ("hidrive_hightone_nano", "Hidrive Hightone Nano"),
            ("hidrive_lowtone", "Hidrive Lowtone"),
            ("hidrive_lowtone_nano", "Hidrive Lowtone Nano"),
            ("lowdrive_centertone", "Lowdrive Centertone"),
            ("lowdrive_centertone_nano", "Lowdrive Centertone Nano"),
            ("lowdrive_hightone", "Lowdrive Hightone"),
            ("lowdrive_hightone_nano", "Lowdrive Hightone Nano"),
            ("lowdrive_lowtone", "Lowdrive Lowtone"),
            ("lowdrive_lowtone_nano", "Lowdrive Lowtone Nano"),
            ("middrive_centertone", "Middrive Centertone"),
            ("middrive_centertone_nano", "Middrive Centertone Nano"),
            ("middrive_hightone", "Middrive Hightone"),
            ("middrive_hightone_nano", "Middrive Hightone Nano"),
            ("middrive_lowtone", "Middrive Lowtone"),
            ("middrive_lowtone_nano", "Middrive Lowtone Nano"),
            ("nodrive_centertone", "Nodrive Centertone"),
            ("nodrive_centertone_nano", "Nodrive Centertone Nano"),
            ("nodrive_hightone", "Nodrive Hightone"),
            ("nodrive_hightone_nano", "Nodrive Hightone Nano"),
            ("nodrive_lowtone", "Nodrive Lowtone"),
            ("nodrive_lowtone_nano", "Nodrive Lowtone Nano"),
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
