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
//! forgets everything.
//!
//! ASIO (cpal 0.18, #978) keeps its own tables for the whole process. A
//! Device there is only the driver's name and cached metadata, and every
//! output stream of a driver must be built from clones of ONE handle, or each
//! stream clears the shared buffer and the last one built erases the others.
//! While a driver runs, enumeration stops at the first other driver's name, so
//! a walk only ever adds to what is remembered.

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

    /// The device `id` names, walking only for an id never seen, and adding
    /// what the walk found to what is already remembered.
    pub(crate) fn find_merging(
        &self,
        id: &str,
        walk: impl FnOnce() -> Result<Vec<(String, D)>>,
    ) -> Result<Option<D>> {
        let mut devices = self.devices.lock().unwrap();
        if let Some(device) = devices.as_ref().and_then(|known| known.get(id)) {
            return Ok(Some(device.clone()));
        }
        let known = devices.get_or_insert_with(HashMap::new);
        for (seen_id, device) in walk()? {
            known.entry(seen_id).or_insert(device);
        }
        Ok(known.get(id).cloned())
    }
}

static INPUTS: Remembered<cpal::Device> = Remembered::new();
static OUTPUTS: Remembered<cpal::Device> = Remembered::new();
/// Never forgotten: see the module docs.
static ASIO_INPUTS: Remembered<cpal::Device> = Remembered::new();
static ASIO_OUTPUTS: Remembered<cpal::Device> = Remembered::new();

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
        let table = if is_input {
            &ASIO_INPUTS
        } else {
            &ASIO_OUTPUTS
        };
        return table.find_merging(device_id, || walk(host, is_input));
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
