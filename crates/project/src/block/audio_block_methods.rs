//! Responsibility: answers what a block declares about itself.
//!
//! Split out of `methods.rs` (#873): validation, descriptors and accessors of
//! [`AudioBlock`], all of which dispatch on the kind.

use crate::param::BlockParameterDescriptor;

use super::dispatch::{describe_block_audio, describe_block_params, normalize_block_params};
use super::types::{AudioBlock, AudioBlockKind, BlockAudioDescriptor, BlockModelRef};

impl AudioBlock {
    pub fn validate_params(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        match &self.kind {
            AudioBlockKind::Nam(stage) => {
                normalize_block_params(
                    block_core::EFFECT_TYPE_NAM,
                    &stage.model,
                    stage.params.clone(),
                )?;
                Ok(())
            }
            AudioBlockKind::Core(core) => core.validate_params(),
            AudioBlockKind::Select(select) => {
                select.validate_structure()?;
                for option in &select.options {
                    option.validate_params()?;
                }
                Ok(())
            }
            AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) => {
                Ok(())
            }
        }
    }

    pub fn parameter_descriptors(&self) -> Result<Vec<BlockParameterDescriptor>, String> {
        match &self.kind {
            AudioBlockKind::Nam(stage) => describe_block_params(
                &self.id,
                block_core::EFFECT_TYPE_NAM,
                &stage.model,
                &stage.params,
            ),
            AudioBlockKind::Core(core) => core.parameter_descriptors(&self.id),
            AudioBlockKind::Select(select) => select
                .selected_option()
                .ok_or_else(|| "select block selected option does not exist".to_string())?
                .parameter_descriptors(),
            AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) => {
                Ok(Vec::new())
            }
        }
    }

    pub fn audio_descriptors(&self) -> Result<Vec<BlockAudioDescriptor>, String> {
        if !self.enabled {
            return Ok(Vec::new());
        }
        match &self.kind {
            AudioBlockKind::Nam(stage) => Ok(vec![describe_block_audio(
                &self.id,
                block_core::EFFECT_TYPE_NAM,
                &stage.model,
            )?]),
            AudioBlockKind::Core(core) => core.audio_descriptors(&self.id),
            AudioBlockKind::Select(select) => select
                .selected_option()
                .ok_or_else(|| "select block selected option does not exist".to_string())?
                .audio_descriptors(),
            AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) => {
                Ok(Vec::new())
            }
        }
    }

    pub fn model_ref(&self) -> Option<BlockModelRef<'_>> {
        match &self.kind {
            AudioBlockKind::Nam(stage) => Some(BlockModelRef {
                effect_type: block_core::EFFECT_TYPE_NAM,
                model: &stage.model,
                params: &stage.params,
            }),
            AudioBlockKind::Core(core) => Some(core.model_ref()),
            AudioBlockKind::Select(_)
            | AudioBlockKind::Input(_)
            | AudioBlockKind::Output(_)
            | AudioBlockKind::Insert(_) => None,
        }
    }
}
