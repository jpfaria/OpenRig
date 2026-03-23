use anyhow::{anyhow, bail, Result};
use asset_runtime::{materialize, EmbeddedAsset};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "murphy_custom_k_k";
pub const DISPLAY_NAME: &str = "Murphy Custom K&K";
const BRAND: &str = "";

macro_rules! capture {
    ($flavor:literal, $asset_id:literal, $relative_path:literal) => {
        MurphyCustomKKCapture {
            flavor: $flavor,
            asset: EmbeddedAsset::new(
                $asset_id,
                $relative_path,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../",
                    $relative_path
                )),
            ),
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MurphyCustomKKCapture {
    pub flavor: &'static str,
    pub asset: EmbeddedAsset,
}

pub const CAPTURES: &[MurphyCustomKKCapture] = &[
    capture!("murphy_custom_44100", "body.murphy_custom_k_k.murphy_custom_44100", "captures/ir/body/murphy_custom_k_k/murphy_custom_44100.wav"),
    capture!("murphy_custom_44100b", "body.murphy_custom_k_k.murphy_custom_44100b", "captures/ir/body/murphy_custom_k_k/murphy_custom_44100b.wav"),
    capture!("murphy_custom_44100_match1", "body.murphy_custom_k_k.murphy_custom_44100_match1", "captures/ir/body/murphy_custom_k_k/murphy_custom_44100_match1.wav"),
    capture!("murphy_custom_44100b_match1", "body.murphy_custom_k_k.murphy_custom_44100b_match1", "captures/ir/body/murphy_custom_k_k/murphy_custom_44100b_match1.wav"),
];

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_BODY.to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![enum_parameter(
            "flavor",
            "Flavor",
            Some("Body"),
            Some("murphy_custom_44100"),
            &[
                ("murphy_custom_44100", "Murphy Custom 44100"),
                ("murphy_custom_44100b", "Murphy Custom 44100B"),
                ("murphy_custom_44100_match1", "Murphy Custom 44100 Match 1"),
                ("murphy_custom_44100b_match1", "Murphy Custom 44100B Match 1"),
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
            let materialized_path = materialize(&capture.asset)?;
            let materialized_path_str = materialized_path.to_string_lossy();
            let ir = IrAsset::load_from_wav(&materialized_path_str)?;
            if ir.channel_count() != 1 {
                bail!(
                    "body model '{}' capture must be mono, got {} channels",
                    MODEL_ID,
                    ir.channel_count()
                );
            }
            let processor = build_mono_ir_processor_from_wav(&materialized_path_str, sample_rate)?;
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
    Ok(format!("asset_id='{}'", capture.asset.id))
}

fn resolve_capture(params: &ParameterSet) -> Result<&'static MurphyCustomKKCapture> {
    let requested = required_string(params, "flavor").map_err(anyhow::Error::msg)?;
    CAPTURES
        .iter()
        .find(|c| c.flavor == requested)
        .ok_or_else(|| {
            anyhow!(
                "body model '{}' does not support flavor '{}'",
                MODEL_ID,
                requested
            )
        })
}
