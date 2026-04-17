use anyhow::{anyhow, bail, Result};
use ir::{build_mono_ir_processor_from_wav, IrAsset};
use crate::registry::BodyModelDefinition;
use crate::BodyBackendKind;
use block_core::param::{enum_parameter, required_string, ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, ModelAudioMode, BlockProcessor};

pub const MODEL_ID: &str = "plywood_top_bc_rich_grand_auditorium";
pub const DISPLAY_NAME: &str = "Plywood Top BC Rich Grand Auditorium";
const BRAND: &str = "bc-rich";

macro_rules! capture {
    ($p1:literal, $ir_file:literal) => {
        PlywoodTopBcRichGrandAuditoriumCapture {
            flavor: $p1,
            ir_file: $ir_file,
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlywoodTopBcRichGrandAuditoriumCapture {
    pub flavor: &'static str,
    pub ir_file: &'static str,
}

pub const CAPTURES: &[PlywoodTopBcRichGrandAuditoriumCapture] = &[
    capture!("cukijfirpwfs2048at48", "body/plywood_top_bc_rich_grand_auditorium/cukijfirpwfs2048at48.wav"),
    capture!("jf45ir5050pwfs", "body/plywood_top_bc_rich_grand_auditorium/jf45ir5050pwfs.wav"),
    capture!("jf45ir1024pwfs", "body/plywood_top_bc_rich_grand_auditorium/jf45ir1024pwfs.wav"),
    capture!("jf45irpwfs", "body/plywood_top_bc_rich_grand_auditorium/jf45irpwfs.wav"),
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
            Some("cukijfirpwfs2048at48"),
            &[
                ("cukijfirpwfs2048at48", "CUKI JF IR PWFS 2048 at 48"),
                ("jf45ir5050pwfs", "JF 45 IR 5050 PWFS"),
                ("jf45ir1024pwfs", "JF 45 IR 1024 PWFS"),
                ("jf45irpwfs", "JF 45 IR PWFS"),
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

fn resolve_capture(params: &ParameterSet) -> Result<&'static PlywoodTopBcRichGrandAuditoriumCapture> {
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
