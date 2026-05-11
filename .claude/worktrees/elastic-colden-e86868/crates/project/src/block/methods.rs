//! Validation, descriptor materialisation, and ergonomic accessors on the
//! block types. The methods here all dispatch on `AudioBlockKind` but
//! delegate effect-type-specific work to `dispatch.rs`.
//!
//! Lifted out of `block.rs` (Phase 7 of issue #194).

use domain::ids::BlockId;

use crate::param::BlockParameterDescriptor;

use super::dispatch::{describe_block_audio, describe_block_params, normalize_block_params};
use super::types::{
    AudioBlock, AudioBlockKind, BlockAudioDescriptor, BlockModelRef, CoreBlock, InputBlock,
    SelectBlock, MAX_SELECT_OPTIONS,
};

impl InputBlock {
    pub fn validate_channel_conflicts(&self) -> Result<(), String> {
        let mut used: Vec<(String, usize)> = Vec::new();
        for entry in &self.entries {
            for &ch in &entry.channels {
                let key = (entry.device_id.0.clone(), ch);
                if used.contains(&key) {
                    return Err(format!(
                        "Channel {} on device '{}' is used by multiple entries",
                        ch, entry.device_id.0
                    ));
                }
                used.push(key);
            }
        }
        Ok(())
    }
}

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

impl CoreBlock {
    pub fn model_ref(&self) -> BlockModelRef<'_> {
        BlockModelRef {
            effect_type: &self.effect_type,
            model: &self.model,
            params: &self.params,
        }
    }

    fn validate_params(&self) -> Result<(), String> {
        normalize_block_params(&self.effect_type, &self.model, self.params.clone())?;
        Ok(())
    }

    fn parameter_descriptors(
        &self,
        block_id: &BlockId,
    ) -> Result<Vec<BlockParameterDescriptor>, String> {
        describe_block_params(block_id, &self.effect_type, &self.model, &self.params)
    }

    fn audio_descriptors(&self, block_id: &BlockId) -> Result<Vec<BlockAudioDescriptor>, String> {
        Ok(vec![describe_block_audio(
            block_id,
            &self.effect_type,
            &self.model,
        )?])
    }
}

impl SelectBlock {
    pub fn selected_option(&self) -> Option<&AudioBlock> {
        self.options
            .iter()
            .find(|option| option.id == self.selected_block_id)
    }

    pub fn validate_structure(&self) -> Result<(), String> {
        if self.options.is_empty() {
            return Err("select block must define at least one option".to_string());
        }
        if self.options.len() > MAX_SELECT_OPTIONS {
            return Err(format!(
                "select block may define up to {} options",
                MAX_SELECT_OPTIONS
            ));
        }

        let mut effect_type = None::<&str>;
        for option in &self.options {
            if matches!(
                option.kind,
                AudioBlockKind::Select(_)
                    | AudioBlockKind::Input(_)
                    | AudioBlockKind::Output(_)
                    | AudioBlockKind::Insert(_)
            ) {
                return Err(
                    "select block options cannot be select, input, output, or insert blocks"
                        .to_string(),
                );
            }

            let model = option.model_ref().ok_or_else(|| {
                format!(
                    "select block option '{}' does not expose a concrete model",
                    option.id.0
                )
            })?;

            match effect_type {
                Some(existing) if existing != model.effect_type => {
                    return Err("select block options must use the same effect type".to_string());
                }
                None => effect_type = Some(model.effect_type),
                _ => {}
            }
        }

        if self.selected_option().is_none() {
            return Err("select block selected option does not exist".to_string());
        }

        Ok(())
    }
}
