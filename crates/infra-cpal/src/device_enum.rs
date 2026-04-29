//! Device-level enumeration + a TTL-cached snapshot the UI hits on every
//! refresh tick.
//!
//! Public surface (re-exported by lib.rs):
//! - `list_devices` — flat human-readable strings, used by the CLI.
//! - `list_input_device_descriptors` / `list_output_device_descriptors` —
//!   structured `AudioDeviceDescriptor`s, cached for 10s.
//! - `invalidate_device_cache` — force-stale on hot-plug.
//! - `has_new_devices` — UI-timer probe (cheap, no PCM open).
//!
//! Internal helpers — all `pub(crate)`:
//! - `is_hardware_device` (filters cpal's plughw / dmix noise on Linux/ALSA),
//! - `count_devices_cheap` (called by `has_new_devices`),
//! - `enumerate_input_devices_uncached` / `enumerate_output_devices_uncached`
//!   (the slow path behind the cache),
//! - `jack_is_running` (Linux+JACK only — public via lib.rs).
//!
//! On Linux+JACK the whole enumeration goes through libjack +
//! /proc/asound (see `usb_proc.rs`); the CPAL host is never created.

#[cfg(all(target_os = "linux", feature = "jack"))]
use anyhow::bail;
use anyhow::Result;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::traits::{DeviceTrait, HostTrait};

use crate::AudioDeviceDescriptor;

#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::host::jack_server_is_running;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::host::{get_host, select_host_for_enumeration};

#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::usb_proc::{
    detect_all_usb_audio_cards, invalidate_proc_cache, jack_enumerate_input_devices,
    jack_enumerate_output_devices,
};

pub fn list_devices() -> Result<Vec<String>> {
    log::trace!("listing all audio devices");

    // On Linux with the jack feature, JACK is the only supported backend for
    // audio streaming. Never fall through to CPAL/ALSA — probing a broken USB
    // audio device via ALSA can block indefinitely on certain kernels (RK3588
    // xHCI, for example). If JACK is not running, fail fast with a clear error.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if !jack_server_is_running() {
            bail!("JACK server is not running — start jackd before enumerating devices");
        }
        let inputs = jack_enumerate_input_devices()?;
        let outputs = jack_enumerate_output_devices()?;
        let mut devices = Vec::new();
        for d in inputs { devices.push(format!("input: {} | device_id: {}", d.name, d.id)); }
        for d in outputs { devices.push(format!("output: {} | device_id: {}", d.name, d.id)); }
        return Ok(devices);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = get_host();
        let mut devices = Vec::new();
        for device in host.input_devices()? {
            let description = device.description()?;
            devices.push(format!(
                "input: {} | device_id: {}",
                description,
                device.id()?
            ));
        }
        for device in host.output_devices()? {
            let description = device.description()?;
            devices.push(format!(
                "output: {} | device_id: {}",
                description,
                device.id()?
            ));
        }
        Ok(devices)
    }
}

/// On Linux/ALSA, cpal lists all logical devices (equivalent to `aplay -L`),
/// which includes dozens of virtual entries per card (surround51, iec958, dmix,
/// plughw, default, etc.). Only hardware devices (`hw:`) are meaningful for
/// the device picker — they map 1:1 to physical cards.
///
/// On other platforms this function always returns true (no filtering needed).
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn is_hardware_device(id: &str) -> bool {
    // cpal formats device IDs as "host:pcm_id", e.g. "alsa:hw:CARD=Gen,DEV=0".
    // On Linux/ALSA, only keep hw: entries — direct hardware, one per physical
    // card/device. Skips plughw, default, surround51, iec958, dmix, etc.
    //
    // cpal enumerates each card twice: once via HintIter (named form:
    // hw:CARD=Gen,DEV=0) and once via hardware scan (numeric form:
    // hw:CARD=1,DEV=0). The two forms may have slightly different device names
    // ("USB Audio Interface, USB Audio" vs "USB Audio Interface"), defeating
    // name-based deduplication. Reject numeric CARD= forms so only the named
    // form survives — it is stable (card numbers can change on reboot).
    #[cfg(target_os = "linux")]
    {
        // When using the JACK host, device IDs start with "jack:" — always accept them.
        if id.starts_with("jack:") {
            return true;
        }
        // For ALSA, only keep hw: entries — direct hardware, one per physical
        // card/device. Skips plughw, default, surround51, iec958, dmix, etc.
        //
        // cpal enumerates each card twice: once via HintIter (named form:
        // hw:CARD=Gen,DEV=0) and once via hardware scan (numeric form:
        // hw:CARD=1,DEV=0). The two forms may have slightly different device names
        // ("USB Audio Interface, USB Audio" vs "USB Audio Interface"), defeating
        // name-based deduplication. Reject numeric CARD= forms so only the named
        // form survives — it is stable (card numbers can change on reboot).
        let pcm_id = id.split_once(':').map(|(_, d)| d).unwrap_or(id);
        if !pcm_id.starts_with("hw:") {
            return false;
        }
        // Accept only named CARD forms: hw:CARD=<letter>...
        // Reject numeric CARD forms: hw:CARD=<digit>...
        let after_card = pcm_id.split("CARD=").nth(1).unwrap_or("");
        !after_card.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = id;
        true
    }
}

// ── Device descriptor cache ─────────────────────────────────────────────────
// Device enumeration (CPAL or JACK) is expensive — on macOS CoreAudio takes
// 200-500ms, on Linux/JACK the first connection takes similar time. The UI
// calls refresh_input/output_devices on every click (30+ call sites). Cache
// the result with a TTL so a click storm produces at most one enumeration
// per window. Concurrent refreshes coalesce via try_lock — extra callers
// receive the current snapshot rather than queueing another enumeration.

const DEVICE_CACHE_TTL: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct TimedDeviceCache {
    devices: Option<Vec<AudioDeviceDescriptor>>,
    fetched_at: Option<Instant>,
}

impl TimedDeviceCache {
    const fn new() -> Self {
        Self { devices: None, fetched_at: None }
    }
    fn is_fresh(&self) -> bool {
        self.fetched_at.map(|t| t.elapsed() < DEVICE_CACHE_TTL).unwrap_or(false)
    }
}

static INPUT_DEVICE_CACHE: Mutex<TimedDeviceCache> = Mutex::new(TimedDeviceCache::new());
static OUTPUT_DEVICE_CACHE: Mutex<TimedDeviceCache> = Mutex::new(TimedDeviceCache::new());
static INPUT_REFRESH_LOCK: Mutex<()> = Mutex::new(());
static OUTPUT_REFRESH_LOCK: Mutex<()> = Mutex::new(());

/// Force-stale the device cache so the next list_*_device_descriptors() call
/// re-enumerates even if the TTL has not elapsed. Call this when we know the
/// topology changed (hot-plug detected).
pub fn invalidate_device_cache() {
    *INPUT_DEVICE_CACHE.lock().unwrap() = TimedDeviceCache::new();
    *OUTPUT_DEVICE_CACHE.lock().unwrap() = TimedDeviceCache::new();
    #[cfg(all(target_os = "linux", feature = "jack"))]
    invalidate_proc_cache();
    log::info!("device descriptor cache invalidated");
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
        log::trace!("list_input_device_descriptors: cache hit ({} devices)", devices.len());
        return Ok(devices);
    }
    // Slow path: try to acquire the refresh lock. If another thread is
    // already refreshing, return whatever stale copy we have instead of
    // running a concurrent enumeration.
    match INPUT_REFRESH_LOCK.try_lock() {
        Ok(_guard) => {
            // Double-check in case a concurrent refresh just finished.
            let already_fresh = {
                let cache = INPUT_DEVICE_CACHE.lock().unwrap();
                if cache.is_fresh() { cache.devices.clone() } else { None }
            };
            if let Some(devices) = already_fresh {
                return Ok(devices);
            }
            log::info!("list_input_device_descriptors: cache stale, enumerating...");
            let devices = enumerate_input_devices_uncached()?;
            *INPUT_DEVICE_CACHE.lock().unwrap() = TimedDeviceCache {
                devices: Some(devices.clone()),
                fetched_at: Some(Instant::now()),
            };
            Ok(devices)
        }
        Err(_) => {
            log::debug!("list_input_device_descriptors: refresh in progress, returning stale snapshot");
            Ok(INPUT_DEVICE_CACHE.lock().unwrap().devices.clone().unwrap_or_default())
        }
    }
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
        log::trace!("list_output_device_descriptors: cache hit ({} devices)", devices.len());
        return Ok(devices);
    }
    match OUTPUT_REFRESH_LOCK.try_lock() {
        Ok(_guard) => {
            let already_fresh = {
                let cache = OUTPUT_DEVICE_CACHE.lock().unwrap();
                if cache.is_fresh() { cache.devices.clone() } else { None }
            };
            if let Some(devices) = already_fresh {
                return Ok(devices);
            }
            log::info!("list_output_device_descriptors: cache stale, enumerating...");
            let devices = enumerate_output_devices_uncached()?;
            *OUTPUT_DEVICE_CACHE.lock().unwrap() = TimedDeviceCache {
                devices: Some(devices.clone()),
                fetched_at: Some(Instant::now()),
            };
            Ok(devices)
        }
        Err(_) => {
            log::debug!("list_output_device_descriptors: refresh in progress, returning stale snapshot");
            Ok(OUTPUT_DEVICE_CACHE.lock().unwrap().devices.clone().unwrap_or_default())
        }
    }
}

fn enumerate_input_devices_uncached() -> Result<Vec<AudioDeviceDescriptor>> {
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if jack_server_is_running() {
            return jack_enumerate_input_devices();
        }
        // JACK not running — detect USB audio cards from /proc/asound/cards and
        // return them with jack:<name> device IDs, matching what is stored in
        // project YAML. This avoids calling supported_input_configs() (which
        // opens the PCM directly) and ensures device_id
        // consistency regardless of ALSA card numbering order (hw:0 vs hw:1).
        log::info!("JACK not running, detecting USB audio cards for input devices (no PCM probe)");
        let usb_cards = detect_all_usb_audio_cards();
        let cards: Vec<AudioDeviceDescriptor> = usb_cards.iter().map(|c| AudioDeviceDescriptor {
            id: c.device_id.clone(),
            name: c.display_name.clone(),
            channels: c.capture_channels as usize,
        }).collect();
        log::info!("[enumerate_input] usb cards: {} devices", cards.len());
        return Ok(cards);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = select_host_for_enumeration();
        let mut devices = Vec::new();
        for device in host.input_devices()? {
            let id = device.id()?.to_string();
            if !is_hardware_device(&id) {
                continue;
            }
            let name = device.description()?.name().to_string();
            if devices.iter().any(|d: &AudioDeviceDescriptor| d.name == name) {
                continue;
            }
            let ch = crate::max_supported_input_channels(&device).unwrap_or(0);
            log::info!("[enumerate_input] device id='{}' name='{}' channels={}", id, name, ch);
            devices.push(AudioDeviceDescriptor { id, name, channels: ch });
        }
        log::info!("[enumerate_input] total {} devices", devices.len());
        devices.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(devices)
    }
}

fn enumerate_output_devices_uncached() -> Result<Vec<AudioDeviceDescriptor>> {
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if jack_server_is_running() {
            return jack_enumerate_output_devices();
        }
        log::info!("JACK not running, detecting USB audio cards for output devices (no PCM probe)");
        let usb_cards = detect_all_usb_audio_cards();
        let cards: Vec<AudioDeviceDescriptor> = usb_cards.iter().map(|c| AudioDeviceDescriptor {
            id: c.device_id.clone(),
            name: c.display_name.clone(),
            channels: c.playback_channels as usize,
        }).collect();
        log::info!("[enumerate_output] usb cards: {} devices", cards.len());
        return Ok(cards);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = select_host_for_enumeration();
        let mut devices = Vec::new();
        for device in host.output_devices()? {
            let id = device.id()?.to_string();
            if !is_hardware_device(&id) {
                continue;
            }
            let name = device.description()?.name().to_string();
            if devices.iter().any(|d: &AudioDeviceDescriptor| d.name == name) {
                continue;
            }
            let ch = crate::max_supported_output_channels(&device).unwrap_or(0);
            log::info!("[enumerate_output] device id='{}' name='{}' channels={}", id, name, ch);
            devices.push(AudioDeviceDescriptor { id, name, channels: ch });
        }
        log::info!("[enumerate_output] total {} devices", devices.len());
        devices.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(devices)
    }
}
