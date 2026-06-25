//! Unit tests for helper functions of `application::validate`.
//!
//! Targets: `layout_label`, `validate_device_settings`. Shared fixtures from
//! sibling `helpers`.
//!
//! #716 (model A): the `layout_from_channel_count`, `validate_unique_channels`
//! and `validate_active_chain_input_channel_conflicts` helpers were removed —
//! they derived device endpoints from per-block `entries`, which no longer
//! exist on the chain (endpoints are resolved from the I/O binding registry at
//! the activation layer). Their unit tests were removed with them.

use super::helpers::*;
use std::collections::HashMap;

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
        midi: None,
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
        midi: None,
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
