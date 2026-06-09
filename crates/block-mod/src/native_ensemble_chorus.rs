use anyhow::{Error, Result};
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{ModelAudioMode, MonoProcessor};
use std::f32::consts::TAU;

pub const MODEL_ID: &str = "ensemble_chorus";
pub const DISPLAY_NAME: &str = "Ensemble Chorus";

const BASE_DELAY_MS: f32 = 20.0;
const MOD_DEPTH_MS: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChorusParams {
    pub rate_hz: f32,
    pub depth: f32,
    pub mix: f32,
}

impl Default for ChorusParams {
    fn default() -> Self {
        Self {
            rate_hz: 0.5,
            depth: 50.0,
            mix: 50.0,
        }
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "modulation".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "rate_hz",
                "Rate",
                None,
                Some(ChorusParams::default().rate_hz),
                0.1,
                5.0,
                0.1,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(ChorusParams::default().depth),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(ChorusParams::default().mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn params_from_set(params: &ParameterSet) -> Result<ChorusParams> {
    Ok(ChorusParams {
        rate_hz: required_f32(params, "rate_hz").map_err(Error::msg)?,
        depth: required_f32(params, "depth").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)?,
    })
}

pub struct EnsembleChorus {
    rate_hz: f32,
    depth: f32,
    mix: f32,
    sample_rate: f32,
    phase: f32,
    buffer: Vec<f32>,
    write_pos: usize,
}

impl EnsembleChorus {
    pub fn new(rate_hz: f32, depth: f32, mix: f32, sample_rate: f32) -> Self {
        let max_delay_samples =
            ((BASE_DELAY_MS + MOD_DEPTH_MS) / 1000.0 * sample_rate) as usize + 2;
        Self {
            rate_hz,
            depth: depth.clamp(0.0, 1.0),
            mix: mix.clamp(0.0, 1.0),
            sample_rate,
            phase: 0.0,
            buffer: vec![0.0; max_delay_samples],
            write_pos: 0,
        }
    }
}

impl MonoProcessor for EnsembleChorus {
    fn process_sample(&mut self, input: f32) -> f32 {
        let buf_len = self.buffer.len();
        self.buffer[self.write_pos] = input;

        let base_delay = BASE_DELAY_MS / 1000.0 * self.sample_rate;
        let mod_depth = MOD_DEPTH_MS / 1000.0 * self.sample_rate * self.depth;

        let mut wet = 0.0_f32;
        for i in 0..3_usize {
            let phase_offset = i as f32 * TAU / 3.0;
            let delay_samples =
                (base_delay + mod_depth * (self.phase + phase_offset).sin()).max(1.0);
            let delay_floor = delay_samples as usize;
            let frac = delay_samples - delay_floor as f32;

            let pos0 = (self.write_pos + buf_len - delay_floor) % buf_len;
            let pos1 = (self.write_pos + buf_len - delay_floor - 1) % buf_len;
            wet += self.buffer[pos0] * (1.0 - frac) + self.buffer[pos1] * frac;
        }
        wet /= 3.0;

        self.phase = (self.phase + TAU * self.rate_hz / self.sample_rate).rem_euclid(TAU);
        self.write_pos = (self.write_pos + 1) % buf_len;

        input * (1.0 - self.mix) + wet * self.mix
    }
}

pub fn build_processor(params: &ParameterSet, sample_rate: f32) -> Result<Box<dyn MonoProcessor>> {
    let p = params_from_set(params)?;
    Ok(Box::new(EnsembleChorus::new(
        p.rate_hz,
        p.depth / 100.0,
        p.mix / 100.0,
        sample_rate,
    )))
}

/// Detuned voices summed into each channel. A Juno-60 / Solina ensemble runs
/// several BBD voices at different LFO phases; the right channel uses the same
/// voices shifted by π so every left voice has an anti-phase twin on the right.
/// Summing distinct voices keeps it lush; the per-voice anti-phase between
/// channels keeps the image wide. Averaging voices to mono and only offsetting
/// L/R (the previous design) collapsed the width to near-mono (issue #681).
const VOICES_PER_CHANNEL: usize = 2;

/// True stereo ensemble over one shared delay line.
struct StereoEnsemble {
    depth: f32,
    mix: f32,
    sample_rate: f32,
    rate_hz: f32,
    base_delay: f32,
    mod_depth_max: f32,
    phase: f32,
    buffer: Vec<f32>,
    write_pos: usize,
}

impl StereoEnsemble {
    fn new(rate_hz: f32, depth: f32, mix: f32, sample_rate: f32) -> Self {
        let max_delay = ((BASE_DELAY_MS + MOD_DEPTH_MS) / 1000.0 * sample_rate) as usize + 4;
        Self {
            depth: depth.clamp(0.0, 1.0),
            mix: mix.clamp(0.0, 1.0),
            sample_rate,
            rate_hz,
            base_delay: BASE_DELAY_MS / 1000.0 * sample_rate,
            mod_depth_max: MOD_DEPTH_MS / 1000.0 * sample_rate,
            phase: 0.0,
            buffer: vec![0.0; max_delay],
            write_pos: 0,
        }
    }

    /// Catmull-Rom Hermite 4-point read of the fractional delay — smoother
    /// than linear under modulation, matching the other pro-tier natives.
    fn read_cubic(&self, delay_samples: f32) -> f32 {
        let len = self.buffer.len();
        let pos = (self.write_pos as f32 - delay_samples).rem_euclid(len as f32);
        let i0 = pos.floor() as usize % len;
        let frac = pos - pos.floor();
        let y_m1 = self.buffer[(i0 + len - 1) % len];
        let y_0 = self.buffer[i0];
        let y_p1 = self.buffer[(i0 + 1) % len];
        let y_p2 = self.buffer[(i0 + 2) % len];
        let c0 = y_0;
        let c1 = 0.5 * (y_p1 - y_m1);
        let c2 = y_m1 - 2.5 * y_0 + 2.0 * y_p1 - 0.5 * y_p2;
        let c3 = 0.5 * (y_p2 - y_m1) + 1.5 * (y_0 - y_p1);
        ((c3 * frac + c2) * frac + c1) * frac + c0
    }
}

impl block_core::StereoProcessor for StereoEnsemble {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        let dry = 0.5 * (input[0] + input[1]);
        self.buffer[self.write_pos] = dry;
        let mod_depth = self.mod_depth_max * self.depth;
        let inv_norm = 1.0 / (VOICES_PER_CHANNEL as f32).sqrt();

        let mut wet_l = 0.0_f32;
        let mut wet_r = 0.0_f32;
        for k in 0..VOICES_PER_CHANNEL {
            // Left voices spread across the LFO cycle; each right voice is the
            // π-shifted twin of its left voice (one comb at its longest while
            // the other is at its shortest) — the source of the wide image.
            let spread = self.phase + k as f32 * std::f32::consts::PI / VOICES_PER_CHANNEL as f32;
            let delay_l = (self.base_delay + mod_depth * spread.sin()).max(1.0);
            let delay_r =
                (self.base_delay + mod_depth * (spread + std::f32::consts::PI).sin()).max(1.0);
            wet_l += self.read_cubic(delay_l);
            wet_r += self.read_cubic(delay_r);
        }
        wet_l *= inv_norm;
        wet_r *= inv_norm;

        self.phase = (self.phase + TAU * self.rate_hz / self.sample_rate).rem_euclid(TAU);
        self.write_pos = (self.write_pos + 1) % self.buffer.len();

        [
            (1.0 - self.mix) * dry + self.mix * wet_l,
            (1.0 - self.mix) * dry + self.mix * wet_r,
        ]
    }
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
        block_core::AudioChannelLayout::Stereo => {
            let p = params_from_set(params)?;
            Ok(block_core::BlockProcessor::Stereo(Box::new(
                StereoEnsemble::new(p.rate_hz, p.depth / 100.0, p.mix / 100.0, sample_rate),
            )))
        }
    }
}

pub const MODEL_DEFINITION: ModModelDefinition = ModModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: ModBackendKind::Native,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};

#[cfg(test)]
#[path = "native_ensemble_chorus_tests.rs"]
mod tests;
