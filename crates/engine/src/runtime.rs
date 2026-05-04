use std::sync::atomic::Ordering;
use std::sync::Arc;

use block_core::AudioChannelLayout;
use crossbeam_queue::ArrayQueue;

use crate::stream_tap::StreamTap;

// Public elastic-target API moved to `runtime_audio_frame` (where ElasticBuffer
// lives); re-exported here so external callers `engine::runtime::*` keep working.
pub use crate::runtime_audio_frame::{
    elastic_target_for_buffer, DEFAULT_ELASTIC_TARGET, ELASTIC_TARGET_FLOOR,
};

// Re-export the audio-frame primitives so tests in `runtime_tests.rs` keep using
// `super::AudioFrame` etc., and the rest of `runtime.rs` keeps the old call sites.
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
pub(crate) use crate::runtime_state::{
    BlockRuntimeNode, ChainProcessingState, FadeState, InputCallbackScratch, InputProcessingState,
    RuntimeProcessor, FADE_IN_FRAMES,
};
// Test-only — `runtime_tests.rs` references SelectRuntimeState via `super::`.
// `ProcessorBuildOutcome` only used inside `runtime_graph.rs` after slice 3.
#[cfg(test)]
pub(crate) use crate::runtime_state::SelectRuntimeState;

// Slice 6 of Phase 2: probe state machine + probe impl methods lifted to
// runtime_probe.rs. Re-exports below preserve `crate::runtime::PROBE_*`
// paths in runtime_graph.rs and probe.rs.
use crate::runtime_probe::{PROBE_ARMED, PROBE_DETECT_THRESHOLD, PROBE_FIRED};
pub(crate) use crate::runtime_probe::{PROBE_BEEP_FRAMES, PROBE_BEEP_FREQ, PROBE_IDLE};

// Slice 3: graph + block builders. External callers keep using
// `engine::runtime::*` paths via these re-exports.
// Slice 7: endpoint resolution + segmentation moved to runtime_endpoints
// and runtime_segments respectively; re-exported here for tests.
#[cfg(test)]
pub(crate) use crate::runtime_block_builders::{
    bypass_runtime_node, next_block_instance_serial, processor_scratch,
};
#[cfg(test)]
pub(crate) use crate::runtime_endpoints::{
    effective_inputs, effective_outputs, insert_return_as_input_entry, insert_send_as_output_entry,
};
pub use crate::runtime_graph::{
    build_chain_runtime_state, build_runtime_graph, update_chain_runtime_state, RuntimeGraph,
};
#[cfg(test)]
pub(crate) use crate::runtime_graph::{build_output_routing_state, ERROR_QUEUE_CAPACITY};
#[cfg(test)]
pub(crate) use crate::runtime_segments::split_chain_into_segments;

// Slices 5+5b: helpers split by what they actually do
// (runtime_dsp / runtime_layout / runtime_io).
#[cfg(test)]
pub(crate) use crate::runtime_dsp::{apply_mixdown, output_limiter};
use crate::runtime_dsp::{blend_frame, ensure_flush_to_zero};
use crate::runtime_io::write_output_frame;
#[cfg(test)]
pub(crate) use crate::runtime_layout::layout_from_channels;
pub(crate) use crate::runtime_layout::layout_label;
use std::any::Any;

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

    // If a latency probe is armed, replace the first portion of this
    // callback's input with a short sine beep and record the injection
    // time. Only the primary input (index 0) probes so we measure the
    // round-trip of the user-visible signal path.
    let probe_buf: Option<Vec<f32>> = if input_index == 0
        && runtime.probe_state.load(Ordering::Acquire) == PROBE_ARMED
    {
        runtime.probe_state.store(PROBE_FIRED, Ordering::Release);
        let injected_at = runtime.created_at.elapsed().as_nanos() as u64;
        runtime
            .last_input_nanos
            .store(injected_at, Ordering::Relaxed);
        let mut buf = data.to_vec();
        let beep_frames = PROBE_BEEP_FRAMES.min(num_frames);
        // The audible pitch of the beep is approximate — we use the
        // nominal 48 kHz for the sine step. The measurement itself
        // does not depend on the beep's frequency.
        let sr = 48_000.0_f32;
        for f in 0..beep_frames {
            let t = f as f32 / sr;
            let envelope = (std::f32::consts::PI * f as f32 / beep_frames as f32).sin();
            let sample = (2.0 * std::f32::consts::PI * PROBE_BEEP_FREQ * t).sin() * 0.95 * envelope;
            for ch in 0..input_total_channels {
                buf[f * input_total_channels + ch] = sample;
            }
        }
        Some(buf)
    } else {
        None
    };
    let data: &[f32] = match probe_buf.as_ref() {
        Some(b) => b.as_slice(),
        None => data,
    };

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
    } = &mut *processing_guard;

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
    let stream_taps = runtime.stream_taps.load();
    for i in 0..scratch.segment_indices.len() {
        let seg_idx = scratch.segment_indices[i];
        process_single_segment(
            input_states,
            &mut scratch,
            seg_idx,
            data,
            input_total_channels,
            num_frames,
            &runtime.error_queue,
            &stream_taps,
        );
    }

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

fn process_single_segment(
    input_states: &mut [InputProcessingState],
    scratch: &mut InputCallbackScratch,
    seg_idx: usize,
    data: &[f32],
    input_total_channels: usize,
    num_frames: usize,
    error_queue: &ArrayQueue<BlockError>,
    stream_taps: &[Arc<StreamTap>],
) {
    let input_state = match input_states.get_mut(seg_idx) {
        Some(s) => s,
        None => return,
    };

    let InputProcessingState {
        input_read_layout,
        processing_layout,
        input_channels,
        blocks,
        frame_buffer,
        fade_in_remaining,
        output_route_indices,
        split_mono_sibling_count,
    } = input_state;

    frame_buffer.clear();
    if num_frames > frame_buffer.capacity() {
        frame_buffer.reserve(num_frames - frame_buffer.capacity());
    }

    for frame in data.chunks(input_total_channels).take(num_frames) {
        let raw_frame = read_input_frame(*input_read_layout, input_channels, frame);
        let chain_frame = match (*input_read_layout, *processing_layout) {
            (AudioChannelLayout::Mono, AudioChannelLayout::Stereo) => {
                let sample = match raw_frame {
                    AudioFrame::Mono(s) => s,
                    _ => unreachable!(),
                };
                AudioFrame::Stereo([sample, sample])
            }
            _ => raw_frame,
        };
        frame_buffer.push(chain_frame);
    }

    for block in blocks.iter_mut() {
        process_audio_block(block, frame_buffer.as_mut_slice(), error_queue);
    }

    // Per-stream sample tap (post-FX, pre-mixdown). The Spectrum window
    // subscribes per-stream so it can show one analyzer per input source
    // even when several inputs share an output device. `frame_buffer`
    // here holds this stream's processed signal in chronological order;
    // we publish each frame's L+R into the matching tap's two SPSC
    // rings. Mono frames are broadcast (L = R = sample) so the consumer
    // sees stereo regardless of the stream's processing layout.
    //
    // Dispatch is `O(num_taps_for_this_stream × num_frames)` and uses
    // only `SpscRing::push` (lock-free, allocation-free). When no taps
    // are registered, `stream_taps` is empty and the loop is skipped —
    // the cost on the audio thread is then a single `is_empty()` check.
    if !stream_taps.is_empty() {
        for tap in stream_taps.iter() {
            if tap.stream_index != seg_idx {
                continue;
            }
            for &frame in frame_buffer.iter() {
                let (l, r) = match frame {
                    AudioFrame::Mono(s) => (s, s),
                    AudioFrame::Stereo([l, r]) => (l, r),
                };
                tap.l_ring.push(l);
                tap.r_ring.push(r);
            }
        }
    }

    if *fade_in_remaining > 0 {
        let fade_total = FADE_IN_FRAMES as f32;
        for frame in frame_buffer.iter_mut() {
            if *fade_in_remaining == 0 {
                break;
            }
            let progress = 1.0 - (*fade_in_remaining as f32 / fade_total);
            let gain = 0.5 * (1.0 - (std::f32::consts::PI * progress).cos());
            match frame {
                AudioFrame::Mono(s) => *s *= gain,
                AudioFrame::Stereo([l, r]) => {
                    *l *= gain;
                    *r *= gain;
                }
            }
            *fade_in_remaining -= 1;
        }
    }

    // Mix this segment's frame_buffer into scratch.mixed_per_route for
    // each route this segment feeds. CLAUDE.md invariant #10 (issue #355):
    // NOTHING in this engine alters per-stream volume without an explicit
    // user request. Every segment — split-mono sibling or not — contributes
    // at UNITY GAIN. The previous `1/N` preemptive attenuation introduced
    // by #350 has been removed: it silently halved a solo guitar's volume
    // in any chain configured for split-mono. Saturation from N loud
    // streams summing into one route is the `output_limiter`'s job (tanh
    // above 0.95) — that limiter is already designed to hold the sum
    // transparent below 0 dBFS and apply gentle saturation above. Adding
    // a preemptive scale before it is a category error.
    //
    // `split_mono_sibling_count` is preserved on the state as structural
    // metadata in case a FUTURE feature needs a user-opt-in auto-mix UI
    // toggle — but until that feature ships with explicit user approval,
    // this multiplier MUST stay at 1.0. Pinned via `volume_invariants_tests`.
    let _ = split_mono_sibling_count;
    let split_scale: f32 = 1.0;
    let scale_frame = |frame: AudioFrame| -> AudioFrame {
        if (split_scale - 1.0).abs() < f32::EPSILON {
            return frame;
        }
        match frame {
            AudioFrame::Mono(s) => AudioFrame::Mono(s * split_scale),
            AudioFrame::Stereo([l, r]) => AudioFrame::Stereo([l * split_scale, r * split_scale]),
        }
    };

    for &route_idx in output_route_indices.iter() {
        let buf = scratch.mixed_per_route.entry(route_idx).or_default();
        if buf.is_empty() {
            for &frame in frame_buffer.iter() {
                buf.push(scale_frame(frame));
            }
        } else {
            for (i, &frame) in frame_buffer.iter().enumerate() {
                if i < buf.len() {
                    let to_add = scale_frame(frame);
                    buf[i] = match (buf[i], to_add) {
                        (AudioFrame::Stereo([l1, r1]), AudioFrame::Stereo([l2, r2])) => {
                            AudioFrame::Stereo([l1 + l2, r1 + r2])
                        }
                        (AudioFrame::Mono(a), AudioFrame::Mono(b)) => AudioFrame::Mono(a + b),
                        (AudioFrame::Stereo([l, r]), AudioFrame::Mono(m)) => {
                            AudioFrame::Stereo([l + m, r + m])
                        }
                        (AudioFrame::Mono(m), AudioFrame::Stereo([l, r])) => {
                            AudioFrame::Stereo([m + l, m + r])
                        }
                    };
                }
            }
        }
    }
}

fn process_audio_block(
    block: &mut BlockRuntimeNode,
    frames: &mut [AudioFrame],
    error_queue: &ArrayQueue<BlockError>,
) {
    // Copy the fade state (it's Copy) so we can call apply_block_processor without
    // holding a borrow into block.fade_state at the same time.
    match block.fade_state {
        FadeState::Bypassed => {
            // Fully bypassed — no processing, no fade. Hard skip.
        }
        FadeState::Active => {
            apply_block_processor(block, frames, error_queue);
        }
        FadeState::FadingIn { frames_remaining } => {
            // Crossfade: dry → wet (block fading in)
            let dry: Vec<AudioFrame> = frames.to_vec();
            apply_block_processor(block, frames, error_queue);
            let fade_total = FADE_IN_FRAMES as f32;
            for (i, frame) in frames.iter_mut().enumerate() {
                if frames_remaining <= i {
                    break;
                }
                let remaining = frames_remaining - i;
                // progress: 0.0 at start of fade, 1.0 at end
                let progress = 1.0 - (remaining as f32 / fade_total);
                let wet_gain = 0.5 * (1.0 - (std::f32::consts::PI * progress).cos());
                let dry_gain = 1.0 - wet_gain;
                blend_frame(frame, dry[i], dry_gain, wet_gain);
            }
            let new_remaining = frames_remaining.saturating_sub(frames.len());
            block.fade_state = if new_remaining == 0 {
                FadeState::Active
            } else {
                FadeState::FadingIn {
                    frames_remaining: new_remaining,
                }
            };
        }
        FadeState::FadingOut { frames_remaining } => {
            // Crossfade: wet → dry (block fading out / being disabled)
            // We still process audio so we can fade out smoothly
            let dry: Vec<AudioFrame> = frames.to_vec();
            apply_block_processor(block, frames, error_queue);
            let fade_total = FADE_IN_FRAMES as f32;
            for (i, frame) in frames.iter_mut().enumerate() {
                if frames_remaining <= i {
                    break;
                }
                let remaining = frames_remaining - i;
                // progress: 0.0 at start of fade-out, 1.0 at end
                let progress = 1.0 - (remaining as f32 / fade_total);
                // wet_gain: 1.0 at start, 0.0 at end (cosine fade-out)
                let wet_gain = 0.5 * (1.0 + (std::f32::consts::PI * progress).cos());
                let dry_gain = 1.0 - wet_gain;
                blend_frame(frame, dry[i], dry_gain, wet_gain);
            }
            let new_remaining = frames_remaining.saturating_sub(frames.len());
            block.fade_state = if new_remaining == 0 {
                FadeState::Bypassed
            } else {
                FadeState::FadingOut {
                    frames_remaining: new_remaining,
                }
            };
        }
    }
}

fn apply_block_processor(
    block: &mut BlockRuntimeNode,
    frames: &mut [AudioFrame],
    error_queue: &ArrayQueue<BlockError>,
) {
    if block.faulted {
        return;
    }
    match &mut block.processor {
        RuntimeProcessor::Audio(processor) => {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                processor.process_buffer(frames, &mut block.scratch);
            }));
            if let Err(payload) = result {
                block.faulted = true;
                for frame in frames.iter_mut() {
                    *frame = AudioFrame::Stereo([0.0, 0.0]);
                }
                let msg = downcast_panic_message(payload);
                log::error!(
                    "block '{}' panicked — permanently bypassed: {}",
                    block.block_id.0,
                    msg
                );
                // Lock-free push. If the queue is full (UI hasn't drained
                // for a long time), the error is dropped silently — this
                // path is only reached on processor panic, which already
                // logs above and faults the block. Losing the queued copy
                // is far better than blocking the audio thread.
                let _ = error_queue.push(BlockError {
                    block_id: block.block_id.clone(),
                    message: msg,
                });
            }
        }
        RuntimeProcessor::Select(select) => {
            if let Some(selected) = select.selected_node_mut() {
                process_audio_block(selected, frames, error_queue);
            }
        }
        RuntimeProcessor::Bypass => {}
    }
}

/// Pull a string out of a `catch_unwind` payload so a faulted DSP block
/// can be reported via `BlockError` instead of taking down the audio
/// thread. Lives here next to its only caller (`apply_block_processor`).
fn downcast_panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
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
    let num_frames = out.len() / output_total_channels;
    for frame in out.chunks_mut(output_total_channels).take(num_frames) {
        frame.fill(0.0);
        let processed = route.buffer.pop();
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

/// Soft limiter — transparent below 0dBFS, gentle saturation above.

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "stream_isolation_tests.rs"]
mod stream_isolation;

#[cfg(test)]
#[path = "volume_invariants_tests.rs"]
mod volume_invariants;

#[cfg(test)]
#[path = "audio_deadline_tests.rs"]
mod audio_deadline;

#[cfg(test)]
#[path = "audio_signal_integrity_tests.rs"]
mod audio_signal_integrity;
