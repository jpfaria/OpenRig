//! Issue #762 — cache the per-device CoreAudio config queries.
//!
//! `supported_input_configs()` / `default_input_config()` (and the output
//! pair) are "hundreds-of-ms" CoreAudio round-trips that also *disturb* USB
//! audio devices (see `validation.rs`). A live re-sync re-queries them for
//! every enabled chain — the same two physical devices probed many times per
//! sync — which freezes the GUI thread for ~750 ms on a multi-chain rig.
//!
//! These results are stable for a given device between device-topology
//! changes, so cache them keyed by `(device_id, is_input)` and invalidate the
//! whole cache whenever the device list is invalidated (`invalidate()`, called
//! from `device_enum::invalidate_device_cache`). A cache hit skips both the
//! latency and the USB disturbance.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use anyhow::Result;
use cpal::traits::DeviceTrait;
use cpal::{SupportedStreamConfig, SupportedStreamConfigRange};

/// Bumped on every device-topology invalidation; a cached entry stamped with
/// an older generation is stale and re-queried.
static GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub(crate) struct DeviceConfigs {
    pub supported: Vec<SupportedStreamConfigRange>,
    pub default: Option<SupportedStreamConfig>,
}

struct Entry {
    generation: u64,
    configs: DeviceConfigs,
}

static CACHE: Mutex<Option<HashMap<(String, bool), Entry>>> = Mutex::new(None);

/// Invalidate every cached device config (device added/removed/changed).
pub(crate) fn invalidate() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

fn query(device: &cpal::Device, is_input: bool) -> Result<DeviceConfigs> {
    let supported: Vec<SupportedStreamConfigRange> = if is_input {
        device.supported_input_configs()?.collect()
    } else {
        device.supported_output_configs()?.collect()
    };
    let default = if is_input {
        device.default_input_config().ok()
    } else {
        device.default_output_config().ok()
    };
    Ok(DeviceConfigs { supported, default })
}

/// Cached `(supported, default)` configs for a device. Queries CoreAudio only
/// on a miss or after an invalidation; the device id is the cache key.
pub(crate) fn configs_for(device: &cpal::Device, is_input: bool) -> Result<DeviceConfigs> {
    let key = match device.id() {
        Ok(id) => Some((id.to_string(), is_input)),
        Err(_) => None, // unkeyable device — never cache, always query live
    };
    let generation = GENERATION.load(Ordering::SeqCst);

    if let Some(key) = &key {
        let guard = CACHE.lock().unwrap();
        if let Some(entry) = guard.as_ref().and_then(|m| m.get(key)) {
            if entry.generation == generation {
                return Ok(entry.configs.clone());
            }
        }
    }

    let configs = query(device, is_input)?;

    if let Some(key) = key {
        let mut guard = CACHE.lock().unwrap();
        guard.get_or_insert_with(HashMap::new).insert(
            key,
            Entry {
                generation,
                configs: configs.clone(),
            },
        );
    }
    Ok(configs)
}
