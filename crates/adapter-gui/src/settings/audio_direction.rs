//! Responsibility: sorts the chosen audio devices by the direction each one plays.
//!
//! The audio section shows one flat device list, but the config records inputs
//! and outputs apart, and the same physical interface can enumerate with a
//! different id per direction. Each chosen device goes where the live
//! descriptors say it belongs: both lists when it is exposed in both
//! directions, and the inputs when it matches neither (stale or disconnected),
//! so a choice is never silently dropped.

use domain::AudioDeviceDescriptor;
use infra_filesystem::GuiAudioDeviceSettings;

pub(crate) fn split_by_direction(
    chosen: &[GuiAudioDeviceSettings],
    input_descriptors: &[AudioDeviceDescriptor],
    output_descriptors: &[AudioDeviceDescriptor],
) -> (Vec<GuiAudioDeviceSettings>, Vec<GuiAudioDeviceSettings>) {
    let listed =
        |descriptors: &[AudioDeviceDescriptor], id: &str| descriptors.iter().any(|d| d.id == id);
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for device in chosen {
        let is_input = listed(input_descriptors, &device.device_id);
        let is_output = listed(output_descriptors, &device.device_id);
        if is_input || !is_output {
            inputs.push(device.clone());
        }
        if is_output {
            outputs.push(device.clone());
        }
    }
    (inputs, outputs)
}
