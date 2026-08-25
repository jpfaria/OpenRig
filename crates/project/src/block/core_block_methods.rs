//! Responsibility: answers what a core block declares about itself.

use domain::ids::BlockId;

use crate::param::BlockParameterDescriptor;

use super::dispatch::{describe_block_audio, describe_block_params, normalize_block_params};
use super::types::{BlockAudioDescriptor, BlockModelRef, CoreBlock};

impl CoreBlock {
    pub fn model_ref(&self) -> BlockModelRef<'_> {
        BlockModelRef {
            effect_type: &self.effect_type,
            model: &self.model,
            params: &self.params,
        }
    }

    pub(crate) fn validate_params(&self) -> Result<(), String> {
        normalize_block_params(&self.effect_type, &self.model, self.params.clone())?;
        Ok(())
    }

    pub(crate) fn parameter_descriptors(
        &self,
        block_id: &BlockId,
    ) -> Result<Vec<BlockParameterDescriptor>, String> {
        describe_block_params(block_id, &self.effect_type, &self.model, &self.params)
    }

    pub(crate) fn audio_descriptors(
        &self,
        block_id: &BlockId,
    ) -> Result<Vec<BlockAudioDescriptor>, String> {
        Ok(vec![describe_block_audio(
            block_id,
            &self.effect_type,
            &self.model,
        )?])
    }
}
