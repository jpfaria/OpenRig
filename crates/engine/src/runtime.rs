use std::sync::atomic::Ordering;
use std::sync::Arc;

// Public elastic-target API moved to `runtime_audio_frame` (where ElasticBuffer
// lives); re-exported here so external callers `engine::runtime::*` keep working.
pub use crate::runtime_audio_frame::{
    elastic_target_for_buffer, DEFAULT_ELASTIC_TARGET, ELASTIC_TARGET_FLOOR,
};

// Re-export the audio-frame primitives so tests in `runtime_tests.rs` keep using
// `super::AudioFrame` / `super::read_input_frame`. The production body no longer
// references them after the #792 segment-processing split moved the per-segment
// DSP out to `runtime_process_segment.rs`.
#[cfg(test)]
pub(crate) use crate::runtime_audio_frame::{read_input_frame, AudioFrame};
// Test-only re-exports: these audio-frame types and processor variants are used
// by `runtime_tests.rs` via `super::` but the production body of `runtime.rs`
// itself doesn't reference them after slice 3 (the builders that did moved to
// `runtime_graph.rs`).
#[cfg(test)]
pub(crate) use crate::runtime_audio_frame::{AudioProcessor, ElasticBuffer, ProcessorScratch};
// Test-only re-exports: these helpers are used by `runtime_tests.rs` but not by
// the production body of `runtime.rs` itself.
#[cfg(test)]
pub(crate) use crate::runtime_audio_frame::{mix_frames, read_channel, silent_frame};

// Slice 2 of Phase 2: state structs lifted to runtime_state.rs.
// Slice 6 of Phase 2: ChainRuntimeState struct + impl + FADE_IN_FRAMES
// also lifted to runtime_state.rs (it's the root state, fits with the
// support state types that already lived there).
// `BlockError` and `ChainRuntimeState` stay `pub` (re-exported as
// `engine::runtime::BlockError` / `ChainRuntimeState` from infra-cpal /
// adapter-console). The rest are `pub(crate)`.
pub use crate::runtime_state::{BlockError, ChainRuntimeState};
pub(crate) use crate::runtime_state::{ChainProcessingState, InputCallbackScratch, FADE_IN_FRAMES};
// Test-only — `runtime_tests.rs` references SelectRuntimeState via `super::`.
// `ProcessorBuildOutcome` only used inside `runtime_graph.rs` after slice 3.
#[cfg(test)]
pub(crate) use crate::runtime_state::SelectRuntimeState;

// Slice 6 of Phase 2: probe state machine + probe impl methods lifted to
// runtime_probe.rs. Re-exports below preserve `crate::runtime::PROBE_*`
// paths in runtime_graph.rs and probe.rs.
use crate::runtime_probe::{PROBE_ARMED, PROBE_DETECT_THRESHOLD, PROBE_FIRED};
pub(crate) use crate::runtime_probe::{PROBE_BEEP_FRAMES, PROBE_IDLE};

// Slice 3: graph + block builders. External callers keep using
// `engine::runtime::*` paths via these re-exports.
// Slice 7: endpoint resolution + segmentation moved to runtime_endpoints
// and runtime_segments respectively; re-exported here for tests.
#[cfg(test)]
pub(crate) use crate::runtime_block_builders::{
    bypass_runtime_node, next_block_instance_serial, processor_scratch,
};
pub use crate::runtime_block_toggle::set_block_enabled;
#[cfg(test)]
pub(crate) use crate::runtime_endpoints::{
    effective_inputs, effective_outputs, insert_return_as_input_entry, insert_send_as_output_entry,
};
pub use crate::runtime_graph::{
    build_chain_runtime_state, build_per_input_runtime_states, build_runtime_graph,
    update_chain_runtime_state, update_chain_runtime_state_spillover, RuntimeGraph,
};
#[cfg(test)]
pub(crate) use crate::runtime_graph::{build_output_routing_state, ERROR_QUEUE_CAPACITY};
#[cfg(test)]
pub(crate) use crate::runtime_segments::split_chain_into_segments;

// Slices 5+5b: helpers split by what they actually do
// (runtime_dsp / runtime_layout / runtime_io).
#[cfg(test)]
pub(crate) use crate::runtime_dsp::apply_mixdown;
use crate::runtime_dsp::ensure_flush_to_zero;
use crate::runtime_dsp::output_limiter;
use crate::runtime_io::write_output_frame;
#[cfg(test)]
pub(crate) use crate::runtime_layout::layout_from_channels;
pub(crate) use crate::runtime_layout::layout_label;
use crate::runtime_process_segment::{process_single_segment, SegmentFeed};
// Test-only re-exports: the #792 segment-processing split moved these out of
// runtime.rs's production body, but runtime_tests.rs (and sibling #[path] test
// modules that `use super::*`) still reference them via `super::`.
#[cfg(test)]
pub(crate) use crate::runtime_process_segment::{
    apply_block_processor, downcast_panic_message, process_audio_block,
};
#[cfg(test)]
pub(crate) use crate::runtime_dsp::blend_frame;
#[cfg(test)]
pub(crate) use crate::runtime_state::{BlockRuntimeNode, FadeState, RuntimeProcessor};
#[cfg(test)]
pub(crate) use block_core::AudioChannelLayout;

pub fn process_input_f32(
    runtime: &Arc<ChainRuntimeState>,
    input_index: usize,
    data: &[f32],
    input_total_channels: usize,
) {
    if runtime.is_draining() {
        return;
    }
    ensure_flush_to_zero();
    let num_frames = data.len() / input_total_channels;

    // Take the processing lock FIRST so we only commit the probe state
    // transition when we are certain the beep will flow through the rest
    // of the pipeline. If try_lock fails (config rebuild in flight) we
    // leave the probe state Armed and retry on the next callback.
    let mut processing_guard = match runtime.processing.try_lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };

    // Issue #580 follow-up: drain queued block-toggle requests inside
    // the same lock we already hold. The GUI thread's
    // `set_block_enabled` is now lock-free (push to ArrayQueue) so it
    // never contends with this `try_lock` — the click the user heard
    // on every UI block on/off is gone. Cheap: the queue is empty in
    // steady state, a `pop` is two atomic ops, and a non-empty drain
    // does one in-place walk over `input_states.blocks` per toggle.
    crate::runtime_block_toggle::drain_pending_block_toggles(runtime, &mut processing_guard);

    // If a latency probe is armed, replace the first portion of this
    // callback's input with a short sine beep and record the injection
    // time. Only the primary input (index 0) probes so we measure the
    // round-trip of the user-visible signal path.
    let probe_buf: Option<Vec<f32>> =
        if input_index == 0 && runtime.probe_state.load(Ordering::Acquire) == PROBE_ARMED {
            runtime.probe_state.store(PROBE_FIRED, Ordering::Release);
            let injected_at = runtime.created_at.elapsed().as_nanos() as u64;
            runtime
                .last_input_nanos
                .store(injected_at, Ordering::Relaxed);
            let mut buf = data.to_vec();
            let beep_frames = PROBE_BEEP_FRAMES.min(num_frames);
            // Synthesize the beep at the runtime's REAL rate (issue #723), not a
            // hardcoded 48 kHz. The measurement is timing-based, but the audible
            // pitch should still be a true 1 kHz on any device rate.
            crate::runtime_probe::write_probe_beep(
                &mut buf,
                input_total_channels,
                runtime.sample_rate,
                beep_frames,
            );
            Some(buf)
        } else {
            None
        };
    let data: &[f32] = match probe_buf.as_ref() {
        Some(b) => b.as_slice(),
        None => data,
    };

    // ── Virtual DI loop (issue #614) ─────────────────────────────────────
    // If a DI loop is published for this chain, every segment reads the loop
    // instead of the device frame. Lock-free (ArcSwapOption load) and
    // zero-alloc: we pass a borrow + a shared start cursor into the segments,
    // and advance the cursor once per callback below. `None` ⇒ one branch,
    // then identical to today's device path. Input taps below intentionally
    // keep reading the device `data` (the tuner tracks the real input).
    let di_guard = runtime.di_loop.load();
    let di_ref: Option<&crate::di_loop::DiLoop> = di_guard.as_deref();
    let di_start = match di_ref {
        Some(_) => runtime.di_loop_pos.load(Ordering::Relaxed),
        None => 0,
    };
    let di_for_seg = di_ref.map(|d| (d, di_start));

    // ── Per-channel sample taps (pre-FX) ─────────────────────────────────
    // Top-level features (Tuner / Spectrum windows) subscribe to raw input
    // samples here. Empty Vec = zero subscribers; the early continue keeps
    // the cost to a single ArcSwap load per callback.
    {
        let taps = runtime.input_taps.load();
        if !taps.is_empty() {
            for tap in taps.iter() {
                if tap.input_index != input_index {
                    continue;
                }
                for (ch_idx, ring_opt) in tap.channel_rings.iter().enumerate() {
                    if let Some(ring) = ring_opt {
                        if ch_idx >= input_total_channels {
                            continue;
                        }
                        for f in 0..num_frames {
                            // SpscRing::push drops on full — safe under
                            // back-pressure from a slow consumer.
                            let _ = ring.push(data[f * input_total_channels + ch_idx]);
                        }
                    }
                }
            }
        }
    }

    let ChainProcessingState {
        input_states,
        input_to_segments,
        input_scratches,
        looper_bank,
    } = &mut *processing_guard;

    // #323: apply the loopers' queued transport/param ops before any segment
    // runs, so a footswitch tap takes effect on the callback that follows it.
    looper_bank.drain_ops(&runtime.loopers);

    // Temporarily take the scratch for this input_index to work around the
    // aliasing rules: we'll put it back before returning. If the slot does
    // not exist we fall back to a scratch allocated on the stack.
    let mut scratch = match input_scratches.get_mut(input_index) {
        Some(s) => std::mem::take(s),
        None => InputCallbackScratch::default(),
    };
    scratch.reset_for_callback();

    if let Some(segments) = input_to_segments.get(input_index) {
        scratch.segment_indices.extend(segments.iter().copied());
    } else if input_index < input_states.len() {
        scratch.segment_indices.push(input_index);
    }

    // Process each segment, mixing into scratch.mixed_per_route.
    //
    // Issue #699: an armed DI loop plays exactly ONCE per chain — only the
    // chain's first segment (seg_idx 0) substitutes the loop for its device
    // frames. Every other segment is fed silence while the loop is armed
    // (DI playback replaces ALL live input; before this fix every segment
    // played its own copy of the loop and the copies summed at the output).
    let stream_taps = runtime.stream_taps.load();
    for i in 0..scratch.segment_indices.len() {
        let seg_idx = scratch.segment_indices[i];
        let feed = match di_for_seg {
            Some((d, pos)) if seg_idx == 0 => SegmentFeed::Loop(d, pos),
            Some(_) => SegmentFeed::Silence,
            None => SegmentFeed::Live,
        };
        // #323: the loopers record and play on the chain's FIRST segment
        // only — like the DI loop (#699), a chain's loop material is heard
        // exactly once, no matter how many segments share the callback.
        let loopers = if seg_idx == 0 && !looper_bank.is_idle() {
            Some(&mut *looper_bank)
        } else {
            None
        };
        process_single_segment(
            input_states,
            &mut scratch,
            seg_idx,
            data,
            input_total_channels,
            num_frames,
            &runtime.error_queue,
            &stream_taps,
            feed,
            loopers,
        );
    }

    // Advance the DI loop playback cursor once per callback (after all
    // segments have consumed frames starting at `di_start`). Wraps modulo
    // loop length. The cursor is advanced here — not inside the segment
    // loop — so that parallel segments sharing the same input callback all
    // read the same window of the loop (consistent with SPSC single-producer
    // invariant and stream isolation). #699: only the callback that owns the
    // playing segment (seg 0) advances — a second device stream feeding
    // other segments of this runtime must not double-step the cursor.
    if let Some(d) = di_ref {
        if scratch.segment_indices.contains(&0) {
            let len = d.len().max(1);
            let next = di_start.wrapping_add(num_frames) % len;
            runtime.di_loop_pos.store(next, Ordering::Relaxed);
        }
    }

    // #323: publish the looper state for the UI and hand any retired layer
    // buffer back to the control thread (dropping happens off this thread).
    looper_bank.publish(&runtime.loopers);

    // Snapshot current output routes via ArcSwap — no lock.
    let routes = runtime.output_routes.load();
    for route_idx in scratch.mixed_per_route.keys() {
        if let Some(arc) = routes.get(*route_idx) {
            scratch.route_arcs.push((*route_idx, Arc::clone(arc)));
        }
    }

    // Push mixed frames to their output routes (lock-free via SPSC).
    for (route_idx, route) in &scratch.route_arcs {
        if let Some(frames) = scratch.mixed_per_route.get(route_idx) {
            for &frame in frames {
                route.buffer.push(frame);
            }
        }
    }

    if let Some(slot) = input_scratches.get_mut(input_index) {
        *slot = scratch;
    }
}

pub fn process_output_f32(
    runtime: &Arc<ChainRuntimeState>,
    output_index: usize,
    out: &mut [f32],
    output_total_channels: usize,
) {
    if runtime.is_draining() {
        out.fill(0.0);
        return;
    }
    ensure_flush_to_zero();

    // Snapshot the current routes via ArcSwap — no lock on the RT thread.
    let routes = runtime.output_routes.load();
    let route = match routes.get(output_index) {
        Some(r) => r,
        None => {
            out.fill(0.0);
            return;
        }
    };
    // Issue #440 / #350 fidelity: apply Chain.volume to the AudioFrame
    // BEFORE `write_output_frame` (which runs the output limiter). Applying
    // it after the limiter let a hot chain × volume>100 clip the DAC with
    // nothing to catch it on the single-stream path. Single atomic load of
    // volume_pct per callback. No clamp here — the limiter inside
    // write_output_frame is the gate (this file's pinned contract:
    // "clipping is the output limiter's job"). Sub-knee signals are
    // unaffected (tanh transparent below 0.95), so k01–k04 stay green.
    let volume_ratio = runtime.volume_pct() / 100.0;
    let num_frames = out.len() / output_total_channels;
    for frame in out.chunks_mut(output_total_channels).take(num_frames) {
        frame.fill(0.0);
        let mut processed = route.buffer.pop();
        if volume_ratio != 1.0 {
            processed = processed.scaled(volume_ratio);
        }
        write_output_frame(
            processed,
            &route.output_channels,
            frame,
            route.output_mixdown,
        );
    }

    // Output mute: silence the entire output stage when toggled by any
    // consumer (e.g. the Tuner window). Single atomic load — cheap.
    if runtime
        .output_muted
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        out.fill(0.0);
    }

    // Latency probe detection: only the primary output (index 0) scans.
    // When Fired, look for the leading edge of the injected beep. Measure
    // wall-clock nanos from injection to detection; that is the real
    // end-to-end latency of the signal path for the user.
    if output_index == 0 && runtime.probe_state.load(Ordering::Acquire) == PROBE_FIRED {
        let detected_at_idx = out.iter().position(|s| s.abs() > PROBE_DETECT_THRESHOLD);
        if detected_at_idx.is_some() {
            let now = runtime.created_at.elapsed().as_nanos() as u64;
            let injected_at = runtime.last_input_nanos.load(Ordering::Relaxed);
            // Measure wall-clock nanos from the input callback that
            // injected the beep to this output callback that detected
            // it. This is callback-level granularity; we intentionally
            // do NOT add the intra-buffer offset because that couples
            // the measurement to signal amplitude (through the
            // threshold-crossing position) and inflates readings for
            // chains that attenuate the signal.
            let delta = now.saturating_sub(injected_at);
            runtime
                .measured_latency_nanos
                .store(delta, Ordering::Relaxed);
            runtime.probe_state.store(PROBE_IDLE, Ordering::Release);
        }
    }
}

/// Drive one physical output device from the N per-input runtimes of a
/// chain (issue #350, phase 3). Each `InputBlock` entry on a distinct
/// physical device is its own isolated [`ChainRuntimeState`] with its own
/// SPSC output ring; the shared output device must sum them at the
/// backend — the ONLY place CLAUDE.md invariant #4 permits mixing across
/// streams. Each ring still has exactly one producer (its own input
/// callback) and is consumed once here, so SPSC is preserved.
///
/// Single-runtime chains (the 99% case, and every `volume_invariants` /
/// golden scenario) take the `[1]` fast path: `process_output_f32` writes
/// straight into `out` with ZERO extra work — byte-identical to pre-#350.
///
/// Multi-runtime: each runtime's output (already per-runtime limited +
/// volume-scaled inside `process_output_f32`) is rendered into the
/// caller-owned `scratch` and summed into `out`; the summed buffer then
/// passes through `output_limiter` (the same tanh the chain already
/// trusts to hold a multi-stream sum transparent below 0 dBFS — see the
/// route-mix note in `mix_segment_into_routes`) so the device never
/// receives a clipped buffer. `scratch` MUST be pre-allocated by the
/// caller at stream-build time and be at least `out.len()` long — this
/// function performs ZERO allocation and ZERO locking on the audio
/// thread (it only does the lock-free work `process_output_f32` already
/// did, once per runtime).
pub fn process_output_f32_mixed(
    runtimes: &[Arc<ChainRuntimeState>],
    output_index: usize,
    out: &mut [f32],
    output_total_channels: usize,
    scratch: &mut [f32],
) {
    match runtimes {
        [] => out.fill(0.0),
        // Fast path: one isolated stream → byte-identical to pre-#350.
        [runtime] => process_output_f32(runtime, output_index, out, output_total_channels),
        many => {
            out.fill(0.0);
            let n = out.len();
            for runtime in many {
                let buf = &mut scratch[..n];
                process_output_f32(runtime, output_index, buf, output_total_channels);
                for (dst, src) in out.iter_mut().zip(buf.iter()) {
                    *dst += *src;
                }
            }
            // Backend mix saturation guard: N per-runtime-limited streams
            // can sum past 1.0; tanh holds it transparent below 0 dBFS.
            for s in out.iter_mut() {
                *s = output_limiter(*s);
            }
        }
    }
}

// Soft limiter — transparent below 0dBFS, gentle saturation above.

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "runtime_block_assembly_tests.rs"]
mod rt_block_assembly;

#[cfg(test)]
#[path = "runtime_effective_io_tests.rs"]
mod rt_effective_io;

#[cfg(test)]
#[path = "runtime_frame_tests.rs"]
mod rt_frame;

#[cfg(test)]
#[path = "runtime_frame_buffer_tests.rs"]
mod rt_frame_buffer;

#[cfg(test)]
#[path = "runtime_graph_tests.rs"]
mod rt_graph;

#[cfg(test)]
#[path = "runtime_integration_tests.rs"]
mod rt_integration;

#[cfg(test)]
#[path = "runtime_process_tests.rs"]
mod rt_process;

#[cfg(test)]
#[path = "stream_isolation_tests.rs"]
mod stream_isolation;

#[cfg(test)]
#[path = "stream_isolation_tests_more.rs"]
mod stream_isolation_more;

#[cfg(test)]
#[path = "stream_isolation_same_device_tests.rs"]
mod stream_isolation_same_device;

#[cfg(test)]
#[path = "volume_invariants_tests.rs"]
mod volume_invariants;

#[cfg(test)]
#[path = "volume_chain_tests.rs"]
mod vol_chain;

#[cfg(test)]
#[path = "volume_splitmono_preset_tests.rs"]
mod vol_splitmono_preset;

#[cfg(test)]
#[path = "volume_spectral_audit_tests.rs"]
mod vol_spectral;

#[cfg(test)]
#[path = "volume_elastic_ring_tests.rs"]
mod vol_elastic;

#[cfg(test)]
#[path = "volume_broadcast_format_tests.rs"]
mod vol_broadcast_format;

#[cfg(test)]
#[path = "rig_spillover_tests.rs"]
mod rig_spillover;

#[cfg(test)]
#[path = "audio_deadline_tests.rs"]
mod audio_deadline;

#[cfg(test)]
#[path = "stream_count_contention_tests.rs"]
mod stream_count_contention;

#[cfg(test)]
#[path = "audio_under_gui_pressure_tests.rs"]
mod audio_under_gui_pressure;

#[cfg(test)]
#[path = "audio_alloc_invariant_tests.rs"]
mod audio_alloc_invariant;

#[cfg(test)]
#[path = "audio_alloc_real_rig_tests.rs"]
mod aa_real_rig;

#[cfg(test)]
#[path = "audio_under_block_toggle_tests.rs"]
mod audio_under_block_toggle;

#[cfg(test)]
#[path = "audio_signal_integrity_tests.rs"]
mod audio_signal_integrity;

#[cfg(test)]
#[path = "audio_signal_integrity_eq_tests.rs"]
mod audio_signal_integrity_eq;

#[cfg(test)]
#[path = "stereo_image_tests.rs"]
mod stereo_image;

#[cfg(test)]
#[path = "runtime_lock_recovery_tests.rs"]
mod runtime_lock_recovery;

#[cfg(test)]
#[path = "block_enabled_fast_path_tests.rs"]
mod block_enabled_fast_path;

#[cfg(test)]
#[path = "di_loop_state_tests.rs"]
mod di_loop_state;

#[cfg(test)]
#[path = "di_loop_injection_tests.rs"]
mod di_loop_injection;

#[cfg(test)]
#[path = "looper_runtime_tests.rs"]
mod looper_runtime;
