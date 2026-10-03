//! The player controller's device-free side: loading, transport and liveness.

use super::*;
use std::path::Path;
use std::time::{Duration, Instant};

use engine::player::pcm::DecodedAudio;
use engine::player::settings::PlayerSettings;
use engine::runtime::RuntimeGraph;

fn fake_decode(path: &Path) -> Result<DecodedAudio, String> {
    match path.to_str() {
        Some("two-seconds") => Ok(DecodedAudio {
            samples: vec![0.1; 2 * 96_000],
            channels: 2,
            sample_rate: 48_000,
        }),
        _ => Err("unreadable".into()),
    }
}

fn controller() -> ProjectRuntimeController {
    ProjectRuntimeController::for_testing(RuntimeGraph {
        chains: Default::default(),
    })
}

fn wait_for(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

#[test]
fn nothing_is_loaded_at_first() {
    let controller = controller();
    assert_eq!(controller.player_track(), None);
    assert!(!controller.player_active());
}

#[test]
fn loading_a_track_publishes_its_duration() {
    let controller = controller();
    controller.load_player_track(Path::new("two-seconds"), fake_decode);
    assert_eq!(
        controller.player_track().as_deref(),
        Some(Path::new("two-seconds"))
    );
    let shared = controller.player_shared();
    assert!(wait_for(|| shared.duration_seconds() == 2.0));
    assert!(!shared.is_loading());
}

#[test]
fn an_unreadable_track_reports_failure() {
    let controller = controller();
    controller.load_player_track(Path::new("broken"), fake_decode);
    let shared = controller.player_shared();
    assert!(wait_for(|| shared.has_failed()));
}

#[test]
fn a_new_track_starts_paused() {
    let controller = controller();
    controller.player_shared().set_playing(true);
    controller.load_player_track(Path::new("two-seconds"), fake_decode);
    assert!(!controller.player_shared().is_playing());
}

#[test]
fn starting_without_a_track_is_refused() {
    let controller = controller();
    assert!(controller.start_player("any-device", &[0, 1]).is_err());
    assert!(!controller.player_active());
}

#[test]
fn pause_keeps_the_position() {
    let controller = controller();
    controller.load_player_track(Path::new("two-seconds"), fake_decode);
    controller.seek_player(1.0);
    controller.pause_player();
    let shared = controller.player_shared();
    assert!(!shared.is_playing());
    assert!(wait_for(|| shared.position_seconds() == 1.0));
}

#[test]
fn stop_rewinds_to_the_start() {
    let controller = controller();
    controller.load_player_track(Path::new("two-seconds"), fake_decode);
    controller.seek_player(1.5);
    let shared = controller.player_shared();
    assert!(wait_for(|| shared.position_seconds() == 1.5));
    controller.stop_player();
    assert!(!shared.is_playing());
    assert!(wait_for(|| shared.position_seconds() == 0.0));
}

#[test]
fn settings_reach_the_shared_state() {
    let controller = controller();
    controller.set_player_settings(PlayerSettings {
        speed: 0.75,
        semitones: -2.0,
        ..PlayerSettings::default()
    });
    let settings = controller.player_shared().settings();
    assert_eq!(settings.speed, 0.75);
    assert_eq!(settings.semitones, -2.0);
}

#[test]
fn a_loaded_but_closed_player_does_not_keep_the_runtime_alive() {
    let controller = controller();
    controller.load_player_track(Path::new("two-seconds"), fake_decode);
    assert!(!controller.is_running());
}
