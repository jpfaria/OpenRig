//! #127: the body of `RuntimeControl::apply_device_settings` — making THIS
//! machine's devices adopt the rate and buffer size the project just saved.
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
