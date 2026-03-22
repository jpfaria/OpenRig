use anyhow::{Error, Result};
use block_core::param::{
    file_path_parameter, required_string, ModelParameterSchema, ParameterSet,
};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor,
};
use ir::{
    build_mono_ir_processor_from_wav, build_stereo_ir_processor_from_wav, IrAsset, IrChannelData,
};

use crate::registry::IrModelDefinition;
use crate::IrBlockBackendKind;

pub const MODEL_ID: &str = "generic_ir";
pub const DISPLAY_NAME: &str = "Impulse Response";

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

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: "ir".to_string(),
        model: MODEL_ID.to_string(),
        display_name: "Impulse Response".to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![file_path_parameter(
            "file",
            "IR File",
            None,
            None,
            &["wav"],
            false,
        )],
    })
}

fn validate(params: &ParameterSet) -> Result<()> {
    let file = required_string(params, "file").map_err(Error::msg)?;
    let _ = IrAsset::load_from_wav(&file)?;
    Ok(())
}

fn asset_summary(params: &ParameterSet) -> Result<String> {
    let file = required_string(params, "file").map_err(Error::msg)?;
    Ok(format!("file='{file}'"))
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let file = required_string(params, "file").map_err(Error::msg)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(
            build_mono_ir_processor_from_wav(&file, sample_rate)?,
        )),
        AudioChannelLayout::Stereo => {
            let asset = IrAsset::load_from_wav(&file)?;
            match asset.channel_data() {
                IrChannelData::Mono(_) => Ok(BlockProcessor::Stereo(Box::new(DualMonoProcessor {
                    left: build_mono_ir_processor_from_wav(&file, sample_rate)?,
                    right: build_mono_ir_processor_from_wav(&file, sample_rate)?,
                }))),
                IrChannelData::Stereo(_, _) => Ok(BlockProcessor::Stereo(
                    build_stereo_ir_processor_from_wav(&file, sample_rate)?,
                )),
            }
        }
    }
}

pub const MODEL_DEFINITION: IrModelDefinition = IrModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: "",
    backend_kind: IrBlockBackendKind::Native,
    schema,
    validate,
    asset_summary,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
};
