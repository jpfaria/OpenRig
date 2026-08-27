//! Responsibility: implements the core cab model.
use anyhow::Result;
use block_core::param::ParameterSet;
use block_core::{
    db_to_lin, AudioChannelLayout, BiquadFilter, BiquadKind, BlockProcessor, MonoProcessor,
    OnePoleLowPass, StereoProcessor,
};

pub use crate::native_cab_schema::{
    asset_summary, model_schema, settings_from_params, validate_params,
};
pub use crate::native_cab_settings::{
    NativeCabProfile, NativeCabSchemaDefaults, NativeCabSettings,
};

struct DualMonoProcessor {
    left: Box<dyn MonoProcessor>,
    right: Box<dyn MonoProcessor>,
}

struct DelayTap {
    buffer: Vec<f32>,
    write_pos: usize,
    delay_samples: usize,
}

/// Biquad cascade voicing a single native cabinet. Every filter is built once in
/// `new` (setup); `process_sample` is allocation-/lock-free and adds no latency.
struct NativeCabProcessor {
    settings: NativeCabSettings,
    output_gain: f32,
    body_hp: BiquadFilter,
    low_bump: BiquadFilter,
    mid_dip: BiquadFilter,
    presence: BiquadFilter,
    speaker_lp1: BiquadFilter,
    speaker_lp2: BiquadFilter,
    brightness_lp: BiquadFilter,
    room_low_pass: OnePoleLowPass,
    room_delay: DelayTap,
}

impl StereoProcessor for DualMonoProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }
}

impl DelayTap {
    fn new(max_delay_ms: f32, sample_rate: f32) -> DelayTap {
        let max_samples = ((max_delay_ms * 0.001 * sample_rate).ceil() as usize).max(2);
        DelayTap {
            buffer: vec![0.0; max_samples + 1],
            write_pos: 0,
            delay_samples: 1,
        }
    }

    fn set_delay_ms(&mut self, delay_ms: f32, sample_rate: f32) {
        let max_index = self.buffer.len().saturating_sub(1);
        self.delay_samples =
            ((delay_ms * 0.001 * sample_rate).round() as usize).clamp(1, max_index.max(1));
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffer_len = self.buffer.len();
        let read_pos = (self.write_pos + buffer_len - self.delay_samples) % buffer_len;
        let delayed = self.buffer[read_pos];
        self.buffer[self.write_pos] = input;
        self.write_pos = (self.write_pos + 1) % buffer_len;
        delayed
    }
}

fn percent_to_gain_db(p: f32) -> f32 {
    -18.0 + (p / 100.0) * 36.0
}

impl NativeCabProcessor {
    fn new(profile: NativeCabProfile, settings: NativeCabSettings, sample_rate: f32) -> Self {
        let mic_position = (settings.mic_position / 100.0).clamp(0.0, 1.0);
        let mic_distance = (settings.mic_distance / 100.0).clamp(0.0, 1.0);
        let nyquist_guard = sample_rate * 0.45;

        // On-axis (high mic_position) brightens: pushes the rolloff up and the
        // presence peak slightly higher, the way moving a mic toward the cone
        // centre does. Off-axis darkens.
        let rolloff_hz =
            (profile.rolloff_hz * (0.85 + mic_position * 0.3)).clamp(1_500.0, nyquist_guard);
        let presence_hz =
            (profile.presence_hz * (0.9 + mic_position * 0.2)).clamp(800.0, nyquist_guard);

        // Knobs scale the intrinsic profile stages, monotonic around the
        // defaults: Resonance drives the low bump, Air the presence.
        let low_bump_db = profile.low_bump_db * (settings.resonance / 100.0 * 1.8).clamp(0.0, 2.0);
        let presence_db =
            profile.presence_db * (0.6 + (settings.air / 100.0) * 0.8).clamp(0.0, 2.0);

        let body_hz = settings.low_cut_hz.clamp(20.0, 400.0);
        let brightness_hz = settings.high_cut_hz.clamp(1_500.0, nyquist_guard);

        let room_delay_ms = profile.room_base_ms + mic_distance * profile.room_span_ms;
        let mut room_delay = DelayTap::new(40.0, sample_rate);
        room_delay.set_delay_ms(room_delay_ms, sample_rate);

        Self {
            settings,
            output_gain: db_to_lin(percent_to_gain_db(settings.output)),
            body_hp: BiquadFilter::new(BiquadKind::HighPass, body_hz, 0.0, 0.707, sample_rate),
            low_bump: BiquadFilter::new(
                BiquadKind::Peak,
                profile.low_bump_hz,
                low_bump_db,
                profile.low_bump_q,
                sample_rate,
            ),
            mid_dip: BiquadFilter::new(
                BiquadKind::Peak,
                profile.mid_dip_hz,
                profile.mid_dip_db,
                profile.mid_dip_q,
                sample_rate,
            ),
            presence: BiquadFilter::new(
                BiquadKind::Peak,
                presence_hz,
                presence_db,
                profile.presence_q,
                sample_rate,
            ),
            // Two cascaded low-passes → ~24 dB/oct skirt, the steep top-end
            // rolloff that makes a cabinet sound like a speaker, not a wire.
            speaker_lp1: BiquadFilter::new(
                BiquadKind::LowPass,
                rolloff_hz,
                0.0,
                profile.rolloff_q,
                sample_rate,
            ),
            speaker_lp2: BiquadFilter::new(
                BiquadKind::LowPass,
                rolloff_hz,
                0.0,
                0.707,
                sample_rate,
            ),
            // The High Cut knob, a gentle extra low-pass on top of the speaker
            // rolloff so the user can darken without redefining the cabinet.
            brightness_lp: BiquadFilter::new(
                BiquadKind::LowPass,
                brightness_hz,
                0.0,
                0.707,
                sample_rate,
            ),
            room_low_pass: OnePoleLowPass::new(2_200.0 - mic_distance * 600.0, sample_rate),
            room_delay,
        }
    }
}

impl MonoProcessor for NativeCabProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let mut sample = self.body_hp.process(input);
        sample = self.low_bump.process(sample);
        sample = self.mid_dip.process(sample);
        sample = self.presence.process(sample);
        sample = self.speaker_lp1.process(sample);
        sample = self.speaker_lp2.process(sample);
        sample = self.brightness_lp.process(sample);

        let mic_distance = (self.settings.mic_distance / 100.0).clamp(0.0, 1.0);
        let room_mix = (self.settings.room_mix / 100.0).clamp(0.0, 1.0);
        let room_source = self.room_low_pass.process(sample);
        let room = self.room_delay.process(room_source) * room_mix * (0.25 + mic_distance * 0.65);
        let close_mix = 1.0 - room_mix * 0.45;

        (sample * close_mix + room) * self.output_gain
    }
}

pub fn build_processor_for_profile(
    profile: NativeCabProfile,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let settings = settings_from_params(params)?;

    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(build_native_cab_mono_processor(
            profile,
            settings,
            sample_rate,
        ))),
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: build_native_cab_mono_processor(profile, settings, sample_rate),
            right: build_native_cab_mono_processor(profile, settings, sample_rate),
        }))),
    }
}

pub fn build_native_cab_mono_processor(
    profile: NativeCabProfile,
    settings: NativeCabSettings,
    sample_rate: f32,
) -> Box<dyn MonoProcessor> {
    Box::new(NativeCabProcessor::new(profile, settings, sample_rate))
}
