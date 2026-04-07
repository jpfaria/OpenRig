use anyhow::{anyhow, Result};
use crate::registry::PreampModelDefinition;
use crate::PreampBackendKind;
use nam::{
    build_processor_with_assets_for_layout, model_schema_for,
    processor::{plugin_params_from_set_with_defaults, NamPluginParams},
};
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};

pub const MODEL_ID: &str = "nam_panama_shaman";
pub const DISPLAY_NAME: &str = "Shaman";
const BRAND: &str = "panama";

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
enum Voicing { Clean, Crunch, Hg, Od }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GainLevel { G1, G3, G5, G7, G8, G9 }

struct ShamanCapture {
    voicing: Voicing,
    gain: GainLevel,
    nam_file: &'static str,
}

const CAPTURES: &[ShamanCapture] = &[
    ShamanCapture { voicing: Voicing::Clean,  gain: GainLevel::G3, nam_file: "preamp/nam_panama_shaman/clean_g3.nam" },
    ShamanCapture { voicing: Voicing::Clean,  gain: GainLevel::G5, nam_file: "preamp/nam_panama_shaman/clean_g5.nam" },
    ShamanCapture { voicing: Voicing::Clean,  gain: GainLevel::G7, nam_file: "preamp/nam_panama_shaman/clean_g7.nam" },
    ShamanCapture { voicing: Voicing::Clean,  gain: GainLevel::G9, nam_file: "preamp/nam_panama_shaman/clean_g9.nam" },
    ShamanCapture { voicing: Voicing::Crunch, gain: GainLevel::G1, nam_file: "preamp/nam_panama_shaman/crunch_g1.nam" },
    ShamanCapture { voicing: Voicing::Crunch, gain: GainLevel::G3, nam_file: "preamp/nam_panama_shaman/crunch_g3.nam" },
    ShamanCapture { voicing: Voicing::Crunch, gain: GainLevel::G5, nam_file: "preamp/nam_panama_shaman/crunch_g5.nam" },
    ShamanCapture { voicing: Voicing::Crunch, gain: GainLevel::G7, nam_file: "preamp/nam_panama_shaman/crunch_g7.nam" },
    ShamanCapture { voicing: Voicing::Crunch, gain: GainLevel::G8, nam_file: "preamp/nam_panama_shaman/crunch_g8.nam" },
    ShamanCapture { voicing: Voicing::Hg,     gain: GainLevel::G3, nam_file: "preamp/nam_panama_shaman/hg_g3.nam" },
    ShamanCapture { voicing: Voicing::Hg,     gain: GainLevel::G7, nam_file: "preamp/nam_panama_shaman/hg_g7.nam" },
    ShamanCapture { voicing: Voicing::Od,     gain: GainLevel::G1, nam_file: "preamp/nam_panama_shaman/od_g1.nam" },
    ShamanCapture { voicing: Voicing::Od,     gain: GainLevel::G3, nam_file: "preamp/nam_panama_shaman/od_g3.nam" },
    ShamanCapture { voicing: Voicing::Od,     gain: GainLevel::G5, nam_file: "preamp/nam_panama_shaman/od_g5.nam" },
    ShamanCapture { voicing: Voicing::Od,     gain: GainLevel::G7, nam_file: "preamp/nam_panama_shaman/od_g7.nam" },
];

fn parse_voicing(v: &str) -> Result<Voicing> {
    match v {
        "clean"  => Ok(Voicing::Clean),
        "crunch" => Ok(Voicing::Crunch),
        "hg"     => Ok(Voicing::Hg),
        "od"     => Ok(Voicing::Od),
        _ => Err(anyhow!("invalid voicing '{}' for '{}'", v, MODEL_ID)),
    }
}

fn parse_gain(g: &str) -> Result<GainLevel> {
    match g {
        "g1" => Ok(GainLevel::G1),
        "g3" => Ok(GainLevel::G3),
        "g5" => Ok(GainLevel::G5),
        "g7" => Ok(GainLevel::G7),
        "g8" => Ok(GainLevel::G8),
        "g9" => Ok(GainLevel::G9),
        _ => Err(anyhow!("invalid gain '{}' for '{}'", g, MODEL_ID)),
    }
}

fn gain_priority(g: GainLevel) -> i32 {
    match g {
        GainLevel::G1 => 1,
        GainLevel::G3 => 3,
        GainLevel::G5 => 5,
        GainLevel::G7 => 7,
        GainLevel::G8 => 8,
        GainLevel::G9 => 9,
    }
}

pub fn model_schema() -> ModelParameterSchema {
    let mut schema = model_schema_for(block_core::EFFECT_TYPE_PREAMP, MODEL_ID, DISPLAY_NAME, false);
    schema.parameters = vec![
        enum_parameter("voicing", "Voicing", Some("Amp"), Some("clean"),
            &[("clean", "Clean"), ("crunch", "Crunch"), ("hg", "High Gain"), ("od", "OD")]),
        enum_parameter("gain", "Gain", Some("Amp"), Some("g5"),
            &[("g1", "G1"), ("g3", "G3"), ("g5", "G5"), ("g7", "G7"), ("g8", "G8"), ("g9", "G9")]),
    ];
    schema
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let nam_file = resolve_capture(params)?;
    let plugin_params = plugin_params_from_set_with_defaults(params, NAM_PLUGIN_DEFAULTS)?;
    let model_path = nam::resolve_nam_capture(nam_file)?;
    build_processor_with_assets_for_layout(&model_path, None, plugin_params, sample_rate, layout)
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static str> {
    let voicing = parse_voicing(&required_string(params, "voicing").map_err(anyhow::Error::msg)?)?;
    let gain = parse_gain(&required_string(params, "gain").map_err(anyhow::Error::msg)?)?;

    // Exact match first
    if let Some(c) = CAPTURES.iter().find(|c| c.voicing == voicing && c.gain == gain) {
        return Ok(c.nam_file);
    }

    // Nearest gain within same voicing
    CAPTURES.iter()
        .filter(|c| c.voicing == voicing)
        .min_by_key(|c| (gain_priority(c.gain) - gain_priority(gain)).abs())
        .map(|c| c.nam_file)
        .ok_or_else(|| anyhow!("no captures for voicing in model '{}'", MODEL_ID))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
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
    let file = resolve_capture(params)?;
    Ok(format!("asset_id='{}'", file))
}
