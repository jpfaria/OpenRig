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

use project::block::{AudioBlockKind, InputEntry, OutputEntry};
use project::chain::Chain;

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
    _effective_outs: &[OutputEntry],
) -> Vec<ChainSegment> {
    // Count regular InputBlock entries and OutputBlock entries.
    let regular_input_count: usize = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Input(ib) => Some(ib.entries.len()),
            _ => None,
        })
        .sum();
    let regular_output_count: usize = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Output(ob) => Some(ob.entries.len()),
            _ => None,
        })
        .sum();

    // Find positions of enabled Insert blocks in chain.blocks.
    let insert_positions: Vec<usize> = chain
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.enabled && matches!(&b.kind, AudioBlockKind::Insert(_)))
        .map(|(i, _)| i)
        .collect();

    if insert_positions.is_empty() {
        return segments_without_inserts(chain, effective_ins, cpal_indices, split_positions);
    }

    segments_with_inserts(
        chain,
        effective_ins,
        split_positions,
        &insert_positions,
        regular_input_count,
        regular_output_count,
    )
}

/// One segment per `(input × output)` pair when no enabled Insert blocks
/// exist. Each output block defines a cut point — only effect blocks that
/// appear BEFORE that output position are included in the segment.
fn segments_without_inserts(
    chain: &Chain,
    effective_ins: &[InputEntry],
    cpal_indices: &[usize],
    split_positions: &[Option<usize>],
) -> Vec<ChainSegment> {
    let mut output_positions: Vec<(usize, usize)> = Vec::new();
    let mut out_entry_idx = 0;
    for (pos, block) in chain.blocks.iter().enumerate() {
        if block.enabled {
            if let AudioBlockKind::Output(ob) = &block.kind {
                for _ in 0..ob.entries.len() {
                    output_positions.push((pos, out_entry_idx));
                    out_entry_idx += 1;
                }
            }
        }
    }

    let input_count = effective_ins.len();
    let mut segments = Vec::new();

    for &(out_pos, out_entry_idx) in &output_positions {
        let block_indices: Vec<usize> = chain
            .blocks
            .iter()
            .enumerate()
            .filter(|(i, b)| {
                *i < out_pos
                    && !matches!(
                        &b.kind,
                        AudioBlockKind::Input(_)
                            | AudioBlockKind::Output(_)
                            | AudioBlockKind::Insert(_)
                    )
            })
            .map(|(i, _)| i)
            .collect();

        for in_idx in 0..input_count {
            segments.push(ChainSegment {
                input: effective_ins[in_idx].clone(),
                cpal_input_index: cpal_indices.get(in_idx).copied().unwrap_or(in_idx),
                block_indices: block_indices.clone(),
                output_route_indices: vec![out_entry_idx],
                split_mono_sibling_count: split_positions.get(in_idx).copied().unwrap_or(None),
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
                if let AudioBlockKind::Output(ob) = &b.kind {
                    for _ in 0..ob.entries.len() {
                        output_indices.push(regular_out_idx);
                        regular_out_idx += 1;
                    }
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
            for i in 0..input_count {
                segments.push(ChainSegment {
                    input: effective_ins[i].clone(),
                    cpal_input_index: i,
                    block_indices: block_indices.clone(),
                    output_route_indices: output_indices.clone(),
                    split_mono_sibling_count: split_positions.get(i).copied().unwrap_or(None),
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
            if let AudioBlockKind::Output(ob) = &b.kind {
                if bi > last_insert_pos {
                    for _ in 0..ob.entries.len() {
                        output_indices.push(regular_out_idx);
                        regular_out_idx += 1;
                    }
                } else {
                    regular_out_idx += ob.entries.len();
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
    });

    segments
}
