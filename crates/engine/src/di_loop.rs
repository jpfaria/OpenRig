//! Ephemeral per-chain virtual DI loop source (issue #614).
//!
//! An immutable, preallocated, read-only buffer of mono-or-stereo frames at
//! the engine sample rate. Built entirely OFF the audio thread (decode +
//! resample + loop crossfade); read lock-free ON the audio thread via
//! [`DiLoop::frame_at`]. Nothing in this module allocates or locks once the
//! buffer is built, so it is safe to read from `process_input_f32`.

use block_core::AudioChannelLayout;

/// One frame of the DI loop, in the loop's own layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DiFrame {
    Mono(f32),
    Stereo([f32; 2]),
}

/// A looping dry-DI buffer at the engine sample rate.
pub struct DiLoop {
    frames: Box<[DiFrame]>,
    layout: AudioChannelLayout,
}

impl DiLoop {
    /// Build a loop from interleaved `samples` (`channels` interleaved) at
    /// `src_sr`, resampled to `engine_sr`, with an equal-power loop crossfade
    /// of `xfade_frames` at the wrap seam (0 = no crossfade). Runs off the
    /// audio thread.
    pub fn from_samples(
        samples: &[f32],
        src_sr: u32,
        channels: usize,
        engine_sr: u32,
        xfade_frames: usize,
    ) -> Self {
        let layout = if channels >= 2 {
            AudioChannelLayout::Stereo
        } else {
            AudioChannelLayout::Mono
        };
        let ch = channels.max(1);

        let src_frames: Vec<DiFrame> = samples
            .chunks(ch)
            .map(|c| match layout {
                AudioChannelLayout::Stereo => {
                    DiFrame::Stereo([*c.first().unwrap_or(&0.0), *c.get(1).unwrap_or(&0.0)])
                }
                AudioChannelLayout::Mono => DiFrame::Mono(*c.first().unwrap_or(&0.0)),
            })
            .collect();

        let mut frames = resample_frames(&src_frames, src_sr, engine_sr, layout);
        apply_loop_crossfade(&mut frames, xfade_frames, layout);

        Self {
            frames: frames.into_boxed_slice(),
            layout,
        }
    }

    #[inline(always)]
    pub fn layout(&self) -> AudioChannelLayout {
        self.layout
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Frame at `pos`, wrapping modulo length. Silence if empty.
    #[inline(always)]
    pub fn frame_at(&self, pos: usize) -> DiFrame {
        if self.frames.is_empty() {
            return match self.layout {
                AudioChannelLayout::Stereo => DiFrame::Stereo([0.0, 0.0]),
                AudioChannelLayout::Mono => DiFrame::Mono(0.0),
            };
        }
        self.frames[pos % self.frames.len()]
    }
}

/// Linear-interpolation resample. Identity (clone) when `src_sr == dst_sr`.
/// Linear is adequate for a practice DI loop; a windowed-sinc upgrade is a
/// follow-up. Runs off the audio thread.
fn resample_frames(
    src: &[DiFrame],
    src_sr: u32,
    dst_sr: u32,
    layout: AudioChannelLayout,
) -> Vec<DiFrame> {
    if src_sr == dst_sr || src.len() < 2 {
        return src.to_vec();
    }
    let ratio = dst_sr as f64 / src_sr as f64;
    let out_len = ((src.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 / ratio;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let a = src[idx.min(src.len() - 1)];
        let b = src[(idx + 1).min(src.len() - 1)];
        out.push(lerp_frame(a, b, frac, layout));
    }
    out
}

#[inline]
fn lerp_frame(a: DiFrame, b: DiFrame, t: f32, layout: AudioChannelLayout) -> DiFrame {
    match layout {
        AudioChannelLayout::Mono => {
            let av = if let DiFrame::Mono(s) = a { s } else { 0.0 };
            let bv = if let DiFrame::Mono(s) = b { s } else { 0.0 };
            DiFrame::Mono(av + (bv - av) * t)
        }
        AudioChannelLayout::Stereo => {
            let [al, ar] = if let DiFrame::Stereo(v) = a { v } else { [0.0, 0.0] };
            let [bl, br] = if let DiFrame::Stereo(v) = b { v } else { [0.0, 0.0] };
            DiFrame::Stereo([al + (bl - al) * t, ar + (br - ar) * t])
        }
    }
}

/// Equal-power crossfade of the loop's tail into its head over `xfade` frames,
/// so the wrap from last->first sample has no click. No-op if `xfade == 0` or
/// the buffer is too short.
fn apply_loop_crossfade(frames: &mut [DiFrame], xfade: usize, layout: AudioChannelLayout) {
    let n = frames.len();
    if xfade == 0 || n < xfade * 2 {
        return;
    }
    for i in 0..xfade {
        let p = (i + 1) as f32 / (xfade + 1) as f32;
        let tail_g = (0.5 * std::f32::consts::PI * p).cos();
        let head_g = (0.5 * std::f32::consts::PI * p).sin();
        let tail_idx = n - xfade + i;
        let head_idx = i;
        let tail = frames[tail_idx];
        let head = frames[head_idx];
        frames[tail_idx] = mix_frame(tail, tail_g, head, head_g, layout);
    }
}

#[inline]
fn mix_frame(a: DiFrame, ga: f32, b: DiFrame, gb: f32, layout: AudioChannelLayout) -> DiFrame {
    match layout {
        AudioChannelLayout::Mono => {
            let av = if let DiFrame::Mono(s) = a { s } else { 0.0 };
            let bv = if let DiFrame::Mono(s) = b { s } else { 0.0 };
            DiFrame::Mono(av * ga + bv * gb)
        }
        AudioChannelLayout::Stereo => {
            let [al, ar] = if let DiFrame::Stereo(v) = a { v } else { [0.0, 0.0] };
            let [bl, br] = if let DiFrame::Stereo(v) = b { v } else { [0.0, 0.0] };
            DiFrame::Stereo([al * ga + bl * gb, ar * ga + br * gb])
        }
    }
}

#[cfg(test)]
#[path = "di_loop_tests.rs"]
mod tests;
