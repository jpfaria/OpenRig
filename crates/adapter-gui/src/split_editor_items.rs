//! Responsibility: lists the knobs a split editor shows.
//!
//! #328 (spec §1.2): the split side (mode, levels into A and B, the Mode II
//! balances) and the mixer side (levels, pans, B polarity, master, master
//! sum) are two views of one `SplitBlock.params`. The rows are the block
//! editor's own (`block_parameter_items_for_specs`), fed `split_param_specs()`.

use project::block::split_params::{
    split_param_specs, BALANCE_A, BALANCE_B, LEVEL_TO_A, LEVEL_TO_B, MIX_B_POLARITY, MIX_LEVEL_A,
    MIX_LEVEL_B, MIX_MASTER, MIX_MASTER_SUM, MIX_PAN_A, MIX_PAN_B, SPLIT_MODE,
};
use project::block::SplitBlock;
use project::param::{ParameterDomain, ParameterSpec};

use crate::block_editor_param_items::block_parameter_items_for_specs;
use crate::BlockParameterItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitEditorKind {
    Split,
    Mixer,
}

const SPLIT_KEYS: [&str; 5] = [SPLIT_MODE, LEVEL_TO_A, LEVEL_TO_B, BALANCE_A, BALANCE_B];
const MIXER_KEYS: [&str; 7] = [
    MIX_LEVEL_A,
    MIX_LEVEL_B,
    MIX_PAN_A,
    MIX_PAN_B,
    MIX_B_POLARITY,
    MIX_MASTER,
    MIX_MASTER_SUM,
];

impl SplitEditorKind {
    /// The overlay's `split-editor-kind`: 0 = split, 1 = mixer.
    pub(crate) fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::Split),
            1 => Some(Self::Mixer),
            _ => None,
        }
    }

    fn keys(self) -> &'static [&'static str] {
        match self {
            Self::Split => &SPLIT_KEYS,
            Self::Mixer => &MIXER_KEYS,
        }
    }

    pub(crate) fn title(self) -> String {
        match self {
            Self::Split => rust_i18n::t!("title-split-editor"),
            Self::Mixer => rust_i18n::t!("title-mixer-editor"),
        }
        .to_string()
    }
}

/// The specs one editor shows, in `split_param_specs()` order.
pub(crate) fn editor_specs(kind: SplitEditorKind) -> Vec<ParameterSpec> {
    let keys = kind.keys();
    split_param_specs()
        .into_iter()
        .filter(|spec| keys.contains(&spec.path.as_str()))
        .collect()
}

pub(crate) fn split_editor_items(
    split: &SplitBlock,
    kind: SplitEditorKind,
) -> Vec<BlockParameterItem> {
    block_parameter_items_for_specs(&editor_specs(kind), &split.params)
}

/// The value a choice row's `index` stands for (`SelectBlockParameterOption`
/// carries both).
pub(crate) fn option_value(kind: SplitEditorKind, path: &str, index: usize) -> Option<String> {
    let spec = editor_specs(kind)
        .into_iter()
        .find(|spec| spec.path == path)?;
    match spec.domain {
        ParameterDomain::Enum { options } => options.into_iter().nth(index).map(|o| o.value),
        _ => None,
    }
}

#[cfg(test)]
#[path = "split_editor_items_tests.rs"]
mod tests;
