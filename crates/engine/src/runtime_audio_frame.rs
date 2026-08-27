//! Responsibility: keeps the historical `runtime_audio_frame` path pointing at the audio primitives.

pub(crate) use crate::audio_frame::{read_input_frame, AudioFrame};

// Used only by the test modules that hang off the crate root, exactly as they
// did before the split (#873).
#[cfg(test)]
pub(crate) use crate::audio_frame::{mix_frames, read_channel, silent_frame};
pub(crate) use crate::audio_processor::{AudioProcessor, ProcessorScratch};
pub(crate) use crate::elastic_buffer::ElasticBuffer;
pub use crate::elastic_buffer::{
    elastic_target_for_buffer, DEFAULT_ELASTIC_TARGET, ELASTIC_TARGET_FLOOR,
};

#[cfg(test)]
pub(crate) use block_core::AudioChannelLayout;

#[cfg(test)]
#[path = "runtime_audio_frame_tests.rs"]
mod tests;
