//! In-place (lock-free) chain-runtime rebuild (issue #792 split from
//! `runtime_graph.rs`).
//!
//! Setup-time only — the swap into the live `ChainRuntimeState` is brief and
//! lock-guarded; the expensive work (building new nodes, dropping old NAM/IR
//! processors) happens OUTSIDE the audio worker's `processing` try_lock so a
//! param/preset edit never drops audio (issue #670). Reuses the shared
//! per-segment assembly helpers in `runtime_graph_assemble` and the segment
//! grouping in `runtime_graph`.

use std::sync::Arc;

use anyhow::{anyhow, Result};

use domain::io_binding::IoBinding;
use project::chain::Chain;

use crate::runtime::{ChainRuntimeState, PROBE_IDLE};
use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io};
use crate::runtime_graph::group_segments_by_input;
use crate::runtime_graph_assemble::{
    build_input_processing_state, build_output_routing_state, collect_bypass_block_ids,
    output_entry_layout, target_for_route,
};
use crate::runtime_segments::{split_chain_into_segments, ChainSegment};
use crate::runtime_state::{
    lock_recover, BlockRuntimeNode, InputCallbackScratch, OutgoingTail, OutputRoutingState,
    SPILLOVER_FRAMES,
};

/// In-place lock-free rebuild (param/preset edit): old processors are reused
/// and dropped. Audio is click-safe via the per-segment fade-in.
pub fn update_chain_runtime_state(
    runtime: &Arc<ChainRuntimeState>,
    chain: &Chain,
    sample_rate: f32,
    reset_output_queue: bool,
    elastic_targets: &[usize],
    registry: &[IoBinding],
) -> Result<()> {
    update_chain_runtime_state_impl(
        runtime,
        chain,
        sample_rate,
        reset_output_queue,
        elastic_targets,
        false,
        registry,
    )
}

/// #454-T5: same lock-free swap, but the *previous* pipeline is retained as
/// a decaying [`OutgoingTail`] on the new state so its delay/reverb tail
/// rings out in parallel (spillover) instead of being cut. The new pipeline
/// is built fresh (no processor reuse) so it fades in cleanly while the old
/// one fades out.
pub fn update_chain_runtime_state_spillover(
    runtime: &Arc<ChainRuntimeState>,
    chain: &Chain,
    sample_rate: f32,
    reset_output_queue: bool,
    elastic_targets: &[usize],
    registry: &[IoBinding],
) -> Result<()> {
    update_chain_runtime_state_impl(
        runtime,
        chain,
        sample_rate,
        reset_output_queue,
        elastic_targets,
        true,
        registry,
    )
}

#[allow(clippy::too_many_arguments)]
fn update_chain_runtime_state_impl(
    runtime: &Arc<ChainRuntimeState>,
    chain: &Chain,
    _sample_rate: f32, // #736: kept for API compat; each runtime reads its own rate via runtime.sample_rate()
    reset_output_queue: bool,
    elastic_targets: &[usize],
    spillover: bool,
    registry: &[IoBinding],
) -> Result<()> {
    let (resolved_inputs, resolved_outputs) = resolve_chain_io(chain, registry);
    let (effective_ins, eff_input_cpal_indices, effective_split_positions, eff_entry_groups) =
        effective_inputs(chain, &resolved_inputs, registry);
    let effective_outs = effective_outputs(chain, &resolved_outputs, registry);
    let all_segments = split_chain_into_segments(
        chain,
        &effective_ins,
        &eff_input_cpal_indices,
        &effective_split_positions,
        &eff_entry_groups,
        &effective_outs,
        registry,
    );
    // Issue #703: a per-entry isolated runtime is refilled with ONLY its
    // own entry's segments. Both entries of a shared device dispatch on
    // the same cpal index, so refilling every runtime with ALL segments
    // would make the one device callback process the same guitar in every
    // sibling runtime — summed at the backend mix (audible double volume).
    // A whole-chain runtime (`owned_entry == None`: probe, offline, JACK)
    // keeps every segment, exactly as before.
    let segments: Vec<ChainSegment> = match runtime.owned_entry {
        Some((group, _)) => group_segments_by_input(chain, all_segments)
            .into_iter()
            .find(|(g, _)| *g == group)
            .map(|(_, segs)| segs)
            .ok_or_else(|| {
                anyhow!(
                    "chain '{}' in-place update: entry group {} no longer exists \
                     (topology change must take the full-rebuild path)",
                    chain.id.0,
                    group
                )
            })?,
        None => all_segments,
    };

    // Step 1: Extract existing blocks from all input states (brief lock)
    let mut existing_per_input: Vec<Vec<BlockRuntimeNode>> = {
        let mut processing = lock_recover(&runtime.processing, "chain runtime");
        processing
            .input_states
            .iter_mut()
            .map(|is| std::mem::take(&mut is.blocks))
            .collect()
    };

    // Step 2: Build new input states OUTSIDE the lock (no audio interruption)
    let mut new_input_states = Vec::with_capacity(segments.len());
    for (i, segment) in segments.iter().enumerate() {
        let old_blocks = if i < existing_per_input.len() {
            std::mem::take(&mut existing_per_input[i])
        } else {
            Vec::new()
        };
        // Spillover: build the new pipeline FRESH (no processor reuse) so it
        // fades in cleanly; keep the old blocks to ring out in parallel.
        // Non-spillover: reuse old processors in place (param-edit path).
        let (existing, tail_blocks) = if spillover && !old_blocks.is_empty() {
            (None, Some(old_blocks))
        } else if old_blocks.is_empty() {
            (None, None)
        } else {
            (Some(old_blocks), None)
        };
        let segment_output_channels: Vec<usize> = segment
            .output_route_indices
            .iter()
            .filter_map(|&idx| effective_outs.get(idx))
            .flat_map(|e| e.channels.iter().copied())
            .collect();
        // #736: rebuild at the runtime's OWN built rate, not the chain scalar
        let input_state = match build_input_processing_state(
            chain,
            &segment.input,
            &segment_output_channels,
            runtime.sample_rate(),
            existing,
            Some(&segment.block_indices),
            segment.output_route_indices.clone(),
            segment.split_mono_sibling_count,
        ) {
            Ok(state) => state,
            Err(e) => {
                // Restore previously-extracted blocks so the chain keeps playing
                log::error!(
                    "[engine] rebuild failed for chain '{}': {e} — restoring previous state",
                    chain.id.0
                );
                let mut processing = lock_recover(&runtime.processing, "chain runtime");
                for (is, old_blocks) in processing
                    .input_states
                    .iter_mut()
                    .zip(existing_per_input.into_iter())
                {
                    if is.blocks.is_empty() {
                        is.blocks = old_blocks;
                    }
                }
                return Err(e);
            }
        };
        let mut input_state = input_state;
        if let Some(blocks) = tail_blocks {
            input_state.outgoing = Some(Box::new(OutgoingTail {
                blocks,
                frames_remaining: SPILLOVER_FRAMES,
                scratch: Vec::with_capacity(2048),
            }));
        }
        new_input_states.push(input_state);
    }

    // Output routes (#670): REUSE the existing route when its endpoint shape
    // is unchanged (the param-edit / block-toggle case). A fresh empty buffer
    // here used to (a) discard the in-flight audio — the audible gap on every
    // edit — and (b) restart the standing cushion at zero, which never
    // refills in producer/consumer lockstep, leaving the chain permanently
    // fragile after the first edit (owner-reported underruns while playing,
    // reproduced by rebuild_while_playing_keeps_the_cushion). Reusing the
    // Arc keeps both the buffered audio and the cushion. A genuinely changed
    // endpoint (or an explicit queue reset) still gets a fresh route.
    let rebuild_has_convolution = crate::elastic_prime::chain_has_convolution(chain);
    let old_output_routes = runtime.output_routes.load();
    let new_output_routes: Vec<Arc<OutputRoutingState>> = effective_outs
        .iter()
        .enumerate()
        .map(|(route_idx, o)| {
            let base = target_for_route(elastic_targets, route_idx);
            let target =
                crate::elastic_prime::elastic_capacity_target(base, rebuild_has_convolution);
            if !reset_output_queue {
                if let Some(old) = old_output_routes.get(route_idx) {
                    if old.output_channels == o.channels
                        && old.buffer.layout() == output_entry_layout(o)
                        && old.buffer.target_level() == target
                    {
                        return Arc::clone(old);
                    }
                }
            }
            // Fresh route on a rebuild. A convolution chain gets the SAME
            // cushion the initial build would give it (#670): the reuse
            // check above rejects exactly when the cushion posture changed —
            // e.g. the chain GAINED its first cab/IR live — and an unprimed
            // route here left the chain permanently fragile (fill ~0, every
            // scheduling wobble on a real USB interface popped the output
            // empty: the owner's random clicks after adding/swapping a cab).
            let prime = if rebuild_has_convolution { target } else { 0 };
            let fresh = build_output_routing_state(o, target, prime);
            if let Some(old) = old_output_routes.get(route_idx) {
                fresh.buffer.seed_last_frame_from(&old.buffer);
            }
            Arc::new(fresh)
        })
        .collect();

    // Step 2.5: Refresh stream_handles — picks up new handles from rebuilt blocks
    // (e.g. block param changed → new processor → new Arc; old Arc in map would be stale)
    {
        let mut handles = lock_recover(&runtime.stream_handles, "stream_handles");
        handles.clear();
        for input_state in &new_input_states {
            for block in &input_state.blocks {
                if let Some(ref handle) = block.stream_handle {
                    handles.insert(block.block_id.clone(), Arc::clone(handle));
                }
            }
        }
    }

    // Step 3: Swap in new state (brief lock). The OLD nodes are NOT dropped
    // inside the critical section: dropping them runs the NAM C++ destructor
    // (frees the model) and the IR FFT state — multi-ms work. The audio
    // worker only try_locks `processing`; holding the lock through those
    // destructors made it emit silence for 3-6 buffers on every cab/model
    // swap (issue #670, owner's click when switching the CAB/IR — reproduced
    // on the real interface, 64-384 underruns per swap). The old Vec is moved
    // out and dropped AFTER the lock is released.
    let old_input_states;
    {
        let mut processing = lock_recover(&runtime.processing, "chain runtime");
        old_input_states = std::mem::replace(&mut processing.input_states, new_input_states);
        // Issue #580: keep the lock-free `stream_count` mirror in sync
        // with the new Vec length. Updated INSIDE the same critical
        // section that swaps the Vec so any concurrent reader sees a
        // consistent (new Vec length, new count) pair after the lock
        // releases. Relaxed ordering — the value is purely advisory for
        // the meter timer's subscription loop.
        runtime.stream_count.store(
            processing.input_states.len(),
            std::sync::atomic::Ordering::Relaxed,
        );
        // Issue #580 follow-up: refresh the lock-free bypass mirror from the
        // new nodes so re-enabling a (still) born-disabled block keeps
        // declining the fast path. Swapped inside the same critical section
        // as the Vec so a reader never sees a stale (nodes, bypass-set) pair.
        runtime
            .bypass_block_ids
            .store(Arc::new(collect_bypass_block_ids(&processing.input_states)));

        // Rebuild input_to_segments mapping from current segments
        let max_input_idx = segments
            .iter()
            .map(|s| s.cpal_input_index)
            .max()
            .unwrap_or(0);
        let mut new_mapping: Vec<Vec<usize>> = vec![Vec::new(); max_input_idx + 1];
        for (seg_idx, segment) in segments.iter().enumerate() {
            if segment.cpal_input_index < new_mapping.len() {
                new_mapping[segment.cpal_input_index].push(seg_idx);
            }
        }
        processing.input_to_segments = new_mapping;
        // Cancel any in-flight latency probe — its beep was pushed into
        // the old queue that we're about to discard, so leaving the state
        // Fired would wait forever for a detection that will never happen.
        runtime
            .probe_state
            .store(PROBE_IDLE, std::sync::atomic::Ordering::Release);
        // Resize scratches to match the new input count, preserving existing
        // allocated capacity for slots that still exist.
        let new_len = processing.input_to_segments.len();
        processing
            .input_scratches
            .resize_with(new_len, InputCallbackScratch::default);
    }
    // Lock released — NOW the old nodes (NAM models, IR FFT states) may run
    // their multi-ms destructors without starving the audio worker (#670).
    drop(old_input_states);

    // Seed each new buffer with the previous buffer's last pushed frame so a
    // brief underrun during the transition repeats the tail of the old audio
    // rather than jumping to silence. We can't migrate queued frames across
    // the swap without introducing locks, but the SPSC's underrun fallback
    // plus a matching `last_frame` makes the seam inaudible for the target
    // scenario (param tweaks that rebuild processors in place).
    if !reset_output_queue {
        let old_routes = runtime.output_routes.load();
        for (new_route, old_route) in new_output_routes.iter().zip(old_routes.iter()) {
            new_route.buffer.seed_last_frame_from(&old_route.buffer);
        }
    }
    runtime.output_routes.store(Arc::new(new_output_routes));

    // Issue #440: chain edits (incluindo o slider de volume) re-aplicam
    // o preset.volume no master output sem destruir o runtime — atomic store
    // que o audio thread vê na próxima callback.
    runtime.set_volume_pct(chain.volume);

    Ok(())
}
