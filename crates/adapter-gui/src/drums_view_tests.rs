use std::path::PathBuf;

use application::drums::{DrumKitEntry, DrumLibrary};
use application::drums_state::DrumsSnapshot;
use feature_dsp::drums::{DrumPattern, Groove};

use super::*;
use crate::metronome_view::ProjectOutput;

fn kit(id: &str, name: &str) -> DrumKitEntry {
    DrumKitEntry {
        id: id.into(),
        name: name.into(),
        dir: PathBuf::from(id),
    }
}

fn groove(id: &str, name: &str, genre: &str, beats_per_bar: u32) -> Groove {
    Groove {
        id: id.into(),
        name: name.into(),
        genre: genre.into(),
        beats_per_bar,
        tempo: 100.0,
        beat: DrumPattern::new(4.0, vec![]),
        fills: vec![],
    }
}

fn output(key: &str, label: &str) -> ProjectOutput {
    ProjectOutput {
        binding_id: "b".into(),
        endpoint: "e".into(),
        key: key.into(),
        label: label.into(),
        device_id: "dev".into(),
        channels: vec![0, 1],
        aliases: Vec::new(),
    }
}

fn library() -> DrumLibrary {
    DrumLibrary {
        kits: vec![
            kit("black-pearl", "Black Pearl"),
            kit("red", "Red Zeppelin"),
        ],
        grooves: vec![
            groove("rock-01", "Rock 1", "rock", 4),
            groove("jazz-01", "Jazz 1", "jazz", 3),
            groove("rock-02", "Rock 2", "rock", 4),
        ],
    }
}

fn snapshot() -> DrumsSnapshot {
    DrumsSnapshot {
        enabled: true,
        playing: true,
        bpm: 96.0,
        volume: 0.5,
        kit: Some("red".into()),
        groove: Some("jazz-01".into()),
        output_key: Some("b".into()),
    }
}

fn pick(key: &str, label: &str, header: bool) -> DrumPick {
    DrumPick {
        key: key.into(),
        label: label.into(),
        header,
    }
}

#[test]
fn every_installed_kit_is_a_choice_in_library_order() {
    let view = drums_view(&snapshot(), &library(), &[]);
    assert_eq!(
        view.kits,
        vec![
            pick("black-pearl", "Black Pearl", false),
            pick("red", "Red Zeppelin", false),
        ]
    );
}

#[test]
fn grooves_are_grouped_under_one_header_per_genre_in_first_seen_order() {
    let view = drums_view(&snapshot(), &library(), &[]);
    assert_eq!(
        view.grooves,
        vec![
            pick("", "rock", true),
            pick("rock-01", "Rock 1", false),
            pick("rock-02", "Rock 2", false),
            pick("", "jazz", true),
            pick("jazz-01", "Jazz 1", false),
        ]
    );
}

#[test]
fn the_chosen_kit_groove_and_output_show_by_name() {
    let outputs = [output("a", "Main · 1-2"), output("b", "Phones · L-R")];
    let view = drums_view(&snapshot(), &library(), &outputs);
    assert_eq!(
        (view.kit_key.as_str(), view.kit_label.as_str()),
        ("red", "Red Zeppelin")
    );
    assert_eq!(
        (view.groove_key.as_str(), view.groove_label.as_str()),
        ("jazz-01", "Jazz 1")
    );
    assert_eq!(
        (view.output_key.as_str(), view.output_label.as_str()),
        ("b", "Phones · L-R")
    );
    assert_eq!(
        view.outputs,
        vec![
            pick("a", "Main · 1-2", false),
            pick("b", "Phones · L-R", false)
        ]
    );
}

#[test]
fn the_bar_has_as_many_lamps_as_the_chosen_groove_has_beats() {
    let view = drums_view(&snapshot(), &library(), &[]);
    assert_eq!(view.beats_per_bar, 3);
}

#[test]
fn transport_tempo_and_level_come_through() {
    let view = drums_view(&snapshot(), &library(), &[]);
    assert!(view.enabled && view.playing);
    assert_eq!((view.bpm, view.volume), (96.0, 0.5));
}

#[test]
fn an_unsaved_output_shows_the_projects_first_endpoint() {
    let mut state = snapshot();
    state.output_key = None;
    let outputs = [output("a", "Main · 1-2"), output("b", "Phones · L-R")];
    let view = drums_view(&state, &library(), &outputs);
    assert_eq!(view.output_label, "Main · 1-2");
}

#[test]
fn an_empty_library_shows_nothing_chosen_and_a_four_beat_bar() {
    let state = DrumsSnapshot::default();
    let view = drums_view(&state, &DrumLibrary::default(), &[]);
    assert!(view.kits.is_empty() && view.grooves.is_empty());
    assert_eq!(
        (view.kit_label.as_str(), view.groove_label.as_str()),
        ("", "")
    );
    assert_eq!(view.output_label, "");
    assert_eq!(view.beats_per_bar, 4);
}
