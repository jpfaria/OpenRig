//! #913 — the capabilities each runtime mode grants.
//!
//! Every frontend asks this type what it may offer: a controller must not show
//! an audio-device picker (it owns no audio), a VST3 plugin must not either
//! (the DAW owns it), and touch optimisation follows the mode except in
//! standalone, where the user's interaction mode decides.

use super::*;

fn ctx(mode: AppRuntimeMode, interaction: InteractionMode) -> UiRuntimeContext {
    UiRuntimeContext::new(mode, interaction)
}

#[test]
fn standalone_owns_its_audio_and_picks_its_device() {
    let c = ctx(AppRuntimeMode::Standalone, InteractionMode::Mouse);
    assert!(c.capabilities.uses_local_audio);
    assert!(c.capabilities.can_select_audio_device);
    assert!(!c.capabilities.can_select_remote_host);
    assert!(!c.capabilities.hosted_by_daw);
}

#[test]
fn standalone_follows_the_interaction_mode_for_touch() {
    assert!(
        !ctx(AppRuntimeMode::Standalone, InteractionMode::Mouse)
            .capabilities
            .touch_optimized
    );
    assert!(
        ctx(AppRuntimeMode::Standalone, InteractionMode::Touch)
            .capabilities
            .touch_optimized
    );
}

#[test]
fn the_pedalboard_is_touch_optimized_even_driven_by_a_mouse() {
    // The hardware is a touchscreen; a mouse plugged in for setup does not
    // turn the targets back into desktop-sized ones.
    let c = ctx(AppRuntimeMode::Pedalboard, InteractionMode::Mouse);
    assert!(c.capabilities.touch_optimized);
    assert!(c.capabilities.uses_local_audio);
}

#[test]
fn a_controller_owns_no_audio_and_reaches_a_remote_host_instead() {
    let c = ctx(AppRuntimeMode::Controller, InteractionMode::Touch);
    assert!(!c.capabilities.uses_local_audio);
    assert!(
        !c.capabilities.can_select_audio_device,
        "a controller has no device to pick — it drives someone else's rig"
    );
    assert!(c.capabilities.can_select_remote_host);
    assert!(c.capabilities.touch_optimized);
}

#[test]
fn a_vst3_plugin_lets_the_daw_own_everything() {
    let c = ctx(AppRuntimeMode::Vst3Plugin, InteractionMode::Touch);
    assert!(c.capabilities.hosted_by_daw);
    assert!(!c.capabilities.uses_local_audio);
    assert!(!c.capabilities.can_select_audio_device);
    assert!(!c.capabilities.can_select_remote_host);
    assert!(
        !c.capabilities.touch_optimized,
        "a plugin window lives in a desktop DAW, whatever the interaction mode says"
    );
}

#[test]
fn exactly_one_mode_hosts_in_a_daw_and_exactly_one_reaches_a_remote_host() {
    let modes = [
        AppRuntimeMode::Standalone,
        AppRuntimeMode::Pedalboard,
        AppRuntimeMode::Controller,
        AppRuntimeMode::Vst3Plugin,
    ];
    let caps: Vec<_> = modes
        .iter()
        .map(|m| ctx(*m, InteractionMode::Mouse).capabilities)
        .collect();
    assert_eq!(caps.iter().filter(|c| c.hosted_by_daw).count(), 1);
    assert_eq!(caps.iter().filter(|c| c.can_select_remote_host).count(), 1);
    for c in &caps {
        assert!(
            !(c.uses_local_audio && c.hosted_by_daw),
            "owning the audio and being hosted by a DAW are exclusive"
        );
        assert!(
            c.can_select_audio_device <= c.uses_local_audio,
            "only a mode that owns audio may pick a device"
        );
    }
}

#[test]
fn every_mode_and_interaction_carries_a_label() {
    for m in [
        AppRuntimeMode::Standalone,
        AppRuntimeMode::Pedalboard,
        AppRuntimeMode::Controller,
        AppRuntimeMode::Vst3Plugin,
    ] {
        assert!(!m.label().is_empty());
    }
    assert_eq!(InteractionMode::Mouse.label(), "Mouse");
    assert_eq!(InteractionMode::Touch.label(), "Touch");
}

#[test]
fn a_context_round_trips_through_yaml_in_snake_case() {
    let c = ctx(AppRuntimeMode::Vst3Plugin, InteractionMode::Touch);
    let yaml = serde_yaml::to_string(&c).expect("serialize");
    assert!(
        yaml.contains("vst3_plugin"),
        "the wire form is snake_case, not the Rust variant name: {yaml}"
    );
    let back: UiRuntimeContext = serde_yaml::from_str(&yaml).expect("deserialize");
    assert_eq!(back, c);
}
