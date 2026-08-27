//! Responsibility: caches what the kernel last reported about the sound cards.

#![cfg(all(target_os = "linux", feature = "jack"))]

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::usb_proc::{
    read_card_channels_raw, server_name_from_bracket, UsbAudioCard, PROC_CACHE_TTL,
};

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

pub(crate) fn proc_cache_is_fresh() -> bool {
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

pub(crate) fn lookup_or_cache_card_channels(display_name: &str, card_num: &str) -> (u32, u32) {
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

pub(crate) fn read_proc_asound_snapshot() -> ProcAsoundSnapshot {
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
pub(crate) fn try_refresh_proc_cache() {
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
pub(crate) fn proc_cache_snapshot() -> Option<ProcAsoundSnapshot> {
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
