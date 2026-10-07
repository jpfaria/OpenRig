//! Responsibility: decides where each button of the first-run setup wizard leads.
//!
//! The page only shows the step it is told to and reports which button was
//! pressed; the order of the steps, which ones may be skipped and when the
//! audio devices are saved are decided here, where a test can pin them.

/// How many steps the wizard has.
pub(crate) const STEP_COUNT: i32 = 5;

/// One page of the wizard, in the order the user walks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetupStep {
    Language = 0,
    Audio = 1,
    IoBindings = 2,
    Midi = 3,
    Tone3000 = 4,
}

impl SetupStep {
    pub(crate) fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::Language),
            1 => Some(Self::Audio),
            2 => Some(Self::IoBindings),
            3 => Some(Self::Midi),
            4 => Some(Self::Tone3000),
            _ => None,
        }
    }

    /// Optional steps: the app works without a MIDI controller or a TONE3000
    /// key, but not without an audio device.
    pub(crate) fn skippable(self) -> bool {
        matches!(self, Self::Midi | Self::Tone3000)
    }

    pub(crate) fn is_last(self) -> bool {
        self as i32 == STEP_COUNT - 1
    }
}

/// What a button press does.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WizardMove {
    /// Show the step at this index.
    Goto(i32),
    /// Save the chosen audio devices; the step moves once the save succeeds.
    SaveAudio,
    /// Close the wizard.
    Finish,
    /// Nothing to do.
    Stay,
}

fn forward(step: SetupStep) -> WizardMove {
    if step.is_last() {
        WizardMove::Finish
    } else {
        WizardMove::Goto(step as i32 + 1)
    }
}

pub(crate) fn on_next(step: i32) -> WizardMove {
    match SetupStep::from_index(step) {
        Some(SetupStep::Audio) => WizardMove::SaveAudio,
        Some(step) => forward(step),
        None => WizardMove::Stay,
    }
}

pub(crate) fn on_skip(step: i32) -> WizardMove {
    match SetupStep::from_index(step) {
        Some(step) if step.skippable() => forward(step),
        _ => WizardMove::Stay,
    }
}

pub(crate) fn on_back(step: i32) -> WizardMove {
    match SetupStep::from_index(step) {
        Some(step) if step as i32 > 0 => WizardMove::Goto(step as i32 - 1),
        _ => WizardMove::Stay,
    }
}

/// The step to show after the audio devices were saved, if the save came from
/// the wizard's audio step (Settings → Audio → Apply runs the same save).
pub(crate) fn after_audio_saved(wizard_visible: bool, step: i32) -> Option<i32> {
    (wizard_visible && SetupStep::from_index(step) == Some(SetupStep::Audio))
        .then_some(SetupStep::IoBindings as i32)
}

/// Whether a press on `step` takes the user off the I/O bindings step, the
/// moment the bindings made there are kept.
pub(crate) fn leaves_io_step(step: i32, mv: &WizardMove) -> bool {
    SetupStep::from_index(step) == Some(SetupStep::IoBindings)
        && matches!(mv, WizardMove::Goto(_) | WizardMove::Finish)
}

#[cfg(test)]
#[path = "setup_wizard_step_tests.rs"]
mod tests;
