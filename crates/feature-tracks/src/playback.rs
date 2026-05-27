//! RT-safe multi-stem player source.
//!
//! Holds N preallocated stereo-interleaved stem buffers, exposes
//! per-stem `gain`, `pan`, `mute`, and `solo` controls as atomics, and
//! mixes them into a shared output buffer on the audio thread.
//!
//! Invariants honoured by `process`:
//! - zero allocation, zero locks, zero syscalls
//! - playhead advances atomically per call and wraps at end-of-stems
//! - solo precedence (`any solo => only soloed stems play`)
//! - linear pan (`L = src * (1 - max(0, pan))`,
//!   `R = src * (1 + min(0, pan))`)
//! - gain applied multiplicatively after solo/mute selection

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

/// RT-safe stem mixer.
///
/// Constructed off-RT with the pre-decoded stem buffers, then handed to
/// the audio thread (via [`std::sync::Arc`] in production). Setters are
/// safe to call from any thread.
pub struct MultiStemPlayer {
    stems: Vec<Vec<f32>>,
    /// Per-stem state, parallel to `stems`.
    state: Vec<StemState>,
    /// Frame count consumed since the player was created or last
    /// wrapped at the end of its buffers.
    playhead: AtomicUsize,
    /// Total frames in the loaded buffers (assumed identical across
    /// stems; the shortest stem dictates the loop length).
    total_frames: usize,
    sample_rate: u32,
}

struct StemState {
    gain_bits: AtomicU32,
    pan_bits: AtomicU32,
    muted: AtomicBool,
    soloed: AtomicBool,
}

impl StemState {
    fn new() -> Self {
        Self {
            gain_bits: AtomicU32::new(1.0_f32.to_bits()),
            pan_bits: AtomicU32::new(0.0_f32.to_bits()),
            muted: AtomicBool::new(false),
            soloed: AtomicBool::new(false),
        }
    }

    fn gain(&self) -> f32 {
        f32::from_bits(self.gain_bits.load(Ordering::Relaxed))
    }

    fn pan(&self) -> f32 {
        f32::from_bits(self.pan_bits.load(Ordering::Relaxed))
    }

    fn is_muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    fn is_soloed(&self) -> bool {
        self.soloed.load(Ordering::Relaxed)
    }
}

impl MultiStemPlayer {
    /// Build a player from stereo-interleaved buffers.
    ///
    /// `stems[k]` is `[L0, R0, L1, R1, ...]`. The shortest buffer
    /// dictates how many frames are loopable (so all stems stay
    /// time-aligned). An empty `stems` vector is valid — `process`
    /// then produces silence.
    #[must_use]
    pub fn new(stems: Vec<Vec<f32>>, sample_rate: u32) -> Self {
        let total_frames = stems.iter().map(|s| s.len() / 2).min().unwrap_or(0);
        let state = (0..stems.len()).map(|_| StemState::new()).collect();
        Self {
            stems,
            state,
            playhead: AtomicUsize::new(0),
            total_frames,
            sample_rate,
        }
    }

    /// Current playhead position, in frames since the start of the
    /// loaded buffers (post-wrap when the playhead has cycled).
    #[must_use]
    pub fn playhead(&self) -> usize {
        self.playhead.load(Ordering::Acquire)
    }

    /// Sample rate the loaded stems were written at. Callers wiring
    /// the player into a real audio device (cpal/JACK) must request a
    /// stream at this rate so playback stays time-correct.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Total frames in the loaded stems (= shortest stem length).
    /// Returns 0 for an empty player.
    #[must_use]
    pub fn total_frames(&self) -> usize {
        self.total_frames
    }

    /// Move the playhead to `frame` (clamped to `[0, total_frames)`).
    /// Safe to call from any thread.
    pub fn seek_to_frame(&self, frame: usize) {
        let target = if self.total_frames == 0 {
            0
        } else {
            frame.min(self.total_frames.saturating_sub(1))
        };
        self.playhead.store(target, Ordering::Release);
    }

    /// Set per-stem gain. Values are not clamped — callers usually
    /// keep them in `[0.0, 2.0]`. Out-of-bounds indices are ignored.
    pub fn set_gain(&self, idx: usize, gain: f32) {
        if let Some(state) = self.state.get(idx) {
            state.gain_bits.store(gain.to_bits(), Ordering::Relaxed);
        }
    }

    /// Set per-stem pan in `[-1.0, 1.0]` (`-1` = full left, `+1` =
    /// full right). Values are clamped on read in `process`.
    pub fn set_pan(&self, idx: usize, pan: f32) {
        if let Some(state) = self.state.get(idx) {
            state.pan_bits.store(pan.to_bits(), Ordering::Relaxed);
        }
    }

    /// Toggle mute on a stem. Out-of-bounds indices are ignored.
    pub fn set_mute(&self, idx: usize, muted: bool) {
        if let Some(state) = self.state.get(idx) {
            state.muted.store(muted, Ordering::Relaxed);
        }
    }

    /// Toggle solo on a stem. Out-of-bounds indices are ignored.
    /// Solo precedence is computed in `process`: when any stem is
    /// soloed, every non-soloed stem is muted.
    pub fn set_solo(&self, idx: usize, soloed: bool) {
        if let Some(state) = self.state.get(idx) {
            state.soloed.store(soloed, Ordering::Relaxed);
        }
    }

    /// Fill `out` with the next chunk of mixed audio.
    ///
    /// `out` is stereo-interleaved (`[L, R, L, R, ...]`) and its length
    /// must be a multiple of 2. The playhead advances by
    /// `out.len() / 2` frames and wraps when it reaches the end of the
    /// shortest stem buffer.
    pub fn process(&self, out: &mut [f32]) {
        for sample in out.iter_mut() {
            *sample = 0.0;
        }
        if self.total_frames == 0 || self.stems.is_empty() {
            return;
        }

        let any_solo = self.state.iter().any(StemState::is_soloed);
        let frames_requested = out.len() / 2;
        let start = self.playhead.load(Ordering::Acquire);

        for frame in 0..frames_requested {
            let src_frame = (start + frame) % self.total_frames;
            let src_idx = src_frame * 2;
            for (stem_idx, stem) in self.stems.iter().enumerate() {
                let state = &self.state[stem_idx];
                let plays = if any_solo {
                    state.is_soloed()
                } else {
                    !state.is_muted()
                };
                if !plays {
                    continue;
                }
                let gain = state.gain();
                let pan = state.pan().clamp(-1.0, 1.0);
                let left_gain = if pan > 0.0 { 1.0 - pan } else { 1.0 };
                let right_gain = if pan < 0.0 { 1.0 + pan } else { 1.0 };
                let l = stem[src_idx] * gain * left_gain;
                let r = stem[src_idx + 1] * gain * right_gain;
                out[frame * 2] += l;
                out[frame * 2 + 1] += r;
            }
        }

        let advanced = (start + frames_requested) % self.total_frames;
        self.playhead.store(advanced, Ordering::Release);
    }
}
