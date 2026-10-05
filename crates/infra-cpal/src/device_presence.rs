//! Responsibility: answers whether the host lists an audio device right now.
//!
//! A chain whose interface is powered off or unplugged does not switch on.
//! This is asked once per switch-on, never on a timer: polling the device list
//! freezes some USB interfaces (see `desktop_app_polling`).

use domain::ids::DeviceId;
use project::binding_discovery::PortDirection;

/// Whether the host lists `device` for `direction`. A host that cannot be
/// enumerated lists nothing.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub fn device_is_present(device: &DeviceId, direction: PortDirection) -> bool {
    let host = crate::host::get_host();
    let found = match direction {
        PortDirection::Input => crate::validation::find_input_device_by_id(host, &device.0),
        PortDirection::Output => crate::validation::find_output_device_by_id(host, &device.0),
    };
    matches!(found, Ok(Some(_)))
}

/// JACK: the server for a card starts only when a chain needs it, so presence
/// is the USB card itself being on the bus.
#[cfg(all(target_os = "linux", feature = "jack"))]
pub fn device_is_present(device: &DeviceId, _direction: PortDirection) -> bool {
    crate::usb_proc::detect_all_usb_audio_cards()
        .iter()
        .any(|card| card.device_id == device.0)
}
