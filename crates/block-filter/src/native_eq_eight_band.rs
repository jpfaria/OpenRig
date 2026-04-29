use anyhow::{Error, Result};
use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use block_core::param::{
    bool_parameter, curve_editor_parameter, enum_parameter, float_parameter, required_bool,
    required_f32, required_string, CurveEditorRole, ModelParameterSchema, ParameterSet,
    ParameterUnit,
};
use block_core::{db_to_lin, BiquadFilter, BiquadKind, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "eq_eight_band_parametric";
pub const DISPLAY_NAME: &str = "8-Band Parametric EQ";

const BAND_TYPES: &[(&str, &str)] = &[
    ("peak", "Peak"),
    ("low_shelf", "Low Shelf"),
    ("high_shelf", "High Shelf"),
    ("low_pass", "Low Pass"),
    ("high_pass", "High Pass"),
    ("notch", "Notch"),
];

struct BandDefault {
    freq: f32,
    gain: f32,
    q: f32,
    band_type: &'static str,
}

const BAND_DEFAULTS: [BandDefault; 8] = [
    BandDefault { freq: 62.0, gain: 0.0, q: 1.0, band_type: "peak" },
    BandDefault { freq: 125.0, gain: 0.0, q: 1.0, band_type: "peak" },
    BandDefault { freq: 250.0, gain: 0.0, q: 1.0, band_type: "peak" },
    BandDefault { freq: 500.0, gain: 0.0, q: 1.0, band_type: "peak" },
    BandDefault { freq: 1000.0, gain: 0.0, q: 1.0, band_type: "peak" },
    BandDefault { freq: 2000.0, gain: 0.0, q: 1.0, band_type: "peak" },
    BandDefault { freq: 4000.0, gain: 0.0, q: 1.0, band_type: "peak" },
    BandDefault { freq: 8000.0, gain: 0.0, q: 1.0, band_type: "peak" },
];

pub fn model_schema() -> ModelParameterSchema {
    let mut parameters = Vec::new();

    for (i, defaults) in BAND_DEFAULTS.iter().enumerate() {
        let n = i + 1;
        let group = format!("Band {n}");

        parameters.push(bool_parameter(
            &format!("band{n}_enabled"),
            "Enabled",
            Some(&group),
            Some(true),
        ));
        parameters.push(enum_parameter(
            &format!("band{n}_type"),
            "Type",
            Some(&group),
            Some(defaults.band_type),
            BAND_TYPES,
        ));
        parameters.push(curve_editor_parameter(
            &format!("band{n}_freq"),
            "Freq",
            Some(&group),
            CurveEditorRole::X,
            Some(defaults.freq),
            20.0,
            20000.0,
            1.0,
            ParameterUnit::Hertz,
        ));
        parameters.push(curve_editor_parameter(
            &format!("band{n}_gain"),
            "Gain",
            Some(&group),
            CurveEditorRole::Y,
            Some(defaults.gain),
            -24.0,
            24.0,
            0.1,
            ParameterUnit::Decibels,
        ));
        parameters.push(curve_editor_parameter(
            &format!("band{n}_q"),
            "Q",
            Some(&group),
            CurveEditorRole::Width,
            Some(defaults.q),
            0.1,
            10.0,
            0.01,
            ParameterUnit::None,
        ));
    }

    // Output trim — applied AFTER the cascaded biquads, before returning
    // the sample. Default 0 dB so existing projects (which don't carry
    // this parameter in YAML) get unity through `normalized_against`.
    // Range -24..+12 dB lets the user pull back when their boost curve
    // (smile, presence push, etc.) is hot enough to engage the chain
    // limiter and saturate audibly. Without this knob the only way to
    // compensate was to retune every band — which is what the user hit.
    parameters.push(float_parameter(
        "output_db",
        "Output",
        Some("Output"),
        Some(0.0),
        -24.0,
        12.0,
        0.1,
        ParameterUnit::Decibels,
    ));

    ModelParameterSchema {
        effect_type: "filter".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters,
    }
}

pub struct EightBandParametricEq {
    filters: Vec<BiquadFilter>,
    enabled: [bool; 8],
    /// Linear gain applied AFTER all bands. Pre-computed from `output_db`
    /// at build time, so the audio thread does no `pow` per sample.
    output_gain: f32,
}

impl MonoProcessor for EightBandParametricEq {
    #[inline]
    fn process_sample(&mut self, input: f32) -> f32 {
        let mut x = input;
        for (filter, &en) in self.filters.iter_mut().zip(self.enabled.iter()) {
            if en {
                x = filter.process(x);
            }
        }
        x * self.output_gain
    }
}

fn parse_band_kind(band_type: &str) -> Result<BiquadKind> {
    match band_type {
        "peak" => Ok(BiquadKind::Peak),
        "low_shelf" => Ok(BiquadKind::LowShelf),
        "high_shelf" => Ok(BiquadKind::HighShelf),
        "low_pass" => Ok(BiquadKind::LowPass),
        "high_pass" => Ok(BiquadKind::HighPass),
        "notch" => Ok(BiquadKind::Notch),
        other => anyhow::bail!("unknown band type '{}'", other),
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let mut filters = Vec::with_capacity(8);
    let mut enabled = [true; 8];

    for i in 0..8usize {
        let n = i + 1;
        let en = required_bool(params, &format!("band{n}_enabled")).map_err(Error::msg)?;
        let band_type = required_string(params, &format!("band{n}_type")).map_err(Error::msg)?;
        let freq = required_f32(params, &format!("band{n}_freq")).map_err(Error::msg)?;
        let gain = required_f32(params, &format!("band{n}_gain")).map_err(Error::msg)?;
        let q = required_f32(params, &format!("band{n}_q")).map_err(Error::msg)?;

        let kind = parse_band_kind(&band_type)?;
        filters.push(BiquadFilter::new(kind, freq, gain, q, sample_rate));
        enabled[i] = en;
    }

    let output_db = required_f32(params, "output_db").map_err(Error::msg)?;
    let output_gain = db_to_lin(output_db);

    Ok(Box::new(EightBandParametricEq {
        filters,
        enabled,
        output_gain,
    }))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: block_core::AudioChannelLayout,
) -> Result<block_core::BlockProcessor> {
    match layout {
        block_core::AudioChannelLayout::Mono => {
            Ok(block_core::BlockProcessor::Mono(build_processor(params, sample_rate)?))
        }
        block_core::AudioChannelLayout::Stereo => anyhow::bail!(
            "filter model '{}' is mono-only and cannot build native stereo processing",
            MODEL_ID
        ),
    }
}

pub const MODEL_DEFINITION: FilterModelDefinition = FilterModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: FilterBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
