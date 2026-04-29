//! USB audio card detection on Linux+JACK via /proc/asound.
//!
//! On Linux+JACK, OpenRig never opens an ALSA PCM directly — every channel
//! count, server name, and device id ultimately comes from /proc/asound and
//! a one-shot probe per physical card. This module concentrates:
//!
//! 1. The `UsbAudioCard` struct that callers hand around to talk about a
//!    detected USB card.
//! 2. The shared TTL-cached snapshot of `/proc/asound/cards` (single mutex,
//!    serialized refresh) so two threads never read the file in parallel.
//! 3. The process-lifetime registry of stream0 channel counts: a Scarlett
//!    4th Gen on RK3588 freezes if you re-read its stream0; we read once
//!    per physical card observed and remember the result forever.
//! 4. JACK device enumeration helpers — they live here because the only
//!    sensible place to call them is right after a USB card list is
//!    refreshed, and they need access to the same internal helpers.
//!
//! Every function is gated on `target_os = "linux", feature = "jack"`
//! because the underlying assumptions (proc filesystem layout, Scarlett
//! firmware quirks, jack-rs presence) only apply there.

#![cfg(all(target_os = "linux", feature = "jack"))]

use anyhow::Result;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::jack_supervisor;
use crate::AudioDeviceDescriptor;

/// Represents a USB audio card detected in /proc/asound/cards.
#[derive(Debug, Clone)]
pub(crate) struct UsbAudioCard {
    /// ALSA card number, e.g. "1"
    pub(crate) card_num: String,
    /// JACK server name derived from bracket name, e.g. "gen" for [Gen]
    pub(crate) server_name: String,
    /// Human-readable name, e.g. "USB Audio Interface"
    pub(crate) display_name: String,
    /// device_id used in chain I/O blocks, e.g. "jack:gen"
    pub(crate) device_id: String,
    /// Capture channel count read from /proc/asound/card{N}/stream0.
    /// Read exactly once when the card is first observed on the USB bus.
    pub(crate) capture_channels: u32,
    /// Playback channel count read from /proc/asound/card{N}/stream0.
    /// Read exactly once when the card is first observed on the USB bus.
    pub(crate) playback_channels: u32,
}

/// Derive a safe JACK server name from the ALSA bracket identifier.
/// E.g. "[Gen            ]" → "gen", "[Card1          ]" → "card1"
fn server_name_from_bracket(bracket: &str) -> String {
    bracket
        .trim_matches(|c: char| c == '[' || c == ']' || c.is_whitespace())
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

// ── Serialized /proc/asound cache ───────────────────────────────────────────
// On RK3588 + Scarlett 4th Gen, concurrent reads of /proc/asound/{cards,card*/
// stream0} trigger scarlett2_notify 0x20000000 which freezes the device. All
// reads must be serialized through a single mutex; requests that arrive while
// another refresh is in progress return cached data instead of queueing.

const PROC_CACHE_TTL: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub(crate) struct ProcAsoundSnapshot {
    pub(crate) cards: Vec<UsbAudioCard>,
    pub(crate) fetched_at: Instant,
}

static PROC_CACHE: Mutex<Option<ProcAsoundSnapshot>> = Mutex::new(None);

static PROC_REFRESH_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn invalidate_proc_cache() {
    *PROC_CACHE.lock().unwrap() = None;
}

fn proc_cache_is_fresh() -> bool {
    PROC_CACHE
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.fetched_at.elapsed() < PROC_CACHE_TTL)
        .unwrap_or(false)
}

/// Read and parse /proc/asound/cards + card{N}/stream0 for each USB card.
/// Direct filesystem I/O — only called under PROC_REFRESH_LOCK.
// Process-lifetime registry of channel counts per physical card. Keyed by
// display_name (e.g. "Scarlett 2i2 4th Gen at usb-xhci-hcd.3.auto-1, ...")
// so the lookup is stable across plug/unplug cycles. stream0 is read exactly
// ONCE per distinct physical card that the app ever observes; the value is
// kept in memory forever. Prevents the Scarlett firmware from seeing repeated
// stream0 reads, which cause scarlett2_notify 0x20000000 → freeze.
static CARD_CHANNELS_REGISTRY: Mutex<Option<std::collections::HashMap<String, (u32, u32)>>> =
    Mutex::new(None);

fn lookup_or_cache_card_channels(display_name: &str, card_num: &str) -> (u32, u32) {
    {
        let guard = CARD_CHANNELS_REGISTRY.lock().unwrap();
        if let Some(map) = guard.as_ref() {
            if let Some(&ch) = map.get(display_name) {
                return ch;
            }
        }
    }
    // First time we see this display_name — read stream0 once and store forever.
    let ch = read_card_channels_raw(card_num);
    let mut guard = CARD_CHANNELS_REGISTRY.lock().unwrap();
    let map = guard.get_or_insert_with(std::collections::HashMap::new);
    map.insert(display_name.to_string(), ch);
    log::info!(
        "[CARD-REGISTRY] learned '{}' → capture={} playback={}",
        display_name,
        ch.0,
        ch.1
    );
    ch
}

fn read_proc_asound_snapshot() -> ProcAsoundSnapshot {
    log::trace!("[PROC-CACHE] >>> OPEN /proc/asound/cards");
    let content = std::fs::read_to_string("/proc/asound/cards").unwrap_or_default();
    let mut cards = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        let Some(first) = trimmed.chars().next() else {
            continue;
        };
        if !first.is_ascii_digit() {
            continue;
        }
        if !(trimmed.contains("USB-Audio") || trimmed.contains("USB Audio")) {
            continue;
        }
        let card_num = match trimmed.split_whitespace().next() {
            Some(n) => n.to_string(),
            None => continue,
        };
        let bracket = match (trimmed.find('['), trimmed.find(']')) {
            (Some(a), Some(b)) if b > a => trimmed[a..=b].to_string(),
            _ => format!("[card{}]", card_num),
        };
        let server_name = server_name_from_bracket(&bracket);
        let display_name = if let Some(pos) = trimmed.find(" - ") {
            trimmed[pos + 3..].trim().to_string()
        } else {
            format!("USB Audio Card {}", card_num)
        };
        let device_id = format!("jack:{}", server_name);
        let (capture_channels, playback_channels) =
            lookup_or_cache_card_channels(&display_name, &card_num);
        cards.push(UsbAudioCard {
            card_num,
            server_name,
            display_name,
            device_id,
            capture_channels,
            playback_channels,
        });
    }
    ProcAsoundSnapshot {
        cards,
        fetched_at: Instant::now(),
    }
}

/// Non-blocking refresh: if another refresh is already running, skip. The
/// caller who was blocked will simply read the existing cache afterwards.
fn try_refresh_proc_cache() {
    let Ok(_guard) = PROC_REFRESH_LOCK.try_lock() else {
        log::debug!("[PROC-CACHE] try_refresh SKIPPED (another refresh in progress)");
        return;
    };
    if proc_cache_is_fresh() {
        log::debug!("[PROC-CACHE] try_refresh SKIPPED (became fresh while waiting)");
        return;
    }
    let caller = std::panic::Location::caller();
    log::debug!(
        "[PROC-CACHE] REFRESH /proc/asound — triggered from {}:{}",
        caller.file(),
        caller.line()
    );
    let snapshot = read_proc_asound_snapshot();
    *PROC_CACHE.lock().unwrap() = Some(snapshot);
}

#[track_caller]
fn proc_cache_snapshot() -> Option<ProcAsoundSnapshot> {
    let fresh = proc_cache_is_fresh();
    if !fresh {
        let caller = std::panic::Location::caller();
        log::debug!(
            "[PROC-CACHE] snapshot STALE — caller={}:{}",
            caller.file(),
            caller.line()
        );
        try_refresh_proc_cache();
    }
    PROC_CACHE.lock().unwrap().clone()
}

/// Detect all USB audio ALSA cards. Serialized + cached: concurrent callers
/// receive a cached snapshot instead of hammering /proc/asound.
pub(crate) fn detect_all_usb_audio_cards() -> Vec<UsbAudioCard> {
    proc_cache_snapshot().map(|s| s.cards).unwrap_or_default()
}

/// Check if a specific named JACK server is running by looking for its socket.
/// jackd -n <name> creates /dev/shm/jack_<name>_<uid>_0
pub(crate) fn jack_server_is_running_for(server_name: &str) -> bool {
    let prefix = format!("jack_{}_", server_name);
    std::fs::read_dir("/dev/shm")
        .ok()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with(&prefix) && s.ends_with("_0")
            })
        })
        .unwrap_or(false)
}

/// Direct /proc/asound/card{N}/stream0 read — only called from inside
/// `lookup_or_cache_card_channels` when a new card is first observed.
fn read_card_channels_raw(card: &str) -> (u32, u32) {
    let path = format!("/proc/asound/card{}/stream0", card);
    log::trace!("[PROC-CACHE] >>> OPEN {}", path);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            log::warn!(
                "read_card_channels_raw: cannot read {}, using defaults 2/2",
                path
            );
            return (2, 2);
        }
    };

    let mut capture_ch: Option<u32> = None;
    let mut playback_ch: Option<u32> = None;
    let mut in_capture = false;
    let mut in_playback = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Capture:") {
            in_capture = true;
            in_playback = false;
        } else if trimmed.starts_with("Playback:") {
            in_playback = true;
            in_capture = false;
        } else if trimmed.starts_with("Channels:") {
            // "Channels: 4" or "Channels: 2"
            if let Some(n) = trimmed
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse::<u32>().ok())
            {
                if in_capture && capture_ch.is_none() {
                    capture_ch = Some(n);
                } else if in_playback && playback_ch.is_none() {
                    playback_ch = Some(n);
                }
            }
        }
    }

    let capture = capture_ch.unwrap_or(2);
    let playback = playback_ch.unwrap_or(2);
    log::info!(
        "read_card_channels_raw: card {} → capture={} playback={}",
        card,
        capture,
        playback
    );
    (capture, playback)
}

/// Enumerate input devices via JACK — one entry per running named JACK server.
/// device_id is "jack:<server_name>" (e.g. "jack:gen", "jack:card1").
pub(crate) fn jack_enumerate_input_devices() -> Result<Vec<AudioDeviceDescriptor>> {
    let cards = detect_all_usb_audio_cards();
    let mut devices = Vec::new();
    for card in &cards {
        if !jack_server_is_running_for(&card.server_name) {
            continue;
        }
        let server = jack_supervisor::ServerName::from(card.server_name.clone());
        if let Ok(meta) = jack_supervisor::live_backend::probe_server_meta(&server) {
            if meta.capture_port_count > 0 {
                devices.push(AudioDeviceDescriptor {
                    id: card.device_id.clone(),
                    name: format!("{} (JACK)", card.display_name),
                    channels: meta.capture_port_count,
                });
            }
        }
    }
    Ok(devices)
}

/// Enumerate output devices via JACK — one entry per running named JACK server.
/// device_id is "jack:<server_name>" (e.g. "jack:gen", "jack:card1").
pub(crate) fn jack_enumerate_output_devices() -> Result<Vec<AudioDeviceDescriptor>> {
    let cards = detect_all_usb_audio_cards();
    let mut devices = Vec::new();
    for card in &cards {
        if !jack_server_is_running_for(&card.server_name) {
            continue;
        }
        let server = jack_supervisor::ServerName::from(card.server_name.clone());
        if let Ok(meta) = jack_supervisor::live_backend::probe_server_meta(&server) {
            if meta.playback_port_count > 0 {
                devices.push(AudioDeviceDescriptor {
                    id: card.device_id.clone(),
                    name: format!("{} (JACK)", card.display_name),
                    channels: meta.playback_port_count,
                });
            }
        }
    }
    Ok(devices)
}
