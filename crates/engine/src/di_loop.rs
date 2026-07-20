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

        let resampled = resample_frames(&src_frames, src_sr, engine_sr, layout);
        let frames = apply_loop_crossfade(resampled, xfade_frames, layout);

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

/// Decoded, un-resampled DI source PCM (interleaved `channels` at `src_sr`).
///
/// The loader decodes the WAV into this ONCE, off the audio thread. The DI
/// arming path then calls [`DiPcm::to_loop_at`] to resample it to EACH
/// runtime's own rate (#749): a multi-rate rig (Scarlett @44.1 + TEYUN @48)
/// plays every output at true speed, instead of stretching a single
/// `engine_sr` buffer on the mismatched-rate output ("está lento"). Holding
/// the source — not a pre-resampled loop — keeps the resample single-hop
/// (source → target), never source → engine_sr → target.
pub struct DiPcm {
    samples: Box<[f32]>,
    src_sr: u32,
    channels: usize,
}

impl DiPcm {
    /// Wrap decoded interleaved PCM. No resample happens here.
    pub fn new(samples: Vec<f32>, src_sr: u32, channels: usize) -> Self {
        Self {
            samples: samples.into_boxed_slice(),
            src_sr,
            channels,
        }
    }

    /// `true` when there are no samples to play.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Source sample rate of the decoded PCM (no resample applied).
    pub fn src_sr(&self) -> u32 {
        self.src_sr
    }

    /// Deinterleave the source PCM into stereo frames, without resampling:
    /// mono broadcasts to both channels, stereo maps L/R directly, and >2
    /// channels keep the first two. Used by the Tone Doctor to feed a chain's
    /// own DI through the offline diagnosis at `src_sr()` (#791).
    pub fn stereo_frames(&self) -> Vec<[f32; 2]> {
        let ch = self.channels.max(1);
        self.samples
            .chunks(ch)
            .map(|frame| match frame {
                [] => [0.0, 0.0],
                [m] => [*m, *m],
                [l, r, ..] => [*l, *r],
            })
            .collect()
    }

    /// Build a [`DiLoop`] resampled to `target_sr`, with a ~10 ms seam
    /// crossfade (rate-relative, so the seam stays ~10 ms at any rate — the
    /// old fixed 480-frame constant was exactly 10 ms only at 48 kHz).
    pub fn to_loop_at(&self, target_sr: u32) -> DiLoop {
        let xfade = (target_sr / 100) as usize;
        DiLoop::from_samples(&self.samples, self.src_sr, self.channels, target_sr, xfade)
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
            let [al, ar] = if let DiFrame::Stereo(v) = a {
                v
            } else {
                [0.0, 0.0]
            };
            let [bl, br] = if let DiFrame::Stereo(v) = b {
                v
            } else {
                [0.0, 0.0]
            };
            DiFrame::Stereo([al + (bl - al) * t, ar + (br - ar) * t])
        }
    }
}

/// Seamless-loop crossfade via overlap-add. Returns a buffer of length
/// `n - xfade`: the dropped last `xfade` frames (the tail) are blended — fading
/// out — into the head (fading in) at the start, with equal-gain (linear)
/// weights.
///
/// Why this shape (issue #614 clipping report): the previous version only
/// pulled the tail toward `head[xfade-1]`, but playback wraps `last -> first`,
/// so a step of `|head[0] - head[xfade-1]|` survived at the actual seam. On a
/// high-gain chain that step is an audible click that sounds like clipping every
/// time the loop restarts. With overlap-add, the new first frame is ~the source
/// frame that followed the new last frame, so the wrap is continuous with the
/// body. Equal-gain (not equal-power) weights sum to 1, so the seam never
/// overshoots the source peak — no added clipping.
///
/// No-op if `xfade == 0` or the buffer is too short to spare the overlap.
fn apply_loop_crossfade(
    frames: Vec<DiFrame>,
    xfade: usize,
    layout: AudioChannelLayout,
) -> Vec<DiFrame> {
    let n = frames.len();
    if xfade == 0 || n < xfade * 2 + 1 {
        return frames;
    }
    let m = n - xfade;
    let mut out = Vec::with_capacity(m);
    // First `xfade` frames: head[i] fades in while the dropped tail frames[m+i]
    // fade out. At i≈0 the output ≈ frames[m] (adjacent in the source to the new
    // last frame frames[m-1] ⇒ continuous wrap); at i=xfade-1 the output
    // ≈ frames[xfade-1] (adjacent to frames[xfade], the first body frame).
    for i in 0..xfade {
        let head_w = (i + 1) as f32 / (xfade + 1) as f32; // 0 -> 1
        let head = frames[i];
        let tail = frames[m + i];
        out.push(mix_frame(head, head_w, tail, 1.0 - head_w, layout));
    }
    out.extend_from_slice(&frames[xfade..m]);
    out
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
            let [al, ar] = if let DiFrame::Stereo(v) = a {
                v
            } else {
                [0.0, 0.0]
            };
            let [bl, br] = if let DiFrame::Stereo(v) = b {
                v
            } else {
                [0.0, 0.0]
            };
            DiFrame::Stereo([al * ga + bl * gb, ar * ga + br * gb])
        }
    }
}

#[cfg(test)]
#[path = "di_loop_tests.rs"]
mod tests;
