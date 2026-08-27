//! Responsibility: keeps the historical `audio_devices` path pointing at the five things it held.
//!
//! It was responsible for the binding status after a refresh, refreshing the
//! device list, editing the device rows, resolving one device's settings, and
//! shaping the picker items (#873).

pub(crate) use crate::binding_status::check_bindings_after_refresh;
pub(crate) use crate::device_refresh_list::{
    ensure_devices_loaded, invalidate_device_cache, refresh_input_devices, refresh_output_devices,
};
pub(crate) use crate::device_rows::{
    build_project_device_rows, toggle_device_row, update_device_bit_depth,
    update_device_buffer_size, update_device_sample_rate,
};
pub(crate) use crate::device_selection_items::{
    build_device_selection_items, mark_unselected_devices,
};
pub(crate) use crate::device_settings_resolve::selected_device_settings;

// `audio_devices_tests.rs` hangs off this module and builds descriptors
// through `super::`, as it did before the split (#873).
#[cfg(test)]
pub(crate) use domain::AudioDeviceDescriptor;

#[cfg(test)]
#[path = "audio_devices_tests.rs"]
mod tests;
