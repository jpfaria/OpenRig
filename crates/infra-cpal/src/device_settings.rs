//! Device-level config helpers exposed to the application layer.
//!
//! - `start_jack_in_background` (Linux+JACK) — spawn one jackd per
//!   detected USB audio card via a standalone supervisor + the live
//!   backend, so the UI's "Start audio" timer can poll the resulting
//!   `Receiver` without blocking.
//! - `apply_device_settings` — push the requested sample rate / buffer
//!   size to the backend on macOS/Windows by building a throwaway cpal
//!   stream (the only thing that forces CoreAudio / WASAPI to
//!   reconfigure). On Linux+JACK this is a no-op because jackd is the
//!   single owner of device config — `start_jack_in_background` already
//!   spawned it with the right values.
//!
//! Public surface: both fns are re-exported by lib.rs to preserve the
//! adapter-gui call sites' import paths.

use anyhow::Result;

use project::device::DeviceSettings;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::traits::DeviceTrait;

use crate::device_enum::invalidate_device_cache;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::host::get_host;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::jack_supervisor;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::usb_proc::detect_all_usb_audio_cards;

/// Start JACK in background threads — one per connected USB audio interface.
/// Returns a channel that resolves when ALL servers are ready (Ok) or any fails (Err).
/// Non-blocking — returns immediately. Poll the receiver from a UI timer.
///
/// Runs a standalone [`jack_supervisor::JackSupervisor`] that owns its own
/// [`jack_supervisor::LiveJackBackend`]. The controller's supervisor (when
/// one is later created via [`ProjectRuntimeController::start`]) instantiates
/// its own backend — both talk to jackd servers by name so they don't
/// conflict. The only shared state is the static
/// `JACK_DEFAULT_SERVER_LOCK` inside `live_backend`, which serialises env-var
/// writes between any number of supervisor instances.
#[cfg(all(target_os = "linux", feature = "jack"))]
pub fn start_jack_in_background(
    device_settings: Vec<DeviceSettings>,
) -> std::sync::mpsc::Receiver<anyhow::Result<()>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> anyhow::Result<()> {
            let cards = detect_all_usb_audio_cards();
            if cards.is_empty() {
                anyhow::bail!(
                    "no USB audio interface found — connect a device before enabling a chain"
                );
            }
            let mut supervisor =
                jack_supervisor::JackSupervisor::new(jack_supervisor::LiveJackBackend::new());
            for card in &cards {
                let matched = device_settings
                    .iter()
                    .find(|s| s.device_id.0 == card.device_id);
                let sample_rate = matched.map(|s| s.sample_rate).unwrap_or(48_000);
                let buffer_size = matched.map(|s| s.buffer_size_frames).unwrap_or(64);
                let nperiods = matched.map(|s| s.nperiods).unwrap_or(3);
                let realtime = matched.map(|s| s.realtime).unwrap_or(true);
                let rt_priority = matched.map(|s| s.rt_priority).unwrap_or(70);
                let config = jack_supervisor::JackConfig {
                    sample_rate,
                    buffer_size,
                    nperiods,
                    realtime,
                    rt_priority,
                    card_num: card.card_num.parse().unwrap_or(0),
                    capture_channels: card.capture_channels,
                    playback_channels: card.playback_channels,
                };
                let server_name = jack_supervisor::ServerName::from(card.server_name.clone());
                let mut hook = |_: &jack_supervisor::ServerName| {};
                supervisor.ensure_server(&server_name, &config, &mut hook)?;
            }
            invalidate_device_cache();
            Ok(())
        })();
        let _ = tx.send(result);
    });
    rx
}

/// Apply device settings (sample rate, buffer size) to hardware devices
/// without requiring active chains. On macOS/CoreAudio, building a stream
/// with the desired sample rate forces the driver to reconfigure the device.
/// The temporary stream is dropped immediately after configuration.
///
/// USB audio devices may take a few seconds to reconfigure.
/// cpal may report a timeout even though the change succeeds — we treat
/// timeouts as warnings and wait for the device to settle.
pub fn apply_device_settings(settings: &[DeviceSettings]) -> Result<()> {
    if settings.is_empty() {
        return Ok(());
    }
    // On Linux with JACK, jackd is always launched with the correct sample_rate
    // and buffer_size from gui-settings.yaml via ensure_jack_running(). Never
    // probe the ALSA PCM here — on RK3588 xHCI, calling supported_input_configs()
    // on Linux/JACK, probing the ALSA PCM can disturb USB audio devices.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        let _ = settings;
        log::info!(
            "apply_device_settings: Linux/JACK — skipping ALSA probe (jackd owns device config)"
        );
        return Ok(());
    }
    // macOS / Windows path: build a temporary stream to force the CoreAudio /
    // WASAPI driver to adopt the requested sample rate and buffer size.
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = get_host();
        let mut needs_settle = false;
        for ds in settings {
            log::info!(
                "apply_device_settings: configuring '{}' sr={} buf={}",
                ds.device_id.0,
                ds.sample_rate,
                ds.buffer_size_frames
            );
            // Try as input device first — on macOS the same physical device
            // often shares one AudioObjectID for both directions, so configuring
            // the input side sets the sample rate for the whole device.
            if let Ok(Some(device)) = crate::find_input_device_by_id(host, &ds.device_id.0) {
                // Check if device already at requested sample rate
                let current_rate = device
                    .default_input_config()
                    .map(|c| c.sample_rate())
                    .unwrap_or(0);
                if current_rate == ds.sample_rate {
                    log::info!(
                        "apply_device_settings: input '{}' already at sr={}, skipping",
                        ds.device_id.0,
                        ds.sample_rate
                    );
                    continue;
                }
                if let Ok(ranges) = device.supported_input_configs() {
                    let ranges: Vec<_> = ranges.collect();
                    if let Some(config) = ranges
                        .iter()
                        .filter(|r| r.channels() >= 1)
                        .filter_map(|r| r.try_with_sample_rate(ds.sample_rate))
                        .next()
                    {
                        let stream_config = crate::build_stream_config(
                            config.channels(),
                            ds.sample_rate,
                            ds.buffer_size_frames,
                        );
                        match device.build_input_stream(
                            &stream_config,
                            |_data: &[f32], _| {},
                            |err| log::warn!("apply_device_settings input error: {err}"),
                            None,
                        ) {
                            Ok(stream) => {
                                log::info!(
                                    "apply_device_settings: input device '{}' configured (sr={} buf={})",
                                    ds.device_id.0, ds.sample_rate, ds.buffer_size_frames
                                );
                                drop(stream);
                            }
                            Err(e) => {
                                // USB audio devices may timeout during sample rate
                                // change but still reconfigure successfully. Treat as warning.
                                let msg = e.to_string();
                                if msg.contains("timeout") {
                                    log::info!(
                                        "apply_device_settings: device '{}' sample rate change in progress (timeout is normal for USB devices)",
                                        ds.device_id.0
                                    );
                                    needs_settle = true;
                                } else {
                                    log::warn!(
                                        "apply_device_settings: failed to configure input '{}': {e}",
                                        ds.device_id.0
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        if needs_settle {
            log::info!("apply_device_settings: waiting 3s for USB device to settle after sample rate change");
            std::thread::sleep(std::time::Duration::from_secs(3));
            // Invalidate device cache since supported configs may have changed
            invalidate_device_cache();
        }
        return Ok(());
    }
}
