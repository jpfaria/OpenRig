//! Responsibility: runs one block's processor over a buffer of frames.
//!
//! Split out of `runtime_audio_frame.rs` (#873).

use block_core::{MonoProcessor, StereoProcessor};

use crate::audio_frame::AudioFrame;

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
