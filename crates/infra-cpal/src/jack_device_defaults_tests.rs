//! #913 — what jackd starts a card with.
//!
//! The fallback buffer is the one that matters. #479: a USB class-compliant
//! interface on a generic (non-RT) desktop kernel cannot sustain 64 frames — it
//! xruns continuously and the sound is unusable garbage. And the other half is
//! just as load-bearing the other way: a card the user DID configure keeps its
//! own values, including the Orange Pi's tuned low-latency buffer, so this
//! fallback can never regress the latency invariant on a configured rig.

use super::{stream_values_for, FALLBACK_BUFFER_FRAMES, FALLBACK_SAMPLE_RATE};
use domain::ids::DeviceId;
use project::device::DeviceSettings;

fn configured(sample_rate: u32, buffer_size_frames: u32) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId("card".into()),
        sample_rate,
        buffer_size_frames,
        bit_depth: 24,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

#[test]
fn an_unconfigured_card_gets_the_safe_usb_buffer() {
    let (_, buffer) = stream_values_for(None);
    assert_eq!(
        buffer, 256,
        "#479: below this a USB interface on a non-RT kernel xruns continuously"
    );
    assert_eq!(buffer, FALLBACK_BUFFER_FRAMES);
}

#[test]
fn an_unconfigured_card_gets_the_fallback_rate() {
    let (rate, _) = stream_values_for(None);
    assert_eq!(rate, 48_000);
    assert_eq!(rate, FALLBACK_SAMPLE_RATE);
}

#[test]
fn a_configured_card_keeps_its_own_values() {
    assert_eq!(
        stream_values_for(Some(&configured(44_100, 128))),
        (44_100, 128)
    );
}

#[test]
fn a_tuned_low_latency_buffer_is_never_raised_to_the_fallback() {
    // The Orange Pi carries an explicit 64 in its saved settings. Clamping it
    // up to the USB fallback here would regress the latency invariant on the
    // one machine that CAN sustain it.
    let (_, buffer) = stream_values_for(Some(&configured(48_000, 64)));
    assert_eq!(buffer, 64);
}

#[test]
fn a_configured_rate_above_the_fallback_is_kept() {
    assert_eq!(
        stream_values_for(Some(&configured(96_000, 512))),
        (96_000, 512)
    );
}
