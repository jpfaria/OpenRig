//! Responsibility: serves the device list from a short-lived snapshot.
//!
//! Split out of `device_enum.rs` (#873). The UI asks on every refresh tick;
//! enumerating CoreAudio each time costs milliseconds, so the answer is cached
//! for a TTL and refreshed off-thread.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use cpal::traits::HostTrait;
use domain::AudioDeviceDescriptor;

use crate::device_enum::{enumerate_input_devices_uncached, enumerate_output_devices_uncached};
use crate::host::select_host_for_enumeration;

const DEVICE_CACHE_TTL: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct TimedDeviceCache {
    devices: Option<Vec<AudioDeviceDescriptor>>,
    fetched_at: Option<Instant>,
}

impl TimedDeviceCache {
    const fn new() -> Self {
        Self {
            devices: None,
            fetched_at: None,
        }
    }
    fn is_fresh(&self) -> bool {
        self.fetched_at
            .map(|t| t.elapsed() < DEVICE_CACHE_TTL)
            .unwrap_or(false)
    }
}

static INPUT_DEVICE_CACHE: Mutex<TimedDeviceCache> = Mutex::new(TimedDeviceCache::new());
static OUTPUT_DEVICE_CACHE: Mutex<TimedDeviceCache> = Mutex::new(TimedDeviceCache::new());
static INPUT_REFRESH_IN_FLIGHT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static OUTPUT_REFRESH_IN_FLIGHT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Force-stale the device cache so list_*_device_descriptors() refreshes
/// even if the TTL has not elapsed. Call this when we know the topology
/// changed (hot-plug detected). #693: the previous snapshot is KEPT — a
/// stale list is served instantly while the refresh runs in the
/// background, so the UI thread never waits on CoreAudio.
pub fn invalidate_device_cache() {
    INPUT_DEVICE_CACHE.lock().unwrap().fetched_at = None;
    OUTPUT_DEVICE_CACHE.lock().unwrap().fetched_at = None;
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    crate::device_config_cache::invalidate();
    #[cfg(all(target_os = "linux", feature = "jack"))]
    invalidate_proc_cache();
    log::info!("device descriptor cache invalidated (stale-while-revalidate)");
}

/// #693: refresh the input cache on a background thread (deduplicated).
fn spawn_input_refresh() {
    use std::sync::atomic::Ordering;
    if INPUT_REFRESH_IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return; // a refresh is already running
    }
    let spawned = std::thread::Builder::new()
        .name("device-enum-in".into())
        .spawn(|| {
            match enumerate_input_devices_uncached() {
                Ok(devices) => {
                    *INPUT_DEVICE_CACHE.lock().unwrap() = TimedDeviceCache {
                        devices: Some(devices),
                        fetched_at: Some(Instant::now()),
                    };
                }
                Err(e) => log::error!("background input device enumeration failed: {e}"),
            }
            INPUT_REFRESH_IN_FLIGHT.store(false, Ordering::SeqCst);
        });
    if spawned.is_err() {
        INPUT_REFRESH_IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}

/// #693: refresh the output cache on a background thread (deduplicated).
fn spawn_output_refresh() {
    use std::sync::atomic::Ordering;
    if OUTPUT_REFRESH_IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("device-enum-out".into())
        .spawn(|| {
            match enumerate_output_devices_uncached() {
                Ok(devices) => {
                    *OUTPUT_DEVICE_CACHE.lock().unwrap() = TimedDeviceCache {
                        devices: Some(devices),
                        fetched_at: Some(Instant::now()),
                    };
                }
                Err(e) => log::error!("background output device enumeration failed: {e}"),
            }
            OUTPUT_REFRESH_IN_FLIGHT.store(false, Ordering::SeqCst);
        });
    if spawned.is_err() {
        OUTPUT_REFRESH_IN_FLIGHT.store(false, Ordering::SeqCst);
    }
}

// ── Hotplug detection ────────────────────────────────────────────────────────
// Cheap device count used by the health timer to detect plug-in events without
// running a full enumeration (no ALSA PCM probe, no JACK client connection).

static LAST_KNOWN_DEVICE_COUNT: Mutex<Option<usize>> = Mutex::new(None);

/// Returns `true` when the audio device count has increased since the last
/// call, indicating that a new interface was plugged in.
///
/// Intentionally cheap — no ALSA probing, no JACK connection. Call from a
/// periodic UI timer; on `true` follow up with `invalidate_device_cache()` and
/// a full device-list refresh.
pub fn has_new_devices() -> bool {
    let current = count_devices_cheap();
    let mut guard = LAST_KNOWN_DEVICE_COUNT.lock().unwrap();
    match *guard {
        None => {
            *guard = Some(current);
            false
        }
        Some(prev) if current > prev => {
            *guard = Some(current);
            log::info!("has_new_devices: count {} → {}", prev, current);
            true
        }
        Some(prev) => {
            if current != prev {
                *guard = Some(current);
            }
            false
        }
    }
}

/// Count audio devices cheaply — no ALSA PCM probing, no JACK client.
fn count_devices_cheap() -> usize {
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        // Pure /proc/asound/cards read — safe, no PCM open, no JACK connection.
        return detect_all_usb_audio_cards().len();
    }
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = select_host_for_enumeration();
        let input = host.input_devices().map(|it| it.count()).unwrap_or(0);
        let output = host.output_devices().map(|it| it.count()).unwrap_or(0);
        input + output
    }
}

/// Returns true if the JACK server is currently running.
/// Fast, non-blocking check — safe to call from the UI thread.
#[cfg(all(target_os = "linux", feature = "jack"))]
pub fn jack_is_running() -> bool {
    jack_server_is_running()
}

pub fn list_input_device_descriptors() -> Result<Vec<AudioDeviceDescriptor>> {
    // Fast path: TTL still fresh, return the cached copy.
    let fresh = {
        let cache = INPUT_DEVICE_CACHE.lock().unwrap();
        if cache.is_fresh() {
            cache.devices.clone()
        } else {
            None
        }
    };
    if let Some(devices) = fresh {
        log::trace!(
            "list_input_device_descriptors: cache hit ({} devices)",
            devices.len()
        );
        return Ok(devices);
    }
    // Stale path (#693): never hold the caller — in practice the GUI
    // thread — on a CoreAudio enumeration (~2s measured). Serve the
    // previous snapshot immediately and refresh in the background; only
    // the very first call ever (no snapshot at all) enumerates inline.
    let stale = INPUT_DEVICE_CACHE.lock().unwrap().devices.clone();
    if let Some(devices) = stale {
        log::debug!(
            "list_input_device_descriptors: serving stale snapshot ({} devices), refreshing in background",
            devices.len()
        );
        spawn_input_refresh();
        return Ok(devices);
    }
    log::info!("list_input_device_descriptors: first enumeration...");
    let devices = enumerate_input_devices_uncached()?;
    *INPUT_DEVICE_CACHE.lock().unwrap() = TimedDeviceCache {
        devices: Some(devices.clone()),
        fetched_at: Some(Instant::now()),
    };
    Ok(devices)
}

pub fn list_output_device_descriptors() -> Result<Vec<AudioDeviceDescriptor>> {
    let fresh = {
        let cache = OUTPUT_DEVICE_CACHE.lock().unwrap();
        if cache.is_fresh() {
            cache.devices.clone()
        } else {
            None
        }
    };
    if let Some(devices) = fresh {
        log::trace!(
            "list_output_device_descriptors: cache hit ({} devices)",
            devices.len()
        );
        return Ok(devices);
    }
    // Stale path (#693): same contract as the input side — serve the
    // previous snapshot now, refresh off-thread; inline only on the very
    // first call ever.
    let stale = OUTPUT_DEVICE_CACHE.lock().unwrap().devices.clone();
    if let Some(devices) = stale {
        log::debug!(
            "list_output_device_descriptors: serving stale snapshot ({} devices), refreshing in background",
            devices.len()
        );
        spawn_output_refresh();
        return Ok(devices);
    }
    log::info!("list_output_device_descriptors: first enumeration...");
    let devices = enumerate_output_devices_uncached()?;
    *OUTPUT_DEVICE_CACHE.lock().unwrap() = TimedDeviceCache {
        devices: Some(devices.clone()),
        fetched_at: Some(Instant::now()),
    };
    Ok(devices)
}
