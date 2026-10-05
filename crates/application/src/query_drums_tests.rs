//! The drums read: the dispatcher's settings, the library and the live beat.

use std::path::PathBuf;

use feature_dsp::drums::{DrumPattern, DrumPosition, Groove};

use super::drums_state_json;
use crate::drums::{DrumKitEntry, DrumLibrary};
use crate::drums_state::DrumsSnapshot;

fn library() -> DrumLibrary {
    DrumLibrary {
        kits: vec![DrumKitEntry {
            id: "one".into(),
            name: "Kit One".into(),
            dir: PathBuf::from("/kits/one"),
        }],
        grooves: vec![Groove {
            id: "rock-01".into(),
            name: "Rock 1".into(),
            genre: "rock".into(),
            beats_per_bar: 4,
            tempo: 100.0,
            beat: DrumPattern::new(4.0, Vec::new()),
            fills: vec![DrumPattern::new(4.0, Vec::new()); 2],
        }],
    }
}

fn snapshot() -> DrumsSnapshot {
    DrumsSnapshot {
        enabled: true,
        playing: true,
        bpm: 96.0,
        volume: 0.5,
        kit: Some("one".into()),
        groove: Some("rock-01".into()),
        output_key: None,
    }
}

#[test]
fn lists_the_settings_the_library_and_the_live_position() {
    let live = DrumPosition {
        playing: true,
        bar: 3,
        beat: 2,
        in_fill: true,
    };
    let json: serde_json::Value =
        serde_json::from_str(&drums_state_json(&snapshot(), &library(), Some(live))).unwrap();

    assert_eq!(json["playing"], true);
    assert_eq!(json["bpm"], 96.0);
    assert_eq!(json["kit"], "one");
    assert_eq!(json["groove"], "rock-01");
    assert_eq!(json["bar"], 3);
    assert_eq!(json["beat"], 2);
    assert_eq!(json["in_fill"], true);
    assert_eq!(json["kits"][0]["id"], "one");
    assert_eq!(json["kits"][0]["name"], "Kit One");
    assert_eq!(json["grooves"][0]["id"], "rock-01");
    assert_eq!(json["grooves"][0]["genre"], "rock");
    assert_eq!(json["grooves"][0]["fills"], 2);
}

#[test]
fn the_live_side_wins_on_whether_anything_plays() {
    let stopped = DrumPosition::default();
    let json: serde_json::Value =
        serde_json::from_str(&drums_state_json(&snapshot(), &library(), Some(stopped))).unwrap();
    assert_eq!(json["playing"], false);
}

#[test]
fn with_no_runtime_the_control_state_answers() {
    let json: serde_json::Value =
        serde_json::from_str(&drums_state_json(&snapshot(), &library(), None)).unwrap();
    assert_eq!(json["playing"], true);
    assert_eq!(json["bar"], 0);
    assert_eq!(json["in_fill"], false);
}
