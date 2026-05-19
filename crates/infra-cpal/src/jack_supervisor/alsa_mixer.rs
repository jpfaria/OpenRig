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
const PLAYBACK_CONTROLS: &[&str] = &[
    "PCM",
    "Master",
    "Speaker",
    "Headphone",
    "Playback",
    "Line Out",
    "Speaker+LO",
];

/// Capture control names. These ship maxed on many USB interfaces
/// (e.g. Teyun Q26: Mic at +27 dB) — a guitar into a +27 dB input
/// clips the ADC, so the signal is loud but distorted/muffled. Unity
/// (0 dB = no boost) is the safe default; the player raises it if they
/// actually need gain (#479).
const CAPTURE_CONTROLS: &[&str] = &["Mic", "Capture", "Line In", "Line", "Mic Boost"];

/// Drive the card to unity: playback AND capture to 0 dB, unmuted.
///
/// `0dB` (not `100%`): on many interfaces 100% is well above unity and
/// either over-drives the output or, on capture, clips the input.
/// Best-effort and never fatal: an absent control is skipped; a missing
/// `amixer` (alsa-utils not installed) is logged once and ignored —
/// jackd still starts, only the level is left as-is.
pub(super) fn set_mixer_unity(card_num: u32) {
    let card = card_num.to_string();
    for ctrl in PLAYBACK_CONTROLS.iter().chain(CAPTURE_CONTROLS) {
        let status = Command::new("amixer")
            .args(["-c", &card, "sset", ctrl, "0dB", "unmute"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match status {
            Ok(s) if s.success() => {
                log::info!("mixer: {ctrl} -> 0dB unmute on card {card_num}");
            }
            Ok(_) => {} // control absent on this card — expected
            Err(e) => {
                log::warn!(
                    "mixer: `amixer` unavailable ({e}); card {card_num} \
                     levels left as-is (install alsa-utils)"
                );
                return;
            }
        }
    }
}
