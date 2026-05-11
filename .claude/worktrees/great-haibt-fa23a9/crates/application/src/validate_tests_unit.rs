//! Unit tests for helper functions of `application::validate`.
//!
//! Targets: `layout_from_channel_count`, `validate_unique_channels`,
//! `layout_label`, `validate_active_chain_input_channel_conflicts`,
//! `validate_device_settings`. Shared fixtures from sibling `helpers`.

use super::helpers::*;
use std::collections::HashMap;

// -----------------------------------------------------------------------
// layout_from_channel_count — unit tests
// -----------------------------------------------------------------------

#[test]
fn layout_from_channel_count_mono_returns_mono() {
    let layout = layout_from_channel_count("test", "id", 1).unwrap();
    assert_eq!(layout, AudioChannelLayout::Mono);
}

#[test]
fn layout_from_channel_count_stereo_returns_stereo() {
    let layout = layout_from_channel_count("test", "id", 2).unwrap();
    assert_eq!(layout, AudioChannelLayout::Stereo);
}

#[test]
fn layout_from_channel_count_zero_channels_fails() {
    let err = layout_from_channel_count("test", "id", 0).unwrap_err();
    assert!(err.to_string().contains("0 channels"));
}

#[test]
fn layout_from_channel_count_four_channels_fails() {
    let err = layout_from_channel_count("test", "id", 4).unwrap_err();
    assert!(err.to_string().contains("4 channels"));
}

// -----------------------------------------------------------------------
// validate_unique_channels — unit tests
// -----------------------------------------------------------------------

#[test]
fn validate_unique_channels_no_duplicates_succeeds() {
    assert!(validate_unique_channels(&[0, 1, 2]).is_ok());
}

#[test]
fn validate_unique_channels_empty_succeeds() {
    assert!(validate_unique_channels(&[]).is_ok());
}

#[test]
fn validate_unique_channels_single_succeeds() {
    assert!(validate_unique_channels(&[5]).is_ok());
}

#[test]
fn validate_unique_channels_duplicate_fails() {
    let err = validate_unique_channels(&[0, 1, 0]).unwrap_err();
    assert!(err.to_string().contains("duplicated channel '0'"));
}

// -----------------------------------------------------------------------
// layout_label — unit tests
// -----------------------------------------------------------------------

#[test]
fn layout_label_mono_returns_mono() {
    assert_eq!(layout_label(AudioChannelLayout::Mono), "mono");
}

#[test]
fn layout_label_stereo_returns_stereo() {
    assert_eq!(layout_label(AudioChannelLayout::Stereo), "stereo");
}

// -----------------------------------------------------------------------
// validate_active_chain_input_channel_conflicts — unit tests
// -----------------------------------------------------------------------

#[test]
fn channel_conflicts_no_chains_succeeds() {
    assert!(validate_active_chain_input_channel_conflicts(&[]).is_ok());
}

#[test]
fn channel_conflicts_single_chain_succeeds() {
    let chain = valid_chain("chain:0");
    assert!(validate_active_chain_input_channel_conflicts(&[chain]).is_ok());
}

#[test]
fn channel_conflicts_different_devices_succeeds() {
    let chain0 = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-a", vec![0]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let chain1 = test_chain(
        "chain:1",
        vec![
            test_input_block("dev-b", vec![0]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    assert!(validate_active_chain_input_channel_conflicts(&[chain0, chain1]).is_ok());
}

#[test]
fn channel_conflicts_same_device_same_channel_fails() {
    let chain0 = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let chain1 = test_chain(
        "chain:1",
        vec![
            test_input_block("dev-in", vec![0]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let err = validate_active_chain_input_channel_conflicts(&[chain0, chain1]).unwrap_err();
    assert!(err.to_string().contains("both use input device"));
}

#[test]
fn channel_conflicts_disabled_chains_ignored() {
    let chain0 = valid_chain("chain:0");
    let mut chain1 = test_chain(
        "chain:1",
        vec![
            test_input_block("dev-in", vec![0]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    chain1.enabled = false;
    assert!(validate_active_chain_input_channel_conflicts(&[chain0, chain1]).is_ok());
}

// -----------------------------------------------------------------------
// validate_device_settings — unit tests
// -----------------------------------------------------------------------

#[test]
fn validate_device_settings_valid_succeeds() {
    let project = valid_project();
    let map: HashMap<_, _> = project
        .device_settings
        .iter()
        .map(|s| (s.device_id.0.clone(), s))
        .collect();
    assert!(validate_device_settings(&project, &map).is_ok());
}

#[test]
fn validate_device_settings_empty_device_id_fails() {
    let settings = vec![DeviceSettings {
        device_id: DeviceId("  ".to_string()),
        sample_rate: 48000,
        buffer_size_frames: 256,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }];
    let project = Project {
        name: Some("test".to_string()),
        device_settings: settings,
        chains: vec![valid_chain("chain:0")],
    };
    let map: HashMap<_, _> = project
        .device_settings
        .iter()
        .map(|s| (s.device_id.0.clone(), s))
        .collect();
    let err = validate_device_settings(&project, &map).unwrap_err();
    assert!(err.to_string().contains("missing device_id"));
}

#[test]
fn validate_device_settings_duplicate_device_id_fails() {
    let settings = vec![test_device_settings("dev-a"), test_device_settings("dev-a")];
    let project = Project {
        name: Some("test".to_string()),
        device_settings: settings.clone(),
        chains: vec![valid_chain("chain:0")],
    };
    // HashMap will deduplicate, so len != original len
    let map: HashMap<_, _> = project
        .device_settings
        .iter()
        .map(|s| (s.device_id.0.clone(), s))
        .collect();
    let err = validate_device_settings(&project, &map).unwrap_err();
    assert!(err.to_string().contains("duplicated device_settings"));
}
