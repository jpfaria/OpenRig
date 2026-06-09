//! Audio frame primitives + per-block processor wrapper. Lifted out of
//! `runtime.rs` so the parent file gets closer to the size cap.
//!
//! These types live on the audio thread (every `ElasticBuffer::push/pop`
//! call, every `AudioProcessor::process_buffer` call). Splitting them into
//! a sibling module is a textual move only — same crate translation unit,
//! same `#[inline]` attributes (none), same call sites, same generated code
//! after LLVM. Per CLAUDE.md the parent issue still requires audible A/B
//! validation before merge.
//!
//! Visibility: every item is `pub(crate)` so `runtime.rs` can re-export
//! and the tests in `runtime_tests.rs` keep using `super::*`.
//!
//! Dependencies upward: none — only `block_core`, `std::sync`, and
//! `crate::spsc::SpscRing`. No reference back into `runtime.rs`.

use std::sync::atomic::{AtomicU64, Ordering};

use block_core::{AudioChannelLayout, MonoProcessor, StereoProcessor};

use crate::spsc::SpscRing;

/// Floor for the elastic buffer target. Below this the buffer cannot absorb
/// even minor scheduling jitter, regardless of how small the device buffer is.
pub const ELASTIC_TARGET_FLOOR: usize = 64;

/// Default elastic target used when no device-derived value is provided
/// (tests, headless tools). Production callers in infra-cpal compute this
/// from the resolved device buffer size via [`elastic_target_for_buffer`].
pub const DEFAULT_ELASTIC_TARGET: usize = 256;

/// Compute the elastic buffer target level (in frames) for a given device
/// buffer size and backend multiplier.
///
/// The elastic buffer absorbs jitter between the producer (input + DSP path)
/// and the consumer (output callback). Sizing it relative to the actual device
/// buffer makes the latency proportional to the user's chosen buffer size
/// instead of a hardcoded constant.
///
/// `multiplier` reflects backend-specific jitter:
/// - `2` — direct CPAL callbacks (macOS/Windows/Linux ALSA): tight, predictable.
/// - `8` — JACK with worker-thread DSP (Linux): non-RT worker adds variance.
pub fn elastic_target_for_buffer(buffer_size_frames: u32, multiplier: u8) -> usize {
    let target = (buffer_size_frames as usize).saturating_mul(multiplier as usize);
    target.max(ELASTIC_TARGET_FLOOR)
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum AudioFrame {
    Mono(f32),
    Stereo([f32; 2]),
}

impl AudioFrame {
    #[inline(always)]
    pub(crate) fn mono_mix(self) -> f32 {
        match self {
            AudioFrame::Mono(sample) => sample,
            AudioFrame::Stereo([left, right]) => (left + right) * 0.5,
        }
    }

    /// Linear gain applied to the frame. Used to apply `Chain.volume`
    /// BEFORE the output limiter so the limiter (in `write_output_frame`)
    /// sees the post-volume signal and holds a hot chain ≤ full scale
    /// instead of clipping at the DAC (volume × already-limited signal).
    #[inline(always)]
    pub(crate) fn scaled(self, k: f32) -> AudioFrame {
        match self {
            AudioFrame::Mono(s) => AudioFrame::Mono(s * k),
            AudioFrame::Stereo([l, r]) => AudioFrame::Stereo([l * k, r * k]),
        }
    }
}

/// Elastic audio buffer for clock drift compensation.
///
/// Lock-free single-producer / single-consumer. The producer is the input
/// DSP path (`process_input_f32`); the consumer is the output callback
/// (`process_output_f32`). Both call `push`/`pop` with `&self`, so there is
/// no `Mutex` in the RT audio path.
///
/// On underrun `pop` returns the most recently pushed frame, providing a
/// brief sustain instead of silence.
pub(crate) struct ElasticBuffer {
    ring: SpscRing<AudioFrame>,
    #[allow(dead_code)]
    target_level: usize,
    layout: AudioChannelLayout,
    /// Bit-packed last-pushed frame, used as the underrun fallback.
    /// Mono: `f32` bits in the low 32 bits.
    /// Stereo: left in low 32 bits, right in high 32 bits.
    last_frame_bits: AtomicU64,
    /// Issue #670 instrumentation: count of `pop`s that found the ring empty
    /// (underrun → a silent frame was emitted = an audible gap). Incremented
    /// on the output callback (RT-safe relaxed atomic, only on the rare
    /// empty branch). Read off-thread to tell an elastic-buffer underrun
    /// apart from a CPU deadline overrun (xrun): a single light chain at
    /// buffer 64 crackling with near-zero xruns points here, not at CPU.
    underrun_count: AtomicU64,
}

impl ElasticBuffer {
    pub(crate) fn new(target_level: usize, layout: AudioChannelLayout) -> Self {
        let init = silent_frame(layout);
        Self {
            ring: SpscRing::new(target_level.saturating_mul(2), init),
            target_level,
            layout,
            last_frame_bits: AtomicU64::new(frame_to_bits(init)),
            underrun_count: AtomicU64::new(0),
        }
    }

    /// Issue #670: number of underruns (empty `pop`s → silent gaps) since
    /// this buffer was built. Read off the audio thread.
    pub(crate) fn underrun_count(&self) -> u64 {
        self.underrun_count.load(Ordering::Relaxed)
    }

    #[inline(always)]
    pub(crate) fn push(&self, frame: AudioFrame) {
        self.last_frame_bits
            .store(frame_to_bits(frame), Ordering::Relaxed);
        // Drop-newest when full — the consumer is behind and a single dropped
        // sample is less disruptive than advancing the tail from the
        // producer side (which would violate the SPSC invariant).
        let _ = self.ring.push(frame);
    }

    #[inline(always)]
    pub(crate) fn pop(&self) -> AudioFrame {
        // Issue #496: the previous form returned `last_frame_bits` on
        // underrun ("brief sustain instead of silence"). Measured cost:
        // every underrun produced a flat-top plateau / DC chunk in the
        // middle of the signal, injecting broadband harmonic distortion
        // and noise (the reported swarm-of-bees artefact). Silence is
        // the standard DAW behavior: a tiny gap is musically inaudible,
        // repeated samples are not.
        match self.ring.pop() {
            Some(frame) => frame,
            None => {
                // Issue #670: underrun — the producer (input DSP) hasn't
                // delivered this frame yet. Count it; the gap is the click.
                self.underrun_count.fetch_add(1, Ordering::Relaxed);
                silent_frame(self.layout)
            }
        }
    }

    /// Pre-fill the buffer with `frames` silent frames so it starts at a
    /// real jitter cushion instead of empty. Used on the INITIAL build of a
    /// chain whose per-block worst-case latency (e.g. an IR convolver's
    /// per-partition FFT spike) can momentarily starve the consumer before
    /// the producer warms up — issue #592. The cushion costs `frames` of
    /// output latency; callers only prime when the chain warrants it.
    pub(crate) fn prime(&self, frames: usize) {
        let silence = silent_frame(self.layout);
        for _ in 0..frames {
            self.push(silence);
        }
    }

    /// Seed the underrun fallback from another buffer's last pushed frame.
    /// Used during chain rebuild so that a brief underrun on the new buffer
    /// repeats the tail of the old buffer instead of jumping to silence.
    pub(crate) fn seed_last_frame_from(&self, other: &ElasticBuffer) {
        self.last_frame_bits.store(
            other.last_frame_bits.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.ring.len()
    }
}

#[inline(always)]
fn frame_to_bits(frame: AudioFrame) -> u64 {
    match frame {
        AudioFrame::Mono(s) => s.to_bits() as u64,
        AudioFrame::Stereo([l, r]) => (l.to_bits() as u64) | ((r.to_bits() as u64) << 32),
    }
}

#[inline(always)]
#[allow(dead_code)] // unused after issue #496: pop() returns silence on
                    // underrun, not the bit-packed last frame. Kept for
                    // potential future use (smooth fade-out fallback).
fn bits_to_frame(bits: u64, layout: AudioChannelLayout) -> AudioFrame {
    match layout {
        AudioChannelLayout::Mono => AudioFrame::Mono(f32::from_bits(bits as u32)),
        AudioChannelLayout::Stereo => {
            let l = f32::from_bits(bits as u32);
            let r = f32::from_bits((bits >> 32) as u32);
            AudioFrame::Stereo([l, r])
        }
    }
}

pub(crate) enum AudioProcessor {
    Mono(Box<dyn MonoProcessor>),
    DualMono {
        left: Box<dyn MonoProcessor>,
        right: Box<dyn MonoProcessor>,
    },
    Stereo(Box<dyn StereoProcessor>),
    StereoFromMono(Box<dyn StereoProcessor>),
}

pub(crate) enum ProcessorScratch {
    None,
    Mono(Vec<f32>),
    DualMono { left: Vec<f32>, right: Vec<f32> },
    Stereo(Vec<[f32; 2]>),
}

impl AudioProcessor {
    /// Process a buffer of audio frames.
    ///
    /// Bus between blocks is ALWAYS stereo. Mono processors receive the left
    /// channel (or mono mix), process it, and output stereo (duplicated).
    #[inline]
    pub(crate) fn process_buffer(
        &mut self,
        frames: &mut [AudioFrame],
        scratch: &mut ProcessorScratch,
    ) {
        match (self, scratch) {
            (AudioProcessor::Mono(processor), ProcessorScratch::Mono(mono)) => {
                mono.clear();
                mono.reserve(frames.len().saturating_sub(mono.capacity()));
                for frame in frames.iter() {
                    mono.push(frame.mono_mix());
                }
                processor.process_block(mono);
                // Always output stereo — mono processors duplicate to both channels
                for (frame, sample) in frames.iter_mut().zip(mono.iter().copied()) {
                    *frame = AudioFrame::Stereo([sample, sample]);
                }
            }
            (
                AudioProcessor::DualMono { left, right },
                ProcessorScratch::DualMono {
                    left: left_buffer,
                    right: right_buffer,
                },
            ) => {
                left_buffer.clear();
                right_buffer.clear();
                left_buffer.reserve(frames.len().saturating_sub(left_buffer.capacity()));
                right_buffer.reserve(frames.len().saturating_sub(right_buffer.capacity()));
                for frame in frames.iter() {
                    match frame {
                        AudioFrame::Stereo([l, r]) => {
                            left_buffer.push(*l);
                            right_buffer.push(*r);
                        }
                        AudioFrame::Mono(s) => {
                            left_buffer.push(*s);
                            right_buffer.push(*s);
                        }
                    }
                }
                left.process_block(left_buffer);
                right.process_block(right_buffer);
                for ((frame, left_sample), right_sample) in frames
                    .iter_mut()
                    .zip(left_buffer.iter().copied())
                    .zip(right_buffer.iter().copied())
                {
                    *frame = AudioFrame::Stereo([left_sample, right_sample]);
                }
            }
            (AudioProcessor::Stereo(processor), ProcessorScratch::Stereo(stereo)) => {
                stereo.clear();
                stereo.reserve(frames.len().saturating_sub(stereo.capacity()));
                for frame in frames.iter() {
                    match frame {
                        AudioFrame::Stereo(sf) => stereo.push(*sf),
                        AudioFrame::Mono(s) => stereo.push([*s, *s]),
                    }
                }
                processor.process_block(stereo);
                for (frame, stereo_frame) in frames.iter_mut().zip(stereo.iter().copied()) {
                    *frame = AudioFrame::Stereo(stereo_frame);
                }
            }
            (AudioProcessor::StereoFromMono(processor), ProcessorScratch::Stereo(stereo)) => {
                stereo.clear();
                stereo.reserve(frames.len().saturating_sub(stereo.capacity()));
                for frame in frames.iter() {
                    match frame {
                        AudioFrame::Mono(s) => stereo.push([*s, *s]),
                        AudioFrame::Stereo(sf) => stereo.push(*sf),
                    }
                }
                processor.process_block(stereo);
                for (frame, stereo_frame) in frames.iter_mut().zip(stereo.iter().copied()) {
                    *frame = AudioFrame::Stereo(stereo_frame);
                }
            }
            _ => {
                debug_assert!(false, "processor scratch layout mismatch");
            }
        }
    }
}

#[inline(always)]
pub(crate) fn read_input_frame(
    input_layout: AudioChannelLayout,
    input_channels: &[usize],
    frame: &[f32],
) -> AudioFrame {
    match input_layout {
        AudioChannelLayout::Mono => AudioFrame::Mono(read_channel(frame, input_channels[0])),
        AudioChannelLayout::Stereo => AudioFrame::Stereo([
            read_channel(frame, input_channels[0]),
            read_channel(frame, input_channels[1]),
        ]),
    }
}

#[inline(always)]
pub(crate) fn read_channel(frame: &[f32], channel_index: usize) -> f32 {
    frame.get(channel_index).copied().unwrap_or(0.0)
}

#[inline(always)]
pub(crate) fn silent_frame(layout: AudioChannelLayout) -> AudioFrame {
    match layout {
        AudioChannelLayout::Mono => AudioFrame::Mono(0.0),
        AudioChannelLayout::Stereo => AudioFrame::Stereo([0.0, 0.0]),
    }
}

/// Sum two audio frames together (for mixing multiple input streams).
#[allow(dead_code)]
pub(crate) fn mix_frames(a: AudioFrame, b: AudioFrame) -> AudioFrame {
    match (a, b) {
        (AudioFrame::Mono(l), AudioFrame::Mono(r)) => AudioFrame::Mono(l + r),
        (AudioFrame::Stereo([l1, r1]), AudioFrame::Stereo([l2, r2])) => {
            AudioFrame::Stereo([l1 + l2, r1 + r2])
        }
        (AudioFrame::Mono(m), AudioFrame::Stereo([l, r])) => AudioFrame::Stereo([m + l, m + r]),
        (AudioFrame::Stereo([l, r]), AudioFrame::Mono(m)) => AudioFrame::Stereo([l + m, r + m]),
    }
}

// ── R. ElasticBuffer direct invariants (issue #496, 30 tests) ─────
// Targets the underrun-repeat-last-frame behavior — the suspected
// source of broadband noise / swarm-of-bees artefact in the real engine.
#[cfg(test)]
mod elastic_tests {
    use super::*;

    fn mono(s: f32) -> AudioFrame {
        AudioFrame::Mono(s)
    }
    fn stereo(l: f32, r: f32) -> AudioFrame {
        AudioFrame::Stereo([l, r])
    }
    fn unwrap_mono(f: AudioFrame) -> f32 {
        match f {
            AudioFrame::Mono(s) => s,
            _ => panic!(),
        }
    }
    fn unwrap_stereo(f: AudioFrame) -> (f32, f32) {
        match f {
            AudioFrame::Stereo([l, r]) => (l, r),
            _ => panic!(),
        }
    }

    #[test]
    fn r01_push_pop_single_mono_frame_round_trips() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        b.push(mono(0.42));
        assert_eq!(unwrap_mono(b.pop()), 0.42);
    }
    #[test]
    fn r02_push_pop_single_stereo_frame_round_trips() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Stereo);
        b.push(stereo(0.3, -0.4));
        assert_eq!(unwrap_stereo(b.pop()), (0.3, -0.4));
    }
    #[test]
    fn r03_initial_pop_returns_silent_mono() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        assert_eq!(unwrap_mono(b.pop()), 0.0);
    }
    #[test]
    fn r04_initial_pop_returns_silent_stereo() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Stereo);
        assert_eq!(unwrap_stereo(b.pop()), (0.0, 0.0));
    }
    #[test]
    fn r05_push_n_pop_n_preserves_fifo_order() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        for i in 0..10 {
            b.push(mono(i as f32 * 0.1));
        }
        for i in 0..10 {
            assert!(
                (unwrap_mono(b.pop()) - i as f32 * 0.1).abs() < 1e-6,
                "i={i}"
            );
        }
    }
    #[test]
    fn r06_len_starts_zero() {
        assert_eq!(ElasticBuffer::new(16, AudioChannelLayout::Mono).len(), 0);
    }
    #[test]
    fn r07_len_after_push_is_one() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        b.push(mono(1.0));
        assert_eq!(b.len(), 1);
    }

    // BUG SURFACE: underrun returns last pushed frame (REPEATS).
    #[test]
    fn r08_underrun_should_not_repeat_last_mono_indefinitely() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        b.push(mono(0.7));
        assert_eq!(unwrap_mono(b.pop()), 0.7);
        for i in 0..10 {
            let v = unwrap_mono(b.pop());
            assert!(
                v.abs() < 1e-6,
                "underrun frame {i} = {v} (repeated last; should be silence/faded)"
            );
        }
    }
    #[test]
    fn r09_underrun_should_not_repeat_last_stereo_indefinitely() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Stereo);
        b.push(stereo(0.4, -0.6));
        b.pop();
        for i in 0..10 {
            let (l, r) = unwrap_stereo(b.pop());
            assert!(
                l.abs() < 1e-6 && r.abs() < 1e-6,
                "stereo underrun frame {i} = ({l},{r}) (repeated); broadband noise source"
            );
        }
    }

    #[test]
    fn r10_underrun_in_middle_of_sine_creates_plateau() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        let sr = 48_000.0_f32;
        let pushed: Vec<f32> = (0..3)
            .map(|i| (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / sr).sin())
            .collect();
        for &s in &pushed {
            b.push(mono(s));
        }
        let mut popped = Vec::new();
        for _ in 0..10 {
            popped.push(unwrap_mono(b.pop()));
        }
        for i in 3..10 {
            assert!(
                (popped[i] - pushed[2]).abs() > 1e-6,
                "frame {i} repeats last pushed ({}); flat-top harmonic-injection bug",
                popped[i]
            );
        }
    }
    #[test]
    fn r11_dc_input_then_underrun_extends_dc_indefinitely() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        b.push(mono(0.5));
        b.pop();
        for _ in 0..50 {
            let v = unwrap_mono(b.pop());
            assert!(v.abs() < 1e-6, "DC plateau extension: {v}");
        }
    }
    #[test]
    fn r12_after_seed_underrun_returns_silence_not_seeded() {
        let a = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        a.push(mono(0.8));
        a.pop();
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        b.seed_last_frame_from(&a);
        let v = unwrap_mono(b.pop());
        assert!(
            v.abs() < 1e-6,
            "after seed, underrun should produce silence (not {v})"
        );
    }
    #[test]
    fn r13_seed_does_not_inject_into_ring() {
        let a = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        a.push(mono(0.3));
        a.pop();
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        b.seed_last_frame_from(&a);
        assert_eq!(b.len(), 0);
    }

    #[test]
    fn r14_overrun_drops_newest_silently() {
        let b = ElasticBuffer::new(2, AudioChannelLayout::Mono);
        for i in 0..10 {
            b.push(mono(i as f32 * 0.1));
        }
        let mut got = Vec::new();
        while b.len() > 0 {
            got.push(unwrap_mono(b.pop()));
        }
        assert_eq!(got.len(), 4);
        for (i, v) in got.iter().enumerate() {
            assert!((v - (i as f32 * 0.1)).abs() < 1e-6, "i={i}");
        }
    }
    #[test]
    fn r15_overrun_does_not_corrupt_existing_frames() {
        let b = ElasticBuffer::new(2, AudioChannelLayout::Mono);
        b.push(mono(0.1));
        b.push(mono(0.2));
        for _ in 0..50 {
            b.push(mono(99.0));
        }
        assert!((unwrap_mono(b.pop()) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn r16_capacity_1_mono() {
        let b = ElasticBuffer::new(1, AudioChannelLayout::Mono);
        b.push(mono(0.5));
        assert_eq!(unwrap_mono(b.pop()), 0.5);
    }
    #[test]
    fn r17_capacity_4_mono() {
        let b = ElasticBuffer::new(4, AudioChannelLayout::Mono);
        for i in 0..8 {
            b.push(mono(i as f32 / 10.0));
        }
        let mut got = Vec::new();
        while b.len() > 0 {
            got.push(unwrap_mono(b.pop()));
        }
        assert_eq!(got.len(), 8);
    }
    #[test]
    fn r18_capacity_256_mono() {
        let b = ElasticBuffer::new(256, AudioChannelLayout::Mono);
        for i in 0..256 {
            b.push(mono(i as f32 / 256.0));
        }
        let mut got = Vec::new();
        while b.len() > 0 {
            got.push(unwrap_mono(b.pop()));
        }
        assert_eq!(got.len(), 256);
    }
    #[test]
    fn r19_capacity_1024_mono() {
        let b = ElasticBuffer::new(1024, AudioChannelLayout::Mono);
        for i in 0..1024 {
            b.push(mono(i as f32 / 1024.0));
        }
        assert_eq!(b.len(), 1024);
    }

    #[test]
    fn r20_alternating_push_pop_steady_state() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        for i in 0..100 {
            b.push(mono(i as f32 / 100.0));
            assert!((unwrap_mono(b.pop()) - i as f32 / 100.0).abs() < 1e-6);
        }
        assert_eq!(b.len(), 0);
    }
    #[test]
    fn r21_push_burst_then_drain() {
        let b = ElasticBuffer::new(64, AudioChannelLayout::Mono);
        for i in 0..32 {
            b.push(mono(i as f32 / 100.0));
        }
        for i in 0..32 {
            assert!((unwrap_mono(b.pop()) - i as f32 / 100.0).abs() < 1e-6);
        }
    }
    #[test]
    fn r22_consumer_ahead_pops_silence_then_recovers() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        b.push(mono(0.4));
        assert_eq!(unwrap_mono(b.pop()), 0.4);
        for i in 0..5 {
            let v = unwrap_mono(b.pop());
            assert!(v.abs() < 1e-6, "consumer-ahead frame {i} = {v}");
        }
        b.push(mono(0.7));
        assert_eq!(unwrap_mono(b.pop()), 0.7);
    }

    #[test]
    fn r23_zero_frame_round_trips() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        b.push(mono(0.0));
        assert_eq!(unwrap_mono(b.pop()), 0.0);
    }
    #[test]
    fn r24_positive_full_scale_round_trips() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        b.push(mono(1.0));
        assert_eq!(unwrap_mono(b.pop()), 1.0);
    }
    #[test]
    fn r25_negative_full_scale_round_trips() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        b.push(mono(-1.0));
        assert_eq!(unwrap_mono(b.pop()), -1.0);
    }
    #[test]
    fn r26_subnormal_value_round_trips() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        let x = f32::MIN_POSITIVE / 2.0;
        b.push(mono(x));
        assert_eq!(unwrap_mono(b.pop()), x);
    }
    #[test]
    fn r27_negative_zero_treated_as_zero() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
        b.push(mono(-0.0));
        assert_eq!(unwrap_mono(b.pop()), 0.0);
    }

    #[test]
    fn r28_stereo_l_r_order_preserved() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Stereo);
        b.push(stereo(0.1, 0.9));
        assert_eq!(unwrap_stereo(b.pop()), (0.1, 0.9));
    }
    #[test]
    fn r29_stereo_underrun_should_be_silence_not_last_pushed() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Stereo);
        b.push(stereo(0.2, -0.7));
        b.pop();
        for i in 0..20 {
            let (l, r) = unwrap_stereo(b.pop());
            assert!(
                l.abs() < 1e-6 && r.abs() < 1e-6,
                "stereo underrun frame {i} = ({l},{r})"
            );
        }
    }
    #[test]
    fn r30_stereo_multi_push_preserves_each_pair() {
        let b = ElasticBuffer::new(16, AudioChannelLayout::Stereo);
        let pairs = [(0.1, 0.2), (0.3, 0.4), (0.5, 0.6), (0.7, 0.8)];
        for &(l, r) in &pairs {
            b.push(stereo(l, r));
        }
        for &(l, r) in &pairs {
            assert_eq!(unwrap_stereo(b.pop()), (l, r));
        }
    }
}
