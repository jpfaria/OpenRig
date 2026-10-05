use super::*;

fn library() -> PlayerLibraryDirs {
    PlayerLibraryDirs {
        bundled: Some("/app/assets/backing-tracks".into()),
        user: Some("/home/me/backing-tracks".into()),
    }
}

#[test]
fn a_restored_state_takes_the_persisted_volume_and_output() {
    let state = PlayerControlState::restored(
        &PlayerConfig {
            volume: 0.3,
            output_device: Some("binding-a\u{1f}main".into()),
        },
        library(),
        Some("/cfg/config.yaml".into()),
    );
    assert_eq!(state.settings().volume, 0.3);
    assert_eq!(state.output_key(), Some("binding-a\u{1f}main"));
    assert_eq!(state.library(), library());
    assert_eq!(state.config_path(), Some(PathBuf::from("/cfg/config.yaml")));
    assert!(!state.playing());
    assert_eq!(state.track(), None);
}

#[test]
fn a_restored_volume_out_of_range_is_clamped() {
    let state = PlayerControlState::restored(
        &PlayerConfig {
            volume: 7.0,
            output_device: None,
        },
        library(),
        None,
    );
    assert_eq!(state.settings().volume, 1.0);
}

#[test]
fn a_new_track_stops_playback_and_clears_the_loop() {
    let mut state = PlayerControlState::default();
    state.set_playing(true);
    state.update_settings(|s| s.loop_range = Some((1.0, 2.0)));
    state.set_track("/tracks/a.wav".into());
    assert_eq!(state.track(), Some(Path::new("/tracks/a.wav")));
    assert!(!state.playing());
    assert_eq!(state.settings().loop_range, None);
}

#[test]
fn settings_edits_are_clamped() {
    let mut state = PlayerControlState::default();
    let stored = state.update_settings(|s| {
        s.speed = 9.0;
        s.semitones = -40.0;
    });
    assert_eq!(stored.speed, 2.0);
    assert_eq!(stored.semitones, -12.0);
    assert_eq!(state.settings(), stored);
}

#[test]
fn changing_the_user_folder_keeps_the_bundled_one() {
    let mut state = PlayerControlState::restored(&PlayerConfig::default(), library(), None);
    state.set_user_dir(Some("/elsewhere".into()));
    assert_eq!(
        state.library(),
        PlayerLibraryDirs {
            bundled: Some("/app/assets/backing-tracks".into()),
            user: Some("/elsewhere".into()),
        }
    );
}
