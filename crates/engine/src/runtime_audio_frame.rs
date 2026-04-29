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

#[derive(Debug, Clone, Copy)]
pub(crate) enum AudioFrame {
    Mono(f32),
    Stereo([f32; 2]),
}

impl AudioFrame {
    pub(crate) fn mono_mix(self) -> f32 {
        match self {
            AudioFrame::Mono(sample) => sample,
            AudioFrame::Stereo([left, right]) => (left + right) * 0.5,
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
}

impl ElasticBuffer {
    pub(crate) fn new(target_level: usize, layout: AudioChannelLayout) -> Self {
        let init = silent_frame(layout);
        Self {
            ring: SpscRing::new(target_level.saturating_mul(2), init),
            target_level,
            layout,
            last_frame_bits: AtomicU64::new(frame_to_bits(init)),
        }
    }

    pub(crate) fn push(&self, frame: AudioFrame) {
        self.last_frame_bits
            .store(frame_to_bits(frame), Ordering::Relaxed);
        // Drop-newest when full — the consumer is behind and a single dropped
        // sample is less disruptive than advancing the tail from the
        // producer side (which would violate the SPSC invariant).
        let _ = self.ring.push(frame);
    }

    pub(crate) fn pop(&self) -> AudioFrame {
        match self.ring.pop() {
            Some(frame) => frame,
            None => bits_to_frame(self.last_frame_bits.load(Ordering::Relaxed), self.layout),
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

fn frame_to_bits(frame: AudioFrame) -> u64 {
    match frame {
        AudioFrame::Mono(s) => s.to_bits() as u64,
        AudioFrame::Stereo([l, r]) => (l.to_bits() as u64) | ((r.to_bits() as u64) << 32),
    }
}

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

pub(crate) fn read_channel(frame: &[f32], channel_index: usize) -> f32 {
    frame.get(channel_index).copied().unwrap_or(0.0)
}

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
