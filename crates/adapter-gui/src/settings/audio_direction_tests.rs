//! The flat device list of the audio section lands in the config's input and
//! output lists by what the live descriptors say each device is.

use domain::AudioDeviceDescriptor;
use infra_filesystem::GuiAudioDeviceSettings;

use super::audio_direction::split_by_direction;

fn chosen(id: &str) -> GuiAudioDeviceSettings {
    GuiAudioDeviceSettings {
        device_id: id.to_string(),
        name: id.to_string(),
        ..Default::default()
    }
}

fn descriptor(id: &str) -> AudioDeviceDescriptor {
    AudioDeviceDescriptor {
        id: id.to_string(),
        name: id.to_string(),
        channels: 2,
    }
}

fn ids(list: &[GuiAudioDeviceSettings]) -> Vec<&str> {
    list.iter().map(|d| d.device_id.as_str()).collect()
}

#[test]
fn an_input_only_device_goes_to_the_inputs() {
    let (inputs, outputs) = split_by_direction(&[chosen("mic")], &[descriptor("mic")], &[]);
    assert_eq!(ids(&inputs), ["mic"]);
    assert!(outputs.is_empty());
}

#[test]
fn an_output_only_device_goes_to_the_outputs() {
    let (inputs, outputs) = split_by_direction(&[chosen("spk")], &[], &[descriptor("spk")]);
    assert!(inputs.is_empty());
    assert_eq!(ids(&outputs), ["spk"]);
}

#[test]
fn a_device_exposed_both_ways_goes_to_both_lists() {
    let (inputs, outputs) = split_by_direction(
        &[chosen("iface")],
        &[descriptor("iface")],
        &[descriptor("iface")],
    );
    assert_eq!(ids(&inputs), ["iface"]);
    assert_eq!(ids(&outputs), ["iface"]);
}

#[test]
fn a_device_no_longer_enumerated_is_kept_as_an_input() {
    let (inputs, outputs) = split_by_direction(&[chosen("gone")], &[], &[]);
    assert_eq!(ids(&inputs), ["gone"]);
    assert!(outputs.is_empty());
}

#[test]
fn an_interface_with_one_id_per_direction_fills_both_lists() {
    let (inputs, outputs) = split_by_direction(
        &[chosen("iface-in"), chosen("iface-out")],
        &[descriptor("iface-in")],
        &[descriptor("iface-out")],
    );
    assert_eq!(ids(&inputs), ["iface-in"]);
    assert_eq!(ids(&outputs), ["iface-out"]);
}
