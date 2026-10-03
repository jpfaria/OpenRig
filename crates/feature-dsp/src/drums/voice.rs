//! Responsibility: plays one triggered drum sample into a stereo buffer.

use std::f32::consts::FRAC_PI_4;

use super::role::DrumRole;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Voice {
    pub active: bool,
    pub role: DrumRole,
    pub layer: usize,
    pub sample: usize,
    pub pos: usize,
    pub gain_l: f32,
    pub gain_r: f32,
    pub choke_group: Option<u8>,
    /// Frames left in a choke fade; meaningful only while `fade_len > 0`.
    pub fade_left: u32,
    pub fade_len: u32,
    /// Start order, used to steal the oldest voice.
    pub seq: u64,
}

impl Voice {
    pub const SILENT: Voice = Voice {
        active: false,
        role: DrumRole::Kick,
        layer: 0,
        sample: 0,
        pos: 0,
        gain_l: 0.0,
        gain_r: 0.0,
        choke_group: None,
        fade_left: 0,
        fade_len: 0,
        seq: 0,
    };

    /// Adds the voice into `left`/`right` until the buffer or the sample ends.
    pub fn render(&mut self, samples: &[f32], left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if self.pos >= samples.len() || (self.fade_len > 0 && self.fade_left == 0) {
                self.active = false;
                return;
            }
            let mut s = samples[self.pos];
            if self.fade_len > 0 {
                s *= self.fade_left as f32 / self.fade_len as f32;
                self.fade_left -= 1;
            }
            *l += s * self.gain_l;
            *r += s * self.gain_r;
            self.pos += 1;
        }
        if self.pos >= samples.len() {
            self.active = false;
        }
    }

    /// Starts a linear fade-out of `fade_len` frames, unless already fading.
    pub fn choke(&mut self, fade_len: u32) {
        if self.fade_len == 0 {
            self.fade_len = fade_len.max(1);
            self.fade_left = self.fade_len;
        }
    }
}

/// Constant-power pan law: `(left, right)` gains for `pan` in -1.0 ..= 1.0.
pub(crate) fn pan_gains(pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * FRAC_PI_4;
    (angle.cos(), angle.sin())
}
