//! #913 — the two answers boot reads before any window exists.
//!
//! Both are visible on the first frame: whether the user is shown the audio
//! wizard, and what rate the VST3 catalog is scanned at.

use super::{needs_audio_settings, vst3_sample_rate};
use infra_filesystem::{GuiAudioDeviceSettings, GuiSystemSettings};
use ui_openrig::{AppRuntimeMode, InteractionMode, UiRuntimeContext};

fn device(rate: u32) -> GuiAudioDeviceSettings {
    GuiAudioDeviceSettings {
        device_id: "dev".into(),
        name: "Device".into(),
        sample_rate: rate,
        ..Default::default()
    }
}

fn settings(
    inputs: Vec<GuiAudioDeviceSettings>,
    outputs: Vec<GuiAudioDeviceSettings>,
) -> GuiSystemSettings {
    GuiSystemSettings {
        input_devices: inputs,
        output_devices: outputs,
        ..Default::default()
    }
}

fn standalone() -> UiRuntimeContext {
    UiRuntimeContext::new(AppRuntimeMode::Standalone, InteractionMode::Mouse)
}

fn controller() -> UiRuntimeContext {
    UiRuntimeContext::new(AppRuntimeMode::Controller, InteractionMode::Touch)
}

fn hosted_by_a_daw() -> UiRuntimeContext {
    UiRuntimeContext::new(AppRuntimeMode::Vst3Plugin, InteractionMode::Mouse)
}

#[test]
fn a_first_launch_that_owns_its_audio_opens_the_wizard() {
    assert!(needs_audio_settings(
        &standalone(),
        &settings(vec![], vec![])
    ));
}

#[test]
fn settings_with_both_sides_configured_skip_the_wizard() {
    assert!(!needs_audio_settings(
        &standalone(),
        &settings(vec![device(48_000)], vec![device(48_000)])
    ));
}

#[test]
fn settings_with_only_one_side_configured_still_open_the_wizard() {
    assert!(needs_audio_settings(
        &standalone(),
        &settings(vec![device(48_000)], vec![])
    ));
    assert!(needs_audio_settings(
        &standalone(),
        &settings(vec![], vec![device(48_000)])
    ));
}

#[test]
fn a_frontend_that_owns_no_audio_never_opens_the_wizard() {
    for context in [controller(), hosted_by_a_daw()] {
        assert!(
            !needs_audio_settings(&context, &settings(vec![], vec![])),
            "asking for a device choice that changes nothing"
        );
    }
}

#[test]
fn the_vst3_catalog_follows_the_configured_input_rate() {
    assert_eq!(
        vst3_sample_rate(&settings(vec![device(44_100)], vec![])),
        44_100.0,
        "scanning at a rate the user did not pick is how a plugin ends up \
         detuned or aliased"
    );
}

#[test]
fn the_first_input_decides_when_several_are_configured() {
    assert_eq!(
        vst3_sample_rate(&settings(vec![device(96_000), device(44_100)], vec![])),
        96_000.0
    );
}

#[test]
fn with_no_input_configured_yet_the_catalog_uses_the_fallback_rate() {
    assert_eq!(vst3_sample_rate(&settings(vec![], vec![])), 48_000.0);
}
