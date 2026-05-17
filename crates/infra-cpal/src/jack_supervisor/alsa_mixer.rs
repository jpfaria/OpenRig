//! ALSA playback-mixer initialization (Linux).
//!
//! Without PipeWire/PulseAudio nothing sets a USB interface's ALSA
//! mixer, so many cards come up attenuated (e.g. −23 dB) — sound is
//! weak and muffled even with everything maxed in-app. macOS CoreAudio
//! handles this; on Linux OpenRig must, before JACK opens the device
//! (issue #479; `docs/audio-config.md` always documented this step —
//! this is the implementation that finally backs the doc).

use std::process::{Command, Stdio};

/// Playback control names seen across the USB interfaces we target.
/// A card only has a few of these; the rest are skipped silently.
const PLAYBACK_CONTROLS: &[&str] = &[
    "PCM",
    "Master",
    "Speaker",
    "Headphone",
    "Playback",
    "Line Out",
    "Speaker+LO",
];

/// Drive the card's playback mixer to unity (100% / 0 dB, unmuted).
///
/// Best-effort and never fatal: an absent control is expected and
/// skipped; a missing `amixer` (alsa-utils not installed) is logged
/// and ignored — jackd still starts, only the level is left as-is.
/// Capture gain is intentionally untouched (instrument level is the
/// player's call / the interface's hardware knob).
pub(super) fn set_playback_mixer_unity(card_num: u32) {
    for ctrl in PLAYBACK_CONTROLS {
        let status = Command::new("amixer")
            .args(["-c", &card_num.to_string(), "sset", ctrl, "100%", "unmute"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match status {
            Ok(s) if s.success() => {
                log::info!("mixer: {ctrl} -> 100% unmute on card {card_num}");
            }
            Ok(_) => {} // control absent on this card — expected
            Err(e) => {
                log::warn!(
                    "mixer: `amixer` unavailable ({e}); card {card_num} \
                     playback left as-is (install alsa-utils)"
                );
                return;
            }
        }
    }
}
