//! Responsibility: glides one stream's mixer gain to its target across a single callback.
//!
//! Issue #1007. A fader move must not click: the gain walks linearly from
//! the value the stream last played to the new target over the frames of
//! one callback, then holds. Unity that stays unity yields no glide at all,
//! so an untouched fader leaves the signal bit-identical. Pure arithmetic —
//! no allocation, no lock, no added latency.

#[derive(Debug, Clone, Copy)]
pub(crate) struct GainGlide {
    from: f32,
    to: f32,
    frames: usize,
}

impl GainGlide {
    /// `None` when the stream sits at unity and stays there.
    #[inline]
    pub(crate) fn begin(current: f32, target: f32, frames: usize) -> Option<Self> {
        if current == 1.0 && target == 1.0 {
            return None;
        }
        Some(Self {
            from: current,
            to: target,
            frames: frames.max(1),
        })
    }

    /// Gain for frame `i` of the callback; the last frame lands on target.
    #[inline]
    pub(crate) fn gain_at(&self, i: usize) -> f32 {
        if self.from == self.to || i + 1 >= self.frames {
            return self.to;
        }
        let t = (i + 1) as f32 / self.frames as f32;
        self.from + (self.to - self.from) * t
    }
}
