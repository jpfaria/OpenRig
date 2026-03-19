use domain::ids::{BlockId, ParameterId, SetupId};
use serde::{Deserialize, Serialize};

use crate::block::{AudioBlock, BlockAudioDescriptor};
use crate::device::{InputDevice, OutputDevice};
use crate::io::{Input, Output};
use crate::param::BlockParameterDescriptor;
use crate::track::Track;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setup {
    pub id: SetupId,
    pub name: String,
    pub input_devices: Vec<InputDevice>,
    pub output_devices: Vec<OutputDevice>,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub tracks: Vec<Track>,
}

impl Setup {
    pub fn parameter_descriptors(&self) -> Result<Vec<BlockParameterDescriptor>, String> {
        let mut descriptors = Vec::new();
        for track in &self.tracks {
            if !track.enabled {
                continue;
            }
            for block in &track.blocks {
                descriptors.extend(collect_block_parameter_descriptors(block)?);
            }
        }
        Ok(descriptors)
    }

    pub fn find_parameter_descriptor(
        &self,
        parameter_id: &ParameterId,
    ) -> Result<Option<BlockParameterDescriptor>, String> {
        Ok(self
            .parameter_descriptors()?
            .into_iter()
            .find(|descriptor| descriptor.id == *parameter_id))
    }

    pub fn block_audio_descriptors(&self) -> Result<Vec<BlockAudioDescriptor>, String> {
        let mut descriptors = Vec::new();
        for track in &self.tracks {
            if !track.enabled {
                continue;
            }
            for block in &track.blocks {
                descriptors.extend(block.audio_descriptors()?);
            }
        }
        Ok(descriptors)
    }

    pub fn find_block(&self, block_id: &BlockId) -> Option<&AudioBlock> {
        self.tracks
            .iter()
            .flat_map(|track| track.blocks.iter())
            .find_map(|block| find_block_recursive(block, block_id))
    }
}

fn collect_block_parameter_descriptors(
    block: &AudioBlock,
) -> Result<Vec<BlockParameterDescriptor>, String> {
    block.parameter_descriptors()
}

fn find_block_recursive<'a>(block: &'a AudioBlock, block_id: &BlockId) -> Option<&'a AudioBlock> {
    if block.id == *block_id {
        return Some(block);
    }
    if let crate::block::AudioBlockKind::Select(select) = &block.kind {
        for option in &select.options {
            if let Some(found) = find_block_recursive(option, block_id) {
                return Some(found);
            }
        }
    }
    None
}
