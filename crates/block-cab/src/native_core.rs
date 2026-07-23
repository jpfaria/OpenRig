use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{
    db_to_lin, AudioChannelLayout, BiquadFilter, BiquadKind, BlockProcessor, ModelAudioMode,
    MonoProcessor, OnePoleLowPass, StereoProcessor,
};

#[derive(Debug, Clone, Copy)]
pub struct NativeCabSettings {
    pub low_cut_hz: f32,
    pub high_cut_hz: f32,
    pub resonance: f32,
    pub air: f32,
    pub mic_position: f32,
    pub mic_distance: f32,
    pub room_mix: f32,
    pub output: f32,
}

/// Per-model magnitude-response fingerprint, approximating the measured response
/// of a reference cabinet with a biquad cascade. These are *descriptive targets*
/// (a 4x12 with Celestion-style speakers, a small warm 1x12, a bright scooped
/// 2x12) — never a named/branded model (zero-coupling rule). The biquad cascade
/// matches the magnitude curve; it does not reproduce the comb-filtering or
/// complex phase of a real cabinet — that only comes from a measured IR.
#[derive(Debug, Clone, Copy)]
pub struct NativeCabProfile {
    /// Speaker high-frequency rolloff corner — the dominant cabinet trait.
    /// Applied as two cascaded low-passes (~24 dB/oct), the steep skirt a real
    /// cone has above its top end.
    pub rolloff_hz: f32,
    pub rolloff_q: f32,
    /// Low-end cone/cabinet resonance bump.
    pub low_bump_hz: f32,
    pub low_bump_db: f32,
    pub low_bump_q: f32,
    /// Mid scoop — the guitar-cab "honk" notch; its centre and depth strongly
    /// separate one cabinet from another.
    pub mid_dip_hz: f32,
    pub mid_dip_db: f32,
    pub mid_dip_q: f32,
    /// Presence/bite peak in the upper mids.
    pub presence_hz: f32,
    pub presence_db: f32,
    pub presence_q: f32,
    /// Room reflection tap (kept from the previous engine).
    pub room_base_ms: f32,
    pub room_span_ms: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct NativeCabSchemaDefaults {
    pub low_cut_hz: f32,
    pub high_cut_hz: f32,
    pub resonance: f32,
    pub air: f32,
    pub mic_position: f32,
    pub mic_distance: f32,
    pub room_mix: f32,
}

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

pub fn model_schema(
    model_id: &'static str,
    display_name: &'static str,
    defaults: NativeCabSchemaDefaults,
) -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "cab".into(),
        model: model_id.into(),
        display_name: display_name.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "low_cut_hz",
                "Low Cut",
                Some("Filtering"),
                Some(defaults.low_cut_hz),
                20.0,
                250.0,
                1.0,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "high_cut_hz",
                "High Cut",
                Some("Filtering"),
                Some(defaults.high_cut_hz),
                2_000.0,
                12_000.0,
                10.0,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "resonance",
                "Resonance",
                Some("Speaker"),
                Some(defaults.resonance),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "air",
                "Air",
                Some("Mic"),
                Some(defaults.air),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mic_position",
                "Mic Position",
                Some("Mic"),
                Some(defaults.mic_position),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mic_distance",
                "Mic Distance",
                Some("Mic"),
                Some(defaults.mic_distance),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "room_mix",
                "Room Mix",
                Some("Room"),
                Some(defaults.room_mix),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "output",
                "Output",
                Some("Output"),
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn settings_from_params(params: &ParameterSet) -> Result<NativeCabSettings> {
    Ok(NativeCabSettings {
        low_cut_hz: required_f32(params, "low_cut_hz").map_err(anyhow::Error::msg)?,
        high_cut_hz: required_f32(params, "high_cut_hz").map_err(anyhow::Error::msg)?,
        resonance: required_f32(params, "resonance").map_err(anyhow::Error::msg)?,
        air: required_f32(params, "air").map_err(anyhow::Error::msg)?,
        mic_position: required_f32(params, "mic_position").map_err(anyhow::Error::msg)?,
        mic_distance: required_f32(params, "mic_distance").map_err(anyhow::Error::msg)?,
        room_mix: required_f32(params, "room_mix").map_err(anyhow::Error::msg)?,
        output: required_f32(params, "output").map_err(anyhow::Error::msg)?,
    })
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = settings_from_params(params)?;
    Ok(())
}

pub fn asset_summary(model_id: &'static str, _params: &ParameterSet) -> Result<String> {
    Ok(format!("native voice='{model_id}'"))
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
