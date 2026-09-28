//! Responsibility: resolves a device id to the cpal device it names.
//!
//! Issue #967. Every endpoint of a chain (head input, tail output, an insert's
//! send and return) and the channel validation before them looked its device
//! up by walking the host's whole device list — one CoreAudio property query
//! per device, per endpoint. On the owner's rig (a Quantum HD 8, two ADA 8200s
//! behind it, USB interfaces, loopbacks) that walk made switching a chain on
//! take 1.8–1.9 s before the 2 ms DSP build and the 235 ms stream open even
//! started, every single time.
//!
//! A walk now remembers every device it passes, so the next lookup of any of
//! them is one property query that confirms the handle still names that id. A
//! device that was unplugged, replugged or renumbered fails that check and is
//! looked up again; a device-list invalidation (`invalidate_device_cache`)
//! forgets everything. ASIO keeps the old walk: holding a driver's handle keeps
//! it loaded, and a host loads one ASIO driver at a time.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};

/// Devices remembered by id from the last walk. Generic so the rule can be
/// pinned without a sound card (`device_lookup_tests`).
pub(crate) struct Remembered<D> {
    devices: Mutex<Option<HashMap<String, D>>>,
}

impl<D: Clone> Remembered<D> {
    pub(crate) const fn new() -> Self {
        Self {
            devices: Mutex::new(None),
        }
    }

    /// Forget every remembered device.
    pub(crate) fn forget(&self) {
        *self.devices.lock().unwrap() = None;
    }

    /// The device `id` names: the remembered one if `still_names(device, id)`
    /// confirms it, otherwise whatever a fresh `walk` finds — and everything
    /// that walk passed is remembered.
    pub(crate) fn find(
        &self,
        id: &str,
        still_names: impl Fn(&D, &str) -> bool,
        walk: impl FnOnce() -> Result<Vec<(String, D)>>,
    ) -> Result<Option<D>> {
        let remembered = self
            .devices
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|devices| devices.get(id).cloned());
        if let Some(device) = remembered {
            if still_names(&device, id) {
                return Ok(Some(device));
            }
        }
        let seen: HashMap<String, D> = walk()?.into_iter().collect();
        let found = seen.get(id).cloned();
        *self.devices.lock().unwrap() = Some(seen);
        Ok(found)
    }
}

static INPUTS: Remembered<cpal::Device> = Remembered::new();
static OUTPUTS: Remembered<cpal::Device> = Remembered::new();

/// Forget every remembered device (the device list changed).
pub(crate) fn invalidate() {
    INPUTS.forget();
    OUTPUTS.forget();
}

fn walk(host: &cpal::Host, is_input: bool) -> Result<Vec<(String, cpal::Device)>> {
    let devices = if is_input {
        host.input_devices()?
    } else {
        host.output_devices()?
    };
    let mut seen = Vec::new();
    for device in devices {
        seen.push((device.id()?.to_string(), device));
    }
    Ok(seen)
}

/// The input (`is_input`) or output device `device_id` names, if any.
pub(crate) fn find(
    host: &cpal::Host,
    device_id: &str,
    is_input: bool,
) -> Result<Option<cpal::Device>> {
    if crate::host::is_asio_host(host) {
        // One ASIO driver loaded at a time: stop at the match, keep nothing.
        let devices = if is_input {
            host.input_devices()?
        } else {
            host.output_devices()?
        };
        for device in devices {
            if device.id()?.to_string() == device_id {
                return Ok(Some(device));
            }
        }
        return Ok(None);
    }
    let table = if is_input { &INPUTS } else { &OUTPUTS };
    table.find(
        device_id,
        |device, id| device.id().is_ok_and(|current| current.to_string() == id),
        || walk(host, is_input),
    )
}

#[cfg(test)]
#[path = "device_lookup_tests.rs"]
mod tests;
