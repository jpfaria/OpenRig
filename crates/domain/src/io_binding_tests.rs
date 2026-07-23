use super::*;
use crate::ids::DeviceId;

// ── ChannelMode wire tokens ──────────────────────────────────────────

#[test]
fn channel_mode_mono_serializes_to_mono() {
    let s = serde_yaml::to_string(&ChannelMode::Mono).unwrap();
    assert_eq!(s.trim(), "mono");
}

#[test]
fn channel_mode_stereo_serializes_to_stereo() {
    let s = serde_yaml::to_string(&ChannelMode::Stereo).unwrap();
    assert_eq!(s.trim(), "stereo");
}

#[test]
fn channel_mode_dual_mono_serializes_to_dual_mono() {
    let s = serde_yaml::to_string(&ChannelMode::DualMono).unwrap();
    assert_eq!(s.trim(), "dual_mono");
}

#[test]
fn channel_mode_mono_deserializes_from_mono() {
    let v: ChannelMode = serde_yaml::from_str("mono").unwrap();
    assert_eq!(v, ChannelMode::Mono);
}

#[test]
fn channel_mode_stereo_deserializes_from_stereo() {
    let v: ChannelMode = serde_yaml::from_str("stereo").unwrap();
    assert_eq!(v, ChannelMode::Stereo);
}

#[test]
fn channel_mode_dual_mono_deserializes_from_dual_mono() {
    let v: ChannelMode = serde_yaml::from_str("dual_mono").unwrap();
    assert_eq!(v, ChannelMode::DualMono);
}

// ── IoBinding round-trip ─────────────────────────────────────────────

#[test]
fn io_binding_round_trip_with_inputs_and_outputs() {
    let binding = IoBinding {
        id: "main".into(),
        name: "Scarlett 2i2".into(),
        inputs: vec![IoEndpoint {
            name: "Guitar In 1".into(),
            device_id: DeviceId("dev-001".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Monitor Out".into(),
            device_id: DeviceId("dev-001".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    };
    let yaml = serde_yaml::to_string(&binding).unwrap();
    let restored: IoBinding = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(binding, restored);
}

#[test]
fn io_binding_round_trip_preserves_dual_mono_endpoint() {
    let binding = IoBinding {
        id: "dual".into(),
        name: "Dual Guitar".into(),
        inputs: vec![IoEndpoint {
            name: "Guitar Pair".into(),
            device_id: DeviceId("dev-002".into()),
            mode: ChannelMode::DualMono,
            channels: vec![0, 1],
        }],
        outputs: vec![],
    };
    let yaml = serde_yaml::to_string(&binding).unwrap();
    let restored: IoBinding = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(binding, restored);
    assert_eq!(restored.inputs[0].mode, ChannelMode::DualMono);
}
