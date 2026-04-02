//! Core building blocks shared by OpenRig block families.
pub mod param;

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioChannelLayout {
    Mono,
    Stereo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelAudioMode {
    MonoOnly,
    DualMono,
    TrueStereo,
    MonoToStereo,
}

impl ModelAudioMode {
    pub const fn accepts_input(self, layout: AudioChannelLayout) -> bool {
        matches!(
            (self, layout),
            (Self::MonoOnly, AudioChannelLayout::Mono)
                | (Self::DualMono, AudioChannelLayout::Mono)
                | (Self::DualMono, AudioChannelLayout::Stereo)
                | (Self::TrueStereo, AudioChannelLayout::Stereo)
                | (Self::MonoToStereo, AudioChannelLayout::Mono)
                | (Self::MonoToStereo, AudioChannelLayout::Stereo)
        )
    }

    pub const fn output_layout(self, input: AudioChannelLayout) -> Option<AudioChannelLayout> {
        match self {
            Self::MonoOnly => Some(match input {
                AudioChannelLayout::Mono => AudioChannelLayout::Mono,
                AudioChannelLayout::Stereo => AudioChannelLayout::Stereo,
            }),
            Self::DualMono => Some(input),
            Self::TrueStereo => match input {
                AudioChannelLayout::Stereo => Some(AudioChannelLayout::Stereo),
                AudioChannelLayout::Mono => None,
            },
            Self::MonoToStereo => Some(AudioChannelLayout::Stereo),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MonoOnly => "mono_only",
            Self::DualMono => "dual_mono",
            Self::TrueStereo => "true_stereo",
            Self::MonoToStereo => "mono_to_stereo",
        }
    }
}

// Instrument type constants
pub const INST_ELECTRIC_GUITAR: &str = "electric_guitar";
pub const INST_ACOUSTIC_GUITAR: &str = "acoustic_guitar";
pub const INST_BASS: &str = "bass";
pub const INST_VOICE: &str = "voice";
pub const INST_KEYS: &str = "keys";
pub const INST_DRUMS: &str = "drums";
pub const INST_GENERIC: &str = "generic";

// Brand constants
pub const BRAND_NATIVE: &str = "native";

// Effect type constants
pub const EFFECT_TYPE_PREAMP: &str = "preamp";
pub const EFFECT_TYPE_AMP: &str = "amp";
pub const EFFECT_TYPE_FULL_RIG: &str = "full_rig";
pub const EFFECT_TYPE_CAB: &str = "cab";
pub const EFFECT_TYPE_IR: &str = "ir";
pub const EFFECT_TYPE_GAIN: &str = "gain";
pub const EFFECT_TYPE_NAM: &str = "nam";
pub const EFFECT_TYPE_DELAY: &str = "delay";
pub const EFFECT_TYPE_REVERB: &str = "reverb";
pub const EFFECT_TYPE_UTILITY: &str = "utility";
pub const EFFECT_TYPE_DYNAMICS: &str = "dynamics";
pub const EFFECT_TYPE_FILTER: &str = "filter";
pub const EFFECT_TYPE_WAH: &str = "wah";
pub const EFFECT_TYPE_PITCH: &str = "pitch";
pub const EFFECT_TYPE_MODULATION: &str = "modulation";
pub const EFFECT_TYPE_BODY: &str = "body";

// Default instrument (used as fallback)
pub const DEFAULT_INSTRUMENT: &str = INST_ELECTRIC_GUITAR;

/// All non-generic instruments
pub const ALL_INSTRUMENTS: &[&str] = &[
    INST_ELECTRIC_GUITAR, INST_ACOUSTIC_GUITAR, INST_BASS,
    INST_VOICE, INST_KEYS, INST_DRUMS,
];

/// Guitar and bass only (for amps, cabs, gain, wah, etc.)
pub const GUITAR_BASS: &[&str] = &[INST_ELECTRIC_GUITAR, INST_BASS];

/// Guitar, acoustic guitar and bass (for preamps)
pub const GUITAR_ACOUSTIC_BASS: &[&str] = &[INST_ELECTRIC_GUITAR, INST_ACOUSTIC_GUITAR, INST_BASS];

/// Describes the position and range of a single knob overlay on the panel SVG.
#[derive(Debug, Clone, Copy)]
pub struct KnobLayoutEntry {
    pub param_key: &'static str,
    pub svg_cx: f32,
    pub svg_cy: f32,
    pub svg_r: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

/// Visual metadata for a model, used by the GUI catalog layer.
#[derive(Debug, Clone, Copy)]
pub struct ModelVisualData {
    pub brand: &'static str,
    pub type_label: &'static str,
    pub supported_instruments: &'static [&'static str],
    pub knob_layout: &'static [KnobLayoutEntry],
}

pub trait MonoProcessor: Send + Sync + 'static {
    fn process_sample(&mut self, input: f32) -> f32;
    fn process_block(&mut self, buffer: &mut [f32]) {
        for sample in buffer {
            *sample = self.process_sample(*sample);
        }
    }
}

pub trait StereoProcessor: Send + Sync + 'static {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2];
    fn process_block(&mut self, buffer: &mut [[f32; 2]]) {
        for frame in buffer {
            *frame = self.process_frame(*frame);
        }
    }
}

pub enum BlockProcessor {
    Mono(Box<dyn MonoProcessor>),
    Stereo(Box<dyn StereoProcessor>),
}

pub trait NamedModel {
    fn model_key(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
}
/// Capitalize the first character of a string, leaving the rest unchanged.
pub fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut result = String::with_capacity(s.len());
            for c in first.to_uppercase() {
                result.push(c);
            }
            result.push_str(chars.as_str());
            result
        }
    }
}

pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}
pub fn lin_to_db(lin: f32) -> f32 {
    if lin > 1e-10 {
        20.0 * lin.log10()
    } else {
        -200.0
    }
}
pub fn calculate_coefficient(time_ms: f32, sample_rate: f32) -> f32 {
    (-1.0 / (sample_rate * 0.001 * time_ms.max(0.001))).exp()
}
pub struct EnvelopeFollower {
    envelope: f32,
    attack_coeff: f32,
    release_coeff: f32,
}
impl EnvelopeFollower {
    pub fn from_ms(attack_ms: f32, release_ms: f32, sample_rate: f32) -> Self {
        Self {
            envelope: 0.0,
            attack_coeff: calculate_coefficient(attack_ms, sample_rate),
            release_coeff: calculate_coefficient(release_ms, sample_rate),
        }
    }
    pub fn set_attack_coeff(&mut self, coeff: f32) {
        self.attack_coeff = coeff;
    }
    pub fn set_release_coeff(&mut self, coeff: f32) {
        self.release_coeff = coeff;
    }
    pub fn value(&self) -> f32 {
        self.envelope
    }
    pub fn process(&mut self, input: f32) -> f32 {
        let abs_input = input.abs();
        if abs_input > self.envelope {
            self.envelope = self
                .attack_coeff
                .mul_add(self.envelope, (1.0 - self.attack_coeff) * abs_input);
        } else {
            self.envelope = self
                .release_coeff
                .mul_add(self.envelope, (1.0 - self.release_coeff) * abs_input);
        }
        self.envelope
    }
}
pub struct OnePoleLowPass {
    state: f32,
    coeff: f32,
}
impl OnePoleLowPass {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let coeff = 1.0 - (-2.0 * PI * cutoff_hz.max(1.0) / sample_rate).exp();
        Self { state: 0.0, coeff }
    }
    pub fn process(&mut self, input: f32) -> f32 {
        self.state = self.coeff.mul_add(input - self.state, self.state);
        self.state
    }
}
pub struct OnePoleHighPass {
    prev_input: f32,
    prev_output: f32,
    coeff: f32,
}
impl OnePoleHighPass {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let rc = 1.0 / (2.0 * PI * cutoff_hz.max(1.0));
        let dt = 1.0 / sample_rate;
        let coeff = rc / (rc + dt);
        Self {
            prev_input: 0.0,
            prev_output: 0.0,
            coeff,
        }
    }
    pub fn process(&mut self, input: f32) -> f32 {
        let output = self.coeff * (self.prev_output + input - self.prev_input);
        self.prev_input = input;
        self.prev_output = output;
        output
    }
}
