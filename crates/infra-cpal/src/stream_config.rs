//! Responsibility: picks the stream configuration a device is opened with.
//! Helpers for picking the cpal `StreamConfig` and reading values out of
//! `ResolvedInputDevice` / `ResolvedOutputDevice`.
//!
//! Three concerns share this file because each helper is only a handful
//! of lines and they all answer the same question: "what numbers do I
//! pass to `device.build_*_stream` for this resolved device?".
//!
//! - `build_stream_config` — wrap (channels, sample_rate, buffer) in a
//!   `cpal::StreamConfig` with a `Fixed` buffer size.
//! - `resolved_input/output_sample_rate` and
//!   `resolved_input/output_buffer_size_frames` — pull the project's
//!   override out of `Option<DeviceSettings>` and fall back to the
//!   device default if the user hasn't picked one.
//! - `required_channel_count`, `select_supported_stream_config`,
//!   `resolve_binding_sample_rates`, `max_supported_input/output_channels`,
//!   `max_supported_channels` — selectors that pick a config from the
//!   ranges cpal returns.
//!
//! `resolve_chain_runtime_sample_rate` lives behind `#[cfg(test)]` —
//! older test cases used to compare a per-input vs per-output rate; the
//! production path resolves per binding-group via
//! `resolve_binding_sample_rates` (#736), which superseded the earlier
//! whole-chain `resolve_multi_io_sample_rate`.
//!
//! Public surface: nothing. All `pub(crate)`.

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::{anyhow, Result};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::{BufferSize, StreamConfig, SupportedStreamConfig, SupportedStreamConfigRange};

#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]
pub(crate) use crate::stream_channels::max_supported_channels;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
// The stream's rate and channel count moved to `stream_rates.rs` and
// `stream_channels.rs` (#873); the importers keep this path.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) use crate::stream_channels::{
    max_supported_input_channels, max_supported_output_channels, required_channel_count,
};
#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]
pub(crate) use crate::stream_rates::resolve_chain_runtime_sample_rate;
pub(crate) use crate::stream_rates::resolved_output_buffer_size_frames;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) use crate::stream_rates::{
    resolve_binding_sample_rates, resolved_input_buffer_size_frames, resolved_input_sample_rate,
    resolved_output_sample_rate,
};

pub(crate) fn build_stream_config(
    channels: u16,
    sample_rate: u32,
    buffer_size_frames: u32,
) -> StreamConfig {
    StreamConfig {
        channels,
        sample_rate,
        buffer_size: BufferSize::Fixed(buffer_size_frames),
    }
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn select_supported_stream_config(
    default_config: &SupportedStreamConfig,
    supported_ranges: &[SupportedStreamConfigRange],
    requested_sample_rate: Option<u32>,
    required_channels: usize,
    context: &str,
) -> Result<SupportedStreamConfig> {
    let target_sample_rate = requested_sample_rate.unwrap_or_else(|| default_config.sample_rate());
    let default_format = default_config.sample_format();
    let default_channels = default_config.channels() as usize;
    // Issue #516: the user may pick `OutputBlock.mode = Mono` with a single
    // channel and that yields `required_channels = 1`, but opening a
    // hardware-stereo USB interface (Scarlett 2i2 etc.) at 1 channel on
    // macOS / CoreAudio silently routes audio nowhere. Never downsize the
    // device below its default config — channel routing inside the
    // interleaved buffer is `write_output_frame`'s job.
    let effective_required = required_channels.max(default_channels);

    let best = supported_ranges
        .iter()
        .filter(|range| range.channels() as usize >= effective_required)
        .filter_map(|range| range.try_with_sample_rate(target_sample_rate))
        .min_by_key(|config| {
            (
                (config.channels() as usize != effective_required) as u8,
                (config.sample_format() != default_format) as u8,
                (config.channels() as usize).saturating_sub(effective_required),
            )
        });

    best.ok_or_else(|| {
        anyhow!(
            "{} invalid: no supported config for sample_rate={} with at least {} channels \
             (required from output block: {}, device default: {})",
            context,
            target_sample_rate,
            effective_required,
            required_channels,
            default_channels
        )
    })
}
