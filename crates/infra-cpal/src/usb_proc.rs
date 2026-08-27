//! Responsibility: detects the USB audio cards Linux reports.
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

use std::time::Duration;

pub(crate) use crate::jack_device_enum::{
    jack_enumerate_input_devices, jack_enumerate_output_devices,
};
pub(crate) use crate::jack_server_presence::jack_server_is_running_for;
pub(crate) use crate::proc_asound_cache::{invalidate_proc_cache, proc_cache_snapshot};

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
pub(crate) fn server_name_from_bracket(bracket: &str) -> String {
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

pub(crate) const PROC_CACHE_TTL: Duration = Duration::from_secs(10);

/// Detect all USB audio ALSA cards. Serialized + cached: concurrent callers
/// receive a cached snapshot instead of hammering /proc/asound.
pub(crate) fn detect_all_usb_audio_cards() -> Vec<UsbAudioCard> {
    proc_cache_snapshot().map(|s| s.cards).unwrap_or_default()
}

/// Direct /proc/asound/card{N}/stream0 read — only called from inside
/// `lookup_or_cache_card_channels` when a new card is first observed.
pub(crate) fn read_card_channels_raw(card: &str) -> (u32, u32) {
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
