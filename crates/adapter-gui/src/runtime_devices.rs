//! Responsibility: performs the driver-level operations on this machine's devices.
//! #127: driver-level operations on THIS machine's devices — the body of
//! `RuntimeControl::apply_device_settings` (making them adopt the rate and
//! buffer size the project just saved) and, on Linux, starting the JACK server
//! the chains will run on.
//!
//! It is not the runtime graph: no chain's stream is opened, paused or rebuilt
//! here (that is `sync_project`, which the same handler calls right after).
//! What happens here is a driver reconfiguration — on macOS/Windows a throwaway
//! stream built at the requested rate, which is the only thing CoreAudio and
//! WASAPI react to; on Linux+JACK the server owns the device configuration and
//! `infra_cpal` skips the ALSA probe entirely.
//!
//! It lived in `settings/audio.rs` — three call sites, each immediately before
//! dispatching `SettingsCommand::SaveAudioSettings` — so a client that
//! dispatched the same command persisted the pick and re-opened the graph
//! against a driver still running the old rate.

use project::device::DeviceSettings;

/// Configure every device the save names, in order. A failure is LOGGED, not
/// propagated: USB interfaces routinely report a timeout and apply the change
/// anyway, and the settings screen has always treated that as a warning. The
/// save must not fail because a device answered slowly.
pub(crate) fn apply_device_settings(settings: &[DeviceSettings]) {
    if let Err(e) = infra_cpal::apply_device_settings(settings) {
        log::warn!("apply_device_settings failed: {e}");
    }
}

/// Launch `jackd` for the cards named in `settings`, off the UI thread, and
/// hand back the channel the outcome arrives on.
///
/// Enabling a chain on Linux waits for this; the caller polls the receiver from
/// a Slint timer so the window keeps drawing while the server comes up. The
/// wrapper exists for the #127 boundary: starting the audio server is a device
/// operation on the machine that hosts it, so it belongs here with the rest of
/// them, and the chain callback that needs it does not have to name the backend
/// to ask.
#[cfg(target_os = "linux")]
pub(crate) fn start_jack_in_background(
    settings: Vec<DeviceSettings>,
) -> std::sync::mpsc::Receiver<anyhow::Result<()>> {
    infra_cpal::start_jack_in_background(settings)
}
