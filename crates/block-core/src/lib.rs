//! Core building blocks shared by OpenRig block families.
pub mod param;

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use std::sync::{Arc, Mutex};

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
/// The processor writes entries during `process_block`; the GUI reads them via polling.
pub type StreamHandle = Arc<Mutex<Vec<StreamEntry>>>;

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
pub trait PluginEditorHandle: Send {
    /// Reposition the embedded plugin view inside its parent.
    ///
    /// `x`, `y` use top-left origin (Slint convention). Only meaningful for
    /// editors opened in embedded mode; default is a no-op.
    fn reposition_embedded(&self, _x: f64, _y: f64) {}
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
mod tests {
    use super::*;

    // ── AudioChannelLayout ──────────────────────────────────────────

    #[test]
    fn audio_channel_layout_debug_and_clone() {
        let mono = AudioChannelLayout::Mono;
        let stereo = AudioChannelLayout::Stereo;
        assert_eq!(mono, mono.clone());
        assert_eq!(stereo, stereo.clone());
        assert!(mono != stereo);
    }

    // ── ModelAudioMode::accepts_input ───────────────────────────────

    #[test]
    fn mono_only_accepts_mono_input() {
        assert!(ModelAudioMode::MonoOnly.accepts_input(AudioChannelLayout::Mono));
    }

    #[test]
    fn mono_only_rejects_stereo_input() {
        assert!(!ModelAudioMode::MonoOnly.accepts_input(AudioChannelLayout::Stereo));
    }

    #[test]
    fn dual_mono_accepts_mono_input() {
        assert!(ModelAudioMode::DualMono.accepts_input(AudioChannelLayout::Mono));
    }

    #[test]
    fn dual_mono_accepts_stereo_input() {
        assert!(ModelAudioMode::DualMono.accepts_input(AudioChannelLayout::Stereo));
    }

    #[test]
    fn true_stereo_rejects_mono_input() {
        assert!(!ModelAudioMode::TrueStereo.accepts_input(AudioChannelLayout::Mono));
    }

    #[test]
    fn true_stereo_accepts_stereo_input() {
        assert!(ModelAudioMode::TrueStereo.accepts_input(AudioChannelLayout::Stereo));
    }

    #[test]
    fn mono_to_stereo_accepts_mono_input() {
        assert!(ModelAudioMode::MonoToStereo.accepts_input(AudioChannelLayout::Mono));
    }

    #[test]
    fn mono_to_stereo_accepts_stereo_input() {
        assert!(ModelAudioMode::MonoToStereo.accepts_input(AudioChannelLayout::Stereo));
    }

    // ── ModelAudioMode::output_layout ───────────────────────────────

    #[test]
    fn mono_only_output_mono_gives_mono() {
        assert_eq!(
            ModelAudioMode::MonoOnly.output_layout(AudioChannelLayout::Mono),
            Some(AudioChannelLayout::Mono)
        );
    }

    #[test]
    fn mono_only_output_stereo_gives_none() {
        assert_eq!(
            ModelAudioMode::MonoOnly.output_layout(AudioChannelLayout::Stereo),
            None
        );
    }

    #[test]
    fn dual_mono_output_preserves_layout() {
        assert_eq!(
            ModelAudioMode::DualMono.output_layout(AudioChannelLayout::Mono),
            Some(AudioChannelLayout::Mono)
        );
        assert_eq!(
            ModelAudioMode::DualMono.output_layout(AudioChannelLayout::Stereo),
            Some(AudioChannelLayout::Stereo)
        );
    }

    #[test]
    fn true_stereo_output_stereo_gives_stereo() {
        assert_eq!(
            ModelAudioMode::TrueStereo.output_layout(AudioChannelLayout::Stereo),
            Some(AudioChannelLayout::Stereo)
        );
    }

    #[test]
    fn true_stereo_output_mono_gives_none() {
        assert_eq!(
            ModelAudioMode::TrueStereo.output_layout(AudioChannelLayout::Mono),
            None
        );
    }

    #[test]
    fn mono_to_stereo_output_always_stereo() {
        assert_eq!(
            ModelAudioMode::MonoToStereo.output_layout(AudioChannelLayout::Mono),
            Some(AudioChannelLayout::Stereo)
        );
        assert_eq!(
            ModelAudioMode::MonoToStereo.output_layout(AudioChannelLayout::Stereo),
            Some(AudioChannelLayout::Stereo)
        );
    }

    // ── ModelAudioMode::as_str ──────────────────────────────────────

    #[test]
    fn as_str_returns_expected_labels() {
        assert_eq!(ModelAudioMode::MonoOnly.as_str(), "mono_only");
        assert_eq!(ModelAudioMode::DualMono.as_str(), "dual_mono");
        assert_eq!(ModelAudioMode::TrueStereo.as_str(), "true_stereo");
        assert_eq!(ModelAudioMode::MonoToStereo.as_str(), "mono_to_stereo");
    }

    // ── capitalize_first ────────────────────────────────────────────

    #[test]
    fn capitalize_first_empty_string() {
        assert_eq!(capitalize_first(""), "");
    }

    #[test]
    fn capitalize_first_single_char() {
        assert_eq!(capitalize_first("a"), "A");
    }

    #[test]
    fn capitalize_first_already_uppercase() {
        assert_eq!(capitalize_first("Hello"), "Hello");
    }

    #[test]
    fn capitalize_first_lowercase_word() {
        assert_eq!(capitalize_first("hello"), "Hello");
    }

    #[test]
    fn capitalize_first_unicode_char() {
        // German sharp-s uppercases to "SS"
        assert_eq!(capitalize_first("\u{00DF}traße"), "SStraße");
    }

    // ── db_to_lin / lin_to_db ───────────────────────────────────────

    #[test]
    fn db_to_lin_zero_db_is_unity() {
        assert!((db_to_lin(0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn db_to_lin_minus_20_is_tenth() {
        assert!((db_to_lin(-20.0) - 0.1).abs() < 1e-4);
    }

    #[test]
    fn db_to_lin_plus_20_is_ten() {
        assert!((db_to_lin(20.0) - 10.0).abs() < 1e-4);
    }

    #[test]
    fn lin_to_db_unity_is_zero() {
        assert!((lin_to_db(1.0) - 0.0).abs() < 1e-4);
    }

    #[test]
    fn lin_to_db_tenth_is_minus_20() {
        assert!((lin_to_db(0.1) - (-20.0)).abs() < 0.01);
    }

    #[test]
    fn lin_to_db_near_zero_returns_minus_200() {
        assert_eq!(lin_to_db(0.0), -200.0);
        assert_eq!(lin_to_db(1e-11), -200.0);
    }

    #[test]
    fn db_lin_roundtrip() {
        let db = -6.0_f32;
        let roundtrip = lin_to_db(db_to_lin(db));
        assert!((roundtrip - db).abs() < 1e-4);
    }

    // ── calculate_coefficient ───────────────────────────────────────

    #[test]
    fn calculate_coefficient_positive_time() {
        let coeff = calculate_coefficient(10.0, 44100.0);
        assert!(coeff > 0.0 && coeff < 1.0);
    }

    #[test]
    fn calculate_coefficient_very_small_time_clamped() {
        // time_ms = 0.0 should be clamped to 0.001 via max()
        let coeff = calculate_coefficient(0.0, 44100.0);
        assert!(coeff.is_finite());
    }

    #[test]
    fn calculate_coefficient_large_time_approaches_one() {
        let coeff = calculate_coefficient(10000.0, 44100.0);
        assert!(coeff > 0.99);
    }

    // ── EnvelopeFollower ────────────────────────────────────────────

    #[test]
    fn envelope_follower_starts_at_zero() {
        let ef = EnvelopeFollower::from_ms(1.0, 10.0, 44100.0);
        assert_eq!(ef.value(), 0.0);
    }

    #[test]
    fn envelope_follower_tracks_positive_signal() {
        let mut ef = EnvelopeFollower::from_ms(0.1, 100.0, 44100.0);
        for _ in 0..1000 {
            ef.process(1.0);
        }
        // After many samples of constant 1.0, envelope should be close to 1.0
        assert!((ef.value() - 1.0).abs() < 0.01);
    }

    #[test]
    fn envelope_follower_tracks_negative_signal() {
        let mut ef = EnvelopeFollower::from_ms(0.1, 100.0, 44100.0);
        for _ in 0..1000 {
            ef.process(-0.5);
        }
        assert!((ef.value() - 0.5).abs() < 0.01);
    }

    #[test]
    fn envelope_follower_release_decays() {
        let mut ef = EnvelopeFollower::from_ms(0.01, 1.0, 44100.0);
        // Attack to near 1.0
        for _ in 0..5000 {
            ef.process(1.0);
        }
        let peak = ef.value();
        // Release
        for _ in 0..5000 {
            ef.process(0.0);
        }
        assert!(ef.value() < peak);
    }

    #[test]
    fn envelope_follower_set_coefficients() {
        let mut ef = EnvelopeFollower::from_ms(1.0, 10.0, 44100.0);
        ef.set_attack_coeff(0.5);
        ef.set_release_coeff(0.9);
        ef.process(1.0);
        assert!(ef.value() > 0.0);
    }

    // ── OnePoleLowPass ──────────────────────────────────────────────

    #[test]
    fn one_pole_low_pass_dc_converges() {
        let mut lp = OnePoleLowPass::new(1000.0, 44100.0);
        for _ in 0..44100 {
            lp.process(1.0);
        }
        assert!((lp.process(1.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn one_pole_low_pass_attenuates_high_freq() {
        let mut lp = OnePoleLowPass::new(100.0, 44100.0);
        // Feed a high-frequency alternating signal
        let mut max_out = 0.0_f32;
        for i in 0..1000 {
            let input = if i % 2 == 0 { 1.0 } else { -1.0 };
            let out = lp.process(input).abs();
            if out > max_out {
                max_out = out;
            }
        }
        // The output should be much smaller than 1.0
        assert!(max_out < 0.1);
    }

    // ── OnePoleHighPass ─────────────────────────────────────────────

    #[test]
    fn one_pole_high_pass_blocks_dc() {
        let mut hp = OnePoleHighPass::new(100.0, 44100.0);
        let mut last = 0.0;
        for _ in 0..44100 {
            last = hp.process(1.0);
        }
        assert!(last.abs() < 0.01);
    }

    #[test]
    fn one_pole_high_pass_passes_high_freq() {
        let mut hp = OnePoleHighPass::new(10.0, 44100.0);
        // High-frequency alternating
        let mut sum_sq = 0.0_f32;
        for i in 0..1000 {
            let input = if i % 2 == 0 { 1.0 } else { -1.0 };
            let out = hp.process(input);
            sum_sq += out * out;
        }
        let rms = (sum_sq / 1000.0).sqrt();
        assert!(rms > 0.5);
    }

    // ── BiquadFilter ────────────────────────────────────────────────

    #[test]
    fn biquad_peak_boost_at_center_frequency() {
        let bq = BiquadFilter::new(BiquadKind::Peak, 1000.0, 12.0, 1.0, 44100.0);
        let mag = bq.magnitude_db(1000.0, 44100.0);
        // Should be approximately +12 dB at center
        assert!((mag - 12.0).abs() < 1.0);
    }

    #[test]
    fn biquad_peak_flat_away_from_center() {
        let bq = BiquadFilter::new(BiquadKind::Peak, 1000.0, 12.0, 1.0, 44100.0);
        let mag = bq.magnitude_db(100.0, 44100.0);
        // Far from center, should be close to 0 dB
        assert!(mag.abs() < 3.0);
    }

    #[test]
    fn biquad_low_shelf_boosts_low_frequencies() {
        let bq = BiquadFilter::new(BiquadKind::LowShelf, 500.0, 6.0, 0.707, 44100.0);
        let mag_low = bq.magnitude_db(50.0, 44100.0);
        let mag_high = bq.magnitude_db(5000.0, 44100.0);
        assert!(mag_low > mag_high);
        assert!(mag_low > 3.0);
    }

    #[test]
    fn biquad_high_shelf_boosts_high_frequencies() {
        let bq = BiquadFilter::new(BiquadKind::HighShelf, 2000.0, 6.0, 0.707, 44100.0);
        let mag_high = bq.magnitude_db(10000.0, 44100.0);
        let mag_low = bq.magnitude_db(100.0, 44100.0);
        assert!(mag_high > mag_low);
        assert!(mag_high > 3.0);
    }

    #[test]
    fn biquad_low_pass_attenuates_high_freq() {
        let bq = BiquadFilter::new(BiquadKind::LowPass, 1000.0, 0.0, 0.707, 44100.0);
        let mag_high = bq.magnitude_db(10000.0, 44100.0);
        assert!(mag_high < -10.0);
    }

    #[test]
    fn biquad_high_pass_attenuates_low_freq() {
        let bq = BiquadFilter::new(BiquadKind::HighPass, 1000.0, 0.0, 0.707, 44100.0);
        let mag_low = bq.magnitude_db(50.0, 44100.0);
        assert!(mag_low < -10.0);
    }

    #[test]
    fn biquad_process_dc_through_peak_filter() {
        let mut bq = BiquadFilter::new(BiquadKind::Peak, 1000.0, 0.0, 1.0, 44100.0);
        // 0 dB peak = unity, DC should pass through
        let mut out = 0.0;
        for _ in 0..44100 {
            out = bq.process(1.0);
        }
        assert!((out - 1.0).abs() < 0.01);
    }

    // ── BlockProcessor enum ─────────────────────────────────────────

    struct DummyMono;
    impl MonoProcessor for DummyMono {
        fn process_sample(&mut self, input: f32) -> f32 {
            input * 2.0
        }
    }

    struct DummyStereo;
    impl StereoProcessor for DummyStereo {
        fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
            [input[0] * 0.5, input[1] * 0.5]
        }
    }

    #[test]
    fn block_processor_mono_variant() {
        let bp = BlockProcessor::Mono(Box::new(DummyMono));
        assert!(matches!(bp, BlockProcessor::Mono(_)));
    }

    #[test]
    fn block_processor_stereo_variant() {
        let bp = BlockProcessor::Stereo(Box::new(DummyStereo));
        assert!(matches!(bp, BlockProcessor::Stereo(_)));
    }

    #[test]
    fn mono_processor_default_process_block() {
        let mut proc = DummyMono;
        let mut buf = [1.0, 2.0, 3.0];
        proc.process_block(&mut buf);
        assert_eq!(buf, [2.0, 4.0, 6.0]);
    }

    #[test]
    fn stereo_processor_default_process_block() {
        let mut proc = DummyStereo;
        let mut buf = [[1.0, 2.0], [4.0, 6.0]];
        proc.process_block(&mut buf);
        assert_eq!(buf, [[0.5, 1.0], [2.0, 3.0]]);
    }

    // ── Constants ───────────────────────────────────────────────────

    #[test]
    fn all_instruments_has_six_entries() {
        assert_eq!(ALL_INSTRUMENTS.len(), 6);
        assert!(ALL_INSTRUMENTS.contains(&INST_ELECTRIC_GUITAR));
        assert!(ALL_INSTRUMENTS.contains(&INST_DRUMS));
    }

    #[test]
    fn guitar_bass_has_two_entries() {
        assert_eq!(GUITAR_BASS.len(), 2);
        assert!(GUITAR_BASS.contains(&INST_ELECTRIC_GUITAR));
        assert!(GUITAR_BASS.contains(&INST_BASS));
    }

    #[test]
    fn guitar_acoustic_bass_has_three_entries() {
        assert_eq!(GUITAR_ACOUSTIC_BASS.len(), 3);
        assert!(GUITAR_ACOUSTIC_BASS.contains(&INST_ACOUSTIC_GUITAR));
    }

    #[test]
    fn default_instrument_is_electric_guitar() {
        assert_eq!(DEFAULT_INSTRUMENT, "electric_guitar");
    }

    #[test]
    fn effect_type_constants_not_empty() {
        assert!(!EFFECT_TYPE_PREAMP.is_empty());
        assert!(!EFFECT_TYPE_AMP.is_empty());
        assert!(!EFFECT_TYPE_CAB.is_empty());
        assert!(!EFFECT_TYPE_GAIN.is_empty());
        assert!(!EFFECT_TYPE_DELAY.is_empty());
        assert!(!EFFECT_TYPE_REVERB.is_empty());
        assert!(!EFFECT_TYPE_MODULATION.is_empty());
        assert!(!EFFECT_TYPE_DYNAMICS.is_empty());
        assert!(!EFFECT_TYPE_FILTER.is_empty());
        assert!(!EFFECT_TYPE_WAH.is_empty());
        assert!(!EFFECT_TYPE_PITCH.is_empty());
        assert!(!EFFECT_TYPE_BODY.is_empty());
        assert!(!EFFECT_TYPE_IR.is_empty());
        assert!(!EFFECT_TYPE_NAM.is_empty());
        assert!(!EFFECT_TYPE_FULL_RIG.is_empty());
        assert!(!EFFECT_TYPE_UTILITY.is_empty());
        assert!(!EFFECT_TYPE_VST3.is_empty());
    }

    #[test]
    fn brand_native_constant() {
        assert_eq!(BRAND_NATIVE, "native");
    }

    // ── StreamEntry ─────────────────────────────────────────────────

    #[test]
    fn stream_entry_clone_and_debug() {
        let entry = StreamEntry {
            key: "freq".to_string(),
            value: 440.0,
            text: "A4".to_string(),
            peak: 0.8,
        };
        let cloned = entry.clone();
        assert_eq!(cloned.key, "freq");
        assert_eq!(cloned.value, 440.0);
        assert_eq!(cloned.text, "A4");
        assert_eq!(cloned.peak, 0.8);
        // Debug should not panic
        let _ = format!("{:?}", entry);
    }

    // ── KnobLayoutEntry ─────────────────────────────────────────────

    #[test]
    fn knob_layout_entry_fields() {
        let entry = KnobLayoutEntry {
            param_key: "gain",
            svg_cx: 100.0,
            svg_cy: 50.0,
            svg_r: 20.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
        };
        assert_eq!(entry.param_key, "gain");
        assert_eq!(entry.svg_cx, 100.0);
        let _ = format!("{:?}", entry);
    }

    // ── ModelVisualData ─────────────────────────────────────────────

    #[test]
    fn model_visual_data_fields() {
        static KNOBS: &[KnobLayoutEntry] = &[];
        let mvd = ModelVisualData {
            brand: "native",
            type_label: "preamp",
            supported_instruments: ALL_INSTRUMENTS,
            knob_layout: KNOBS,
        };
        assert_eq!(mvd.brand, "native");
        assert_eq!(mvd.type_label, "preamp");
        assert_eq!(mvd.supported_instruments.len(), 6);
        let _ = format!("{:?}", mvd);
    }

    // ── Serde roundtrip for AudioChannelLayout ──────────────────────

    #[test]
    fn audio_channel_layout_serde_roundtrip() {
        let mono = AudioChannelLayout::Mono;
        let json = serde_json::to_string(&mono).unwrap();
        assert_eq!(json, "\"mono\"");
        let back: AudioChannelLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mono);

        let stereo = AudioChannelLayout::Stereo;
        let json = serde_json::to_string(&stereo).unwrap();
        assert_eq!(json, "\"stereo\"");
    }

    // ── Serde roundtrip for ModelAudioMode ──────────────────────────

    #[test]
    fn model_audio_mode_serde_roundtrip() {
        let mode = ModelAudioMode::MonoToStereo;
        let json = serde_json::to_string(&mode).unwrap();
        assert_eq!(json, "\"mono_to_stereo\"");
        let back: ModelAudioMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mode);
    }
}
