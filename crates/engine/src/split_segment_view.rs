//! Responsibility: shapes a Y split into the split one segment runs.
//!
//! #328 — a Y → A/B split has no mixer: every output runs the paths whose
//! output node checks it (`segment_paths`). Each segment therefore builds its
//! split as a Split → Mix (spec §4.2: "same code as 4.1, with neutral mixer
//! knobs") whose mixer passes the running paths at unity — centred, B not
//! inverted, master at unity, no sum — and whose other path is left empty at
//! level zero. One path: the output carries exactly that path, and because an
//! empty path has no latency the running path is never delayed. Both paths:
//! their unity sum, aligned by the Split → Mix code. The split's own knobs
//! (mode, level into each path, balance) still apply. Setup-time only.

use std::borrow::Cow;

use domain::value_objects::ParameterValue;
use project::block::split_params::{
    default_split_params, MIX_B_POLARITY, MIX_LEVEL_A, MIX_LEVEL_B, MIX_MASTER, MIX_MASTER_SUM,
    MIX_PAN_A, MIX_PAN_B,
};
use project::block::{AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::segment_types::SegmentPaths;

/// A 0–100 gain knob at unity: the split knobs are linear gain `x / 100`
/// (spec §1.2).
const UNITY_PCT: f32 = 100.0;

/// `chain` as one segment builds it: its Y split replaced by
/// [`block_for_segment`]. A chain without a Y split is itself.
pub(crate) fn chain_for_segment(chain: &Chain, paths: SegmentPaths) -> Cow<'_, Chain> {
    if !chain.blocks.iter().any(is_y_split) {
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

/// The block one segment builds in place of `block`: a Y split becomes the
/// neutral Split → Mix of the paths the segment runs; every other block is
/// itself. `SegmentPaths::None` reaches a Y split only in a render with no
/// per-output routing (offline, tone doctor): it plays both paths, as an
/// output with both checked would.
pub(crate) fn block_for_segment(block: &AudioBlock, paths: SegmentPaths) -> Cow<'_, AudioBlock> {
    let AudioBlockKind::Split(split) = &block.kind else {
        return Cow::Borrowed(block);
    };
    if !matches!(split.end, SplitEnd::Y) {
        return Cow::Borrowed(block);
    }
    let (runs_a, runs_b) = match paths {
        SegmentPaths::A => (true, false),
        SegmentPaths::B => (false, true),
        SegmentPaths::AB | SegmentPaths::None => (true, true),
    };
    Cow::Owned(AudioBlock {
        id: block.id.clone(),
        enabled: block.enabled,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Mix,
            params: neutral_mixer(&split.params, runs_a, runs_b),
            a: if runs_a { split.a.clone() } else { Vec::new() },
            b: if runs_b { split.b.clone() } else { Vec::new() },
        }),
    })
}

fn is_y_split(block: &AudioBlock) -> bool {
    matches!(&block.kind, AudioBlockKind::Split(split) if matches!(split.end, SplitEnd::Y))
}

/// `params` with the mixer set to pass the running paths through at unity.
/// Pan, polarity and sum take their defaults (centre, normal, off — spec
/// §1.2), copied so their value type is whatever the split's schema declares.
fn neutral_mixer(params: &ParameterSet, runs_a: bool, runs_b: bool) -> ParameterSet {
    let defaults = default_split_params();
    let mut params = params.clone();
    for key in [MIX_PAN_A, MIX_PAN_B, MIX_B_POLARITY, MIX_MASTER_SUM] {
        if let Some(value) = defaults.get(key) {
            params.insert(key, value.clone());
        }
    }
    let level = |runs: bool| ParameterValue::Float(if runs { UNITY_PCT } else { 0.0 });
    params.insert(MIX_LEVEL_A, level(runs_a));
    params.insert(MIX_LEVEL_B, level(runs_b));
    params.insert(MIX_MASTER, ParameterValue::Float(UNITY_PCT));
    params
}

#[cfg(test)]
#[path = "split_segment_view_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "issue_328_y_audio_tests.rs"]
mod issue_328_y_audio_tests;
