//! Chain segmentation at Insert block boundaries.
//!
//! Lifted out of `runtime_graph.rs` (slice 7 of the Phase 2 split) so the
//! parent file gets back under the 600 LOC cap.
//!
//! What lives here:
//!   - `ChainSegment` — describes one segment of a chain after splitting at
//!     enabled Insert blocks: the input source, the effect blocks in this
//!     segment, the output routes, and the originating split-mono sibling
//!     count for fan-out scaling.
//!   - `split_chain_into_segments` — splits a chain into `Vec<ChainSegment>`
//!     by walking enabled Insert positions and partitioning effect blocks
//!     between them. With no Inserts, produces one segment per
//!     (input × output) pair.
//!
//! What's NOT here: the routing-state builder
//! (`build_output_routing_state`) and the per-stream processing-state
//! builder (`build_input_processing_state`) — those convert these segment
//! descriptions into the runtime state structures and live with the rest
//! of the graph assembly in `runtime_graph.rs`.

use project::block::AudioBlockKind;
use project::chain::Chain;

use domain::io_binding::IoBinding;

use crate::runtime_endpoints::{resolve_chain_io_by_binding, BindingIo, InputEntry, OutputEntry};

/// Describes a chain segment: an input source, its effect blocks, and its
/// output targets.
#[allow(dead_code)]
pub(crate) struct ChainSegment {
    pub(crate) input: InputEntry,
    pub(crate) cpal_input_index: usize,
    pub(crate) block_indices: Vec<usize>,
    pub(crate) output_route_indices: Vec<usize>,
    /// Inherited from the originating effective input. `Some(N)` when this
    /// segment came from a split-mono entry (one InputBlock with
    /// `mode: mono` and >1 channel) and owns output channel position N.
    /// `None` for stereo / dual-mono / single-channel-mono / Insert-return
    /// segments — they keep the historical broadcast/sum behaviour.
    pub(crate) split_mono_sibling_count: Option<usize>,
    /// RAW input-entry index this segment's effective input came from
    /// (issue #703). The runtime graph partitions segments by this id:
    /// distinct raw entries become isolated runtimes even on one shared
    /// physical device, while split-mono siblings (same raw entry) stay
    /// together so the pinned g02/g03 sum-before-limiter math holds.
    pub(crate) entry_group: usize,
}

/// Split a chain into segments at enabled Insert block boundaries.
///
/// Example: `[Input, Comp, EQ, Insert, Delay, Reverb, Output]`
///   - Segment 1: input=`InputBlock` entries, blocks=[Comp, EQ], outputs=[Insert send]
///   - Segment 2: input=Insert return, blocks=[Delay, Reverb], outputs=[`OutputBlock` entries]
///
/// If no Insert blocks exist, a single segment covers the entire chain
/// (one segment per `(input, output)` pair).
pub(crate) fn split_chain_into_segments(
    chain: &Chain,
    effective_ins: &[InputEntry],
    cpal_indices: &[usize],
    split_positions: &[Option<usize>],
    entry_groups: &[usize],
    _effective_outs: &[OutputEntry],
    registry: &[IoBinding],
) -> Vec<ChainSegment> {
    // Model A: each Input/Output block is ONE binding endpoint (no `entries`).
    // Count the in-chain (mid) I/O blocks — head/tail endpoints come from the
    // bindings (`effective_ins`/`effective_outs`), not from chain blocks.
    let regular_input_count: usize = chain
        .blocks
        .iter()
        .filter(|b| b.enabled && matches!(&b.kind, AudioBlockKind::Input(_)))
        .count();
    let regular_output_count: usize = chain
        .blocks
        .iter()
        .filter(|b| b.enabled && matches!(&b.kind, AudioBlockKind::Output(_)))
        .count();

    // Find positions of enabled Insert blocks in chain.blocks.
    let insert_positions: Vec<usize> = chain
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.enabled && matches!(&b.kind, AudioBlockKind::Insert(_)))
        .map(|(i, _)| i)
        .collect();

    if insert_positions.is_empty() {
        return segments_without_inserts(
            chain,
            effective_ins,
            cpal_indices,
            split_positions,
            entry_groups,
            _effective_outs,
            registry,
        );
    }

    segments_with_inserts(
        chain,
        effective_ins,
        split_positions,
        entry_groups,
        &insert_positions,
        regular_input_count,
        regular_output_count,
    )
}

/// One segment per `(input × output)` pair when no enabled Insert blocks
/// exist. Model A: the chain's outputs come from the bindings (`effective_outs`)
/// and sit at the chain TAIL, so every effect block feeds every output — one
/// segment per (input, output) covering all enabled effect blocks. (Mid output
/// blocks at an offset — a partial cut — are a follow-up; for the head/tail
/// case this is bit-exact to the legacy single-tail-output path.)
fn binding_of_input<'a>(by: &'a [BindingIo], e: &InputEntry) -> Option<&'a str> {
    by.iter()
        .find(|b| {
            b.inputs
                .iter()
                .any(|i| i.device_id == e.device_id && i.channels == e.channels)
        })
        .map(|b| b.binding_id.as_str())
}

fn binding_of_output<'a>(by: &'a [BindingIo], e: &OutputEntry) -> Option<&'a str> {
    by.iter()
        .find(|b| {
            b.outputs
                .iter()
                .any(|o| o.device_id == e.device_id && o.channels == e.channels)
        })
        .map(|b| b.binding_id.as_str())
}

fn segments_without_inserts(
    chain: &Chain,
    effective_ins: &[InputEntry],
    cpal_indices: &[usize],
    split_positions: &[Option<usize>],
    entry_groups: &[usize],
    effective_outs: &[OutputEntry],
    registry: &[IoBinding],
) -> Vec<ChainSegment> {
    // Every effect block (NOT an I/O / Insert port). Disabled effect blocks
    // are KEPT — they become Bypass nodes so a live enable/disable is a
    // lock-free toggle, never a rebuild (#580/#706). Do not filter on enabled.
    let block_indices: Vec<usize> = chain
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| {
            !matches!(
                &b.kind,
                AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_)
            )
        })
        .map(|(i, _)| i)
        .collect();

    let input_count = effective_ins.len();
    let mut segments = Vec::new();

    // #716: pair an input only with its OWN binding's output — never cross to
    // another binding (TEYUN in must not exit the SCARLET out). When both
    // bindings are known and differ, skip the pair. Unknown bindings (single
    // binding, or mid I/O blocks) keep the legacy all-pairs behavior, so a
    // single-binding chain is bit-identical (golden).
    let by_binding = resolve_chain_io_by_binding(chain, registry);

    for out_entry_idx in 0..effective_outs.len() {
        let out_binding = binding_of_output(&by_binding, &effective_outs[out_entry_idx]);
        for (in_idx, input) in effective_ins.iter().take(input_count).enumerate() {
            if let (Some(a), Some(b)) = (binding_of_input(&by_binding, input), out_binding) {
                if a != b {
                    continue;
                }
            }
            segments.push(ChainSegment {
                input: input.clone(),
                cpal_input_index: cpal_indices.get(in_idx).copied().unwrap_or(in_idx),
                block_indices: block_indices.clone(),
                output_route_indices: vec![out_entry_idx],
                split_mono_sibling_count: split_positions.get(in_idx).copied().unwrap_or(None),
                entry_group: entry_groups.get(in_idx).copied().unwrap_or(in_idx),
            });
        }
    }

    segments
}

/// Walks insert positions left-to-right, building one segment per Insert
/// boundary plus a final segment from the last Insert to end of chain.
fn segments_with_inserts(
    chain: &Chain,
    effective_ins: &[InputEntry],
    split_positions: &[Option<usize>],
    entry_groups: &[usize],
    insert_positions: &[usize],
    regular_input_count: usize,
    regular_output_count: usize,
) -> Vec<ChainSegment> {
    let mut segments = Vec::new();
    // Insert return entries start after regular inputs; same for sends.
    let mut insert_return_idx = regular_input_count;
    let mut insert_send_idx = regular_output_count;

    let mut segment_start: usize = 0;
    for (insert_order, &insert_pos) in insert_positions.iter().enumerate() {
        // Effect blocks for this segment: blocks between segment_start and
        // insert_pos (excluding Input, Output, Insert routing blocks).
        let block_indices: Vec<usize> = (segment_start..insert_pos)
            .filter(|&i| {
                let b = &chain.blocks[i];
                !matches!(
                    &b.kind,
                    AudioBlockKind::Input(_)
                        | AudioBlockKind::Output(_)
                        | AudioBlockKind::Insert(_)
                )
            })
            .collect();

        // Output routes for this segment: any OutputBlock entries that
        // appear BEFORE this Insert plus the Insert send.
        let mut output_indices = Vec::new();
        let mut regular_out_idx = 0;
        for b in &chain.blocks[..insert_pos] {
            if b.enabled {
                if let AudioBlockKind::Output(_ob) = &b.kind {
                    // Model A: one endpoint per output block.
                    output_indices.push(regular_out_idx);
                    regular_out_idx += 1;
                }
            }
        }
        output_indices.push(insert_send_idx);

        if insert_order == 0 {
            // First segment: use regular InputBlock entries.
            let input_count = if regular_input_count > 0 {
                regular_input_count
            } else {
                1
            };
            for (i, input) in effective_ins.iter().take(input_count).enumerate() {
                segments.push(ChainSegment {
                    input: input.clone(),
                    cpal_input_index: i,
                    block_indices: block_indices.clone(),
                    output_route_indices: output_indices.clone(),
                    split_mono_sibling_count: split_positions.get(i).copied().unwrap_or(None),
                    entry_group: entry_groups.get(i).copied().unwrap_or(i),
                });
            }
        } else {
            // Subsequent segments before an insert: use previous insert's return.
            let prev_return_idx = insert_return_idx - 1;
            segments.push(ChainSegment {
                input: effective_ins[prev_return_idx].clone(),
                cpal_input_index: prev_return_idx,
                block_indices,
                output_route_indices: output_indices,
                split_mono_sibling_count: None,
                entry_group: entry_groups
                    .get(prev_return_idx)
                    .copied()
                    .unwrap_or(prev_return_idx),
            });
        }

        insert_return_idx += 1;
        insert_send_idx += 1;
        segment_start = insert_pos + 1;
    }

    // Final segment: after the last Insert to end of chain.
    let block_indices: Vec<usize> = (segment_start..chain.blocks.len())
        .filter(|&i| {
            let b = &chain.blocks[i];
            !matches!(
                &b.kind,
                AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_)
            )
        })
        .collect();

    // Output routes: regular OutputBlock entries that appear AFTER the last Insert.
    let last_insert_pos = *insert_positions.last().unwrap();
    let mut output_indices = Vec::new();
    let mut regular_out_idx = 0;
    for (bi, b) in chain.blocks.iter().enumerate() {
        if b.enabled {
            if let AudioBlockKind::Output(_ob) = &b.kind {
                // Model A: one endpoint per output block.
                if bi > last_insert_pos {
                    output_indices.push(regular_out_idx);
                    regular_out_idx += 1;
                } else {
                    regular_out_idx += 1;
                }
            }
        }
    }
    if output_indices.is_empty() {
        output_indices = (0..regular_output_count).collect();
    }

    let last_return_idx = insert_return_idx - 1;
    segments.push(ChainSegment {
        input: effective_ins[last_return_idx].clone(),
        cpal_input_index: last_return_idx,
        block_indices,
        output_route_indices: output_indices,
        split_mono_sibling_count: None,
        entry_group: entry_groups
            .get(last_return_idx)
            .copied()
            .unwrap_or(last_return_idx),
    });

    segments
}
