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

use crate::runtime_endpoints::{resolve_chain_io_by_binding, BindingIo, InputEntry, OutputEntry};

/// An `Output` block sitting BETWEEN effect blocks (issue #85): it emits the
/// signal as processed UP TO ITS POSITION while the chain keeps flowing to the
/// blocks after it. Non-destructive — a tap, not a cut.
#[derive(Clone, Copy)]
pub(crate) struct MidOutputTap {
    /// Index of the `Output` block within `chain.blocks`.
    pub(crate) offset: usize,
    /// Route index of the endpoint it writes to (position among the chain's
    /// resolved outputs, the same order the runtime numbers routes with).
    pub(crate) route_idx: usize,
}

/// A tap resolved against ONE segment: how many of that segment's blocks run
/// before the signal is emitted, and where it goes.
#[derive(Clone, Copy)]
pub(crate) struct SegmentTap {
    /// Number of this segment's blocks processed before the tap emits.
    pub(crate) blocks_before: usize,
    pub(crate) route_idx: usize,
}

/// Describes a chain segment: an input source, its effect blocks, and its
/// output targets.
#[allow(dead_code)]
pub(crate) struct ChainSegment {
    pub(crate) input: InputEntry,
    pub(crate) cpal_input_index: usize,
    pub(crate) block_indices: Vec<usize>,
    pub(crate) output_route_indices: Vec<usize>,
    /// Mid-chain output taps this segment emits while processing (#85).
    pub(crate) mid_output_taps: Vec<SegmentTap>,
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
    // Model A: each Input/Output block is ONE binding endpoint (no `entries`).
    // Count the in-chain (mid) I/O blocks — head/tail endpoints come from the
    // bindings (`effective_ins`/`effective_outs`), not from chain blocks.
    let regular_input_count: usize = chain
        .blocks
        .iter()
        .filter(|b| b.enabled && matches!(&b.kind, AudioBlockKind::Input(_)))
        .count();

    // Find positions of enabled Insert blocks in chain.blocks.
    let insert_positions: Vec<usize> = chain
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.enabled && matches!(&b.kind, AudioBlockKind::Insert(_)))
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

    segments_with_inserts(
        chain,
        effective_ins,
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

/// The blocks a segment runs when its input enters at `entry`.
fn blocks_from(block_indices: &[usize], entry: usize) -> Vec<usize> {
    block_indices
        .iter()
        .copied()
        .filter(|&i| i >= entry)
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

/// Resolve the taps whose position falls inside `block_range` against one
/// segment's `block_indices`, converting each chain offset into "after how many
/// of THIS segment's blocks".
fn taps_for_segment(
    mid_taps: &[MidOutputTap],
    block_indices: &[usize],
    block_range: std::ops::Range<usize>,
) -> Vec<SegmentTap> {
    mid_taps
        .iter()
        .filter(|t| block_range.contains(&t.offset))
        .map(|t| SegmentTap {
            blocks_before: block_indices.iter().filter(|&&i| i < t.offset).count(),
            route_idx: t.route_idx,
        })
        .collect()
}

/// One segment per `(input × tail output)` pair when no enabled Insert blocks
/// exist. Model A: the chain's tail outputs come from the bindings and sit at
/// the chain END, so every effect block feeds every tail output — one segment
/// per (input, tail output) covering all enabled effect blocks. Mid `Output`
/// blocks are carried as taps on those segments (#85), never as pairs of their
/// own; for the head/tail case this stays bit-exact to the legacy
/// single-tail-output path.
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

    // #85: a mid `Output` is NOT an end-of-chain destination, so it never
    // spawns its own (input × output) segment — it rides the segment that
    // covers its position and emits there. Exactly one segment per input
    // carries the taps, otherwise the same signal would sum into the mid
    // route once per tail output.
    let taps = taps_for_segment(mid_taps, &block_indices, 0..chain.blocks.len());
    let mut inputs_with_taps: Vec<usize> = Vec::new();

    // A chain with no resolved output endpoint at all gets the synthetic
    // fallback route `effective_outputs` invents — it is the chain's tail.
    let fallback_routes: Vec<usize> = (0..effective_outs.len()).collect();
    let tail_routes = if resolved_output_count == 0 {
        fallback_routes.as_slice()
    } else {
        tail_routes
    };

    for &out_entry_idx in tail_routes {
        let Some(out_entry) = effective_outs.get(out_entry_idx) else {
            continue;
        };
        let out_binding = binding_of_output(&by_binding, out_entry);
        for (in_idx, input) in effective_ins.iter().take(input_count).enumerate() {
            let entry = entry_offsets.get(in_idx).copied().unwrap_or(0);
            // #716: a HEAD input only pairs with its own binding's output (the
            // TEYUN in must not exit the SCARLET out). A mid `Input` block is a
            // different animal: the user dropped it INSIDE this chain, so it
            // feeds the chain's tail wherever that tail comes from (#85).
            if entry == 0 {
                if let (Some(a), Some(b)) = (binding_of_input(&by_binding, input), out_binding) {
                    if a != b {
                        continue;
                    }
                }
            }
            // The taps ride the segment that covers the whole chain (a head
            // input), never a mid input's shorter one.
            let owns_taps = !taps.is_empty() && entry == 0 && !inputs_with_taps.contains(&in_idx);
            if owns_taps {
                inputs_with_taps.push(in_idx);
            }
            segments.push(ChainSegment {
                input: input.clone(),
                cpal_input_index: cpal_indices.get(in_idx).copied().unwrap_or(in_idx),
                block_indices: blocks_from(&block_indices, entry),
                output_route_indices: vec![out_entry_idx],
                mid_output_taps: if owns_taps { taps.clone() } else { Vec::new() },
                split_mono_sibling_count: split_positions.get(in_idx).copied().unwrap_or(None),
                entry_group: entry_groups.get(in_idx).copied().unwrap_or(in_idx),
            });
        }
    }

    // A chain whose ONLY outputs are mid taps (no bound tail endpoint) still
    // has to run: one segment per input, feeding the taps and nothing else.
    if !taps.is_empty() {
        for (in_idx, input) in effective_ins.iter().take(input_count).enumerate() {
            if inputs_with_taps.contains(&in_idx) {
                continue;
            }
            let entry = entry_offsets.get(in_idx).copied().unwrap_or(0);
            segments.push(ChainSegment {
                input: input.clone(),
                cpal_input_index: cpal_indices.get(in_idx).copied().unwrap_or(in_idx),
                block_indices: blocks_from(&block_indices, entry),
                output_route_indices: Vec::new(),
                mid_output_taps: if entry == 0 { taps.clone() } else { Vec::new() },
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
                    cpal_input_index: i,
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
                cpal_input_index: prev_return_idx,
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
        cpal_input_index: last_return_idx,
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
