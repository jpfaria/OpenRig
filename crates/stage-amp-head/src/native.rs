use anyhow::{bail, Result};
use stage_core::param::{
    bool_parameter, float_parameter, required_bool, required_f32, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use stage_core::{
    db_to_lin, AudioChannelLayout, EnvelopeFollower, ModelAudioMode, MonoProcessor,
    OnePoleHighPass, OnePoleLowPass, StageProcessor, StereoProcessor,
};

pub const BRIT_CRUNCH_HEAD_ID: &str = "brit_crunch_head";
pub const AMERICAN_CLEAN_HEAD_ID: &str = "american_clean_head";
pub const MODERN_HIGH_GAIN_HEAD_ID: &str = "modern_high_gain_head";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeAmpHeadVoice {
    BritCrunch,
    AmericanClean,
    ModernHighGain,
}

#[derive(Debug, Clone, Copy)]
pub struct NativeAmpHeadSettings {
    pub input_db: f32,
    pub gain: f32,
    pub bass: f32,
    pub middle: f32,
    pub treble: f32,
    pub presence: f32,
    pub depth: f32,
    pub master: f32,
    pub output_db: f32,
    pub bright: bool,
    pub sag: f32,
}

#[derive(Debug, Clone, Copy)]
struct VoiceProfile {
    model_id: &'static str,
    display_name: &'static str,
    input_trim_db: f32,
    drive_scale: f32,
    asymmetry: f32,
    bright_mix: f32,
    low_voice: f32,
    mid_voice: f32,
    high_voice: f32,
    presence_voice: f32,
    depth_voice: f32,
    power_drive: f32,
    low_cut_hz: f32,
    top_end_hz: f32,
}

struct DualMonoProcessor {
    left: Box<dyn MonoProcessor>,
    right: Box<dyn MonoProcessor>,
}

struct NativeAmpHeadProcessor {
    profile: VoiceProfile,
    settings: NativeAmpHeadSettings,
    input_gain: f32,
    output_gain: f32,
    pre_high_pass: OnePoleHighPass,
    bright_high_pass: OnePoleHighPass,
    tone_low: OnePoleLowPass,
    tone_high: OnePoleHighPass,
    presence_high: OnePoleHighPass,
    depth_low: OnePoleLowPass,
    post_low_pass: OnePoleLowPass,
    sag_envelope: EnvelopeFollower,
}

impl NativeAmpHeadVoice {
    fn profile(self) -> VoiceProfile {
        match self {
            Self::BritCrunch => VoiceProfile {
                model_id: BRIT_CRUNCH_HEAD_ID,
                display_name: "Brit Crunch Head",
                input_trim_db: 1.5,
                drive_scale: 2.8,
                asymmetry: 0.12,
                bright_mix: 0.12,
                low_voice: 0.92,
                mid_voice: 1.15,
                high_voice: 0.95,
                presence_voice: 0.55,
                depth_voice: 0.38,
                power_drive: 1.35,
                low_cut_hz: 48.0,
                top_end_hz: 8_400.0,
            },
            Self::AmericanClean => VoiceProfile {
                model_id: AMERICAN_CLEAN_HEAD_ID,
                display_name: "American Clean Head",
                input_trim_db: 3.0,
                drive_scale: 1.75,
                asymmetry: 0.04,
                bright_mix: 0.22,
                low_voice: 1.05,
                mid_voice: 0.88,
                high_voice: 1.12,
                presence_voice: 0.44,
                depth_voice: 0.33,
                power_drive: 0.95,
                low_cut_hz: 36.0,
                top_end_hz: 10_500.0,
            },
            Self::ModernHighGain => VoiceProfile {
                model_id: MODERN_HIGH_GAIN_HEAD_ID,
                display_name: "Modern High Gain Head",
                input_trim_db: -1.0,
                drive_scale: 4.1,
                asymmetry: 0.18,
                bright_mix: 0.08,
                low_voice: 0.82,
                mid_voice: 0.92,
                high_voice: 1.02,
                presence_voice: 0.62,
                depth_voice: 0.58,
                power_drive: 1.55,
                low_cut_hz: 72.0,
                top_end_hz: 7_600.0,
            },
        }
    }
}

impl StereoProcessor for DualMonoProcessor {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }
}

impl NativeAmpHeadProcessor {
    fn new(
        profile: VoiceProfile,
        settings: NativeAmpHeadSettings,
        sample_rate: f32,
    ) -> NativeAmpHeadProcessor {
        NativeAmpHeadProcessor {
            profile,
            settings,
            input_gain: db_to_lin(settings.input_db + profile.input_trim_db),
            output_gain: db_to_lin(settings.output_db),
            pre_high_pass: OnePoleHighPass::new(profile.low_cut_hz, sample_rate),
            bright_high_pass: OnePoleHighPass::new(1_500.0, sample_rate),
            tone_low: OnePoleLowPass::new(260.0, sample_rate),
            tone_high: OnePoleHighPass::new(1_650.0, sample_rate),
            presence_high: OnePoleHighPass::new(2_900.0, sample_rate),
            depth_low: OnePoleLowPass::new(115.0, sample_rate),
            post_low_pass: OnePoleLowPass::new(profile.top_end_hz, sample_rate),
            sag_envelope: EnvelopeFollower::from_ms(4.0, 110.0, sample_rate),
        }
    }

    fn drive_stage(input: f32, drive: f32, asymmetry: f32) -> f32 {
        let biased = input * drive;
        let shaped = biased + asymmetry * biased * biased.signum();
        shaped.tanh()
    }

    fn normalized_percent(value: f32) -> f32 {
        (value / 100.0).clamp(0.0, 1.0)
    }
}

impl MonoProcessor for NativeAmpHeadProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let mut sample = self.pre_high_pass.process(input * self.input_gain);
        if self.settings.bright {
            sample += self.bright_high_pass.process(sample) * self.profile.bright_mix;
        }

        let gain = Self::normalized_percent(self.settings.gain);
        let master = Self::normalized_percent(self.settings.master);
        let sag = Self::normalized_percent(self.settings.sag);
        let envelope = self.sag_envelope.process(sample).min(1.0);
        let dynamic_drive = (1.1 + gain * self.profile.drive_scale) * (1.0 - sag * 0.45 * envelope);

        sample = Self::drive_stage(sample, dynamic_drive, self.profile.asymmetry);
        sample = Self::drive_stage(
            sample,
            1.0 + gain * self.profile.power_drive,
            self.profile.asymmetry * 0.6,
        );

        let low = self.tone_low.process(sample);
        let high = self.tone_high.process(sample);
        let mid = sample - low - high;

        let bass_gain =
            0.45 + Self::normalized_percent(self.settings.bass) * self.profile.low_voice;
        let mid_gain =
            0.40 + Self::normalized_percent(self.settings.middle) * self.profile.mid_voice;
        let treble_gain =
            0.38 + Self::normalized_percent(self.settings.treble) * self.profile.high_voice;

        sample = low * bass_gain + mid * mid_gain + high * treble_gain;

        let presence_push = self.presence_high.process(sample)
            * ((Self::normalized_percent(self.settings.presence) - 0.5)
                * self.profile.presence_voice);
        let depth_push = self.depth_low.process(sample)
            * ((Self::normalized_percent(self.settings.depth) - 0.5) * self.profile.depth_voice);

        sample += presence_push + depth_push;
        sample = Self::drive_stage(sample, 0.8 + master * self.profile.power_drive, 0.04);
        sample = self.post_low_pass.process(sample);
        sample * (0.35 + master * 1.35) * self.output_gain
    }
}

pub fn supports_model(model: &str) -> bool {
    matches!(
        model,
        BRIT_CRUNCH_HEAD_ID | AMERICAN_CLEAN_HEAD_ID | MODERN_HIGH_GAIN_HEAD_ID
    )
}

pub fn model_schema(model: &str) -> Result<ModelParameterSchema> {
    let voice = resolve_voice(model)?;
    let profile = voice.profile();

    Ok(ModelParameterSchema {
        effect_type: "amp".into(),
        model: profile.model_id.into(),
        display_name: profile.display_name.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "input_db",
                "Input",
                Some("Input"),
                Some(0.0),
                -18.0,
                18.0,
                0.5,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "gain",
                "Gain",
                Some("Amp"),
                Some(match voice {
                    NativeAmpHeadVoice::AmericanClean => 34.0,
                    NativeAmpHeadVoice::BritCrunch => 56.0,
                    NativeAmpHeadVoice::ModernHighGain => 72.0,
                }),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "bass",
                "Bass",
                Some("EQ"),
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "middle",
                "Middle",
                Some("EQ"),
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "treble",
                "Treble",
                Some("EQ"),
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "presence",
                "Presence",
                Some("Power"),
                Some(match voice {
                    NativeAmpHeadVoice::AmericanClean => 54.0,
                    NativeAmpHeadVoice::BritCrunch => 58.0,
                    NativeAmpHeadVoice::ModernHighGain => 62.0,
                }),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "depth",
                "Depth",
                Some("Power"),
                Some(match voice {
                    NativeAmpHeadVoice::AmericanClean => 42.0,
                    NativeAmpHeadVoice::BritCrunch => 48.0,
                    NativeAmpHeadVoice::ModernHighGain => 60.0,
                }),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "master",
                "Master",
                Some("Power"),
                Some(62.0),
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
            bool_parameter(
                "bright",
                "Bright",
                Some("Switches"),
                Some(matches!(voice, NativeAmpHeadVoice::AmericanClean)),
            ),
            float_parameter(
                "sag",
                "Sag",
                Some("Power"),
                Some(match voice {
                    NativeAmpHeadVoice::AmericanClean => 16.0,
                    NativeAmpHeadVoice::BritCrunch => 24.0,
                    NativeAmpHeadVoice::ModernHighGain => 30.0,
                }),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    })
}

pub fn settings_from_params(params: &ParameterSet) -> Result<NativeAmpHeadSettings> {
    Ok(NativeAmpHeadSettings {
        input_db: required_f32(params, "input_db").map_err(anyhow::Error::msg)?,
        gain: required_f32(params, "gain").map_err(anyhow::Error::msg)?,
        bass: required_f32(params, "bass").map_err(anyhow::Error::msg)?,
        middle: required_f32(params, "middle").map_err(anyhow::Error::msg)?,
        treble: required_f32(params, "treble").map_err(anyhow::Error::msg)?,
        presence: required_f32(params, "presence").map_err(anyhow::Error::msg)?,
        depth: required_f32(params, "depth").map_err(anyhow::Error::msg)?,
        master: required_f32(params, "master").map_err(anyhow::Error::msg)?,
        output_db: required_f32(params, "output_db").map_err(anyhow::Error::msg)?,
        bright: required_bool(params, "bright").map_err(anyhow::Error::msg)?,
        sag: required_f32(params, "sag").map_err(anyhow::Error::msg)?,
    })
}

pub fn validate_params(model: &str, params: &ParameterSet) -> Result<()> {
    let _ = resolve_voice(model)?;
    let _ = settings_from_params(params)?;
    Ok(())
}

pub fn asset_summary(model: &str, _params: &ParameterSet) -> Result<String> {
    let profile = resolve_voice(model)?.profile();
    Ok(format!("native voice='{}'", profile.model_id))
}

pub fn build_processor_for_model(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    let voice = resolve_voice(model)?;
    let settings = settings_from_params(params)?;

    match layout {
        AudioChannelLayout::Mono => Ok(StageProcessor::Mono(build_native_head_mono_processor(
            voice,
            settings,
            sample_rate,
        ))),
        AudioChannelLayout::Stereo => Ok(StageProcessor::Stereo(Box::new(DualMonoProcessor {
            left: build_native_head_mono_processor(voice, settings, sample_rate),
            right: build_native_head_mono_processor(voice, settings, sample_rate),
        }))),
    }
}

pub fn build_native_head_mono_processor(
    voice: NativeAmpHeadVoice,
    settings: NativeAmpHeadSettings,
    sample_rate: f32,
) -> Box<dyn MonoProcessor> {
    Box::new(NativeAmpHeadProcessor::new(
        voice.profile(),
        settings,
        sample_rate,
    ))
}

fn resolve_voice(model: &str) -> Result<NativeAmpHeadVoice> {
    match model {
        BRIT_CRUNCH_HEAD_ID => Ok(NativeAmpHeadVoice::BritCrunch),
        AMERICAN_CLEAN_HEAD_ID => Ok(NativeAmpHeadVoice::AmericanClean),
        MODERN_HIGH_GAIN_HEAD_ID => Ok(NativeAmpHeadVoice::ModernHighGain),
        _ => bail!("unsupported native amp-head model '{}'", model),
    }
}
