// Single owner of the jackd lifecycle on Linux (issue #308). The supervisor
// types compile on any platform with the jack feature so unit tests can
// exercise the state machine via MockBackend in the macOS/Windows dev loop.
// On those platforms the module has no live consumer (LiveJackBackend and the
// RuntimeController supervisor field are linux+jack-only), hence the targeted
// allow below; Linux production builds keep the strict lint.
#[cfg(feature = "jack")]
#[cfg_attr(
    not(all(target_os = "linux", feature = "jack")),
    allow(dead_code, unused_imports)
)]
mod jack_supervisor;

mod host;

#[cfg(all(target_os = "linux", feature = "jack"))]
mod usb_proc;

// is_jack_host() removed — CPAL JACK host is never created.
// Use using_jack_direct() to check if the direct JACK backend is active.

mod elastic;

#[cfg(all(target_os = "linux", feature = "jack"))]
mod cpu_affinity;

#[cfg(all(target_os = "linux", feature = "jack"))]
mod jack_handlers;

mod active_runtime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceDescriptor {
    pub id: String,
    pub name: String,
    pub channels: usize,
}

mod resolved;

#[cfg(all(target_os = "linux", feature = "jack"))]
mod jack_direct;

mod controller;
pub use controller::ProjectRuntimeController;
mod device_enum;
pub use device_enum::{
    has_new_devices, invalidate_device_cache, list_devices, list_input_device_descriptors,
    list_output_device_descriptors,
};
#[cfg(all(target_os = "linux", feature = "jack"))]
pub use device_enum::jack_is_running;

mod device_settings;
pub use device_settings::apply_device_settings;
#[cfg(all(target_os = "linux", feature = "jack"))]
pub use device_settings::start_jack_in_background;

mod chain_resolve;
pub use chain_resolve::resolve_project_chain_sample_rates;

mod validation;

mod stream_config;
mod stream_builder;
pub use stream_builder::build_streams_for_project;

// Cross-module helpers — these used to live in lib.rs and are referenced
// by sibling modules (chain_resolve, controller, validation, device_enum,
// device_settings, elastic) via `crate::<name>`. Re-export them at the
// crate root so existing call sites keep resolving without an import
// flip-day across every file.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) use stream_builder::{build_active_chain_runtime, build_chain_stream_signature_multi};
#[cfg(all(target_os = "linux", feature = "jack"))]
pub(crate) use stream_builder::{build_active_chain_runtime, jack_resolve_chain_config};
pub(crate) use stream_config::{
    build_stream_config, resolved_output_buffer_size_frames,
};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) use stream_config::{
    max_supported_input_channels, max_supported_output_channels, required_channel_count,
    resolve_multi_io_sample_rate, select_supported_stream_config,
};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) use validation::{
    find_input_device_by_id, find_output_device_by_id, validate_buffer_size,
};

#[cfg(test)]
mod tests {
    use super::stream_config::{build_stream_config, resolve_chain_runtime_sample_rate};
    use super::{AudioDeviceDescriptor, ProjectRuntimeController};
    use cpal::BufferSize;
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    use super::stream_config::{
        max_supported_channels, required_channel_count, select_supported_stream_config,
    };
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    use super::validation::validate_buffer_size;
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

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            Some(44_100),
            2,
            "test-device",
        )
        .expect("supported non-default sample rate should resolve");

        assert_eq!(resolved.sample_rate(), 44_100);
        assert_eq!(resolved.channels(), 2);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn select_supported_stream_config_no_requested_rate_uses_default() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![supported_range(2, 44_100, 96_000)];

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            None,
            2,
            "test-device",
        )
        .expect("should use default sample rate");

        assert_eq!(resolved.sample_rate(), 48_000);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn select_supported_stream_config_unsupported_rate_returns_error() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![supported_range(2, 44_100, 44_100)];

        let result = select_supported_stream_config(
            &default_config,
            &supported,
            Some(96_000),
            2,
            "test-device",
        );

        assert!(result.is_err());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn select_supported_stream_config_insufficient_channels_returns_error() {
        let default_config = supported_range(1, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![supported_range(1, 44_100, 96_000)];

        let result = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            4,
            "test-device",
        );

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

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            2,
            "test-device",
        )
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
        let resolved =
            max_supported_channels(Some(2), None).expect("default channels should resolve");

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
        let resolved =
            max_supported_channels(None, Some(6)).expect("should use supported channels");
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
        assert!(result.unwrap_err().to_string().contains("outside supported range"));
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

        let result = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            2,
            "test-device",
        );

        assert!(result.is_err(), "empty ranges should return error");
    }

    #[test]
    fn select_supported_stream_config_zero_channels_required() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![supported_range(2, 44_100, 96_000)];

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            0,
            "test-device",
        )
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

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            2,
            "test-device",
        )
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
        use project::block::{InsertBlock, InsertEndpoint};
        use project::chain::ChainInputMode;
        use domain::ids::DeviceId;

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
        use project::block::{InsertBlock, InsertEndpoint};
        use project::chain::{ChainInputMode, ChainOutputMode};
        use domain::ids::DeviceId;

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
        use project::block::{InsertBlock, InsertEndpoint};
        use project::chain::{ChainInputMode, ChainOutputMode};
        use domain::ids::DeviceId;

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

    #[test]
    fn is_healthy_returns_true_when_no_chains_active() {
        let mut controller = ProjectRuntimeController {
            runtime_graph: engine::runtime::RuntimeGraph {
                chains: std::collections::HashMap::new(),
            },
            active_chains: std::collections::HashMap::new(),
            #[cfg(all(target_os = "linux", feature = "jack"))]
            supervisor: super::jack_supervisor::JackSupervisor::new(
                super::jack_supervisor::LiveJackBackend::new(),
            ),
        };
        assert!(controller.is_healthy());
    }

    #[test]
    fn is_running_returns_false_when_no_chains() {
        let controller = ProjectRuntimeController {
            runtime_graph: engine::runtime::RuntimeGraph {
                chains: std::collections::HashMap::new(),
            },
            active_chains: std::collections::HashMap::new(),
            #[cfg(all(target_os = "linux", feature = "jack"))]
            supervisor: super::jack_supervisor::JackSupervisor::new(
                super::jack_supervisor::LiveJackBackend::new(),
            ),
        };
        assert!(!controller.is_running());
    }

    // ── Regression tests for issue #294: stale JACK client on chain reconfigure ──
    //
    // Reconfiguring input channels on an active chain (e.g. unchecking a channel
    // in a stereo input) used to leave the previous JACK client alive while the
    // replacement client was being built, because HashMap::insert only dropped
    // the old ActiveChainRuntime AFTER constructing the new one. On JACK, the
    // new client would get a suffixed name while connect_ports_by_name still
    // used the literal (unsuffixed) name — so the connections bound to the
    // OLD client's ports, which then vanished when the old client was finally
    // dropped, leaving the new client orphaned and audio silent.
    //
    // The fix tears down the existing ActiveChainRuntime BEFORE building the
    // replacement (teardown_active_chain_for_rebuild), mirroring the pattern
    // in remove_chain. These tests cover the teardown helper directly; the
    // end-to-end "audio still flows after channel toggle" behavior is
    // verifiable only on real JACK hardware and is exercised manually on the
    // Orange Pi during regression testing.

    #[test]
    fn teardown_active_chain_for_rebuild_drops_entry_when_present() {
        let chain_id = domain::ids::ChainId("chain:0".into());
        let mut controller = ProjectRuntimeController {
            runtime_graph: engine::runtime::RuntimeGraph {
                chains: std::collections::HashMap::new(),
            },
            active_chains: std::collections::HashMap::new(),
            #[cfg(all(target_os = "linux", feature = "jack"))]
            supervisor: super::jack_supervisor::JackSupervisor::new(
                super::jack_supervisor::LiveJackBackend::new(),
            ),
        };
        controller.active_chains.insert(chain_id.clone(), super::active_runtime::ActiveChainRuntime {
            stream_signature: super::resolved::ChainStreamSignature { inputs: vec![], outputs: vec![] },
            _input_streams: vec![],
            _output_streams: vec![],
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _jack_client: None,
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _dsp_worker: None,
        });
        assert!(controller.active_chains.contains_key(&chain_id));

        controller.teardown_active_chain_for_rebuild(&chain_id);

        assert!(!controller.active_chains.contains_key(&chain_id),
            "active_chains entry must be removed so the old JACK client/DSP worker are dropped \
             before a replacement is built");
    }

    #[test]
    fn teardown_active_chain_for_rebuild_is_noop_when_chain_absent() {
        let chain_id = domain::ids::ChainId("chain:missing".into());
        let mut controller = ProjectRuntimeController {
            runtime_graph: engine::runtime::RuntimeGraph {
                chains: std::collections::HashMap::new(),
            },
            active_chains: std::collections::HashMap::new(),
            #[cfg(all(target_os = "linux", feature = "jack"))]
            supervisor: super::jack_supervisor::JackSupervisor::new(
                super::jack_supervisor::LiveJackBackend::new(),
            ),
        };

        controller.teardown_active_chain_for_rebuild(&chain_id);

        assert!(controller.active_chains.is_empty());
    }

    // ── Regression #316: teardown clears the draining flag for rebuild ──
    //
    // The JACK fix from #294 (this same `teardown_active_chain_for_rebuild`)
    // calls `set_draining(true)` on the live `Arc<ChainRuntimeState>` so the
    // audio callback bails out while the old CPAL/JACK streams are dropped.
    // The Arc stays alive in `runtime_graph` because the caller is about to
    // re-upsert it, and `RuntimeGraph::upsert_chain` reuses an existing
    // entry instead of rebuilding the state. Without a matching reset the
    // new streams' callbacks observe `is_draining()==true` from the very
    // first invocation and silence every segment on the chain — including
    // sibling InputEntries that were not touched by the channel edit. The
    // user-visible symptom is "remove a channel from one entry → audio of
    // the other entry on the same chain stops too" (issue #316). Toggling
    // the chain off then on works because `remove_chain` drops the Arc, so
    // the next enable rebuilds a fresh `ChainRuntimeState` with the flag
    // already initialized to `false`.
    #[test]
    fn teardown_active_chain_for_rebuild_clears_draining_so_rebuild_can_resume_audio() {
        use std::sync::Arc;
        let chain_id = domain::ids::ChainId("chain:316".into());
        let chain = project::chain::Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![],
        };
        let runtime_arc = Arc::new(
            engine::runtime::build_chain_runtime_state(&chain, 48_000.0, &[1024])
                .expect("empty chain runtime should build"),
        );

        let mut graph = engine::runtime::RuntimeGraph {
            chains: std::collections::HashMap::new(),
        };
        graph.chains.insert(chain_id.clone(), Arc::clone(&runtime_arc));

        let mut active_chains = std::collections::HashMap::new();
        active_chains.insert(
            chain_id.clone(),
            super::active_runtime::ActiveChainRuntime {
                stream_signature: super::resolved::ChainStreamSignature {
                    inputs: vec![],
                    outputs: vec![],
                },
                _input_streams: vec![],
                _output_streams: vec![],
                #[cfg(all(target_os = "linux", feature = "jack"))]
                _jack_client: None,
                #[cfg(all(target_os = "linux", feature = "jack"))]
                _dsp_worker: None,
            },
        );

        let mut controller = ProjectRuntimeController {
            runtime_graph: graph,
            active_chains,
            #[cfg(all(target_os = "linux", feature = "jack"))]
            supervisor: super::jack_supervisor::JackSupervisor::new(
                super::jack_supervisor::LiveJackBackend::new(),
            ),
        };

        assert!(!runtime_arc.is_draining(), "freshly built runtime starts un-drained");

        controller.teardown_active_chain_for_rebuild(&chain_id);

        assert!(
            !runtime_arc.is_draining(),
            "teardown_active_chain_for_rebuild must clear the draining flag — \
             the Arc<ChainRuntimeState> is reused by the rebuild that follows, \
             and leaving the flag set silences every CPAL/JACK callback on the \
             chain (including sibling InputEntries) until the chain is fully \
             removed and re-added (#316)"
        );
    }

    // ── jack_config_for_card reads DeviceSettings (#308) ─────────────────
    //
    // Guarded to Linux+jack because that is the only cfg the function is
    // compiled for. On macOS/Windows these tests are compiled out — same
    // as the function itself.

    #[cfg(all(target_os = "linux", feature = "jack"))]
    fn test_card(device_id: &str) -> super::usb_proc::UsbAudioCard {
        super::usb_proc::UsbAudioCard {
            card_num: "4".into(),
            server_name: "openrig_hw4".into(),
            display_name: "test card".into(),
            device_id: device_id.into(),
            capture_channels: 2,
            playback_channels: 2,
        }
    }

    #[cfg(all(target_os = "linux", feature = "jack"))]
    fn empty_project() -> project::Project {
        project::Project {
            name: None,
            device_settings: Vec::new(),
            chains: Vec::new(),
        }
    }

    #[cfg(all(target_os = "linux", feature = "jack"))]
    #[test]
    fn jack_config_for_card_uses_device_settings_values() {
        use domain::ids::DeviceId;
        use project::device::DeviceSettings;

        let card = test_card("hw:4");
        let mut project = empty_project();
        project.device_settings.push(DeviceSettings {
            device_id: DeviceId("hw:4".into()),
            sample_rate: 48_000,
            buffer_size_frames: 64,
            bit_depth: 32,
            realtime: true,
            rt_priority: 80,
            nperiods: 2,
        });

        let config = ProjectRuntimeController::jack_config_for_card(&card, &project);

        assert!(config.realtime);
        assert_eq!(config.rt_priority, 80);
        assert_eq!(config.nperiods, 2);
        assert_eq!(config.sample_rate, 48_000);
        assert_eq!(config.buffer_size, 64);
    }

    #[cfg(all(target_os = "linux", feature = "jack"))]
    #[test]
    fn jack_config_for_card_falls_back_to_realtime_defaults_when_no_match() {
        let card = test_card("hw:4");
        // No matching device_settings — defaults are realtime + nperiods=3.
        // We ship nperiods=3 (not 2) because nperiods=2 triggered ALSA Broken
        // pipe on Q26 USB audio + RK3588 in hardware validation; the extra
        // period gives the USB driver enough slack without meaningfully
        // increasing latency (one period at 128 frames / 48kHz ≈ 2.7ms).
        let project = empty_project();

        let config = ProjectRuntimeController::jack_config_for_card(&card, &project);

        assert!(config.realtime);
        assert_eq!(config.rt_priority, 70);
        assert_eq!(config.nperiods, 3);
        assert_eq!(config.sample_rate, 48_000);
        assert_eq!(config.buffer_size, 64);
    }
}
