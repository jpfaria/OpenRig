use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "takamine_p2dc";
pub const DISPLAY_NAME: &str = "P2DC";
const BRAND: &str = "takamine";

macro_rules! capture {
    ($p1:literal, $ir_file:literal) => {
        TakamineP2dcCapture {
            voicing: $p1,
            ir_file: $ir_file,
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TakamineP2dcCapture {
    pub voicing: &'static str,
    pub ir_file: &'static str,
}

pub const CAPTURES: &[TakamineP2dcCapture] = &[
    capture!("piezo_taka_pdc_piezo_1a", "body/takamine_p2dc/piezo_taka_pdc_piezo_1a.wav"),
    capture!("piezo_taka_pdc_piezo_1b", "body/takamine_p2dc/piezo_taka_pdc_piezo_1b.wav"),
    capture!("piezo_taka_pdc_piezo_1c", "body/takamine_p2dc/piezo_taka_pdc_piezo_1c.wav"),
    capture!("piezo_taka_pdc_piezo_2a", "body/takamine_p2dc/piezo_taka_pdc_piezo_2a.wav"),
    capture!("piezo_taka_pdc_piezo_2b", "body/takamine_p2dc/piezo_taka_pdc_piezo_2b.wav"),
    capture!("piezo_taka_pdc_piezo_2c", "body/takamine_p2dc/piezo_taka_pdc_piezo_2c.wav"),
    capture!("piezo_taka_pdc_piezo_3a", "body/takamine_p2dc/piezo_taka_pdc_piezo_3a.wav"),
    capture!("piezo_taka_pdc_piezo_3b", "body/takamine_p2dc/piezo_taka_pdc_piezo_3b.wav"),
    capture!("piezo_taka_pdc_piezo_3c", "body/takamine_p2dc/piezo_taka_pdc_piezo_3c.wav"),
    capture!("piezo_taka_pdc_piezo_4a", "body/takamine_p2dc/piezo_taka_pdc_piezo_4a.wav"),
    capture!("piezo_taka_pdc_piezo_4b", "body/takamine_p2dc/piezo_taka_pdc_piezo_4b.wav"),
    capture!("piezo_taka_pdc_piezo_4c", "body/takamine_p2dc/piezo_taka_pdc_piezo_4c.wav"),
    capture!("piezo_taka_pdc_piezo_5a", "body/takamine_p2dc/piezo_taka_pdc_piezo_5a.wav"),
    capture!("piezo_taka_pdc_piezo_5b", "body/takamine_p2dc/piezo_taka_pdc_piezo_5b.wav"),
    capture!("piezo_taka_pdc_piezo_5c", "body/takamine_p2dc/piezo_taka_pdc_piezo_5c.wav"),
    capture!("soundhole_taka_pdc_soundhole_1a", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_1a.wav"),
    capture!("soundhole_taka_pdc_soundhole_1b", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_1b.wav"),
    capture!("soundhole_taka_pdc_soundhole_1c", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_1c.wav"),
    capture!("soundhole_taka_pdc_soundhole_2a", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_2a.wav"),
    capture!("soundhole_taka_pdc_soundhole_2b", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_2b.wav"),
    capture!("soundhole_taka_pdc_soundhole_2c", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_2c.wav"),
    capture!("soundhole_taka_pdc_soundhole_3a", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_3a.wav"),
    capture!("soundhole_taka_pdc_soundhole_3b", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_3b.wav"),
    capture!("soundhole_taka_pdc_soundhole_3c", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_3c.wav"),
    capture!("soundhole_taka_pdc_soundhole_4a", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_4a.wav"),
    capture!("soundhole_taka_pdc_soundhole_4b", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_4b.wav"),
    capture!("soundhole_taka_pdc_soundhole_4c", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_4c.wav"),
    capture!("soundhole_taka_pdc_soundhole_5a", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_5a.wav"),
    capture!("soundhole_taka_pdc_soundhole_5b", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_5b.wav"),
    capture!("soundhole_taka_pdc_soundhole_5c", "body/takamine_p2dc/soundhole_taka_pdc_soundhole_5c.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_BODY.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "voicing",
            "Voicing",
            Some("Body"),
            Some("piezo_taka_pdc_piezo_1a"),
            &[
                ("piezo_taka_pdc_piezo_1a", "Piezo 1A"),
                ("piezo_taka_pdc_piezo_1b", "Piezo 1B"),
                ("piezo_taka_pdc_piezo_1c", "Piezo 1C"),
                ("piezo_taka_pdc_piezo_2a", "Piezo 2A"),
                ("piezo_taka_pdc_piezo_2b", "Piezo 2B"),
                ("piezo_taka_pdc_piezo_2c", "Piezo 2C"),
                ("piezo_taka_pdc_piezo_3a", "Piezo 3A"),
                ("piezo_taka_pdc_piezo_3b", "Piezo 3B"),
                ("piezo_taka_pdc_piezo_3c", "Piezo 3C"),
                ("piezo_taka_pdc_piezo_4a", "Piezo 4A"),
                ("piezo_taka_pdc_piezo_4b", "Piezo 4B"),
                ("piezo_taka_pdc_piezo_4c", "Piezo 4C"),
                ("piezo_taka_pdc_piezo_5a", "Piezo 5A"),
                ("piezo_taka_pdc_piezo_5b", "Piezo 5B"),
                ("piezo_taka_pdc_piezo_5c", "Piezo 5C"),
                ("soundhole_taka_pdc_soundhole_1a", "Soundhole 1A"),
                ("soundhole_taka_pdc_soundhole_1b", "Soundhole 1B"),
                ("soundhole_taka_pdc_soundhole_1c", "Soundhole 1C"),
                ("soundhole_taka_pdc_soundhole_2a", "Soundhole 2A"),
                ("soundhole_taka_pdc_soundhole_2b", "Soundhole 2B"),
                ("soundhole_taka_pdc_soundhole_2c", "Soundhole 2C"),
                ("soundhole_taka_pdc_soundhole_3a", "Soundhole 3A"),
                ("soundhole_taka_pdc_soundhole_3b", "Soundhole 3B"),
                ("soundhole_taka_pdc_soundhole_3c", "Soundhole 3C"),
                ("soundhole_taka_pdc_soundhole_4a", "Soundhole 4A"),
                ("soundhole_taka_pdc_soundhole_4b", "Soundhole 4B"),
                ("soundhole_taka_pdc_soundhole_4c", "Soundhole 4C"),
                ("soundhole_taka_pdc_soundhole_5a", "Soundhole 5A"),
                ("soundhole_taka_pdc_soundhole_5b", "Soundhole 5B"),
                ("soundhole_taka_pdc_soundhole_5c", "Soundhole 5C"),
            ],
        )],
    }
}

pub fn build_processor_for_model(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    match layout {
        AudioChannelLayout::Mono => {
            let capture = resolve_capture(params)?;
            let wav_path = ir::resolve_ir_capture(capture.ir_file)?;
            
            let ir = IrAsset::load_from_wav(&wav_path)?;
            if ir.channel_count() != 1 {
                bail!(
                    "body model '{}' capture must be mono, got {} channels",
                    MODEL_ID,
                    ir.channel_count()
                );
            }
            let processor = build_mono_ir_processor_from_wav(&wav_path, sample_rate)?;
            Ok(BlockProcessor::Mono(processor))
        }
        AudioChannelLayout::Stereo => bail!(
            "body model '{}' currently expects mono processor layout",
            MODEL_ID
        ),
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    build_processor_for_model(params, sample_rate, layout)
}

pub const MODEL_DEFINITION: BodyModelDefinition = BodyModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: BodyBackendKind::Ir,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: &[block_core::INST_ACOUSTIC_GUITAR],
    knob_layout: &[],
};

pub fn validate_params(params: &ParameterSet) -> Result<()> {
    resolve_capture(params).map(|_| ())
}

pub fn asset_summary(params: &ParameterSet) -> Result<String> {
    let capture = resolve_capture(params)?;
    Ok(format!("asset_id='{}'", capture.ir_file))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static TakamineP2dcCapture> {
    let requested = required_string(params, "voicing").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.voicing == requested)
        .ok_or_else(|| {
            anyhow!(
                "body model '{}' does not support voicing '{}'",
                MODEL_ID,
                requested
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{asset_summary, build_processor_for_model, model_schema, validate_params};
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, BlockProcessor};
    use domain::value_objects::ParameterValue;

    #[test]
    fn schema_exposes_voicing_parameter() {
        let schema = model_schema();
        assert_eq!(schema.parameters.len(), 1);
        assert_eq!(schema.parameters[0].path, "voicing");
    }

    #[test]
    fn rejects_unknown_voicing() {
        let mut params = ParameterSet::default();
        params.insert("voicing", ParameterValue::String("unknown".into()));

        let error = validate_params(&params).expect_err("unknown voicing should fail");
        assert!(error.to_string().contains("voicing"));
    }

    #[test]
    #[ignore]
    fn builds_mono_processor() {
        let mut params = ParameterSet::default();
        params.insert("voicing", ParameterValue::String("piezo_taka_pdc_piezo_1a".into()));

        let processor = build_processor_for_model(&params, 48_000.0, AudioChannelLayout::Mono)
            .expect("body processor should build");

        match processor {
            BlockProcessor::Mono(_) => {}
            BlockProcessor::Stereo(_) => panic!("expected mono processor"),
        }

        let summary = asset_summary(&params).expect("asset summary should resolve");
        assert!(summary.contains("body.takamine_p2dc.piezo_taka_pdc_piezo_1a"));
    }
}
