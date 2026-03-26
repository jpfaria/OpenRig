use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor,
    OnePoleHighPass, OnePoleLowPass, StereoProcessor,
};

pub const MODEL_ID: &str = "ibanez_ts9";
pub const DISPLAY_NAME: &str = "TS9 Tube Screamer";
const BRAND: &str = "ibanez";

#[derive(Debug, Clone, Copy)]
pub struct Ts9Settings {
    pub drive: f32,
    pub tone: f32,
    pub level: f32,
}

struct DualMonoProcessor {
    left: Ts9Processor,
    right: Ts9Processor,
}

struct Ts9Processor {
    settings: Ts9Settings,
    input_high_pass: OnePoleHighPass,
    clip_low_pass: OnePoleLowPass,
    tone_low_pass: OnePoleLowPass,
    tone_high_pass: OnePoleHighPass,
    output_high_pass: OnePoleHighPass,
}

impl StereoProcessor for DualMonoProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }
}

impl Ts9Processor {
    fn new(settings: Ts9Settings, sample_rate: f32) -> Self {
        Self {
            settings,
            input_high_pass: OnePoleHighPass::new(85.0, sample_rate),
            clip_low_pass: OnePoleLowPass::new(4_200.0, sample_rate),
            tone_low_pass: OnePoleLowPass::new(820.0, sample_rate),
            tone_high_pass: OnePoleHighPass::new(1_150.0, sample_rate),
            output_high_pass: OnePoleHighPass::new(35.0, sample_rate),
        }
    }

    fn normalized_percent(value: f32) -> f32 {
        (value / 100.0).clamp(0.0, 1.0)
    }

    fn soft_clip(sample: f32) -> f32 {
        let limited = sample.clamp(-3.0, 3.0);
        limited - (limited * limited * limited) / 3.0
    }
}

impl MonoProcessor for Ts9Processor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let drive = Self::normalized_percent(self.settings.drive);
        let tone = Self::normalized_percent(self.settings.tone);
        let level = Self::normalized_percent(self.settings.level);

        let mut sample = self.input_high_pass.process(input);

        // Drive stage — TS9 has moderate gain, not extreme
        let pre_gain = 1.0 + drive * 8.0;
        let mid_push = 1.0 + drive * 0.5;

        sample *= pre_gain;
        sample = self.clip_low_pass.process(sample) * mid_push;
        sample = Self::soft_clip(sample);

        // Tone stack
        let low_band = self.tone_low_pass.process(sample);
        let high_band = self.tone_high_pass.process(sample);
        let mid_band = sample - low_band - high_band;

        let low_mix = 0.85 - tone * 0.40;
        let high_mix = 0.12 + tone * 0.90;
        let mid_mix = 0.90 + (1.0 - (tone - 0.5).abs() * 2.0) * 0.15;

        let voiced = low_band * low_mix + mid_band * mid_mix + high_band * high_mix;
        let output = self.output_high_pass.process(voiced);

        // Level: linear gain from 0 to 2x (0% = silent, 50% = unity, 100% = +6dB)
        let level_gain = level * 2.0;

        output * level_gain
    }
}

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: "TS9 Tube Screamer".into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "drive",
                "Drive",
                Some("Gain"),
                Some(35.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "tone",
                "Tone",
                Some("EQ"),
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "level",
                "Level",
                Some("Output"),
                Some(55.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = read_settings(params)?;
    Ok(())
}

pub fn asset_summary(_params: &ParameterSet) -> Result<String> {
    Ok("native='ibanez_ts9' oracle='nam'".to_string())
}

pub fn build_processor_for_layout(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let settings = read_settings(params)?;
    Ok(match layout {
        AudioChannelLayout::Mono => {
            BlockProcessor::Mono(Box::new(Ts9Processor::new(settings, sample_rate)))
        }
        AudioChannelLayout::Stereo => BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: Ts9Processor::new(settings, sample_rate),
            right: Ts9Processor::new(settings, sample_rate),
        })),
    })
}

fn read_settings(params: &ParameterSet) -> Result<Ts9Settings> {
    Ok(Ts9Settings {
        drive: required_f32(params, "drive").map_err(anyhow::Error::msg)?,
        tone: required_f32(params, "tone").map_err(anyhow::Error::msg)?,
        level: required_f32(params, "level").map_err(anyhow::Error::msg)?,
    })
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_processor_for_layout(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Native,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
