use super::*;
use domain::ids::DeviceId;
use project::device::DeviceSettings;

fn device_settings(device: &str, sample_rate: u32) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(device.into()),
        sample_rate,
        buffer_size_frames: 256,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

fn empty_project() -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    }
}

#[test]
fn resolve_input_sample_rate_uses_device_setting_when_present() {
    let mut project = empty_project();
    project.device_settings = vec![device_settings("dev:1", 44_100)];

    // The configured rate is authoritative — the live fallback is ignored.
    assert_eq!(
        resolve_input_sample_rate(&project, &DeviceId("dev:1".into()), 96_000),
        44_100,
    );
}

#[test]
fn resolve_input_sample_rate_falls_back_to_live_rate_not_48000() {
    // No device_settings entry: the input must inherit the sample rate the
    // stream actually negotiated, NOT a hardcoded 48000. Issue #723: a
    // 48000-assumed / 44100-live mismatch biases YIN by ~1.47 semitones, so
    // a played E is shown as F.
    let project = empty_project();
    assert!(project.device_settings.is_empty());

    assert_eq!(
        resolve_input_sample_rate(&project, &DeviceId("dev:1".into()), 44_100),
        44_100,
    );
}
