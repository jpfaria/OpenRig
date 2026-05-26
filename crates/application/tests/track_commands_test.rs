//! RED-first coverage for the remaining #553 track commands.

use application::command::Command;
use application::command_schema::command_variant_names;

fn names() -> &'static [&'static str] {
    command_variant_names()
}

#[test]
fn track_lifecycle_commands_exist() {
    for variant in ["LoadTrack", "UnloadTrack", "RenameTrack", "DeleteTrack"] {
        assert!(
            names().contains(&variant),
            "{variant} must be a Command variant, got {:?}",
            names()
        );
    }
}

#[test]
fn track_transport_commands_exist() {
    for variant in ["TrackPlay", "TrackPause", "TrackSeek"] {
        assert!(
            names().contains(&variant),
            "{variant} must be a Command variant, got {:?}",
            names()
        );
    }
}

#[test]
fn per_stem_control_commands_exist() {
    for variant in ["SetStemMute", "SetStemSolo", "SetStemGain", "SetStemPan"] {
        assert!(
            names().contains(&variant),
            "{variant} must be a Command variant, got {:?}",
            names()
        );
    }
}

#[test]
fn load_track_carries_track_id_payload() {
    let cmd = Command::LoadTrack {
        track_id: "01HXEMPTEST".to_string(),
    };
    let json = serde_json::to_value(&cmd).expect("serialize");
    assert_eq!(json["LoadTrack"]["track_id"], "01HXEMPTEST");
}

#[test]
fn set_stem_gain_carries_index_and_value_payload() {
    let cmd = Command::SetStemGain {
        stem_index: 2,
        gain: 0.75,
    };
    let json = serde_json::to_value(&cmd).expect("serialize");
    assert_eq!(json["SetStemGain"]["stem_index"], 2);
    assert!((json["SetStemGain"]["gain"].as_f64().unwrap() - 0.75).abs() < 1e-6);
}

#[test]
fn track_seek_carries_position_payload() {
    let cmd = Command::TrackSeek { secs: 12.5 };
    let json = serde_json::to_value(&cmd).expect("serialize");
    assert!((json["TrackSeek"]["secs"].as_f64().unwrap() - 12.5).abs() < 1e-6);
}
