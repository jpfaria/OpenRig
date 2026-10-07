//! Responsibility: lists the devices a host offers for one direction.
//!
//! #1081: cpal's `input_devices()` / `output_devices()` keep a device when
//! `supported_*_configs()` answers, and on CoreAudio every one of those calls
//! builds an AudioUnit. A new AUHAL attaches to the default output device
//! first, so each call started and stopped an IOProc on the default output —
//! the HD 8 on the owner's rig — inside OpenRig's own IO context on it. The
//! device list is re-read every 10 s while chains play. Here the direction
//! comes from the cached configs (`device_config_cache`), asked once per
//! device list, so a scan of an unchanged list builds no AudioUnit. ASIO
//! keeps cpal's lists: a host loads one ASIO driver at a time.

use anyhow::Result;
use cpal::traits::HostTrait;

/// The input (`is_input`) or output devices of `host`.
pub(crate) fn devices_of(host: &cpal::Host, is_input: bool) -> Result<Vec<cpal::Device>> {
    if crate::host::is_asio_host(host) {
        return Ok(if is_input {
            host.input_devices()?.collect()
        } else {
            host.output_devices()?.collect()
        });
    }
    Ok(host
        .devices()?
        .filter(|device| {
            crate::device_config_cache::configs_for(device, is_input)
                .is_ok_and(|configs| !configs.supported.is_empty())
        })
        .collect())
}

#[cfg(test)]
#[path = "device_list_tests.rs"]
mod tests;
