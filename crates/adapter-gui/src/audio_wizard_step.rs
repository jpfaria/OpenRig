//! Responsibility: decides whether the audio wizard may leave the input step.
//!
//! Split out of `audio_wizard_wiring` (#913). Moving the step and painting the
//! toast is screen work; the gate is not — the wizard runs once, on first
//! launch, and letting it reach the output step with no input selected produces
//! a rig that opens with nothing to listen to.

use std::rc::Rc;

use slint::VecModel;

use crate::audio_devices::selected_device_settings;
use crate::DeviceSelectionItem;

/// What the "next" button may do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WizardStep {
    /// At least one usable input is selected.
    Advance,
    /// Nothing selected — the user is told to pick one.
    NeedsAnInput,
    /// A selected row could not be read (bad rate / buffer text); carries the
    /// message to show.
    Invalid(String),
}

/// Read the input rows and decide.
pub(crate) fn next_step(input_devices: &Rc<VecModel<DeviceSelectionItem>>) -> WizardStep {
    match selected_device_settings(input_devices, "input") {
        Ok(devices) if !devices.is_empty() => WizardStep::Advance,
        Ok(_) => WizardStep::NeedsAnInput,
        Err(error) => WizardStep::Invalid(error.to_string()),
    }
}

#[cfg(test)]
#[path = "audio_wizard_step_tests.rs"]
mod tests;
