use domain::ids::{BlockId, ParameterId};
use serde::Serialize;

use crate::block::{AudioBlock, BlockAudioDescriptor};
use crate::chain::Chain;
use crate::device::DeviceSettings;
use crate::param::BlockParameterDescriptor;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Project {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub device_settings: Vec<DeviceSettings>,
    pub chains: Vec<Chain>,
}

impl Project {
    pub fn parameter_descriptors(&self) -> Result<Vec<BlockParameterDescriptor>, String> {
        let mut descriptors = Vec::new();
        for chain in &self.chains {
            for block in &chain.blocks {
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
        for chain in &self.chains {
            for block in &chain.blocks {
                descriptors.extend(block.audio_descriptors()?);
            }
        }
        Ok(descriptors)
    }

    pub fn find_block(&self, block_id: &BlockId) -> Option<&AudioBlock> {
        self.chains
            .iter()
            .flat_map(|chain| chain.blocks.iter())
            .find_map(|block| find_block_recursive(block, block_id))
    }

    /// Moves the chain at `index` one position toward the top of the list.
    /// No-op when the chain is already first or the index is out of bounds.
    /// Returns `true` when the order changed (caller should mark the project dirty).
    pub fn move_chain_up(&mut self, index: usize) -> bool {
        if index == 0 || index >= self.chains.len() {
            return false;
        }
        self.chains.swap(index - 1, index);
        true
    }

    /// Moves the chain at `index` one position toward the bottom of the list.
    /// No-op when the chain is already last or the index is out of bounds.
    /// Returns `true` when the order changed (caller should mark the project dirty).
    pub fn move_chain_down(&mut self, index: usize) -> bool {
        if index + 1 >= self.chains.len() {
            return false;
        }
        self.chains.swap(index, index + 1);
        true
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

#[cfg(test)]
#[path = "project_tests.rs"]
mod tests;
