use super::is_rate_change_timeout;

#[test]
fn cpal_018_rate_change_timeout_is_recognised() {
    // The exact error cpal 0.18 returns from a CoreAudio rate change that has
    // not settled (coreaudio/macos/device.rs). The 0.17 text matched
    // "timeout"; this one does not, so the 3 s USB settle and the device-cache
    // invalidation silently stopped happening (#978).
    let err = cpal::Error::with_message(
        cpal::ErrorKind::DeviceNotAvailable,
        "Sample rate update timed out",
    );
    assert!(is_rate_change_timeout(&err));
}

#[test]
fn a_device_that_is_simply_gone_is_not_a_rate_change_timeout() {
    let err = cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable);
    assert!(!is_rate_change_timeout(&err));
    let other = cpal::Error::with_message(cpal::ErrorKind::BackendError, "timed out");
    assert!(!is_rate_change_timeout(&other));
}

#[test]
fn asio_devices_are_not_probed_with_a_throwaway_stream() {
    use super::probes_rate_with_a_stream;
    assert!(!probes_rate_with_a_stream(true));
    assert!(
        probes_rate_with_a_stream(false),
        "CoreAudio/WASAPI keep the probe"
    );
}
