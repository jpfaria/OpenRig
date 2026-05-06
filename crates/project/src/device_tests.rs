use super::*;

fn sample() -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId("coreaudio:scarlett".into()),
        sample_rate: 48000,
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

#[test]
fn device_settings_construction() {
    let settings = sample();
    assert_eq!(settings.device_id.0, "coreaudio:scarlett");
    assert_eq!(settings.sample_rate, 48000);
    assert_eq!(settings.buffer_size_frames, 256);
    assert_eq!(settings.bit_depth, 32);
    #[cfg(target_os = "linux")]
    {
        assert!(settings.realtime);
        assert_eq!(settings.rt_priority, 70);
        assert_eq!(settings.nperiods, 3);
    }
}

#[test]
fn device_settings_clone_equality() {
    let settings = DeviceSettings {
        device_id: DeviceId("dev".into()),
        sample_rate: 44100,
        buffer_size_frames: 128,
        ..sample()
    };
    let cloned = settings.clone();
    assert_eq!(settings, cloned);
}

#[test]
fn device_settings_inequality_different_sample_rate() {
    let a = sample();
    let b = DeviceSettings {
        sample_rate: 96000,
        ..sample()
    };
    assert_ne!(a, b);
}

#[test]
fn device_settings_inequality_different_device() {
    let a = DeviceSettings {
        device_id: DeviceId("dev-a".into()),
        ..sample()
    };
    let b = DeviceSettings {
        device_id: DeviceId("dev-b".into()),
        ..sample()
    };
    assert_ne!(a, b);
}

#[test]
fn device_settings_inequality_different_buffer_size() {
    let a = DeviceSettings {
        buffer_size_frames: 64,
        ..sample()
    };
    let b = DeviceSettings {
        buffer_size_frames: 512,
        ..sample()
    };
    assert_ne!(a, b);
}

#[test]
fn device_settings_common_sample_rates() {
    for rate in [44100, 48000, 88200, 96000] {
        let settings = DeviceSettings {
            sample_rate: rate,
            ..sample()
        };
        assert_eq!(settings.sample_rate, rate);
    }
}

#[test]
fn device_settings_common_buffer_sizes() {
    for size in [32, 64, 128, 256, 512, 1024] {
        let settings = DeviceSettings {
            buffer_size_frames: size,
            ..sample()
        };
        assert_eq!(settings.buffer_size_frames, size);
    }
}

#[test]
fn device_settings_bit_depth_values() {
    for depth in [16, 24, 32] {
        let settings = DeviceSettings {
            bit_depth: depth,
            ..sample()
        };
        assert_eq!(settings.bit_depth, depth);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn device_settings_realtime_toggle() {
    let on = sample();
    let off = DeviceSettings {
        realtime: false,
        ..sample()
    };
    assert_ne!(off, on);
}

#[cfg(target_os = "linux")]
#[test]
fn device_settings_nperiods_range() {
    for n in [2, 3, 4] {
        let settings = DeviceSettings {
            nperiods: n,
            ..sample()
        };
        assert_eq!(settings.nperiods, n);
    }
}
