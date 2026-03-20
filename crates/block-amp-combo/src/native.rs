use anyhow::{bail, Result};
use block_amp_head::native::{
    build_native_head_mono_processor, NativeAmpHeadSettings, NativeAmpHeadVoice,
};
use block_cab::native::{build_native_cab_mono_processor, NativeCabSettings, NativeCabVoice};
use block_core::param::{
    bool_parameter, float_parameter, required_bool, required_f32, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{
    AudioChannelLayout, ModelAudioMode, MonoProcessor, BlockProcessor, StereoProcessor,
};

pub const BLACKFACE_CLEAN_COMBO_ID: &str = "blackface_clean_combo";
pub const TWEED_BREAKUP_COMBO_ID: &str = "tweed_breakup_combo";
pub const CHIME_COMBO_ID: &str = "chime_combo";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeAmpComboVoice {
    BlackfaceClean,
    TweedBreakup,
    Chime,
}

#[derive(Debug, Clone, Copy)]
pub struct NativeAmpComboSettings {
    pub input_db: f32,
    pub gain: f32,
    pub bass: f32,
    pub middle: f32,
    pub treble: f32,
    pub master: f32,
    pub bright: bool,
    pub sag: f32,
    pub room_mix: f32,
    pub output_db: f32,
}

#[derive(Debug, Clone, Copy)]
struct ComboProfile {
    model_id: &'static str,
    display_name: &'static str,
    head_voice: NativeAmpHeadVoice,
    cab_voice: NativeCabVoice,
    fixed_presence: f32,
    fixed_depth: f32,
    cab_low_cut_hz: f32,
    cab_high_cut_hz: f32,
    cab_resonance: f32,
    cab_air: f32,
    cab_mic_position: f32,
    cab_mic_distance: f32,
    gain_bias: f32,
}

struct DualMonoProcessor {
    left: Box<dyn MonoProcessor>,
    right: Box<dyn MonoProcessor>,
}

struct NativeAmpComboProcessor {
    head: Box<dyn MonoProcessor>,
    cab: Box<dyn MonoProcessor>,
}

impl NativeAmpComboVoice {
    fn profile(self) -> ComboProfile {
        match self {
            Self::BlackfaceClean => ComboProfile {
                model_id: BLACKFACE_CLEAN_COMBO_ID,
                display_name: "Blackface Clean Combo",
                head_voice: NativeAmpHeadVoice::AmericanClean,
                cab_voice: NativeCabVoice::American2x12,
                fixed_presence: 58.0,
                fixed_depth: 34.0,
                cab_low_cut_hz: 66.0,
                cab_high_cut_hz: 8_200.0,
                cab_resonance: 48.0,
                cab_air: 30.0,
                cab_mic_position: 58.0,
                cab_mic_distance: 22.0,
                gain_bias: -8.0,
            },
            Self::TweedBreakup => ComboProfile {
                model_id: TWEED_BREAKUP_COMBO_ID,
                display_name: "Tweed Breakup Combo",
                head_voice: NativeAmpHeadVoice::BritCrunch,
                cab_voice: NativeCabVoice::Vintage1x12,
                fixed_presence: 42.0,
                fixed_depth: 30.0,
                cab_low_cut_hz: 92.0,
                cab_high_cut_hz: 5_900.0,
                cab_resonance: 57.0,
                cab_air: 18.0,
                cab_mic_position: 42.0,
                cab_mic_distance: 18.0,
                gain_bias: -15.0,
            },
            Self::Chime => ComboProfile {
                model_id: CHIME_COMBO_ID,
                display_name: "Chime Combo",
                head_voice: NativeAmpHeadVoice::AmericanClean,
                cab_voice: NativeCabVoice::Brit4x12,
                fixed_presence: 64.0,
                fixed_depth: 28.0,
                cab_low_cut_hz: 78.0,
                cab_high_cut_hz: 8_800.0,
                cab_resonance: 44.0,
                cab_air: 36.0,
                cab_mic_position: 68.0,
                cab_mic_distance: 20.0,
                gain_bias: -10.0,
            },
        }
    }
}

impl MonoProcessor for NativeAmpComboProcessor {
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

pub fn supports_model(model: &str) -> bool {
    matches!(
        model,
        BLACKFACE_CLEAN_COMBO_ID | TWEED_BREAKUP_COMBO_ID | CHIME_COMBO_ID
    )
}

pub fn model_schema(model: &str) -> Result<ModelParameterSchema> {
    let voice = resolve_voice(model)?;
    let profile = voice.profile();

    Ok(ModelParameterSchema {
        effect_type: "amp_combo".into(),
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
                    NativeAmpComboVoice::BlackfaceClean => 32.0,
                    NativeAmpComboVoice::TweedBreakup => 54.0,
                    NativeAmpComboVoice::Chime => 38.0,
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
                Some(match voice {
                    NativeAmpComboVoice::Chime => 58.0,
                    _ => 50.0,
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
            bool_parameter(
                "bright",
                "Bright",
                Some("Switches"),
                Some(matches!(
                    voice,
                    NativeAmpComboVoice::BlackfaceClean | NativeAmpComboVoice::Chime
                )),
            ),
            float_parameter(
                "sag",
                "Sag",
                Some("Power"),
                Some(match voice {
                    NativeAmpComboVoice::BlackfaceClean => 14.0,
                    NativeAmpComboVoice::TweedBreakup => 34.0,
                    NativeAmpComboVoice::Chime => 18.0,
                }),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "room_mix",
                "Room Mix",
                Some("Cab"),
                Some(14.0),
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
    })
}

pub fn settings_from_params(params: &ParameterSet) -> Result<NativeAmpComboSettings> {
    Ok(NativeAmpComboSettings {
        input_db: required_f32(params, "input_db").map_err(anyhow::Error::msg)?,
        gain: required_f32(params, "gain").map_err(anyhow::Error::msg)?,
        bass: required_f32(params, "bass").map_err(anyhow::Error::msg)?,
        middle: required_f32(params, "middle").map_err(anyhow::Error::msg)?,
        treble: required_f32(params, "treble").map_err(anyhow::Error::msg)?,
        master: required_f32(params, "master").map_err(anyhow::Error::msg)?,
        bright: required_bool(params, "bright").map_err(anyhow::Error::msg)?,
        sag: required_f32(params, "sag").map_err(anyhow::Error::msg)?,
        room_mix: required_f32(params, "room_mix").map_err(anyhow::Error::msg)?,
        output_db: required_f32(params, "output_db").map_err(anyhow::Error::msg)?,
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
) -> Result<BlockProcessor> {
    let voice = resolve_voice(model)?;
    let settings = settings_from_params(params)?;

    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(build_native_combo_mono_processor(
            voice,
            settings,
            sample_rate,
        ))),
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(DualMonoProcessor {
            left: build_native_combo_mono_processor(voice, settings, sample_rate),
            right: build_native_combo_mono_processor(voice, settings, sample_rate),
        }))),
    }
}

fn build_native_combo_mono_processor(
    voice: NativeAmpComboVoice,
    settings: NativeAmpComboSettings,
    sample_rate: f32,
) -> Box<dyn MonoProcessor> {
    let profile = voice.profile();
    let head = build_native_head_mono_processor(
        profile.head_voice,
        NativeAmpHeadSettings {
            input_db: settings.input_db,
            gain: (settings.gain + profile.gain_bias).clamp(0.0, 100.0),
            bass: settings.bass,
            middle: settings.middle,
            treble: settings.treble,
            presence: profile.fixed_presence,
            depth: profile.fixed_depth,
            master: settings.master,
            output_db: 0.0,
            bright: settings.bright,
            sag: settings.sag,
        },
        sample_rate,
    );

    let cab = build_native_cab_mono_processor(
        profile.cab_voice,
        NativeCabSettings {
            low_cut_hz: profile.cab_low_cut_hz,
            high_cut_hz: profile.cab_high_cut_hz,
            resonance: profile.cab_resonance,
            air: profile.cab_air,
            mic_position: profile.cab_mic_position,
            mic_distance: profile.cab_mic_distance,
            room_mix: settings.room_mix,
            output_db: settings.output_db,
        },
        sample_rate,
    );

    Box::new(NativeAmpComboProcessor { head, cab })
}

fn resolve_voice(model: &str) -> Result<NativeAmpComboVoice> {
    match model {
        BLACKFACE_CLEAN_COMBO_ID => Ok(NativeAmpComboVoice::BlackfaceClean),
        TWEED_BREAKUP_COMBO_ID => Ok(NativeAmpComboVoice::TweedBreakup),
        CHIME_COMBO_ID => Ok(NativeAmpComboVoice::Chime),
        _ => bail!("unsupported native amp-combo model '{}'", model),
    }
}
