//! Responsibility: reads a WAV file the render driver needs.
//! WAV I/O helpers for the offline render driver.
//!
//! All samples are normalized `f32` in `[-1.0, 1.0]` inside the engine path.
//! Disk-side bit depth is `BitDepth` (16-bit PCM, 24-bit PCM, or 32-bit
//! float). Determinism is required: the same logical input must produce a
//! byte-identical WAV — verified by `issue_552_wav_io.rs`.

pub use crate::channel_layout_convert::{broadcast_mono_to_stereo, interleaved_to_stereo_frames};
pub use crate::wav_read::read_wav;
pub use crate::wav_types::{BitDepth, WavData, WavError};
pub use crate::wav_write::write_wav_stereo;
