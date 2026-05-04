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
//!   `resolve_multi_io_sample_rate`, `max_supported_input/output_channels`,
//!   `max_supported_channels` — selectors that pick a config from the
//!   ranges cpal returns.
//!
//! `resolve_chain_runtime_sample_rate` lives behind `#[cfg(test)]` —
//! older test cases used to compare a per-input vs per-output rate; the
//! production path went through `resolve_multi_io_sample_rate` long
//! before this split.
//!
//! Public surface: nothing. All `pub(crate)`.

#[cfg(any(not(all(target_os = "linux", feature = "jack")), test))]
use anyhow::bail;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::{anyhow, Result};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::SupportedStreamConfigRange;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::{BufferSize, StreamConfig, SupportedStreamConfig};

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::traits::DeviceTrait;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::resolved::{ResolvedInputDevice, ResolvedOutputDevice};

#[cfg(not(all(target_os = "linux", feature = "jack")))]
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
pub(crate) fn resolved_input_sample_rate(resolved: &ResolvedInputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.sample_rate)
        .unwrap_or_else(|| resolved.supported.sample_rate())
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolved_output_sample_rate(resolved: &ResolvedOutputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.sample_rate)
        .unwrap_or_else(|| resolved.supported.sample_rate())
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolved_input_buffer_size_frames(resolved: &ResolvedInputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.buffer_size_frames)
        .unwrap_or(256)
}

pub(crate) fn resolved_output_buffer_size_frames(resolved: &ResolvedOutputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.buffer_size_frames)
        .unwrap_or(256)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn required_channel_count(channels: &[usize]) -> usize {
    channels
        .iter()
        .copied()
        .max()
        .map(|channel| channel + 1)
        .unwrap_or(0)
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

    let best = supported_ranges
        .iter()
        .filter(|range| range.channels() as usize >= required_channels)
        .filter_map(|range| range.try_with_sample_rate(target_sample_rate))
        .min_by_key(|config| {
            (
                (config.channels() as usize != required_channels) as u8,
                (config.sample_format() != default_format) as u8,
                (config.channels() as usize).saturating_sub(required_channels),
            )
        });

    best.ok_or_else(|| {
        anyhow!(
            "{} invalid: no supported config for sample_rate={} with at least {} channels",
            context,
            target_sample_rate,
            required_channels
        )
    })
}

#[cfg(test)]
pub(crate) fn resolve_chain_runtime_sample_rate(
    chain_id: &str,
    input: &SupportedStreamConfig,
    output: &SupportedStreamConfig,
) -> Result<f32> {
    if input.sample_rate() != output.sample_rate() {
        bail!(
            "chain '{}' invalid: input sample_rate={} differs from output sample_rate={}",
            chain_id,
            input.sample_rate(),
            output.sample_rate()
        );
    }

    Ok(input.sample_rate() as f32)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_multi_io_sample_rate(
    chain_id: &str,
    inputs: &[ResolvedInputDevice],
    outputs: &[ResolvedOutputDevice],
) -> Result<f32> {
    let mut rate: Option<u32> = None;
    for ri in inputs {
        let sr = resolved_input_sample_rate(ri);
        if let Some(prev) = rate {
            if prev != sr {
                bail!(
                    "chain '{}' invalid: mismatched sample rates across inputs ({} vs {})",
                    chain_id,
                    prev,
                    sr
                );
            }
        }
        rate = Some(sr);
    }
    for ro in outputs {
        let sr = resolved_output_sample_rate(ro);
        if let Some(prev) = rate {
            if prev != sr {
                bail!(
                    "chain '{}' invalid: mismatched sample rates across I/O ({} vs {})",
                    chain_id,
                    prev,
                    sr
                );
            }
        }
        rate = Some(sr);
    }
    rate.map(|r| r as f32)
        .ok_or_else(|| anyhow!("chain '{}' has no inputs or outputs", chain_id))
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn max_supported_input_channels(device: &cpal::Device) -> Result<usize> {
    let max_supported = match device.supported_input_configs() {
        Ok(configs) => {
            let max = configs.map(|config| config.channels() as usize).max();
            log::info!(
                "[max_supported_input_channels] supported_input_configs max={:?}",
                max
            );
            max
        }
        Err(e) => {
            log::warn!(
                "[max_supported_input_channels] supported_input_configs error: {}",
                e
            );
            return Err(e.into());
        }
    };
    let default_channels = match device.default_input_config() {
        Ok(config) => {
            let ch = config.channels() as usize;
            log::info!(
                "[max_supported_input_channels] default_input_config channels={}",
                ch
            );
            Some(ch)
        }
        Err(e) => {
            log::info!(
                "[max_supported_input_channels] default_input_config error: {}",
                e
            );
            None
        }
    };
    max_supported_channels(default_channels, max_supported)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn max_supported_output_channels(device: &cpal::Device) -> Result<usize> {
    let max_supported = match device.supported_output_configs() {
        Ok(configs) => {
            let max = configs.map(|config| config.channels() as usize).max();
            log::info!(
                "[max_supported_output_channels] supported_output_configs max={:?}",
                max
            );
            max
        }
        Err(e) => {
            log::warn!(
                "[max_supported_output_channels] supported_output_configs error: {}",
                e
            );
            return Err(e.into());
        }
    };
    let default_channels = match device.default_output_config() {
        Ok(config) => {
            let ch = config.channels() as usize;
            log::info!(
                "[max_supported_output_channels] default_output_config channels={}",
                ch
            );
            Some(ch)
        }
        Err(e) => {
            log::info!(
                "[max_supported_output_channels] default_output_config error: {}",
                e
            );
            None
        }
    };
    max_supported_channels(default_channels, max_supported)
}

#[cfg(any(not(all(target_os = "linux", feature = "jack")), test))]
pub(crate) fn max_supported_channels(
    default_channels: Option<usize>,
    max_supported_channels: Option<usize>,
) -> Result<usize> {
    max_supported_channels
        .or(default_channels)
        .ok_or_else(|| anyhow!("device exposes no supported channels"))
}
