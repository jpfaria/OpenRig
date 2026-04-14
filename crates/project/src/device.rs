use domain::ids::DeviceId;
use serde::{Deserialize, Serialize};

fn default_bit_depth() -> u32 {
    32
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceSettings {
    pub device_id: DeviceId,
    pub sample_rate: u32,
    pub buffer_size_frames: u32,
    #[serde(default = "default_bit_depth")]
    pub bit_depth: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_settings_construction() {
        let settings = DeviceSettings {
            device_id: DeviceId("coreaudio:scarlett".into()),
            sample_rate: 48000,
            buffer_size_frames: 256,
            bit_depth: 32,
        };
        assert_eq!(settings.device_id.0, "coreaudio:scarlett");
        assert_eq!(settings.sample_rate, 48000);
        assert_eq!(settings.buffer_size_frames, 256);
        assert_eq!(settings.bit_depth, 32);
    }

    #[test]
    fn device_settings_clone_equality() {
        let settings = DeviceSettings {
            device_id: DeviceId("dev".into()),
            sample_rate: 44100,
            buffer_size_frames: 128,
            bit_depth: 32,
        };
        let cloned = settings.clone();
        assert_eq!(settings, cloned);
    }

    #[test]
    fn device_settings_inequality_different_sample_rate() {
        let a = DeviceSettings {
            device_id: DeviceId("dev".into()),
            sample_rate: 44100,
            buffer_size_frames: 128,
            bit_depth: 32,
        };
        let b = DeviceSettings {
            device_id: DeviceId("dev".into()),
            sample_rate: 96000,
            buffer_size_frames: 128,
            bit_depth: 32,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn device_settings_inequality_different_device() {
        let a = DeviceSettings {
            device_id: DeviceId("dev-a".into()),
            sample_rate: 48000,
            buffer_size_frames: 256,
            bit_depth: 32,
        };
        let b = DeviceSettings {
            device_id: DeviceId("dev-b".into()),
            sample_rate: 48000,
            buffer_size_frames: 256,
            bit_depth: 32,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn device_settings_inequality_different_buffer_size() {
        let a = DeviceSettings {
            device_id: DeviceId("dev".into()),
            sample_rate: 48000,
            buffer_size_frames: 64,
            bit_depth: 32,
        };
        let b = DeviceSettings {
            device_id: DeviceId("dev".into()),
            sample_rate: 48000,
            buffer_size_frames: 512,
            bit_depth: 32,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn device_settings_common_sample_rates() {
        for rate in [44100, 48000, 88200, 96000] {
            let settings = DeviceSettings {
                device_id: DeviceId("dev".into()),
                sample_rate: rate,
                buffer_size_frames: 256,
                bit_depth: 32,
            };
            assert_eq!(settings.sample_rate, rate);
        }
    }

    #[test]
    fn device_settings_common_buffer_sizes() {
        for size in [32, 64, 128, 256, 512, 1024] {
            let settings = DeviceSettings {
                device_id: DeviceId("dev".into()),
                sample_rate: 48000,
                buffer_size_frames: size,
                bit_depth: 32,
            };
            assert_eq!(settings.buffer_size_frames, size);
        }
    }

    #[test]
    fn device_settings_bit_depth_values() {
        for depth in [16, 24, 32] {
            let settings = DeviceSettings {
                device_id: DeviceId("dev".into()),
                sample_rate: 48000,
                buffer_size_frames: 256,
                bit_depth: depth,
            };
            assert_eq!(settings.bit_depth, depth);
        }
    }

}
