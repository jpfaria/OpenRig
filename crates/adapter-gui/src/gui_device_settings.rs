//! Responsibility: turns the screen's device choices into engine settings.

use domain::ids::DeviceId;
use infra_filesystem::GuiAudioDeviceSettings;
use project::device::DeviceSettings;

pub(crate) fn build_device_settings_from_gui(
    input_devices: &[GuiAudioDeviceSettings],
    output_devices: &[GuiAudioDeviceSettings],
) -> Vec<DeviceSettings> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for g in input_devices.iter().chain(output_devices.iter()) {
        if seen.insert(g.device_id.clone()) {
            result.push(DeviceSettings {
                device_id: DeviceId(g.device_id.clone()),
                sample_rate: g.sample_rate,
                buffer_size_frames: g.buffer_size_frames,
                bit_depth: g.bit_depth,
                #[cfg(target_os = "linux")]
                realtime: g.realtime,
                #[cfg(target_os = "linux")]
                rt_priority: g.rt_priority,
                #[cfg(target_os = "linux")]
                nperiods: g.nperiods,
            });
        }
    }
    result
}
