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
mod controller_taps;
mod device_enum;
#[cfg(all(target_os = "linux", feature = "jack"))]
pub use device_enum::jack_is_running;
pub use device_enum::{
    has_new_devices, invalidate_device_cache, list_devices, list_input_device_descriptors,
    list_output_device_descriptors,
};

mod device_settings;
pub use device_settings::apply_device_settings;
#[cfg(all(target_os = "linux", feature = "jack"))]
pub use device_settings::start_jack_in_background;

mod chain_resolve;
pub use chain_resolve::resolve_project_chain_sample_rates;

#[cfg(all(target_os = "linux", feature = "jack"))]
mod jack_chain_resolve;

mod validation;

mod stream_builder;
mod stream_config;
pub use stream_builder::build_streams_for_project;

// Cross-module helpers — these used to live in lib.rs and are referenced
// by sibling modules (chain_resolve, controller, validation, device_enum,
// device_settings, elastic) via `crate::<name>`. Re-export them at the
// crate root so existing call sites keep resolving without an import
// flip-day across every file.
#[cfg(all(target_os = "linux", feature = "jack"))]
pub(crate) use jack_chain_resolve::jack_resolve_chain_config;
#[cfg(all(target_os = "linux", feature = "jack"))]
pub(crate) use stream_builder::build_active_chain_runtime;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) use stream_builder::{build_active_chain_runtime, build_chain_stream_signature_multi};
pub(crate) use stream_config::resolved_output_buffer_size_frames;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) use stream_config::{
    build_stream_config, max_supported_input_channels, max_supported_output_channels,
    required_channel_count, resolve_multi_io_sample_rate, select_supported_stream_config,
};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) use validation::{
    find_input_device_by_id, find_output_device_by_id, validate_buffer_size,
};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_regression;
#[cfg(test)]
mod tests_signatures;
