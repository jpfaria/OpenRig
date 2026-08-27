//! Responsibility: counts the channels a device can give a stream.

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::{anyhow, Result};

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
