//! where each button of the first-run setup wizard leads.
//!
//! The wizard walks Language → Audio interface → I/O → TONE3000. Audio is the
//! one step that cannot be passed without saving: a rig with no device opens
//! silent. TONE3000 is optional and can be skipped.

use super::{
    after_audio_saved, enters_audio_step, leaves_io_step, on_back, on_next, on_skip, SetupStep,
    WizardMove, STEP_COUNT,
};

#[test]
fn steps_run_in_the_agreed_order() {
    let order: Vec<SetupStep> = (0..STEP_COUNT).filter_map(SetupStep::from_index).collect();
    assert_eq!(
        order,
        vec![
            SetupStep::Language,
            SetupStep::Audio,
            SetupStep::IoBindings,
            SetupStep::Tone3000,
        ]
    );
    assert_eq!(SetupStep::from_index(STEP_COUNT), None);
    assert_eq!(SetupStep::from_index(-1), None);
}

#[test]
fn next_on_language_goes_to_audio() {
    assert_eq!(on_next(SetupStep::Language as i32), WizardMove::Goto(1));
}

#[test]
fn next_on_audio_saves_the_devices_instead_of_moving() {
    assert_eq!(on_next(SetupStep::Audio as i32), WizardMove::SaveAudio);
}

#[test]
fn next_on_the_io_step_moves_to_tone3000() {
    assert_eq!(on_next(SetupStep::IoBindings as i32), WizardMove::Goto(3));
}

#[test]
fn next_on_the_last_step_finishes() {
    assert_eq!(on_next(SetupStep::Tone3000 as i32), WizardMove::Finish);
}

#[test]
fn only_tone3000_can_be_skipped() {
    assert!(!SetupStep::Language.skippable());
    assert!(!SetupStep::Audio.skippable());
    assert!(!SetupStep::IoBindings.skippable());
    assert!(SetupStep::Tone3000.skippable());
}

#[test]
fn skip_moves_past_an_optional_step() {
    assert_eq!(on_skip(SetupStep::Tone3000 as i32), WizardMove::Finish);
}

#[test]
fn skip_on_a_required_step_does_nothing() {
    assert_eq!(on_skip(SetupStep::Audio as i32), WizardMove::Stay);
    assert_eq!(on_skip(SetupStep::IoBindings as i32), WizardMove::Stay);
    assert_eq!(on_skip(SetupStep::Language as i32), WizardMove::Stay);
}

#[test]
fn back_goes_to_the_previous_step_and_stops_at_the_first() {
    assert_eq!(on_back(SetupStep::Audio as i32), WizardMove::Goto(0));
    assert_eq!(on_back(SetupStep::Tone3000 as i32), WizardMove::Goto(2));
    assert_eq!(on_back(SetupStep::Language as i32), WizardMove::Stay);
}

#[test]
fn an_unknown_step_never_moves() {
    assert_eq!(on_next(99), WizardMove::Stay);
    assert_eq!(on_skip(-1), WizardMove::Stay);
    assert_eq!(on_back(99), WizardMove::Stay);
}

#[test]
fn saving_audio_inside_the_wizard_advances_to_io() {
    assert_eq!(after_audio_saved(true, SetupStep::Audio as i32), Some(2));
}

#[test]
fn saving_audio_from_settings_leaves_the_wizard_alone() {
    // Settings → Audio → Apply runs the same save; the wizard is closed then.
    assert_eq!(after_audio_saved(false, SetupStep::Audio as i32), None);
    assert_eq!(after_audio_saved(true, SetupStep::Tone3000 as i32), None);
}

#[test]
fn moving_on_from_the_io_step_leaves_it() {
    assert!(leaves_io_step(2, &on_next(2)));
}

#[test]
fn going_back_from_the_io_step_leaves_it() {
    assert!(leaves_io_step(2, &on_back(2)));
}

#[test]
fn moves_on_other_steps_never_leave_the_io_step() {
    for step in [0, 1, 3] {
        assert!(!leaves_io_step(step, &on_next(step)), "step {step}");
        assert!(!leaves_io_step(step, &on_back(step)), "step {step}");
    }
}

#[test]
fn staying_on_the_io_step_does_not_leave_it() {
    assert!(!leaves_io_step(2, &WizardMove::Stay));
}

#[test]
fn the_wizard_has_no_midi_step() {
    assert_eq!(STEP_COUNT, 4);
    assert!(SetupStep::Tone3000.is_last());
}

#[test]
fn moving_to_the_audio_step_enters_it() {
    // Entering the audio step is when the app looks at the hardware: the
    // device list is enumerated then, never at boot.
    assert!(enters_audio_step(&on_next(SetupStep::Language as i32)));
    assert!(enters_audio_step(&on_back(SetupStep::IoBindings as i32)));
}

#[test]
fn other_moves_never_enter_the_audio_step() {
    assert!(!enters_audio_step(&on_next(SetupStep::IoBindings as i32)));
    assert!(!enters_audio_step(&on_back(SetupStep::Audio as i32)));
    assert!(!enters_audio_step(&WizardMove::SaveAudio));
    assert!(!enters_audio_step(&WizardMove::Finish));
    assert!(!enters_audio_step(&WizardMove::Stay));
}
