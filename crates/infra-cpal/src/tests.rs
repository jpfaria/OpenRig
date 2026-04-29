//! Unit tests for `infra-cpal`. Lives in its own file because the
//! aggregate body crossed the 1000-LOC test ceiling.
//!
//! From here `super::*` resolves to the crate root (lib.rs), so the
//! existing `super::resolved::*`, `super::active_runtime::*`,
//! `super::host::*`, etc. paths keep working without further change.

use super::stream_config::{build_stream_config, resolve_chain_runtime_sample_rate};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use super::stream_config::{
    max_supported_channels, required_channel_count, select_supported_stream_config,
};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use super::validation::validate_buffer_size;
use super::{AudioDeviceDescriptor, ProjectRuntimeController};
use cpal::BufferSize;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::{SampleFormat, SupportedBufferSize, SupportedStreamConfigRange};

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn supported_range(
    channels: u16,
    min_sample_rate: u32,
    max_sample_rate: u32,
) -> SupportedStreamConfigRange {
    SupportedStreamConfigRange::new(
        channels,
        min_sample_rate,
        max_sample_rate,
        SupportedBufferSize::Range { min: 64, max: 1024 },
        SampleFormat::F32,
    )
}

// ── AudioDeviceDescriptor ───────────────────────────────────────

#[test]
fn audio_device_descriptor_construction_stores_fields() {
    let desc = AudioDeviceDescriptor {
        id: "coreaudio:abc123".to_string(),
        name: "USB Audio Interface".to_string(),
        channels: 2,
    };
    assert_eq!(desc.id, "coreaudio:abc123");
    assert_eq!(desc.name, "USB Audio Interface");
    assert_eq!(desc.channels, 2);
}

#[test]
fn audio_device_descriptor_equality_same_values_returns_true() {
    let a = AudioDeviceDescriptor {
        id: "dev1".to_string(),
        name: "Device".to_string(),
        channels: 4,
    };
    let b = AudioDeviceDescriptor {
        id: "dev1".to_string(),
        name: "Device".to_string(),
        channels: 4,
    };
    assert_eq!(a, b);
}

#[test]
fn audio_device_descriptor_equality_different_id_returns_false() {
    let a = AudioDeviceDescriptor {
        id: "dev1".to_string(),
        name: "Device".to_string(),
        channels: 4,
    };
    let b = AudioDeviceDescriptor {
        id: "dev2".to_string(),
        name: "Device".to_string(),
        channels: 4,
    };
    assert_ne!(a, b);
}

#[test]
fn audio_device_descriptor_clone_produces_equal_copy() {
    let original = AudioDeviceDescriptor {
        id: "dev1".to_string(),
        name: "My Device".to_string(),
        channels: 8,
    };
    let cloned = original.clone();
    assert_eq!(original, cloned);
}

#[test]
fn audio_device_descriptor_debug_format_contains_fields() {
    let desc = AudioDeviceDescriptor {
        id: "dev1".to_string(),
        name: "Test".to_string(),
        channels: 2,
    };
    let debug = format!("{:?}", desc);
    assert!(debug.contains("dev1"));
    assert!(debug.contains("Test"));
}

// ── select_supported_stream_config ──────────────────────────────

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn select_supported_stream_config_accepts_non_default_sample_rate_when_device_supports_it() {
    let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
    let supported = vec![
        supported_range(2, 44_100, 96_000),
        supported_range(1, 44_100, 96_000),
    ];

    let resolved =
        select_supported_stream_config(&default_config, &supported, Some(44_100), 2, "test-device")
            .expect("supported non-default sample rate should resolve");

    assert_eq!(resolved.sample_rate(), 44_100);
    assert_eq!(resolved.channels(), 2);
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn select_supported_stream_config_no_requested_rate_uses_default() {
    let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
    let supported = vec![supported_range(2, 44_100, 96_000)];

    let resolved =
        select_supported_stream_config(&default_config, &supported, None, 2, "test-device")
            .expect("should use default sample rate");

    assert_eq!(resolved.sample_rate(), 48_000);
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn select_supported_stream_config_unsupported_rate_returns_error() {
    let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
    let supported = vec![supported_range(2, 44_100, 44_100)];

    let result =
        select_supported_stream_config(&default_config, &supported, Some(96_000), 2, "test-device");

    assert!(result.is_err());
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn select_supported_stream_config_insufficient_channels_returns_error() {
    let default_config = supported_range(1, 48_000, 48_000).with_max_sample_rate();
    let supported = vec![supported_range(1, 44_100, 96_000)];

    let result =
        select_supported_stream_config(&default_config, &supported, Some(48_000), 4, "test-device");

    assert!(result.is_err());
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn select_supported_stream_config_picks_minimum_channels_matching() {
    let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
    let supported = vec![
        supported_range(8, 44_100, 96_000),
        supported_range(2, 44_100, 96_000),
    ];

    let resolved =
        select_supported_stream_config(&default_config, &supported, Some(48_000), 2, "test-device")
            .unwrap();

    assert_eq!(resolved.channels(), 2);
}

// ── resolve_chain_runtime_sample_rate ────────────────────────────

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn resolve_chain_runtime_sample_rate_rejects_mismatched_input_and_output_sample_rates() {
    let input = supported_range(2, 48_000, 48_000).with_max_sample_rate();
    let output = supported_range(2, 44_100, 44_100).with_max_sample_rate();

    let error = resolve_chain_runtime_sample_rate("chain:0", &input, &output)
        .expect_err("mismatched rates should fail");

    assert!(error.to_string().contains("sample_rate"));
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn resolve_chain_runtime_sample_rate_matching_rates_returns_rate() {
    let input = supported_range(2, 48_000, 48_000).with_max_sample_rate();
    let output = supported_range(2, 48_000, 48_000).with_max_sample_rate();

    let rate = resolve_chain_runtime_sample_rate("chain:0", &input, &output).unwrap();

    assert_eq!(rate, 48_000.0);
}

// ── max_supported_channels ──────────────────────────────────────

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn max_supported_channels_prefers_supported_capacity_over_default() {
    let resolved =
        max_supported_channels(Some(2), Some(8)).expect("supported channels should resolve");

    assert_eq!(resolved, 8);
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn max_supported_channels_uses_default_when_supported_list_is_empty() {
    let resolved = max_supported_channels(Some(2), None).expect("default channels should resolve");

    assert_eq!(resolved, 2);
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn max_supported_channels_both_none_returns_error() {
    let result = max_supported_channels(None, None);
    assert!(result.is_err());
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn max_supported_channels_only_supported_uses_supported() {
    let resolved = max_supported_channels(None, Some(6)).expect("should use supported channels");
    assert_eq!(resolved, 6);
}

// ── required_channel_count ──────────────────────────────────────

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn required_channel_count_empty_returns_zero() {
    assert_eq!(required_channel_count(&[]), 0);
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn required_channel_count_single_channel_zero_returns_one() {
    assert_eq!(required_channel_count(&[0]), 1);
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn required_channel_count_stereo_returns_two() {
    assert_eq!(required_channel_count(&[0, 1]), 2);
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn required_channel_count_non_contiguous_returns_max_plus_one() {
    assert_eq!(required_channel_count(&[0, 3, 7]), 8);
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn required_channel_count_single_high_channel_returns_correct() {
    assert_eq!(required_channel_count(&[5]), 6);
}

// ── validate_buffer_size ────────────────────────────────────────

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn validate_buffer_size_within_range_succeeds() {
    let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
    let result = validate_buffer_size(256, &supported, "test");
    assert!(result.is_ok());
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn validate_buffer_size_at_min_boundary_succeeds() {
    let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
    let result = validate_buffer_size(64, &supported, "test");
    assert!(result.is_ok());
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn validate_buffer_size_at_max_boundary_succeeds() {
    let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
    let result = validate_buffer_size(1024, &supported, "test");
    assert!(result.is_ok());
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn validate_buffer_size_below_min_returns_error() {
    let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
    let result = validate_buffer_size(32, &supported, "test");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("outside supported range"));
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn validate_buffer_size_above_max_returns_error() {
    let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
    let result = validate_buffer_size(2048, &supported, "test");
    assert!(result.is_err());
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn validate_buffer_size_unknown_always_succeeds() {
    let supported = SupportedBufferSize::Unknown;
    let result = validate_buffer_size(9999, &supported, "test");
    assert!(result.is_ok());
}

// ── build_stream_config ─────────────────────────────────────────

#[test]
fn build_stream_config_sets_channels_and_rate() {
    let config = build_stream_config(2, 48_000, 256);
    assert_eq!(config.channels, 2);
    assert_eq!(config.sample_rate, 48_000);
    assert_eq!(config.buffer_size, BufferSize::Fixed(256));
}

#[test]
fn build_stream_config_mono_128_buffer() {
    let config = build_stream_config(1, 44_100, 128);
    assert_eq!(config.channels, 1);
    assert_eq!(config.sample_rate, 44_100);
    assert_eq!(config.buffer_size, BufferSize::Fixed(128));
}

// ── build_stream_config edge cases ──────────────────────────────────────

#[test]
fn build_stream_config_high_sample_rate() {
    let config = build_stream_config(2, 96_000, 512);
    assert_eq!(config.channels, 2);
    assert_eq!(config.sample_rate, 96_000);
    assert_eq!(config.buffer_size, BufferSize::Fixed(512));
}

#[test]
fn build_stream_config_large_buffer() {
    let config = build_stream_config(8, 48_000, 1024);
    assert_eq!(config.channels, 8);
    assert_eq!(config.buffer_size, BufferSize::Fixed(1024));
}

// ── validate_buffer_size edge cases ─────────────────────────────────────

#[test]
fn validate_buffer_size_exactly_one_element_range_succeeds() {
    let supported = SupportedBufferSize::Range { min: 256, max: 256 };
    let result = validate_buffer_size(256, &supported, "test");
    assert!(result.is_ok());
}

#[test]
fn validate_buffer_size_exactly_one_element_range_rejects_other() {
    let supported = SupportedBufferSize::Range { min: 256, max: 256 };
    let result = validate_buffer_size(128, &supported, "test");
    assert!(result.is_err());
}

// ── required_channel_count more edge cases ──────────────────────────────

#[test]
fn required_channel_count_duplicate_channels() {
    // Duplicate channels should still return max+1
    assert_eq!(required_channel_count(&[0, 0, 0]), 1);
}

#[test]
fn required_channel_count_unsorted_channels() {
    assert_eq!(required_channel_count(&[3, 1, 5, 2]), 6);
}

// ── max_supported_channels additional tests ─────────────────────────────

#[test]
fn max_supported_channels_same_default_and_supported() {
    let resolved = max_supported_channels(Some(4), Some(4)).unwrap();
    assert_eq!(resolved, 4);
}

#[test]
fn max_supported_channels_zero_default_with_some_supported() {
    let resolved = max_supported_channels(Some(0), Some(2)).unwrap();
    assert_eq!(resolved, 2);
}

// ── select_supported_stream_config additional tests ─────────────────────

#[test]
fn select_supported_stream_config_empty_ranges_returns_error() {
    let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
    let supported: Vec<SupportedStreamConfigRange> = vec![];

    let result =
        select_supported_stream_config(&default_config, &supported, Some(48_000), 2, "test-device");

    assert!(result.is_err(), "empty ranges should return error");
}

#[test]
fn select_supported_stream_config_zero_channels_required() {
    let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
    let supported = vec![supported_range(2, 44_100, 96_000)];

    let resolved =
        select_supported_stream_config(&default_config, &supported, Some(48_000), 0, "test-device")
            .expect("zero required channels should match any range");

    assert!(resolved.channels() >= 1);
}

#[test]
fn select_supported_stream_config_prefers_exact_channel_match() {
    let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
    let supported = vec![
        supported_range(4, 44_100, 96_000),
        supported_range(2, 44_100, 96_000),
        supported_range(8, 44_100, 96_000),
    ];

    let resolved =
        select_supported_stream_config(&default_config, &supported, Some(48_000), 2, "test-device")
            .unwrap();

    assert_eq!(resolved.channels(), 2, "should prefer exact channel count");
}

// ── resolve_chain_runtime_sample_rate tests ─────────────────────────────

#[test]
fn resolve_chain_runtime_sample_rate_high_rate_matching() {
    let input = supported_range(2, 96_000, 96_000).with_max_sample_rate();
    let output = supported_range(2, 96_000, 96_000).with_max_sample_rate();
    let rate = resolve_chain_runtime_sample_rate("chain:0", &input, &output).unwrap();
    assert_eq!(rate, 96_000.0);
}

#[test]
fn resolve_chain_runtime_sample_rate_low_rate_matching() {
    let input = supported_range(2, 44_100, 44_100).with_max_sample_rate();
    let output = supported_range(2, 44_100, 44_100).with_max_sample_rate();
    let rate = resolve_chain_runtime_sample_rate("chain:0", &input, &output).unwrap();
    assert_eq!(rate, 44_100.0);
}

// ── is_asio_host (non-Windows always returns false) ─────────────────────

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn is_asio_host_returns_false_on_non_windows() {
    use super::host::is_asio_host;
    let host = cpal::default_host();
    assert!(!is_asio_host(&host), "non-Windows host should not be ASIO");
}

// ── insert_return_as_input_entry ────────────────────────────────────────

#[test]
fn insert_return_as_input_entry_copies_return_fields() {
    use super::chain_resolve::insert_return_as_input_entry;
    use domain::ids::DeviceId;
    use project::block::{InsertBlock, InsertEndpoint};
    use project::chain::ChainInputMode;

    let insert = InsertBlock {
        model: "external_loop".into(),
        send: InsertEndpoint {
            device_id: DeviceId("send".into()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        },
        return_: InsertEndpoint {
            device_id: DeviceId("return".into()),
            mode: ChainInputMode::Stereo,
            channels: vec![2, 3],
        },
    };
    let entry = insert_return_as_input_entry(&insert);
    assert_eq!(entry.device_id.0, "return");
    assert_eq!(entry.channels, vec![2, 3]);
}

// ── insert_send_as_output_entry ─────────────────────────────────────────

#[test]
fn insert_send_as_output_entry_mono_becomes_mono() {
    use super::chain_resolve::insert_send_as_output_entry;
    use domain::ids::DeviceId;
    use project::block::{InsertBlock, InsertEndpoint};
    use project::chain::{ChainInputMode, ChainOutputMode};

    let insert = InsertBlock {
        model: "external_loop".into(),
        send: InsertEndpoint {
            device_id: DeviceId("send".into()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        },
        return_: InsertEndpoint {
            device_id: DeviceId("return".into()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        },
    };
    let entry = insert_send_as_output_entry(&insert);
    assert_eq!(entry.device_id.0, "send");
    assert!(matches!(entry.mode, ChainOutputMode::Mono));
}

#[test]
fn insert_send_as_output_entry_stereo_becomes_stereo() {
    use super::chain_resolve::insert_send_as_output_entry;
    use domain::ids::DeviceId;
    use project::block::{InsertBlock, InsertEndpoint};
    use project::chain::{ChainInputMode, ChainOutputMode};

    let insert = InsertBlock {
        model: "external_loop".into(),
        send: InsertEndpoint {
            device_id: DeviceId("send".into()),
            mode: ChainInputMode::Stereo,
            channels: vec![0, 1],
        },
        return_: InsertEndpoint {
            device_id: DeviceId("return".into()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        },
    };
    let entry = insert_send_as_output_entry(&insert);
    assert!(matches!(entry.mode, ChainOutputMode::Stereo));
}
