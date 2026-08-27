//! Responsibility: enumerates the devices JACK exposes on this machine.

#![cfg(all(target_os = "linux", feature = "jack"))]

use anyhow::Result;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::jack_supervisor;
use domain::AudioDeviceDescriptor;

use crate::usb_proc::detect_all_usb_audio_cards;

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
