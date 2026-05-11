//! Equality / clone / Debug tests for the value types — `AudioDeviceDescriptor`
//! plus the `*StreamSignature` family.
//!
//! Pulled out of `tests.rs` to keep that file under the 600-LOC cap.

#![cfg(test)]

use super::AudioDeviceDescriptor;

// ── AudioDeviceDescriptor additional tests ──────────────────────────────

#[test]
fn audio_device_descriptor_different_channels_not_equal() {
    let a = AudioDeviceDescriptor {
        id: "dev1".to_string(),
        name: "Device".to_string(),
        channels: 2,
    };
    let b = AudioDeviceDescriptor {
        id: "dev1".to_string(),
        name: "Device".to_string(),
        channels: 4,
    };
    assert_ne!(a, b);
}

#[test]
fn audio_device_descriptor_different_name_not_equal() {
    let a = AudioDeviceDescriptor {
        id: "dev1".to_string(),
        name: "Device A".to_string(),
        channels: 2,
    };
    let b = AudioDeviceDescriptor {
        id: "dev1".to_string(),
        name: "Device B".to_string(),
        channels: 2,
    };
    assert_ne!(a, b);
}

#[test]
fn audio_device_descriptor_zero_channels() {
    let desc = AudioDeviceDescriptor {
        id: "dev0".to_string(),
        name: "Null".to_string(),
        channels: 0,
    };
    assert_eq!(desc.channels, 0);
}

// ── InputStreamSignature / OutputStreamSignature equality ────────────────

#[test]
fn input_stream_signature_equality() {
    use super::resolved::InputStreamSignature;
    let a = InputStreamSignature {
        device_id: "dev1".to_string(),
        channels: vec![0, 1],
        stream_channels: 2,
        sample_rate: 48_000,
        buffer_size_frames: 256,
    };
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn input_stream_signature_different_rate_not_equal() {
    use super::resolved::InputStreamSignature;
    let a = InputStreamSignature {
        device_id: "dev1".to_string(),
        channels: vec![0, 1],
        stream_channels: 2,
        sample_rate: 48_000,
        buffer_size_frames: 256,
    };
    let b = InputStreamSignature {
        sample_rate: 44_100,
        ..a.clone()
    };
    assert_ne!(a, b);
}

#[test]
fn output_stream_signature_equality() {
    use super::resolved::OutputStreamSignature;
    let a = OutputStreamSignature {
        device_id: "dev1".to_string(),
        channels: vec![0, 1],
        stream_channels: 2,
        sample_rate: 48_000,
        buffer_size_frames: 256,
    };
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn output_stream_signature_different_channels_not_equal() {
    use super::resolved::OutputStreamSignature;
    let a = OutputStreamSignature {
        device_id: "dev1".to_string(),
        channels: vec![0, 1],
        stream_channels: 2,
        sample_rate: 48_000,
        buffer_size_frames: 256,
    };
    let b = OutputStreamSignature {
        channels: vec![0],
        ..a.clone()
    };
    assert_ne!(a, b);
}

// ── ChainStreamSignature equality ───────────────────────────────────────

#[test]
fn chain_stream_signature_equality() {
    use super::resolved::{ChainStreamSignature, InputStreamSignature, OutputStreamSignature};
    let a = ChainStreamSignature {
        inputs: vec![InputStreamSignature {
            device_id: "dev1".to_string(),
            channels: vec![0],
            stream_channels: 1,
            sample_rate: 48_000,
            buffer_size_frames: 256,
        }],
        outputs: vec![OutputStreamSignature {
            device_id: "dev2".to_string(),
            channels: vec![0, 1],
            stream_channels: 2,
            sample_rate: 48_000,
            buffer_size_frames: 256,
        }],
    };
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn chain_stream_signature_different_inputs_not_equal() {
    use super::resolved::{ChainStreamSignature, InputStreamSignature};
    let a = ChainStreamSignature {
        inputs: vec![InputStreamSignature {
            device_id: "dev1".to_string(),
            channels: vec![0],
            stream_channels: 1,
            sample_rate: 48_000,
            buffer_size_frames: 256,
        }],
        outputs: vec![],
    };
    let b = ChainStreamSignature {
        inputs: vec![InputStreamSignature {
            device_id: "dev2".to_string(),
            channels: vec![0],
            stream_channels: 1,
            sample_rate: 48_000,
            buffer_size_frames: 256,
        }],
        outputs: vec![],
    };
    assert_ne!(a, b);
}
