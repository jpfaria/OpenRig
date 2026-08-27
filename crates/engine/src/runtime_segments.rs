//! Responsibility: cuts a chain into segments at its insert points.
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

use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::block::AudioBlockKind;
use project::chain::Chain;

use domain::io_binding::IoBinding;

use crate::runtime_endpoints::{
    insert_is_bound, resolve_chain_io_by_binding, InputEntry, OutputEntry,
};
pub(crate) use crate::segment_taps::{binding_of_input, binding_of_output, taps_for_segment};
pub(crate) use crate::segment_types::{ChainSegment, MidOutputTap, SegmentTap};

/// Split a chain into segments at enabled Insert block boundaries.
///
/// Example: `[Input, Comp, EQ, Insert, Delay, Reverb]`
///   - Segment 1: input=head endpoints, blocks=[Comp, EQ], outputs=[Insert send]
///   - Segment 2: input=Insert return, blocks=[Delay, Reverb], outputs=[tail routes]
///
/// If no Insert blocks exist, a single segment covers the entire chain
/// (one segment per `(input, tail output)` pair). Mid `Output` blocks never
/// split anything — they ride the segment covering their position as taps
/// (#85).
pub(crate) fn split_chain_into_segments(
    chain: &Chain,
    effective_ins: &[InputEntry],
    cpal_indices: &[usize],
    split_positions: &[Option<usize>],
    entry_groups: &[usize],
    _effective_outs: &[OutputEntry],
    registry: &[IoBinding],
) -> Vec<ChainSegment> {
    // Find positions of enabled Insert blocks in chain.blocks. Only an insert
    // whose binding resolves on BOTH sides is a boundary (#881): the send and
    // return shims are what `effective_outputs` / `effective_inputs` append, so
    // an unbound insert has no endpoints to split at — it is bypassed and the
    // chain flows straight through it.
    let insert_positions: Vec<usize> = chain
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.enabled && insert_is_bound(&b.kind, registry))
        .map(|(i, _)| i)
        .collect();

    let (tail_routes, mid_taps, resolved_output_count) = classify_output_routes(chain, registry);

    if insert_positions.is_empty() {
        return segments_without_inserts(
            chain,
            effective_ins,
            cpal_indices,
            split_positions,
            entry_groups,
            _effective_outs,
            &tail_routes,
            &mid_taps,
            resolved_output_count,
            registry,
        );
    }

    // The insert returns are the LAST entries of `effective_ins` — one per bound
    // insert — so everything before them is a regular input. Counting Input
    // BLOCKS instead broke split-mono (#881): one mono endpoint over N channels
    // is N entries, and the walker landed on a sibling instead of the return.
    let regular_input_count = effective_ins.len().saturating_sub(insert_positions.len());

    segments_with_inserts(
        chain,
        effective_ins,
        cpal_indices,
        split_positions,
        entry_groups,
        &insert_positions,
        regular_input_count,
        resolved_output_count,
        &tail_routes,
        &mid_taps,
    )
}

/// Split the chain's resolved output routes into the TAIL routes (the chain's
/// end-of-chain endpoints, materialized from the bindings it selects) and the
/// MID taps (`Output` blocks placed between effect blocks, #85). The index of
/// each route is its position among the resolved outputs — the same order
/// `effective_outputs` (and therefore the runtime's route list) uses.
fn classify_output_routes(
    chain: &Chain,
    registry: &[IoBinding],
) -> (Vec<usize>, Vec<MidOutputTap>, usize) {
    let tail = chain.blocks.len();
    let mut tail_routes = Vec::new();
    let mut mid_taps = Vec::new();
    let mut resolved_count = 0;
    for (route_idx, port) in resolve_chain_ports(chain, registry)
        .into_iter()
        .filter(|p| p.direction == PortDirection::Output)
        .enumerate()
    {
        resolved_count = route_idx + 1;
        if port.offset >= tail {
            tail_routes.push(route_idx);
        } else if chain.blocks[port.offset].enabled {
            // A disabled Output block keeps its (silent) route but emits
            // nothing — same rule as any other disabled block.
            mid_taps.push(MidOutputTap {
                offset: port.offset,
                route_idx,
            });
        }
    }
    (tail_routes, mid_taps, resolved_count)
}

/// The blocks one (input × output) pipeline runs: those between the input's
/// entry point and the output's position. A pipeline that ends at a mid
/// `Output` therefore runs fewer blocks than the chain's main one, which runs
/// them all — the ports themselves are never blocks of any pipeline.
fn blocks_between(block_indices: &[usize], entry: usize, end: usize) -> Vec<usize> {
    block_indices
        .iter()
        .copied()
        .filter(|&i| i >= entry && i < end)
        .collect()
}

/// The chain offset each resolved INPUT enters at, in `effective_ins` order.
/// A head input enters before block 0; a mid `Input` block enters right after
/// the block it sits at, so the blocks before it never see its signal (#85).
fn input_ports(chain: &Chain, registry: &[IoBinding]) -> Vec<usize> {
    resolve_chain_ports(chain, registry)
        .into_iter()
        .filter(|p| p.direction == PortDirection::Input)
        .map(|p| if p.from_block { p.offset + 1 } else { 0 })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn segments_without_inserts(
    chain: &Chain,
    effective_ins: &[InputEntry],
    cpal_indices: &[usize],
    split_positions: &[Option<usize>],
    entry_groups: &[usize],
    effective_outs: &[OutputEntry],
    tail_routes: &[usize],
    mid_taps: &[MidOutputTap],
    resolved_output_count: usize,
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
    // #85: where each input enters the chain, in `effective_ins` order. A head
    // input (from the bindings) runs every block; a mid `Input` block brings its
    // own E/S in AT ITS POSITION, so its segment runs only what comes after it —
    // otherwise the port's signal goes through the whole chain from the top and
    // the blocks before it colour (or gate away) a signal they never touch.
    let entry_offsets: Vec<usize> = input_ports(chain, registry);
    let mut segments = Vec::new();

    // #716: pair an input only with its OWN binding's output — never cross to
    // another binding (TEYUN in must not exit the SCARLET out). When both
    // bindings are known and differ, skip the pair. Unknown bindings (single
    // binding, or mid I/O blocks) keep the legacy all-pairs behavior, so a
    // single-binding chain is bit-identical (golden).
    let by_binding = resolve_chain_io_by_binding(chain, registry);

    // LAW (stream isolation) — a stream is ONE (input × output) pair, and each
    // pair is an independent pipeline: "se eu tenho 1 input e dois outputs eu
    // tenho dois streams". A mid `Output` is therefore the END of its own
    // pipeline, not a tap riding someone else's, and a mid `Input` is the START
    // of its own. Which blocks a pipeline runs follows from the two positions,
    // so a pipeline that ends early simply has fewer blocks.
    //
    // Every resolved output with the chain offset it sits at: a tail output is
    // past the last block, a mid `Output` at its own index. A chain with no
    // resolved output at all gets the synthetic fallback route
    // `effective_outputs` invents — that one is its tail.
    let tail = chain.blocks.len();
    let mut outputs: Vec<(usize, usize)> = if resolved_output_count == 0 {
        (0..effective_outs.len()).map(|r| (r, tail)).collect()
    } else {
        let mut outs: Vec<(usize, usize)> = tail_routes.iter().map(|&r| (r, tail)).collect();
        outs.extend(mid_taps.iter().map(|t| (t.route_idx, t.offset)));
        outs.sort_by_key(|&(route, _)| route);
        outs
    };
    outputs.dedup_by_key(|&mut (route, _)| route);

    for (out_entry_idx, out_offset) in outputs {
        let Some(out_entry) = effective_outs.get(out_entry_idx) else {
            continue;
        };
        let out_binding = binding_of_output(&by_binding, out_entry);
        for (in_idx, input) in effective_ins.iter().take(input_count).enumerate() {
            let entry = entry_offsets.get(in_idx).copied().unwrap_or(0);
            // An output BEFORE this input is not downstream of it — no pair.
            if entry > out_offset {
                continue;
            }
            // #716: a HEAD input only pairs with its own binding's TAIL output
            // (the TEYUN in must not exit the SCARLET out). A mid port is a
            // different animal: the user dropped it INSIDE this chain, so it
            // pairs by position, not by binding (#85).
            if entry == 0 && out_offset >= tail {
                if let (Some(a), Some(b)) = (binding_of_input(&by_binding, input), out_binding) {
                    if a != b {
                        continue;
                    }
                }
            }
            segments.push(ChainSegment {
                input: input.clone(),
                cpal_input_index: cpal_indices.get(in_idx).copied().unwrap_or(in_idx),
                block_indices: blocks_between(&block_indices, entry, out_offset),
                output_route_indices: vec![out_entry_idx],
                mid_output_taps: Vec::new(),
                split_mono_sibling_count: split_positions.get(in_idx).copied().unwrap_or(None),
                entry_group: entry_groups.get(in_idx).copied().unwrap_or(in_idx),
            });
        }
    }

    segments
}

/// Walks insert positions left-to-right, building one segment per Insert
/// boundary plus a final segment from the last Insert to end of chain.
#[allow(clippy::too_many_arguments)]
fn segments_with_inserts(
    chain: &Chain,
    effective_ins: &[InputEntry],
    cpal_indices: &[usize],
    split_positions: &[Option<usize>],
    entry_groups: &[usize],
    insert_positions: &[usize],
    regular_input_count: usize,
    resolved_output_count: usize,
    tail_routes: &[usize],
    mid_taps: &[MidOutputTap],
) -> Vec<ChainSegment> {
    let mut segments = Vec::new();
    // Insert return entries start after regular inputs; the sends start after
    // EVERY resolved output (`effective_outputs` appends them last), so a
    // chain with bound tail outputs does not alias its sends onto them.
    let mut insert_return_idx = regular_input_count;
    let mut insert_send_idx = resolved_output_count;

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

        // The only end-of-segment destination is the Insert send; any
        // `Output` block inside this stretch is a mid tap that emits at its
        // own position (#85), not at the segment's end.
        let output_indices = vec![insert_send_idx];
        let taps = taps_for_segment(mid_taps, &block_indices, segment_start..insert_pos);

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
                    // The DEVICE's stream index, never the entry's position
                    // (#881) — infra-cpal dedupes its input streams by device.
                    cpal_input_index: cpal_indices.get(i).copied().unwrap_or(i),
                    block_indices: block_indices.clone(),
                    output_route_indices: output_indices.clone(),
                    // One segment per input here, so every input taps the
                    // mid route exactly once — same fan-in the tail routes
                    // already have.
                    mid_output_taps: taps.clone(),
                    split_mono_sibling_count: split_positions.get(i).copied().unwrap_or(None),
                    entry_group: entry_groups.get(i).copied().unwrap_or(i),
                });
            }
        } else {
            // Subsequent segments before an insert: use previous insert's return.
            let prev_return_idx = insert_return_idx - 1;
            segments.push(ChainSegment {
                input: effective_ins[prev_return_idx].clone(),
                cpal_input_index: cpal_indices
                    .get(prev_return_idx)
                    .copied()
                    .unwrap_or(prev_return_idx),
                block_indices,
                output_route_indices: output_indices,
                mid_output_taps: taps,
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

    // The chain ENDS here, so this segment feeds the tail routes; `Output`
    // blocks after the last Insert stay taps at their own position (#85).
    let last_insert_pos = *insert_positions.last().unwrap();
    let taps = taps_for_segment(
        mid_taps,
        &block_indices,
        last_insert_pos..chain.blocks.len(),
    );

    let last_return_idx = insert_return_idx - 1;
    segments.push(ChainSegment {
        input: effective_ins[last_return_idx].clone(),
        cpal_input_index: cpal_indices
            .get(last_return_idx)
            .copied()
            .unwrap_or(last_return_idx),
        block_indices,
        output_route_indices: tail_routes.to_vec(),
        mid_output_taps: taps,
        split_mono_sibling_count: None,
        entry_group: entry_groups
            .get(last_return_idx)
            .copied()
            .unwrap_or(last_return_idx),
    });

    segments
}
