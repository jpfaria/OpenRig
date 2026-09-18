use domain::ids::DeviceId;
use project::device::DeviceSettings;

use super::settings_still_match;

fn settings(id: &str, rate: u32, buffer: u32) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(id.into()),
        sample_rate: rate,
        buffer_size_frames: buffer,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

#[test]
fn unchanged_settings_let_the_live_config_serve_the_next_build() {
    let live = vec![settings("quantum", 44_100, 64)];
    let project = vec![
        settings("other", 48_000, 128),
        settings("quantum", 44_100, 64),
    ];
    assert!(settings_still_match(&live, &["quantum".into()], &project));
}

#[test]
fn a_new_buffer_size_needs_a_fresh_resolve() {
    let live = vec![settings("quantum", 44_100, 64)];
    let project = vec![settings("quantum", 44_100, 128)];
    assert!(!settings_still_match(&live, &["quantum".into()], &project));
}

#[test]
fn settings_added_for_a_device_resolved_without_them_need_a_fresh_resolve() {
    let project = vec![settings("quantum", 48_000, 64)];
    assert!(!settings_still_match(&[], &["quantum".into()], &project));
}
