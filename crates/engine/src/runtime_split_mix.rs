//! Responsibility: computes the per-sample gains a split applies to its paths.
//!
//! Pure numbers, no engine types (#328, spec §1.2, §11.2). Levels and master
//! are linear gains (`x/100` of the knob); balance and pan run −50…+50. Every
//! path has its own split side (level, balance) and mixer side (level, pan,
//! polarity); the master stage runs once on the sum.

/// Where a balance or pan knob reaches its end stop.
pub const PAN_EDGE: f32 = 50.0;

/// One path's knobs as the audio thread uses them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathKnobs {
    pub level_to: f32,
    pub balance: f32,
    pub mix_level: f32,
    pub mix_pan: f32,
    pub invert: bool,
}

/// The split-wide knobs as the audio thread uses them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixKnobs {
    pub dual_mono: bool,
    pub master: f32,
    pub master_sum: bool,
}

impl PathKnobs {
    /// A path that reaches and leaves the split at unity, centred, normal.
    pub const fn neutral() -> Self {
        Self {
            level_to: 1.0,
            balance: 0.0,
            mix_level: 1.0,
            mix_pan: 0.0,
            invert: false,
        }
    }

    /// Y: the paths meet at unity; the mixer knobs do not apply.
    pub fn with_neutral_mixer(self) -> Self {
        Self {
            mix_level: 1.0,
            mix_pan: 0.0,
            invert: false,
            ..self
        }
    }
}

impl MixKnobs {
    /// Y: the master stage does not apply.
    pub fn with_neutral_mixer(self) -> Self {
        Self {
            master: 1.0,
            master_sum: false,
            ..self
        }
    }
}

/// Balance law (spec §1.2): centre is unity on both sides; toward one side
/// the opposite side falls linearly to 0 at ±50.
#[inline]
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let pan = pan.clamp(-PAN_EDGE, PAN_EDGE);
    if pan >= 0.0 {
        (1.0 - pan / PAN_EDGE, 1.0)
    } else {
        (1.0, 1.0 + pan / PAN_EDGE)
    }
}

/// Mode II source for one path: −50 = L, 0 = (L+R)/2, +50 = R, linear
/// in between.
#[inline]
fn balance_pick(frame: [f32; 2], balance: f32) -> f32 {
    let right = (balance.clamp(-PAN_EDGE, PAN_EDGE) + PAN_EDGE) / (2.0 * PAN_EDGE);
    frame[0] * (1.0 - right) + frame[1] * right
}

/// What one path receives from one bus frame: Mode I = the bus × level,
/// Mode II = the dual-mono channel its balance picks × level.
#[inline]
pub fn path_input(frame: [f32; 2], path: &PathKnobs, mix: &MixKnobs) -> [f32; 2] {
    if mix.dual_mono {
        let sample = balance_pick(frame, path.balance) * path.level_to;
        [sample, sample]
    } else {
        [frame[0] * path.level_to, frame[1] * path.level_to]
    }
}

/// Add one path's output to the mixer sum through its pan, level and
/// polarity.
#[inline]
pub fn accumulate_path(acc: &mut [f32; 2], out: [f32; 2], path: &PathKnobs) {
    let (left, right) = pan_gains(path.mix_pan);
    let gain = if path.invert {
        -path.mix_level
    } else {
        path.mix_level
    };
    acc[0] += out[0] * left * gain;
    acc[1] += out[1] * right * gain;
}

/// The master stage on the mixer sum, with the optional sum to dual mono.
#[inline]
pub fn finish_mix(acc: [f32; 2], mix: &MixKnobs) -> [f32; 2] {
    let left = acc[0] * mix.master;
    let right = acc[1] * mix.master;
    if mix.master_sum {
        let mid = (left + right) * 0.5;
        [mid, mid]
    } else {
        [left, right]
    }
}

#[cfg(test)]
#[path = "runtime_split_mix_tests.rs"]
mod tests;
