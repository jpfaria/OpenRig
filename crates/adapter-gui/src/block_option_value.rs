//! Responsibility: resolves the value a choice row's index stands for on a block.
//!
//! #328: a split has no model schema — its choices come from
//! `split_param_specs()` — so the lookup cannot go through
//! `schema_for_block_model` alone.

use block_core::param::ParameterDomain;
use project::block::{AudioBlock, AudioBlockKind};

use crate::split_editor_items::{option_value, SplitEditorKind};

pub(crate) fn block_option_value(block: &AudioBlock, path: &str, index: usize) -> Option<String> {
    if let AudioBlockKind::Split(_) = &block.kind {
        return [SplitEditorKind::Split, SplitEditorKind::Mixer]
            .into_iter()
            .find_map(|kind| option_value(kind, path, index));
    }
    let data = crate::block_editor::block_editor_data(block)?;
    let schema = project::block::schema_for_block_model(&data.effect_type, &data.model_id).ok()?;
    let spec = schema.parameters.iter().find(|p| p.path == path)?;
    match &spec.domain {
        ParameterDomain::Enum { options } => options.get(index).map(|o| o.value.clone()),
        _ => None,
    }
}
