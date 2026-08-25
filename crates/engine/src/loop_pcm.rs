//! Responsibility: holds a recorded loop's audio behind a handle.
//! #323/#127 — a recorded loop's audio, as a HANDLE.
//!
//! A loop's mixdown is megabytes of PCM. When it travels between the runtime
//! that recorded it and the code that writes it to disk, it must travel as one
//! `Arc` — never as samples copied across a seam, and never through
//! [`crate::LooperStatus`] or any other reading (`LiveSource` returns finished
//! readings; PCM is not one).
//!
//! Interleaved stereo, at the rate the store recorded it, exactly like the
//! sidecar wav it becomes. The sibling of [`crate::DiPcm`], which does the same
//! job in the other direction (a decoded file on its way INTO a runtime).

/// One looper's recorded mixdown: interleaved stereo at `sample_rate`.
pub struct LoopPcm {
    samples: Box<[f32]>,
    sample_rate: u32,
}

impl LoopPcm {
    /// Wrap an exported mixdown. No resample and no copy of the samples
    /// happens after this — the buffer is moved in and then shared.
    pub fn new(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            samples: samples.into_boxed_slice(),
            sample_rate,
        }
    }

    /// The interleaved-stereo samples, for the writer that serializes them.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// The rate the loop was recorded at — carried WITH the audio so a save
    /// can never stamp a sidecar with a rate that belongs to another stream.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// `true` when nothing was recorded.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}
