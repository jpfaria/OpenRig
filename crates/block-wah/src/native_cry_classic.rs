use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor,
};
use std::f32::consts::PI;

use crate::registry::WahModelDefinition;
use crate::WahBackendKind;

pub const MODEL_ID: &str = "cry_classic";
pub const DISPLAY_NAME: &str = "Cry Classic";

#[derive(Clone, Copy)]
struct WahSettings {
    position: f32,
    q: f32,
    mix: f32,
    output_db: f32,
}

struct WahProcessor {
    sample_rate: f32,
    a0: f32,
    a1: f32,
    a2: f32,
    b1: f32,
    b2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    mix: f32,
    output_gain: f32,
}

struct DualMonoProcessor {
    left: Box<dyn MonoProcessor>,
    right: Box<dyn MonoProcessor>,
}

impl StereoProcessor for DualMonoProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }
}

impl WahProcessor {
    fn new(settings: WahSettings, sample_rate: f32) -> Self {
        let mut processor = Self {
            sample_rate,
            a0: 0.0,
            a1: 0.0,
            a2: 0.0,
            b1: 0.0,
            b2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
            mix: settings.mix.clamp(0.0, 1.0),
            output_gain: block_core::db_to_lin(settings.output_db),
        };
        processor.update_coefficients(settings.position, settings.q);
        processor
    }

    fn update_coefficients(&mut self, position: f32, q: f32) {
        let min_hz = 350.0;
        let max_hz = 2200.0;
        let center_hz = min_hz + position.clamp(0.0, 1.0) * (max_hz - min_hz);
        let q = q.clamp(0.2, 12.0);
        let omega = 2.0 * PI * center_hz / self.sample_rate.max(1.0);
        let alpha = omega.sin() / (2.0 * q);
        let cos_omega = omega.cos();
        let b0 = alpha;
        let b1 = 0.0;
        let b2 = -alpha;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_omega;
        let a2 = 1.0 - alpha;

        self.a0 = b0 / a0;
        self.a1 = b1 / a0;
        self.a2 = b2 / a0;
        self.b1 = a1 / a0;
        self.b2 = a2 / a0;
    }
}

impl MonoProcessor for WahProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let wet = self.a0 * input + self.a1 * self.x1 + self.a2 * self.x2
            - self.b1 * self.y1
            - self.b2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = wet;
        let mixed = (1.0 - self.mix) * input + self.mix * wet;
        mixed * self.output_gain
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: "wah".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Cry Classic".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "position",
                "Position",
                Some("Wah"),
                Some(0.55),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "q",
                "Q",
                Some("Wah"),
                Some(1.8),
                0.2,
                12.0,
                0.1,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                Some("Output"),
                Some(1.0),
                0.0,
                1.0,
                0.01,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "output_db",
                "Output",
                Some("Output"),
                Some(0.0),
                -24.0,
                24.0,
                0.1,
                ParameterUnit::Decibels,
            ),
        ],
    })
}

fn settings_from_params(params: &ParameterSet) -> Result<WahSettings> {
    Ok(WahSettings {
        position: required_f32(params, "position").map_err(Error::msg)?,
        q: required_f32(params, "q").map_err(Error::msg)?,
        mix: required_f32(params, "mix").map_err(Error::msg)?,
        output_db: required_f32(params, "output_db").map_err(Error::msg)?,
    })
}

fn validate(params: &ParameterSet) -> Result<()> {
    let _ = settings_from_params(params)?;
    Ok(())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let settings = settings_from_params(params)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(WahProcessor::new(
            settings,
            sample_rate,
        )))),
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: Box::new(WahProcessor::new(settings, sample_rate)),
            right: Box::new(WahProcessor::new(settings, sample_rate)),
        }))),
    }
}

pub const MODEL_DEFINITION: WahModelDefinition = WahModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: WahBackendKind::Native,
    schema,
    validate,
    build,
};

