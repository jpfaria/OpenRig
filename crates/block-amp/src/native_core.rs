use anyhow::Result;
use block_preamp::native_core::{
    build_native_head_mono_processor, NativeAmpHeadProfile, NativeAmpHeadSettings,
};
use block_cab::native_core::{build_native_cab_mono_processor, NativeCabProfile, NativeCabSettings};
use block_core::param::{
    bool_parameter, float_parameter, required_bool, required_f32, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{
    AudioChannelLayout, ModelAudioMode, MonoProcessor, BlockProcessor, StereoProcessor,
};

#[derive(Debug, Clone, Copy)]
pub struct NativeAmpSettings {
    pub input: f32,
    pub gain: f32,
    pub bass: f32,
    pub middle: f32,
    pub treble: f32,
    pub master: f32,
    pub bright: bool,
    pub sag: f32,
    pub room_mix: f32,
    pub output: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct NativeAmpProfile {
    pub head_profile: NativeAmpHeadProfile,
    pub cab_profile: NativeCabProfile,
    pub fixed_presence: f32,
    pub fixed_depth: f32,
    pub cab_low_cut_hz: f32,
    pub cab_high_cut_hz: f32,
    pub cab_resonance: f32,
    pub cab_air: f32,
    pub cab_mic_position: f32,
    pub cab_mic_distance: f32,
    pub gain_bias: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct NativeAmpSchemaDefaults {
    pub gain: f32,
    pub treble: f32,
    pub bright: bool,
    pub sag: f32,
    pub room_mix: f32,
}

struct DualMonoProcessor {
    left: Box<dyn MonoProcessor>,
    right: Box<dyn MonoProcessor>,
}

struct NativeAmpProcessor {
    head: Box<dyn MonoProcessor>,
    cab: Box<dyn MonoProcessor>,
}

impl MonoProcessor for NativeAmpProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        let sample = self.head.process_sample(input);
        self.cab.process_sample(sample)
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

pub fn model_schema(
    model_id: &'static str,
    display_name: &'static str,
    defaults: NativeAmpSchemaDefaults,
) -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "amp".into(),
        model: model_id.into(),
        display_name: display_name.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "input",
                "Input",
                Some("Input"),
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "gain",
                "Gain",
                Some("Amp"),
                Some(defaults.gain),
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
                Some(defaults.treble),
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
            bool_parameter(
                "bright",
                "Bright",
                Some("Switches"),
                Some(defaults.bright),
            ),
            float_parameter(
                "sag",
                "Sag",
                Some("Power"),
                Some(defaults.sag),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "room_mix",
                "Room Mix",
                Some("Cab"),
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

pub fn settings_from_params(params: &ParameterSet) -> Result<NativeAmpSettings> {
    Ok(NativeAmpSettings {
        input: required_f32(params, "input").map_err(anyhow::Error::msg)?,
        gain: required_f32(params, "gain").map_err(anyhow::Error::msg)?,
        bass: required_f32(params, "bass").map_err(anyhow::Error::msg)?,
        middle: required_f32(params, "middle").map_err(anyhow::Error::msg)?,
        treble: required_f32(params, "treble").map_err(anyhow::Error::msg)?,
        master: required_f32(params, "master").map_err(anyhow::Error::msg)?,
        bright: required_bool(params, "bright").map_err(anyhow::Error::msg)?,
        sag: required_f32(params, "sag").map_err(anyhow::Error::msg)?,
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
    profile: NativeAmpProfile,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let settings = settings_from_params(params)?;

    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(build_native_combo_mono_processor(
            profile,
            settings,
            sample_rate,
        ))),
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: build_native_combo_mono_processor(profile, settings, sample_rate),
            right: build_native_combo_mono_processor(profile, settings, sample_rate),
        }))),
    }
}

fn build_native_combo_mono_processor(
    profile: NativeAmpProfile,
    settings: NativeAmpSettings,
    sample_rate: f32,
) -> Box<dyn MonoProcessor> {
    fn percent_to_gain_db(p: f32) -> f32 {
        -18.0 + (p / 100.0) * 36.0
    }

    let head = build_native_head_mono_processor(
        profile.head_profile,
        NativeAmpHeadSettings {
            input: settings.input,
            gain: (settings.gain + profile.gain_bias).clamp(0.0, 100.0),
            bass: settings.bass,
            middle: settings.middle,
            treble: settings.treble,
            presence: profile.fixed_presence,
            depth: profile.fixed_depth,
            master: settings.master,
            output: 50.0, // unity (0 dB) for head; cab handles output
            bright: settings.bright,
            sag: settings.sag,
        },
        sample_rate,
    );

    let cab = build_native_cab_mono_processor(
        profile.cab_profile,
        NativeCabSettings {
            low_cut_hz: profile.cab_low_cut_hz,
            high_cut_hz: profile.cab_high_cut_hz,
            resonance: profile.cab_resonance,
            air: profile.cab_air,
            mic_position: profile.cab_mic_position,
            mic_distance: profile.cab_mic_distance,
            room_mix: settings.room_mix,
            output: settings.output,
        },
        sample_rate,
    );

    Box::new(NativeAmpProcessor { head, cab })
}
