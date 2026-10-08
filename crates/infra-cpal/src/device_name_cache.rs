//! Responsibility: caches the name each device reports.
//!
//! #1081: cpal's `description()` on CoreAudio reads the name, then calls
//! `supported_input_configs()` and `supported_output_configs()`, and each of
//! those builds an AudioUnit. A new AUHAL attaches to the default output
//! device before it is pointed at the device asked about, so every call
//! starts and stops an IOProc on the default output — the HD 8 on the
//! owner's rig — inside OpenRig's own IO context on it. The device list is
//! re-read every 10 s (`device_cache`), which made that 15 IOProc starts and
//! stops on the running HD 8 per scan, forever. A device's name does not
//! change while the device list stays the same, so it is read once per id
//! and kept until the list is invalidated.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use cpal::traits::DeviceTrait;

/// Names remembered by device id. Generic over the query so the rule is
/// pinned without a sound card (`device_name_cache_tests`).
pub(crate) struct Names {
    names: Mutex<Option<HashMap<String, String>>>,
}

impl Names {
    pub(crate) const fn new() -> Self {
        Self {
            names: Mutex::new(None),
        }
    }

    /// Forget every remembered name.
    pub(crate) fn forget(&self) {
        *self.names.lock().unwrap() = None;
    }

    /// The name of device `id`: the remembered one, or what `ask` returns.
    pub(crate) fn name(&self, id: &str, ask: impl FnOnce() -> Result<String>) -> Result<String> {
        let remembered = self
            .names
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|names| names.get(id).cloned());
        if let Some(name) = remembered {
            return Ok(name);
        }
        let name = ask()?;
        self.names
            .lock()
            .unwrap()
            .get_or_insert_with(HashMap::new)
            .insert(id.to_string(), name.clone());
        Ok(name)
    }
}

static NAMES: Names = Names::new();

/// Forget every remembered name (the device list changed).
pub(crate) fn invalidate() {
    NAMES.forget();
}

/// The name `device` (whose id is `id`) reports, asked once per device list.
pub(crate) fn name_of(device: &cpal::Device, id: &str) -> Result<String> {
    NAMES.name(id, || Ok(device.description()?.name().to_string()))
}

#[cfg(test)]
#[path = "device_name_cache_tests.rs"]
mod tests;
