//! Per-segment audio processing (issue #792 split from `runtime.rs`).
//!
//! Audio-thread hot path. Called from `process_input_f32` once per segment
//! per callback. RT-safe: no allocation, no locking, no syscalls — the same
//! contract as the rest of the callback. Pure mechanical move out of
//! `runtime.rs`; every function is byte-identical to its previous inline form.

use std::any::Any;
use std::sync::Arc;

use block_core::AudioChannelLayout;
use crossbeam_queue::ArrayQueue;

use crate::runtime_audio_frame::{read_input_frame, AudioFrame};
use crate::runtime_dsp::{blend_frame, ensure_flush_to_zero};
use crate::runtime_state::{
    BlockError, BlockRuntimeNode, FadeState, InputCallbackScratch, InputProcessingState,
    RuntimeProcessor, FADE_IN_FRAMES,
};
use crate::stream_tap::StreamTap;

/// #454-T5 spillover tail: feed the retained previous pipeline SILENCE so
/// only its delay/reverb tail emits, equal-power fade it out over the
/// spillover window, and sum it into `frame_buffer` (the caller pushes once
/// per route afterwards, so SPSC stays intact). `None` ⇒ pure no-op,
/// byte-identical to pre-#454-T5. RT-safe: no alloc/lock (the `scratch`
/// Vec is pre-grown and only `clear()`/`push()` up to its capacity).
#[inline]
fn mix_outgoing_tail(
    outgoing: &mut Option<Box<crate::runtime_state::OutgoingTail>>,
    frame_buffer: &mut [AudioFrame],
    processing_layout: AudioChannelLayout,
    error_queue: &ArrayQueue<BlockError>,
) {
    if let Some(tail) = outgoing.as_mut() {
        let n = frame_buffer.len();
        let silent = match processing_layout {
            AudioChannelLayout::Stereo => AudioFrame::Stereo([0.0, 0.0]),
            _ => AudioFrame::Mono(0.0),
        };
        tail.scratch.clear();
        if n > tail.scratch.capacity() {
            tail.scratch.reserve(n - tail.scratch.capacity());
        }
        for _ in 0..n {
            tail.scratch.push(silent);
        }
        for block in tail.blocks.iter_mut() {
            process_audio_block(block, tail.scratch.as_mut_slice(), error_queue);
        }
        let total = crate::runtime_state::SPILLOVER_FRAMES as f32;
        for (i, fb) in frame_buffer.iter_mut().enumerate() {
            if tail.frames_remaining == 0 {
                break;
            }
            let progress = 1.0 - (tail.frames_remaining as f32 / total).min(1.0);
            // wet→dry equal-power: 1.0 at switch, 0.0 at window end.
            let g = 0.5 * (1.0 + (std::f32::consts::PI * progress).cos());
            let t = tail.scratch[i];
            match (fb, t) {
                (AudioFrame::Stereo([l, r]), AudioFrame::Stereo([tl, tr])) => {
                    *l += tl * g;
                    *r += tr * g;
                }
                (AudioFrame::Mono(s), AudioFrame::Mono(ts)) => {
                    *s += ts * g;
                }
                (AudioFrame::Stereo([l, r]), AudioFrame::Mono(ts)) => {
                    *l += ts * g;
                    *r += ts * g;
                }
                (AudioFrame::Mono(s), AudioFrame::Stereo([tl, tr])) => {
                    *s += (tl + tr) * 0.5 * g;
                }
            }
            tail.frames_remaining -= 1;
        }
        if tail.frames_remaining == 0 {
            *outgoing = None;
        }
    }
}

/// What fills a segment's frame buffer for one callback (issue #699).
/// `Live` reads the device frames; `Loop` substitutes the armed DI loop
/// (first segment only); `Silence` mutes the segment while a loop is
/// armed elsewhere in the chain.
#[derive(Clone, Copy)]
pub(crate) enum SegmentFeed<'a> {
    Live,
    Loop(&'a crate::di_loop::DiLoop, usize),
    Silence,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn process_single_segment(
    input_states: &mut [InputProcessingState],
    scratch: &mut InputCallbackScratch,
    seg_idx: usize,
    data: &[f32],
    input_total_channels: usize,
    num_frames: usize,
    error_queue: &ArrayQueue<BlockError>,
    stream_taps: &[Arc<StreamTap>],
    feed: SegmentFeed<'_>,
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
        outgoing,
    } = input_state;

    frame_buffer.clear();
    if num_frames > frame_buffer.capacity() {
        frame_buffer.reserve(num_frames - frame_buffer.capacity());
    }

    match feed {
        SegmentFeed::Silence => {
            // #699: a DI loop is armed and plays in another segment — this
            // segment is muted for the callback (DI replaces ALL live input).
            let silent = match *processing_layout {
                AudioChannelLayout::Stereo => AudioFrame::Stereo([0.0, 0.0]),
                AudioChannelLayout::Mono => AudioFrame::Mono(0.0),
            };
            for _ in 0..num_frames {
                frame_buffer.push(silent);
            }
            let _ = (input_read_layout, input_channels);
        }
        SegmentFeed::Loop(di_loop, start_pos) => {
            use crate::di_loop::DiFrame;
            for i in 0..num_frames {
                let f = di_loop.frame_at(start_pos.wrapping_add(i));
                let chain_frame = match (*processing_layout, f) {
                    (AudioChannelLayout::Stereo, DiFrame::Mono(s)) => AudioFrame::Stereo([s, s]),
                    (AudioChannelLayout::Stereo, DiFrame::Stereo(lr)) => AudioFrame::Stereo(lr),
                    (AudioChannelLayout::Mono, DiFrame::Mono(s)) => AudioFrame::Mono(s),
                    (AudioChannelLayout::Mono, DiFrame::Stereo([l, r])) => {
                        AudioFrame::Mono((l + r) * 0.5)
                    }
                };
                frame_buffer.push(chain_frame);
            }
            let _ = (input_read_layout, input_channels);
        }
        SegmentFeed::Live => {
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
        }
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

    // #454-T5 spillover: the previous pipeline keeps decaying in parallel,
    // fed silence, equal-power faded, summed into THIS segment's
    // frame_buffer *before* the single per-route push below (so there is
    // still exactly one producer per output ring — SPSC intact). Extracted
    // to keep this function's cognitive complexity in budget; `None` ⇒ it
    // is a no-op and behaviour is byte-identical to pre-#454-T5.
    mix_outgoing_tail(outgoing, frame_buffer, *processing_layout, error_queue);

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

pub(crate) fn process_audio_block(
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
            // Issue #400 bug #4: reuse pre-allocated buffer instead of
            // `frames.to_vec()` (which allocates on every audio callback).
            // `mem::take` swaps with Vec::new() (zero alloc); clear() keeps
            // capacity; extend_from_slice() only reallocs if capacity is
            // exceeded — after the first call, capacity is sufficient and
            // this path is alloc-free.
            let mut dry = std::mem::take(&mut block.fade_dry_buffer);
            dry.clear();
            dry.extend_from_slice(frames);
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
            block.fade_dry_buffer = dry;
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
            // We still process audio so we can fade out smoothly.
            // Issue #400 bug #4: same alloc-free pattern as FadingIn.
            let mut dry = std::mem::take(&mut block.fade_dry_buffer);
            dry.clear();
            dry.extend_from_slice(frames);
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
            block.fade_dry_buffer = dry;
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

pub(crate) fn apply_block_processor(
    block: &mut BlockRuntimeNode,
    frames: &mut [AudioFrame],
    error_queue: &ArrayQueue<BlockError>,
) {
    if block.faulted {
        return;
    }
    // Issue #670: re-arm flush-to-zero before EVERY block. The engine arms FZ
    // once per callback, but a C++ block (NAM A2 inference, LV2 reverb) can
    // clear the FPCR FZ bit mid-chain; every block after it then processes the
    // note's decaying (subnormal) tail on the FPU gradual-underflow path, and a
    // single 64-frame buffer stalls ~100x — blowing the deadline as the audio
    // overload / "beehive" (reproduced by beat_it_green_day_di_analysis: the
    // heaviest buffers are all on quiet/decay passages). One cheap FPCR check
    // per block guarantees no block ever runs denormals unprotected.
    ensure_flush_to_zero();
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
pub(crate) fn downcast_panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}
