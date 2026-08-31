//! Responsibility: decides what the boot does with the settings it found.
//!
//! Split out of `desktop_app::run_desktop_app` (#913). Two answers are read
//! before a single window exists, and each one is visible to the user on the
//! very first frame: whether the audio wizard opens, and what rate the VST3
//! catalog is scanned at.

use infra_filesystem::GuiSystemSettings;
use ui_openrig::UiRuntimeContext;

/// The rate the VST3 catalog is initialised at when nothing says otherwise.
const FALLBACK_SAMPLE_RATE: u32 = 48_000;

/// Whether the first-run audio wizard has to open.
///
/// A frontend that does not own its audio never asks: a controller drives
/// someone else's rig and a VST3 plugin is given its stream by the DAW, so
/// putting a device wizard in front of either would ask for a choice that
/// changes nothing.
pub(crate) fn needs_audio_settings(
    context: &UiRuntimeContext,
    settings: &GuiSystemSettings,
) -> bool {
    context.capabilities.can_select_audio_device && !settings.is_complete()
}

/// The rate to initialise the VST3 catalog at: the first configured input's,
/// or the fallback before any device is chosen.
///
/// A plugin instantiated at one rate and run at another is the classic source
/// of a detuned or aliased scan, so this follows the device the user picked
/// rather than a constant.
pub(crate) fn vst3_sample_rate(settings: &GuiSystemSettings) -> f64 {
    settings
        .input_devices
        .first()
        .map(|device| device.sample_rate)
        .unwrap_or(FALLBACK_SAMPLE_RATE) as f64
}

#[cfg(test)]
#[path = "boot_decisions_tests.rs"]
mod tests;
