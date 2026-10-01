//! Responsibility: lists the knobs a split editor shows.
//!
//! #328 (spec §1.2, §11.2): the split side (mode, then the level into and the
//! Mode II balance of each path) and the mixer side (level, pan and polarity
//! of each path, master, master sum) are two views of one
//! `SplitBlock.params`. The rows are the block editor's own
//! (`block_parameter_items_for_specs`), fed the `split_param_specs` of the
//! split's path count, so each path's knobs sit together, path by path.

use project::block::split_param_keys::path_of_key;
use project::block::split_params::{split_param_specs, MIXER_GROUP, SPLIT_GROUP};
use project::block::{SplitBlock, MIN_SPLIT_PATHS};
use project::param::{ParameterDomain, ParameterSpec};

use crate::block_editor_param_items::block_parameter_items_for_specs;
use crate::BlockParameterItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitEditorKind {
    Split,
    Mixer,
}

impl SplitEditorKind {
    /// The overlay's `split-editor-kind`: 0 = split, 1 = mixer.
    pub(crate) fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::Split),
            1 => Some(Self::Mixer),
            _ => None,
        }
    }

    /// The `ParameterSpec::group` of this editor's knobs.
    fn group(self) -> &'static str {
        match self {
            Self::Split => SPLIT_GROUP,
            Self::Mixer => MIXER_GROUP,
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

/// The specs one editor shows for a split with `path_count` paths, in
/// `split_param_specs` order.
pub(crate) fn editor_specs(kind: SplitEditorKind, path_count: usize) -> Vec<ParameterSpec> {
    split_param_specs(path_count)
        .into_iter()
        .filter(|spec| spec.group.as_deref() == Some(kind.group()))
        .collect()
}

pub(crate) fn split_editor_items(
    split: &SplitBlock,
    kind: SplitEditorKind,
) -> Vec<BlockParameterItem> {
    block_parameter_items_for_specs(&editor_specs(kind, split.paths.len()), &split.params)
}

/// The value a choice row's `index` stands for (`SelectBlockParameterOption`
/// carries both). The choices of a knob do not depend on the path count, so
/// the specs are listed for just enough paths to hold `path`.
pub(crate) fn option_value(kind: SplitEditorKind, path: &str, index: usize) -> Option<String> {
    let path_count =
        path_of_key(path).map_or(MIN_SPLIT_PATHS, |(_, at)| (at + 1).max(MIN_SPLIT_PATHS));
    let spec = editor_specs(kind, path_count)
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
