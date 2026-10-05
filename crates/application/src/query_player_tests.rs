use super::*;
use engine::player::settings::PlayerSettings;

fn parse(json: &str) -> serde_json::Value {
    serde_json::from_str(json).expect("valid JSON")
}

fn snapshot() -> PlayerSnapshot {
    PlayerSnapshot {
        track: Some("/tracks/blues.m4a".into()),
        settings: PlayerSettings {
            volume: 0.5,
            speed: 0.75,
            semitones: -2.0,
            loop_range: Some((4.0, 12.0)),
        },
        output_key: Some("binding-a\u{1f}main".into()),
        playing: true,
    }
}

#[test]
fn the_dispatchers_settings_are_reported() {
    let value = parse(&player_state_json(
        &snapshot(),
        None,
        &PlayerLibraryDirs::default(),
    ));
    assert_eq!(value["track"], "/tracks/blues.m4a");
    assert_eq!(value["volume"], 0.5);
    assert_eq!(value["speed"], 0.75);
    assert_eq!(value["semitones"], -2.0);
    assert_eq!(value["loop"]["start"], 4.0);
    assert_eq!(value["loop"]["end"], 12.0);
    assert_eq!(value["output"], "binding-a\u{1f}main");
}

#[test]
fn without_a_hosted_player_the_position_is_the_start() {
    let value = parse(&player_state_json(
        &snapshot(),
        None,
        &PlayerLibraryDirs::default(),
    ));
    assert_eq!(value["playing"], true);
    assert_eq!(value["position_seconds"], 0.0);
    assert_eq!(value["duration_seconds"], 0.0);
    assert_eq!(value["loading"], false);
    assert_eq!(value["failed"], false);
}

#[test]
fn the_live_stream_wins_over_the_record() {
    let live = PlayerReading {
        playing: false,
        position_seconds: 31.5,
        duration_seconds: 180.0,
        loading: false,
        failed: false,
    };
    let value = parse(&player_state_json(
        &snapshot(),
        Some(live),
        &PlayerLibraryDirs::default(),
    ));
    assert_eq!(value["playing"], false);
    assert_eq!(value["position_seconds"], 31.5);
    assert_eq!(value["duration_seconds"], 180.0);
}

#[test]
fn nothing_loaded_reads_as_nulls() {
    let value = parse(&player_state_json(
        &PlayerSnapshot::default(),
        None,
        &PlayerLibraryDirs::default(),
    ));
    assert!(value["track"].is_null());
    assert!(value["loop"].is_null());
    assert!(value["output"].is_null());
    assert_eq!(value["library"], serde_json::json!([]));
}

#[test]
fn the_library_lists_every_loadable_track() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Jam.wav"), b"x").unwrap();
    let value = parse(&player_state_json(
        &PlayerSnapshot::default(),
        None,
        &PlayerLibraryDirs {
            bundled: None,
            user: Some(dir.path().into()),
        },
    ));
    assert_eq!(value["library"][0]["name"], "Jam");
    assert_eq!(value["library"][0]["bundled"], false);
    assert_eq!(
        value["library"][0]["path"],
        dir.path().join("Jam.wav").to_string_lossy().as_ref()
    );
}
