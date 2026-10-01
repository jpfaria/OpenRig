//! Responsibility: shapes the Y splits of a chain into the splits one segment runs.
//!
//! #328 — a Y split has no mixer: every output runs the Y leaves whose output
//! node checks it (`segment_paths`). Each segment therefore builds every Y as
//! a Split → Mix (spec §4.2: "same code as 4.1, with neutral mixer knobs")
//! whose mixer passes the paths leading to its leaves at unity — centred, not
//! inverted, master at unity, no sum — and whose other paths are left empty
//! at level zero (spec §11.3). One running path: the output carries exactly
//! that path, and because an empty path has no latency the running path is
//! never delayed. Several: their unity sum, aligned by the Split → Mix code.
//! A Y nested at any depth is shaped the same way; a Mix keeps its knobs.
//! The split's own knobs (mode, level into each path, balance) still apply.
//! Setup-time only.

use std::borrow::Cow;

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::split_param_keys::{mix_level, mix_pan, mix_polarity};
use project::block::split_params::{default_split_params, MIX_MASTER, MIX_MASTER_SUM};
use project::block::{walk_blocks, y_leaves, AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::segment_types::SegmentPaths;

/// A 0–100 gain knob at unity: the split knobs are linear gain `x / 100`
/// (spec §1.2).
const UNITY_PCT: f32 = 100.0;

/// `chain` as one segment builds it: every block passed through
/// [`block_for_segment`]. A chain without a Y split is itself.
pub(crate) fn chain_for_segment<'a>(chain: &'a Chain, paths: &SegmentPaths) -> Cow<'a, Chain> {
    if y_leaves(&chain.blocks).is_empty() {
        return Cow::Borrowed(chain);
    }
    let mut shaped = chain.clone();
    shaped.blocks = chain
        .blocks
        .iter()
        .map(|block| block_for_segment(block, paths).into_owned())
        .collect();
    Cow::Owned(shaped)
}

/// The block one segment builds in place of `block`: a split holding a Y at
/// any depth is rebuilt with every Y turned into the neutral Split → Mix of
/// the paths the segment runs; every other block is itself.
/// `SegmentPaths::None` reaches a Y only in a render with no per-output
/// routing (offline, tone doctor): it plays every path, as an output with
/// every leaf checked would.
pub(crate) fn block_for_segment<'a>(
    block: &'a AudioBlock,
    paths: &SegmentPaths,
) -> Cow<'a, AudioBlock> {
    if y_leaves(std::slice::from_ref(block)).is_empty() {
        return Cow::Borrowed(block);
    }
    Cow::Owned(shape(block, paths))
}

fn shape(block: &AudioBlock, paths: &SegmentPaths) -> AudioBlock {
    let AudioBlockKind::Split(split) = &block.kind else {
        return block.clone();
    };
    let runs: Vec<bool> = match split.end {
        SplitEnd::Mix => vec![true; split.paths.len()],
        SplitEnd::Y => (0..split.paths.len())
            .map(|index| path_runs(&block.id, index, &split.paths[index], paths))
            .collect(),
    };
    let shaped_paths = split
        .paths
        .iter()
        .zip(&runs)
        .map(|(path, &runs)| {
            if runs {
                path.iter().map(|inner| shape(inner, paths)).collect()
            } else {
                Vec::new()
            }
        })
        .collect();
    let params = match split.end {
        SplitEnd::Mix => split.params.clone(),
        SplitEnd::Y => neutral_mixer(&split.params, &runs),
    };
    AudioBlock {
        id: block.id.clone(),
        enabled: block.enabled,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Mix,
            params,
            paths: shaped_paths,
        }),
    }
}

/// Whether path `index` of Y `split` leads to a leaf the segment runs: it is
/// that leaf, or it holds the Y the leaf belongs to.
fn path_runs(split: &BlockId, index: usize, path: &[AudioBlock], paths: &SegmentPaths) -> bool {
    let SegmentPaths::Only(leaves) = paths else {
        return true;
    };
    leaves.iter().any(|leaf| {
        (leaf.split == *split && leaf.path == index)
            || walk_blocks(path).iter().any(|inner| inner.id == leaf.split)
    })
}

/// `params` with the mixer set to pass the running paths through at unity.
/// Pan, polarity and sum take their defaults (centre, normal, off — spec
/// §1.2), copied so their value type is whatever the split's schema declares.
fn neutral_mixer(params: &ParameterSet, runs: &[bool]) -> ParameterSet {
    let defaults = default_split_params(runs.len());
    let mut params = params.clone();
    let mut neutral = |key: &str| {
        if let Some(value) = defaults.get(key) {
            params.insert(key, value.clone());
        }
    };
    neutral(MIX_MASTER_SUM);
    for index in 0..runs.len() {
        neutral(&mix_pan(index));
        neutral(&mix_polarity(index));
    }
    for (index, &runs) in runs.iter().enumerate() {
        let level = if runs { UNITY_PCT } else { 0.0 };
        params.insert(mix_level(index), ParameterValue::Float(level));
    }
    params.insert(MIX_MASTER, ParameterValue::Float(UNITY_PCT));
    params
}

#[cfg(test)]
#[path = "split_segment_view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "issue_328_y_audio_tests.rs"]
mod issue_328_y_audio_tests;
