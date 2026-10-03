//! #1048 — a physical endpoint is named by device and 1-based channels, the
//! way the output pickers show it ("Quantum HD 8 · Out 1/2").

use super::physical_endpoint_label;
use domain::AudioDeviceDescriptor;

fn devices() -> Vec<AudioDeviceDescriptor> {
    vec![AudioDeviceDescriptor {
        id: "hd8".into(),
        name: "Quantum HD 8".into(),
        channels: 30,
    }]
}

#[test]
fn a_stereo_output_is_named_by_device_and_both_channels() {
    assert_eq!(
        physical_endpoint_label("Out", "hd8", &[24, 25], &devices()),
        "Quantum HD 8 · Out 25/26"
    );
}

#[test]
fn a_mono_input_is_named_by_its_one_channel() {
    assert_eq!(
        physical_endpoint_label("In", "hd8", &[0], &devices()),
        "Quantum HD 8 · In 1"
    );
}

#[test]
fn a_device_the_host_no_longer_lists_shows_its_id() {
    assert_eq!(
        physical_endpoint_label("Out", "gone", &[2, 3], &devices()),
        "gone · Out 3/4"
    );
}
