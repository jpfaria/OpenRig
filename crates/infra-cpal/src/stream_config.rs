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
use anyhow::bail;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::{anyhow, Result};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::SupportedStreamConfigRange;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::{BufferSize, StreamConfig, SupportedStreamConfig};

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::resolved::ResolvedInputDevice;
use crate::resolved::ResolvedOutputDevice;

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

#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]
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

/// Pure unification of the resolved per-device rates: every input and every
/// output of one chain must agree (one engine clock). Returns the agreed rate
/// or a precise error naming whether the disagreement is input↔input or
/// input↔output. Pure — no hardware — so it is directly unit-testable.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn unify_io_sample_rates(
    chain_id: &str,
    input_rates: &[u32],
    output_rates: &[u32],
) -> Result<f32> {
    let mut rate: Option<u32> = None;
    for &sr in input_rates {
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
    for &sr in output_rates {
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

/// Resolve the sample rate **per binding-group** instead of once for the whole
/// chain (#736). Each `(input_rates, output_rates)` tuple is one I/O binding's
/// device rates. Within a binding, every input and output must agree (one
/// isolated stream needs no internal resample) — reuses `unify_io_sample_rates`,
/// so the within-binding error wording is unchanged ("across inputs" / "across
/// I/O"). Across bindings, rates may DIFFER freely — that is the whole point of
/// invariant #4 (stream isolation): two isolated streams share no clock.
///
/// Returns the FIRST binding's rate as the chain's representative scalar (used
/// for legacy single-rate consumers: stream-signature back-compat, DI-loop
/// resample target). The authoritative per-device rates flow separately through
/// `ResolvedChainAudioConfig::by_device`. With a single binding this equals the
/// legacy whole-chain `unify_io_sample_rates` result — single-binding chains are
/// bit-identical.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_binding_sample_rates(
    chain_id: &str,
    bindings: &[(Vec<u32>, Vec<u32>)],
) -> Result<f32> {
    let mut representative: Option<f32> = None;
    for (input_rates, output_rates) in bindings {
        let rate = unify_io_sample_rates(chain_id, input_rates, output_rates)?;
        if representative.is_none() {
            representative = Some(rate);
        }
    }
    representative.ok_or_else(|| anyhow!("chain '{}' has no inputs or outputs", chain_id))
}

#[cfg(all(test, not(all(target_os = "linux", feature = "jack"))))]
mod unify_rate_tests {
    use super::unify_io_sample_rates;

    #[test]
    fn agreeing_rates_resolve_to_that_rate() {
        assert_eq!(
            unify_io_sample_rates("c", &[44_100, 44_100], &[44_100]).unwrap(),
            44_100.0
        );
    }

    #[test]
    fn no_io_is_an_error() {
        assert!(unify_io_sample_rates("c", &[], &[]).is_err());
    }

    #[test]
    fn mismatched_inputs_error_names_inputs() {
        let e = unify_io_sample_rates("c", &[48_000, 44_100], &[48_000])
            .unwrap_err()
            .to_string();
        assert!(e.contains("across inputs"), "got: {e}");
    }

    #[test]
    fn mismatched_input_vs_output_error_names_io() {
        // Inputs agree (44.1k) but the output is 48k — the exact #669/#698
        // crackle shape (engine clock disagreeing with the device). Must be a
        // loud error, never a silent resample.
        let e = unify_io_sample_rates("c", &[44_100], &[48_000])
            .unwrap_err()
            .to_string();
        assert!(e.contains("across I/O"), "got: {e}");
    }

    #[test]
    fn single_output_only_resolves() {
        assert_eq!(
            unify_io_sample_rates("c", &[], &[48_000]).unwrap(),
            48_000.0
        );
    }

    use super::resolve_binding_sample_rates;

    #[test]
    fn two_bindings_at_different_rates_resolve_without_error() {
        // Scarlett binding @44.1k, TEYUN binding @48k — the #736 case.
        // Cross-binding difference is allowed; representative = first binding.
        let rate = resolve_binding_sample_rates(
            "c",
            &[(vec![44_100], vec![44_100]), (vec![48_000], vec![48_000])],
        )
        .expect("cross-binding rate difference must be allowed");
        assert_eq!(rate, 44_100.0);
    }

    #[test]
    fn within_binding_input_output_mismatch_still_errors() {
        // 44.1k input + 48k output INSIDE one binding — one isolated stream
        // cannot internally resample, so this stays a loud error (#669 shape).
        let e = resolve_binding_sample_rates("c", &[(vec![44_100], vec![48_000])])
            .unwrap_err()
            .to_string();
        assert!(e.contains("across I/O"), "got: {e}");
    }

    #[test]
    fn single_binding_matches_legacy_unify() {
        // One binding with all the chain's I/O → identical to whole-chain unify.
        let rate =
            resolve_binding_sample_rates("c", &[(vec![44_100, 44_100], vec![44_100])]).unwrap();
        assert_eq!(rate, 44_100.0);
    }

    #[test]
    fn no_bindings_is_an_error() {
        assert!(resolve_binding_sample_rates("c", &[]).is_err());
    }
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn max_supported_input_channels(device: &cpal::Device) -> Result<usize> {
    // #762: cached CoreAudio query — repeated live syncs hit the same devices.
    let cfg = crate::device_config_cache::configs_for(device, true)?;
    let max_supported = cfg.supported.iter().map(|c| c.channels() as usize).max();
    let default_channels = cfg.default.as_ref().map(|c| c.channels() as usize);
    max_supported_channels(default_channels, max_supported)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn max_supported_output_channels(device: &cpal::Device) -> Result<usize> {
    // #762: cached CoreAudio query — repeated live syncs hit the same devices.
    let cfg = crate::device_config_cache::configs_for(device, false)?;
    let max_supported = cfg.supported.iter().map(|c| c.channels() as usize).max();
    let default_channels = cfg.default.as_ref().map(|c| c.channels() as usize);
    max_supported_channels(default_channels, max_supported)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn max_supported_channels(
    default_channels: Option<usize>,
    max_supported_channels: Option<usize>,
) -> Result<usize> {
    max_supported_channels
        .or(default_channels)
        .ok_or_else(|| anyhow!("device exposes no supported channels"))
}
