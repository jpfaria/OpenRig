//! Responsibility: computes the per-sample gains a split applies to its paths.
//!
//! Pure numbers, no engine types (#328, spec §1.2). Levels and master are
//! linear gains (`x/100` of the knob); balance and pan run −50…+50.

/// Where a balance or pan knob reaches its end stop.
pub const PAN_EDGE: f32 = 50.0;

/// The split and mixer knobs as the audio thread uses them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitKnobValues {
    pub dual_mono: bool,
    pub level_to_a: f32,
    pub level_to_b: f32,
    pub balance_a: f32,
    pub balance_b: f32,
    pub mix_level_a: f32,
    pub mix_level_b: f32,
    pub mix_pan_a: f32,
    pub mix_pan_b: f32,
    pub mix_b_invert: bool,
    pub mix_master: f32,
    pub mix_master_sum: bool,
}

impl SplitKnobValues {
    /// Y → A/B: the paths meet at unity; the mixer knobs do not apply.
    pub fn with_neutral_mixer(self) -> Self {
        Self {
            mix_level_a: 1.0,
            mix_level_b: 1.0,
            mix_pan_a: 0.0,
            mix_pan_b: 0.0,
            mix_b_invert: false,
            mix_master: 1.0,
            mix_master_sum: false,
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

#[inline]
fn path_input(frame: [f32; 2], dual_mono: bool, balance: f32, level: f32) -> [f32; 2] {
    if dual_mono {
        let sample = balance_pick(frame, balance) * level;
        [sample, sample]
    } else {
        [frame[0] * level, frame[1] * level]
    }
}

/// What each path receives from one bus frame: Mode I = the bus × level,
/// Mode II = the dual-mono channel its balance picks × level.
#[inline]
pub fn split_inputs(frame: [f32; 2], knobs: &SplitKnobValues) -> ([f32; 2], [f32; 2]) {
    (
        path_input(frame, knobs.dual_mono, knobs.balance_a, knobs.level_to_a),
        path_input(frame, knobs.dual_mono, knobs.balance_b, knobs.level_to_b),
    )
}

/// The mixer: per-path balance and level, B polarity, master, optional
/// master sum to dual mono.
#[inline]
pub fn mix_frame(a: [f32; 2], b: [f32; 2], knobs: &SplitKnobValues) -> [f32; 2] {
    let (a_left, a_right) = pan_gains(knobs.mix_pan_a);
    let (b_left, b_right) = pan_gains(knobs.mix_pan_b);
    let b_gain = if knobs.mix_b_invert {
        -knobs.mix_level_b
    } else {
        knobs.mix_level_b
    };
    let left = (a[0] * a_left * knobs.mix_level_a + b[0] * b_left * b_gain) * knobs.mix_master;
    let right = (a[1] * a_right * knobs.mix_level_a + b[1] * b_right * b_gain) * knobs.mix_master;
    if knobs.mix_master_sum {
        let mid = (left + right) * 0.5;
        [mid, mid]
    } else {
        [left, right]
    }
}

#[cfg(test)]
#[path = "runtime_split_mix_tests.rs"]
mod tests;
