//! Core building blocks shared by OpenRig block families.
pub mod param;

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use std::sync::Arc;

/// A single key-value entry in a real-time data stream.
/// Any block can publish stream entries for the GUI to display.
#[derive(Debug, Clone)]
pub struct StreamEntry {
    pub key: String,
    pub value: f32,
    pub text: String,
    /// Peak hold level (0.0–1.0). Used by spectrum-type streams; 0.0 for others.
    pub peak: f32,
}

/// Shared handle for publishing stream data from a processor to the GUI.
///
/// Wait-free on both sides: the producer (block worker thread) does
/// `stream.store(Arc::new(new_entries))` to publish a snapshot, and the
/// GUI does `stream.load()` to read the latest snapshot atomically. No
/// `Mutex`, no contention, no priority inversion. The producer's
/// `Arc::new(...)` allocation is acceptable because it runs on a worker
/// thread (e.g. `tuner-detection`, `spectrum-analyzer`) that the RT
/// audio callback only feeds via a bounded channel — never on the RT
/// callback path itself.
pub type StreamHandle = Arc<ArcSwap<Vec<StreamEntry>>>;

/// Construct a fresh, empty `StreamHandle`. Use this in block builders
/// instead of `Arc::new(Mutex::new(Vec::new()))`.
pub fn new_stream_handle() -> StreamHandle {
    Arc::new(ArcSwap::from_pointee(Vec::new()))
}

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
            Self::MonoOnly => match input {
                AudioChannelLayout::Mono => Some(AudioChannelLayout::Mono),
                AudioChannelLayout::Stereo => None,
            },
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
pub const EFFECT_TYPE_VST3: &str = "vst3";

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

/// Opaque handle to an open plugin editor window.
///
/// Dropping the handle closes the window and releases all resources.
/// The concrete type is an implementation detail of the plugin host crate.
pub trait PluginEditorHandle: Send {}
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

/// Second-order IIR (biquad) filter supporting peaking EQ, low shelf and high shelf.
pub struct BiquadFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

pub enum BiquadKind {
    Peak,
    LowShelf,
    HighShelf,
    HighPass,
    LowPass,
    Notch,
}

impl BiquadFilter {
    pub fn new(kind: BiquadKind, freq_hz: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();

        let (b0, b1, b2, a0, a1, a2) = match kind {
            BiquadKind::Peak => {
                let a = 10.0_f32.powf(gain_db / 40.0);
                let alpha = sin_w0 / (2.0 * q.max(0.01));
                (
                    1.0 + alpha * a,
                    -2.0 * cos_w0,
                    1.0 - alpha * a,
                    1.0 + alpha / a,
                    -2.0 * cos_w0,
                    1.0 - alpha / a,
                )
            }
            BiquadKind::LowShelf => {
                let a = 10.0_f32.powf(gain_db / 40.0);
                let alpha = sin_w0 / (2.0 * q.max(0.01));
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                    (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                    (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                )
            }
            BiquadKind::HighShelf => {
                let a = 10.0_f32.powf(gain_db / 40.0);
                let alpha = sin_w0 / (2.0 * q.max(0.01));
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                    (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                    (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                )
            }
            BiquadKind::HighPass => {
                let alpha = sin_w0 / (2.0 * q.max(0.01));
                (
                    (1.0 + cos_w0) / 2.0,
                    -(1.0 + cos_w0),
                    (1.0 + cos_w0) / 2.0,
                    1.0 + alpha,
                    -2.0 * cos_w0,
                    1.0 - alpha,
                )
            }
            BiquadKind::LowPass => {
                let alpha = sin_w0 / (2.0 * q.max(0.01));
                (
                    (1.0 - cos_w0) / 2.0,
                    1.0 - cos_w0,
                    (1.0 - cos_w0) / 2.0,
                    1.0 + alpha,
                    -2.0 * cos_w0,
                    1.0 - alpha,
                )
            }
            BiquadKind::Notch => {
                let alpha = sin_w0 / (2.0 * q.max(0.01));
                (
                    1.0,
                    -2.0 * cos_w0,
                    1.0,
                    1.0 + alpha,
                    -2.0 * cos_w0,
                    1.0 - alpha,
                )
            }
        };

        let inv_a0 = 1.0 / a0;
        Self {
            b0: b0 * inv_a0,
            b1: b1 * inv_a0,
            b2: b2 * inv_a0,
            a1: a1 * inv_a0,
            a2: a2 * inv_a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;
        output
    }

    /// Magnitude response in dB at the given frequency.
    pub fn magnitude_db(&self, freq_hz: f32, sample_rate: f32) -> f32 {
        let w = 2.0 * PI * freq_hz / sample_rate;
        let cos_w = w.cos();
        let sin_w = w.sin();
        let cos_2w = (2.0 * w).cos();
        let sin_2w = (2.0 * w).sin();
        let nr = self.b0 + self.b1 * cos_w + self.b2 * cos_2w;
        let ni = -(self.b1 * sin_w + self.b2 * sin_2w);
        let dr = 1.0 + self.a1 * cos_w + self.a2 * cos_2w;
        let di = -(self.a1 * sin_w + self.a2 * sin_2w);
        let mag_sq = (nr * nr + ni * ni) / (dr * dr + di * di).max(1e-30);
        10.0 * mag_sq.max(1e-30_f32).log10()
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
