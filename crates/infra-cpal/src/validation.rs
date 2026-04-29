//! Pre-stream sanity checks against the device the project actually
//! intends to open.
//!
//! Two related concerns live here:
//!
//! 1. `validate_buffer_size` — does the requested buffer fit inside
//!    `cpal::SupportedBufferSize::Range`? Cross-platform (it doesn't
//!    open the device, just inspects the range cpal already returned),
//!    so the helper is never cfg-gated even though all of its
//!    callers are non-JACK.
//! 2. The `validate_*_channels_against_*` family — does each chain's
//!    selected channel index fit inside the device's
//!    `max_supported_*_channels`? On Linux+JACK these are no-ops because
//!    JACK validates port counts at connect time and probing the ALSA
//!    PCM here can disturb USB devices.
//! 3. `find_input/output_device_by_id` — small lookup helpers used by
//!    both validation and `chain_resolve` / `device_settings`.
//!
//! Internal-only crate (`pub(crate)` on every item); no public surface.

use anyhow::Result;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::{anyhow, bail, Context};

use cpal::SupportedBufferSize;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::traits::{DeviceTrait, HostTrait};

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use project::chain::Chain;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use project::project::Project;

pub(crate) fn validate_buffer_size(
    requested: u32,
    supported: &SupportedBufferSize,
    context: &str,
) -> Result<()> {
    match supported {
        SupportedBufferSize::Range { min, max } => {
            if requested < *min || requested > *max {
                anyhow::bail!(
                    "{} invalid: buffer_size_frames={} outside supported range [{}..={}]",
                    context,
                    requested,
                    min,
                    max
                );
            }
        }
        SupportedBufferSize::Unknown => {}
    }
    Ok(())
}
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn validate_channels_against_devices(project: &Project, host: &cpal::Host) -> Result<()> {
    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }
        validate_chain_channels_against_devices(host, chain)?;
    }
    Ok(())
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn validate_chain_channels_against_devices(host: &cpal::Host, chain: &Chain) -> Result<()> {
    for (_, input) in chain.input_blocks() {
        for entry in &input.entries {
            validate_input_channels_against_device(host, &chain.id.0, &entry.device_id.0, &entry.channels)?;
        }
    }

    for (_, output) in chain.output_blocks() {
        for entry in &output.entries {
            validate_output_channels_against_device(host, &chain.id.0, &entry.device_id.0, &entry.channels)?;
        }
    }

    // Validate Insert block endpoints
    for (_, insert) in chain.insert_blocks() {
        if !insert.send.device_id.0.is_empty() {
            validate_output_channels_against_device(host, &chain.id.0, &insert.send.device_id.0, &insert.send.channels)?;
        }
        if !insert.return_.device_id.0.is_empty() {
            validate_input_channels_against_device(host, &chain.id.0, &insert.return_.device_id.0, &insert.return_.channels)?;
        }
    }

    Ok(())
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn validate_input_channels_against_device(
    host: &cpal::Host,
    chain_id: &str,
    device_id: &str,
    channels: &[usize],
) -> Result<()> {
    // On Linux with JACK, skip ALL ALSA channel validation — calling
    // supported_input_configs() can disturb USB audio devices regardless of
    // whether JACK is already running. JACK validates port counts at connect time.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        let _ = (host, chain_id, device_id, channels);
        log::debug!("[validate_input_channels] skipping — Linux/JACK (JACK validates at connect time)");
        return Ok(());
    }
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        log::info!(
            "[validate_input_channels] chain='{}' device='{}' channels={:?} jack_direct=false",
            chain_id, device_id, channels
        );
        let device = find_input_device_by_id(host, device_id)?.ok_or_else(|| {
            anyhow!("chain '{}' missing input device '{}'", chain_id, device_id)
        })?;
        log::info!("[validate_input_channels] device found, querying channel capacity...");
        let total_channels = crate::max_supported_input_channels(&device).with_context(|| {
            format!(
                "failed to resolve input channel capacity for '{}'",
                device_id
            )
        })?;
        log::info!("[validate_input_channels] device '{}' has {} channels", device_id, total_channels);
        for channel in channels {
            if *channel >= total_channels {
                bail!(
                    "chain '{}' invalid: input channel '{}' outside device range (channels={})",
                    chain_id,
                    channel,
                    total_channels
                );
            }
        }
        Ok(())
    }
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn validate_output_channels_against_device(
    host: &cpal::Host,
    chain_id: &str,
    device_id: &str,
    channels: &[usize],
) -> Result<()> {
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        let _ = (host, chain_id, device_id, channels);
        log::debug!("[validate_output_channels] skipping — Linux/JACK (JACK validates at connect time)");
        return Ok(());
    }
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        log::info!(
            "[validate_output_channels] chain='{}' device='{}' channels={:?} jack_direct=false",
            chain_id, device_id, channels
        );
        let device = find_output_device_by_id(host, device_id)?.ok_or_else(|| {
            anyhow!("chain '{}' missing output device '{}'", chain_id, device_id)
        })?;
        log::info!("[validate_output_channels] device found, querying channel capacity...");
        let total_channels = crate::max_supported_output_channels(&device).with_context(|| {
            format!(
                "failed to resolve output channel capacity for '{}'",
                device_id
            )
        })?;
        log::info!("[validate_output_channels] device '{}' has {} channels", device_id, total_channels);
        for channel in channels {
            if *channel >= total_channels {
                bail!(
                    "chain '{}' invalid: output channel '{}' outside device range (channels={})",
                    chain_id,
                    channel,
                    total_channels
                );
            }
        }
        Ok(())
    }
}
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn find_input_device_by_id(host: &cpal::Host, device_id: &str) -> Result<Option<cpal::Device>> {
    for device in host.input_devices()? {
        if device.id()?.to_string() == device_id {
            return Ok(Some(device));
        }
    }
    Ok(None)
}
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn find_output_device_by_id(host: &cpal::Host, device_id: &str) -> Result<Option<cpal::Device>> {
    for device in host.output_devices()? {
        if device.id()?.to_string() == device_id {
            return Ok(Some(device));
        }
    }
    Ok(None)
}
