//! Responsibility: builds the block a catalog entry stands for.

use crate::block::{build_audio_block_kind, AudioBlockKind};
use crate::param::ParameterSet;

pub fn build_block_kind(
    effect_type: &str,
    model_id: &str,
    params: ParameterSet,
) -> Result<AudioBlockKind, String> {
    log::debug!(
        "building block kind: effect_type='{}', model_id='{}'",
        effect_type,
        model_id
    );
    build_audio_block_kind(effect_type, model_id, params)
}
