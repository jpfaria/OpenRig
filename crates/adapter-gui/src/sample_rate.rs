//! Single source of truth for resolving the sample rate a live-analysis
//! consumer (tuner, spectrum, latency probe) must use for an input.
//!
//! The detectors are rate-agnostic *as long as they are told the true rate*.
//! Issue #723: every consumer used to fall back to a hardcoded 48000 when an
//! input had no saved per-device setting. On a device whose default rate is
//! 44100 that biased pitch/spectrum readings by `48000/44100 ≈ +1.47`
//! semitones (a played E displayed as F) and skewed latency estimates. The
//! DI loop suffered the same class of bug (#669) before it learned to
//! resample to the device's live rate. Each interface can run at whatever
//! rate suits it; nothing downstream may assume a fixed rate.

use domain::ids::DeviceId;
use project::project::Project;

/// Resolve the sample rate to use for an input on `device_id`.
///
/// The project's per-device setting is authoritative when present — the live
/// stream is forced to that exact rate or fails to build, so it can never
/// disagree. When no setting exists the input inherits `live_sample_rate`:
/// the rate the controller actually negotiated with the device, NEVER a
/// hardcoded guess.
pub(crate) fn resolve_input_sample_rate(
    project: &Project,
    device_id: &DeviceId,
    live_sample_rate: u32,
) -> usize {
    project
        .device_settings
        .iter()
        .find(|d| &d.device_id == device_id)
        .map(|d| d.sample_rate)
        .unwrap_or(live_sample_rate) as usize
}

#[cfg(test)]
#[path = "sample_rate_tests.rs"]
mod tests;
