//! Responsibility: decides the output gain that puts a block's reference output at the target peak.
//!
//! Port of OpenRig-plugins `tools/loudness_audit/src/level.rs` (#143): the
//! reference output peaks at the engine limiter threshold, clamped to the
//! Output knob range. A cab is probed with amp-level signals (clean and
//! saturated DI), a body with the raw DI.

use block_core::db_to_lin;

use super::convolve::convolve;

pub const TARGET_PEAK_DBFS: f32 = -1.0;
pub const OUTPUT_KNOB_MIN_DB: f32 = -24.0;
pub const OUTPUT_KNOB_MAX_DB: f32 = 24.0;
/// Drive into the `tanh` stage of the saturated cab probe.
pub const CAB_PROBE_DRIVE_DB: f32 = 30.0;

/// Which reference signal an IR is levelled against.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum IrRole {
    Cab,
    Body,
}

/// Peak of a signal in dBFS; silence reads -120.
pub fn peak_dbfs(samples: &[f32]) -> f32 {
    let peak = samples.iter().fold(0.0_f32, |a, s| a.max(s.abs()));
    if peak == 0.0 {
        -120.0
    } else {
        20.0 * peak.log10()
    }
}

pub fn target_gain_db(measured_peak_dbfs: f32) -> f32 {
    (TARGET_PEAK_DBFS - measured_peak_dbfs).clamp(OUTPUT_KNOB_MIN_DB, OUTPUT_KNOB_MAX_DB)
}

/// The clean DI and a `tanh`-saturated DI, both peaking at the target.
pub fn amp_level_probes(di: &[f32]) -> [Vec<f32>; 2] {
    let drive = db_to_lin(CAB_PROBE_DRIVE_DB);
    let driven: Vec<f32> = di.iter().map(|s| (s * drive).tanh()).collect();
    [normalised_to_target(di), normalised_to_target(&driven)]
}

/// Reference peak of an IR before makeup: the worst probe for a cab, the
/// DI for a body.
pub fn ir_level_peak_dbfs(ir: &[f32], di: &[f32], role: IrRole) -> f32 {
    match role {
        IrRole::Body => peak_dbfs(&convolve(di, ir)),
        IrRole::Cab => amp_level_probes(di)
            .iter()
            .map(|p| peak_dbfs(&convolve(p, ir)))
            .fold(f32::NEG_INFINITY, f32::max),
    }
}

fn normalised_to_target(x: &[f32]) -> Vec<f32> {
    let scale = db_to_lin(TARGET_PEAK_DBFS - peak_dbfs(x));
    x.iter().map(|s| s * scale).collect()
}
