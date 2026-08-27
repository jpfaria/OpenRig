//! Responsibility: refreshes the list of devices the host reports.

use domain::AudioDeviceDescriptor;
use infra_cpal::{list_input_device_descriptors, list_output_device_descriptors};
use slint::{SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

/// Drop the cached enumeration so the next refresh asks the host again.
///
/// #127: the wiring modules that re-scan after a hot-swap or a settings save
/// call THIS, not `infra_cpal` — enumeration is this module's job, and a
/// callback that only wants a fresh device list has no business linking the
/// audio backend.
pub(crate) fn invalidate_device_cache() {
    infra_cpal::invalidate_device_cache();
}

pub(crate) fn refresh_input_devices(
    device_options_model: &Rc<VecModel<SharedString>>,
) -> Vec<AudioDeviceDescriptor> {
    let devices = list_input_device_descriptors().unwrap_or_default();
    let names: Vec<SharedString> = devices
        .iter()
        .map(|d| SharedString::from(d.name.as_str()))
        .collect();
    device_options_model.set_vec(names);
    devices
}

pub(crate) fn refresh_output_devices(
    device_options_model: &Rc<VecModel<SharedString>>,
) -> Vec<AudioDeviceDescriptor> {
    let devices = list_output_device_descriptors().unwrap_or_default();
    let names: Vec<SharedString> = devices
        .iter()
        .map(|d| SharedString::from(d.name.as_str()))
        .collect();
    device_options_model.set_vec(names);
    devices
}

pub(crate) fn ensure_devices_loaded(
    input: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
    output: &Rc<RefCell<Vec<AudioDeviceDescriptor>>>,
) {
    if input.borrow().is_empty() {
        *input.borrow_mut() = list_input_device_descriptors().unwrap_or_default();
    }
    if output.borrow().is_empty() {
        *output.borrow_mut() = list_output_device_descriptors().unwrap_or_default();
    }
}
