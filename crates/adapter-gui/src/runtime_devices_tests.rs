//! #913 — applying the driver-level settings must never fail the save.
//!
//! USB interfaces routinely report a timeout and apply the change anyway, so
//! this layer LOGS a failure instead of propagating it: the settings screen has
//! always treated that as a warning, and a propagated error would abort the
//! save the user just asked for. An empty save names no device and must be a
//! quiet no-op rather than an error path.

use super::apply_device_settings;

#[test]
fn a_save_that_names_no_device_applies_nothing_and_does_not_fail() {
    apply_device_settings(&[]);
}

#[test]
fn applying_is_repeatable() {
    apply_device_settings(&[]);
    apply_device_settings(&[]);
}
