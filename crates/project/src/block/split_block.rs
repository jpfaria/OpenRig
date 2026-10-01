//! Responsibility: describes the data a split block holds.
//!
//! #328 (spec §11.1): a chain split with N paths (at least two). Blocks before
//! it in the chain are shared by every path. With `Mix` the paths meet at the
//! mixer and the blocks after it are shared again; with `Y` nothing but the
//! chain's own ports may follow it and each path ends at its own outputs. The
//! split and mixer knobs live in `params` (see [`super::split_params`]).

use serde::{Deserialize, Serialize};

use super::split_params::default_split_params;
use super::types::AudioBlock;
use crate::param::ParameterSet;

/// The fewest paths a split may hold.
pub const MIN_SPLIT_PATHS: usize = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SplitBlock {
    pub end: SplitEnd,
    pub params: ParameterSet,
    pub paths: Vec<Vec<AudioBlock>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SplitEnd {
    /// Split → Mix: every path meets at the mixer and the chain continues.
    Mix,
    /// Y: each path ends at its own output node.
    Y,
}

impl SplitEnd {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mix => "mix",
            Self::Y => "y",
        }
    }
}

impl SplitBlock {
    /// An empty two-path split with the default knobs.
    pub fn new(end: SplitEnd) -> Self {
        Self::with_paths(end, vec![Vec::new(); MIN_SPLIT_PATHS])
    }

    /// A split holding `paths`, with the default knobs for that many paths.
    pub fn with_paths(end: SplitEnd, paths: Vec<Vec<AudioBlock>>) -> Self {
        Self {
            end,
            params: default_split_params(paths.len()),
            paths,
        }
    }
}
