use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{
    db_to_lin, AudioChannelLayout, ModelAudioMode, MonoProcessor, OnePoleHighPass, OnePoleLowPass,
    BlockProcessor, StereoProcessor,
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
    pub output_db: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct NativeCabProfile {
    pub resonance_hz: f32,
    pub air_hz: f32,
    pub room_base_ms: f32,
    pub room_span_ms: f32,
    pub resonance_gain: f32,
    pub air_gain: f32,
    pub high_cut_scale: f32,
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

struct NativeCabProcessor {
    settings: NativeCabSettings,
    profile: NativeCabProfile,
    output_gain: f32,
    low_cut: OnePoleHighPass,
    core_low_pass: OnePoleLowPass,
    resonance_high_pass: OnePoleHighPass,
    resonance_low_pass: OnePoleLowPass,
    air_high_pass: OnePoleHighPass,
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

impl NativeCabProcessor {
    fn new(profile: NativeCabProfile, settings: NativeCabSettings, sample_rate: f32) -> Self {
        let mic_position = (settings.mic_position / 100.0).clamp(0.0, 1.0);
        let mic_distance = (settings.mic_distance / 100.0).clamp(0.0, 1.0);
        let effective_high_cut =
            (settings.high_cut_hz * profile.high_cut_scale * (0.8 + mic_position * 0.4))
                .clamp(1_800.0, sample_rate * 0.45);
        let room_delay_ms = profile.room_base_ms + mic_distance * profile.room_span_ms;
        let mut room_delay = DelayTap::new(40.0, sample_rate);
        room_delay.set_delay_ms(room_delay_ms, sample_rate);

        Self {
            settings,
            profile,
            output_gain: db_to_lin(settings.output_db),
            low_cut: OnePoleHighPass::new(settings.low_cut_hz, sample_rate),
            core_low_pass: OnePoleLowPass::new(effective_high_cut, sample_rate),
            resonance_high_pass: OnePoleHighPass::new(profile.resonance_hz * 0.75, sample_rate),
            resonance_low_pass: OnePoleLowPass::new(profile.resonance_hz * 1.8, sample_rate),
            air_high_pass: OnePoleHighPass::new(profile.air_hz, sample_rate),
            room_low_pass: OnePoleLowPass::new(2_200.0 - mic_distance * 600.0, sample_rate),
            room_delay,
        }
    }
}

impl MonoProcessor for NativeCabProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let mut sample = self.low_cut.process(input);
        sample = self.core_low_pass.process(sample);

        let resonance_band = self
            .resonance_low_pass
            .process(self.resonance_high_pass.process(sample));
        let resonance_amount = (self.settings.resonance / 100.0).clamp(0.0, 1.0);
        sample += resonance_band * resonance_amount * self.profile.resonance_gain;

        let mic_position = (self.settings.mic_position / 100.0).clamp(0.0, 1.0);
        let mic_distance = (self.settings.mic_distance / 100.0).clamp(0.0, 1.0);
        let air = self.air_high_pass.process(sample)
            * (self.settings.air / 100.0).clamp(0.0, 1.0)
            * self.profile.air_gain
            * (0.35 + mic_position * 0.85)
            * (1.0 - mic_distance * 0.35);
        sample += air;

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
                "output_db",
                "Output",
                Some("Output"),
                Some(0.0),
                -18.0,
                18.0,
                0.5,
                ParameterUnit::Decibels,
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
        output_db: required_f32(params, "output_db").map_err(anyhow::Error::msg)?,
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
