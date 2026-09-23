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

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait};

static INPUTS: Mutex<Option<HashMap<String, cpal::Device>>> = Mutex::new(None);
static OUTPUTS: Mutex<Option<HashMap<String, cpal::Device>>> = Mutex::new(None);

fn table(is_input: bool) -> &'static Mutex<Option<HashMap<String, cpal::Device>>> {
    if is_input {
        &INPUTS
    } else {
        &OUTPUTS
    }
}

/// Forget every remembered device (the device list changed).
pub(crate) fn invalidate() {
    *INPUTS.lock().unwrap() = None;
    *OUTPUTS.lock().unwrap() = None;
}

/// The input (`is_input`) or output device `device_id` names, if any.
pub(crate) fn find(
    host: &cpal::Host,
    device_id: &str,
    is_input: bool,
) -> Result<Option<cpal::Device>> {
    let remembered = table(is_input)
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|devices| devices.get(device_id).cloned());
    if let Some(device) = remembered {
        if device.id().is_ok_and(|id| id.to_string() == device_id) {
            return Ok(Some(device));
        }
    }

    let mut seen = HashMap::new();
    let mut found = None;
    let devices = if is_input {
        host.input_devices()?
    } else {
        host.output_devices()?
    };
    for device in devices {
        let id = device.id()?.to_string();
        if id == device_id {
            found = Some(device.clone());
        }
        seen.insert(id, device);
    }
    *table(is_input).lock().unwrap() = Some(seen);
    Ok(found)
}
