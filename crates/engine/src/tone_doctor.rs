//! Tone Doctor — offline blame-by-ablation (#791, Layer 2).
//!
//! Given a chain and a slice of the player's own DI, this re-renders the chain
//! several times through the deterministic offline path ([`render_chain`]) to
//! prove *which block* introduced a tonal problem — not by instrumenting the
//! audio thread, but by measuring the chain with blocks progressively added
//! and then with the suspect removed.
//!
//! - **Growth curve** — render with the enabled processing blocks turned on one
//!   at a time, in order. The symptom is *born* at the first prefix where its
//!   descriptor crosses the healthy limit. That prefix names the block.
//! - **Bypass confirmation** — re-render the full chain with that one block
//!   disabled. If the symptom clears, the blame is causal; if not, the cause is
//!   an interaction between blocks and the report says so.
//!
//! Everything runs offline and reuses [`crate::offline::render_chain`], so no
//! audio-thread work, no per-block instrumentation, and per-chain isolation is
//! preserved by construction (each render sees only this chain).

use anyhow::Result;
use feature_dsp::tone_descriptors::{
    analyze, Symptom, ToneDescriptors, CLIP_FRACTION_LIMIT, FIZZ_RATIO_LIMIT, MUD_RATIO_LIMIT,
};
use project::block::AudioBlockKind;
use project::chain::Chain;


/// Tail (in frames) appended to each render so time-based blocks flush.
const DIAGNOSE_TAIL_FRAMES: usize = 4_096;

/// One point on the growth curve: the chain rendered with the enabled
/// processing blocks turned on up to and including `block_index`.
#[derive(Debug, Clone)]
pub struct GrowthStage {
    /// Index into `chain.blocks` of the block added at this stage.
    pub block_index: usize,
    /// Model identity of that block (e.g. `gain:fuzz_si`), for the UI.
    pub label: String,
    /// Descriptors of the chain output at this prefix.
    pub descriptors: ToneDescriptors,
}

/// The outcome of a diagnosis run.
#[derive(Debug, Clone)]
pub struct Diagnosis {
    /// Symptom of the fully-rendered chain.
    pub full_symptom: Symptom,
    /// Descriptors of the fully-rendered chain.
    pub full_descriptors: ToneDescriptors,
    /// The growth curve, one entry per enabled processing block, in order.
    pub curve: Vec<GrowthStage>,
    /// Index into `chain.blocks` of the block that introduced the symptom,
    /// or `None` when the chain is healthy.
    pub culprit: Option<usize>,
    /// Whether disabling the culprit cleared the symptom. `false` with a
    /// `Some(culprit)` means the cause is a cross-block interaction.
    pub bypass_resolved: bool,
}

/// The scalar the culprit search follows for a given symptom.
pub(crate) fn symptom_metric(symptom: Symptom, d: &ToneDescriptors) -> Option<(f32, f32)> {
    match symptom {
        Symptom::Fizz => Some((d.fizz_ratio, FIZZ_RATIO_LIMIT)),
        Symptom::Mud => Some((d.mud_ratio, MUD_RATIO_LIMIT)),
        Symptom::Clipping => Some((d.clip_fraction, CLIP_FRACTION_LIMIT)),
        Symptom::Ok => None,
    }
}

/// Indices into `chain.blocks` of the enabled processing blocks (everything
/// except the I/O endpoints and already-bypassed blocks), in signal order.
fn enabled_processing_blocks(chain: &Chain) -> Vec<usize> {
    chain
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| {
            b.enabled && !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Diagnose a chain against a slice of the player's own DI.
///
/// Builds the chain's processors ONCE and re-renders each ablation variant with
/// an enabled-mask, so the NAM is loaded a single time instead of once per pass
/// (#791 perf — the ablation only toggles which blocks run, never their params).
pub fn diagnose(
    chain: &Chain,
    sample_rate: f32,
    input: &[[f32; 2]],
    block_size: usize,
) -> Result<Diagnosis> {
    let mut nodes = crate::offline::build_offline_nodes(chain, sample_rate)?;
    let render = |nodes: &mut Vec<_>, mask: &[bool]| {
        let out = crate::offline::render_nodes_masked(
            nodes,
            input,
            block_size,
            DIAGNOSE_TAIL_FRAMES,
            mask,
        );
        analyze(&out, sample_rate)
    };

    // The chain as-is (respecting the user's enabled flags).
    let full_mask: Vec<bool> = chain.blocks.iter().map(|b| b.enabled).collect();
    let full_descriptors = render(&mut nodes, &full_mask);
    let full_symptom = full_descriptors.symptom();

    // A healthy chain has nothing to blame — skip the expensive ablation.
    let metric = match symptom_metric(full_symptom, &full_descriptors) {
        Some(m) => m,
        None => {
            return Ok(Diagnosis {
                full_symptom,
                full_descriptors,
                curve: Vec::new(),
                culprit: None,
                bypass_resolved: false,
            })
        }
    };

    let stages = enabled_processing_blocks(chain);

    // Growth curve: turn the enabled processing blocks on one at a time, in
    // order. Stage `p` keeps the first `p+1` on and forces the rest off.
    let mut curve = Vec::with_capacity(stages.len());
    for (p, &block_index) in stages.iter().enumerate() {
        let mut mask = full_mask.clone();
        for (later_pos, &later_index) in stages.iter().enumerate() {
            if later_pos > p {
                mask[later_index] = false;
            }
        }
        let descriptors = render(&mut nodes, &mask);
        curve.push(GrowthStage {
            block_index,
            label: chain.blocks[block_index].kind.model_identity(),
            descriptors,
        });
    }

    // The symptom is born at the first stage whose metric crosses the limit.
    let (_, limit) = metric;
    let culprit_stage = curve.iter().position(|s| {
        symptom_metric(full_symptom, &s.descriptors)
            .map(|(value, _)| value > limit)
            .unwrap_or(false)
    });
    let culprit = culprit_stage.map(|p| stages[p]);

    // Bypass confirmation: re-render with just the culprit off.
    let bypass_resolved = if let Some(culprit_index) = culprit {
        let mut mask = full_mask.clone();
        mask[culprit_index] = false;
        render(&mut nodes, &mask).symptom() != full_symptom
    } else {
        false
    };

    Ok(Diagnosis {
        full_symptom,
        full_descriptors,
        curve,
        culprit,
        bypass_resolved,
    })
}

#[cfg(test)]
#[path = "tone_doctor_tests.rs"]
mod tests;
