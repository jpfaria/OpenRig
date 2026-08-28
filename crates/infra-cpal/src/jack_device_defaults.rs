//! Responsibility: picks the stream values jackd starts an unconfigured card with.
//!
//! Split out of `device_settings` (#913). A card the user HAS configured brings
//! its own values and they stand — including the Orange Pi's tuned low-latency
//! buffer, so the latency invariant is untouched. This is only the fallback for
//! a card nobody configured, and the buffer half of it is a hard-won number:
//! #479, a USB class-compliant interface on a generic (non-RT) desktop kernel
//! cannot sustain 64 frames — it xruns continuously and the sound is unusable.

//! Only jackd consumes these, so on macOS/Windows the module is dead code —
//! but the values are pinned by tests that run everywhere, which is the point:
//! the #479 minimum must not be edited from a Mac without the guard firing.
#![cfg_attr(not(all(target_os = "linux", feature = "jack")), allow(dead_code))]

use project::device::DeviceSettings;

/// The rate an unconfigured card is started at.
pub(crate) const FALLBACK_SAMPLE_RATE: u32 = 48_000;
/// The buffer an unconfigured card is started at. 256 is the safe minimum for
/// USB audio on a non-RT kernel (#479); anything smaller xruns continuously.
pub(crate) const FALLBACK_BUFFER_FRAMES: u32 = 256;

/// `(sample_rate, buffer_size_frames)` for a card, configured or not.
pub(crate) fn stream_values_for(configured: Option<&DeviceSettings>) -> (u32, u32) {
    match configured {
        Some(settings) => (settings.sample_rate, settings.buffer_size_frames),
        None => (FALLBACK_SAMPLE_RATE, FALLBACK_BUFFER_FRAMES),
    }
}

#[cfg(test)]
#[path = "jack_device_defaults_tests.rs"]
mod tests;
