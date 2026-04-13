use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "martin_gpc_special_16";
pub const DISPLAY_NAME: &str = "GPC Special 16";
const BRAND: &str = "martin";

macro_rules! capture {
    ($voicing:literal, $ir_file:literal) => {
        Capture {
            voicing: $voicing,
            ir_file: $ir_file,
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capture {
    pub voicing: &'static str,
    pub ir_file: &'static str,
}

pub const CAPTURES: &[Capture] = &[
    capture!("standard_martin_gpc16e_special_rosewood_ir_std", "body/martin_gpc_special_16/standard_martin_gpc16e_special_rosewood_ir_std.wav"),
    capture!("standard_martin_gpc16e_special_rosewood_ir_std_44100_16_1", "body/martin_gpc_special_16/standard_martin_gpc16e_special_rosewood_ir_std_44100_16_1.wav"),
    capture!("ir_itp_mph_44100_16_1", "body/martin_gpc_special_16/ir_itp_mph_44100_16_1.wav"),
    capture!("ir_itp_mph", "body/martin_gpc_special_16/ir_itp_mph.wav"),
    capture!("ir_itp_raw_44100_16_1", "body/martin_gpc_special_16/ir_itp_raw_44100_16_1.wav"),
    capture!("ir_itp_raw", "body/martin_gpc_special_16/ir_itp_raw.wav"),
    capture!("ir_itp_bld_44100_16_1", "body/martin_gpc_special_16/ir_itp_bld_44100_16_1.wav"),
    capture!("ir_itp_bld", "body/martin_gpc_special_16/ir_itp_bld.wav"),
    capture!("ir_itp_44100_16_1", "body/martin_gpc_special_16/ir_itp_44100_16_1.wav"),
    capture!("ir_itp", "body/martin_gpc_special_16/ir_itp.wav"),
    capture!("ir_std_44100_16_1", "body/martin_gpc_special_16/ir_std_44100_16_1.wav"),
    capture!("ir_std", "body/martin_gpc_special_16/ir_std.wav"),
    capture!("jf45_ir_std", "body/martin_gpc_special_16/jf45_ir_std.wav"),
    capture!("jf45_ir_std_44100_16_1", "body/martin_gpc_special_16/jf45_ir_std_44100_16_1.wav"),
    capture!("jf45_ir_itp_44100_16_1", "body/martin_gpc_special_16/jf45_ir_itp_44100_16_1.wav"),
    capture!("jf45_ir_itp", "body/martin_gpc_special_16/jf45_ir_itp.wav"),
    capture!("jf45_ir_itp_jf45_44100_16_1", "body/martin_gpc_special_16/jf45_ir_itp_jf45_44100_16_1.wav"),
    capture!("jf45_ir_itp_jf45", "body/martin_gpc_special_16/jf45_ir_itp_jf45.wav"),
    capture!("fbf_martin_gpc16e_special_rosewood_ir_std_44100_16_1", "body/martin_gpc_special_16/fbf_martin_gpc16e_special_rosewood_ir_std_44100_16_1.wav"),
    capture!("fbf_martin_gpc16e_special_rosewood_ir_std", "body/martin_gpc_special_16/fbf_martin_gpc16e_special_rosewood_ir_std.wav"),
    capture!("fbf_gpc16e_ir_std", "body/martin_gpc_special_16/fbf_gpc16e_ir_std.wav"),
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
            Some("ir_std"),
            &[
                ("standard_martin_gpc16e_special_rosewood_ir_std", "Standard GPC16e Rosewood"),
                ("standard_martin_gpc16e_special_rosewood_ir_std_44100_16_1", "Standard GPC16e Rosewood 44100"),
                ("ir_itp_mph_44100_16_1", "ITP MPH 44100"),
                ("ir_itp_mph", "ITP MPH"),
                ("ir_itp_raw_44100_16_1", "ITP Raw 44100"),
                ("ir_itp_raw", "ITP Raw"),
                ("ir_itp_bld_44100_16_1", "ITP Blend 44100"),
                ("ir_itp_bld", "ITP Blend"),
                ("ir_itp_44100_16_1", "ITP 44100"),
                ("ir_itp", "ITP"),
                ("ir_std_44100_16_1", "Standard 44100"),
                ("ir_std", "Standard"),
                ("jf45_ir_std", "JF45 Standard"),
                ("jf45_ir_std_44100_16_1", "JF45 Standard 44100"),
                ("jf45_ir_itp_44100_16_1", "JF45 ITP 44100"),
                ("jf45_ir_itp", "JF45 ITP"),
                ("jf45_ir_itp_jf45_44100_16_1", "JF45 ITP JF45 44100"),
                ("jf45_ir_itp_jf45", "JF45 ITP JF45"),
                ("fbf_martin_gpc16e_special_rosewood_ir_std_44100_16_1", "FBF GPC16e Rosewood 44100"),
                ("fbf_martin_gpc16e_special_rosewood_ir_std", "FBF GPC16e Rosewood"),
                ("fbf_gpc16e_ir_std", "FBF GPC16e Standard"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static Capture> {
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
    fn builds_mono_processor() {
        let mut params = ParameterSet::default();
        params.insert("voicing", ParameterValue::String("ir_std".into()));
        let processor = build_processor_for_model(&params, 48_000.0, AudioChannelLayout::Mono)
            .expect("body processor should build");
        match processor {
            BlockProcessor::Mono(_) => {}
            BlockProcessor::Stereo(_) => panic!("expected mono processor"),
        }
        let summary = asset_summary(&params).expect("asset summary should resolve");
        assert!(summary.contains("body.martin_gpc_special_16.ir_std"));
    }
}
