use super::probe_input_device;

#[test]
fn an_unknown_device_is_reported_as_not_found() {
    let probe = probe_input_device("coreaudio:openrig-no-such-device-979");

    assert_eq!(probe.device_id, "coreaudio:openrig-no-such-device-979");
    assert!(!probe.found);
    assert!(probe.nominal_rate.is_none());
    assert!(probe.input_streams.is_empty());
    assert!(probe.clients.is_empty());
}

/// Read-only: the HAL properties of the machine's default input, when it has
/// one. Proves the probe reads a real device, not just the not-found path.
#[cfg(target_os = "macos")]
#[test]
fn the_default_input_reports_its_rate_and_streams() {
    use cpal::traits::{DeviceTrait, HostTrait};
    let Some(device) = cpal::default_host().default_input_device() else {
        return;
    };
    let Ok(id) = device.id() else {
        return;
    };

    let probe = probe_input_device(&id.to_string());

    assert!(probe.found, "{probe:?}");
    assert!(probe.nominal_rate.is_some_and(|r| r > 0.0), "{probe:?}");
    assert!(probe.name.is_some(), "{probe:?}");
    let stream = probe.input_streams.first().expect("an input stream");
    let format = stream.virtual_format.as_ref().expect("a virtual format");
    assert!(format.channels > 0 && format.sample_rate > 0.0, "{probe:?}");
}
