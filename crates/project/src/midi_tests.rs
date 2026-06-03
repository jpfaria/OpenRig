//! Tests for the project-level MIDI binding data types (ADR 0003, #499).
//!
//! Bindings live in `project.openrig` so they travel with the rig. The data
//! types (`Source`, `Scale`, `Binding`) used to live in `adapter-midi`; this
//! suite covers the surface needed for a YAML round-trip inside a project
//! file. The runtime validator stays in `adapter-midi`.
use super::midi::{Binding, Scale, Source};

#[test]
fn source_serializes_with_lowercase_kind_tag() {
    let src = Source::NoteOn {
        channel: 1,
        note: 60,
    };
    let yaml = serde_yaml::to_string(&src).unwrap();
    // The existing wire format (matches `examples/midi-map.default.yaml`)
    // tags the variant with a lowercase, snake-case `kind` field.
    assert!(yaml.contains("kind: note_on"), "got: {yaml}");
    assert!(yaml.contains("channel: 1"), "got: {yaml}");
    assert!(yaml.contains("note: 60"), "got: {yaml}");
}

#[test]
fn cc_source_serializes_with_controller_field() {
    let src = Source::Cc {
        channel: 1,
        controller: 7,
    };
    let yaml = serde_yaml::to_string(&src).unwrap();
    assert!(yaml.contains("kind: cc"), "got: {yaml}");
    assert!(yaml.contains("controller: 7"), "got: {yaml}");
}

#[test]
fn binding_round_trips_through_yaml() {
    let yaml = r#"
source: { kind: note_on, channel: 1, note: 60 }
command: SaveProject
"#;
    let b: Binding = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(b.command, "SaveProject");
    assert_eq!(
        b.source,
        Source::NoteOn {
            channel: 1,
            note: 60
        }
    );

    // Round-trip: emit and re-parse, structurally identical.
    let back = serde_yaml::to_string(&b).unwrap();
    let again: Binding = serde_yaml::from_str(&back).unwrap();
    assert_eq!(b.command, again.command);
    assert_eq!(b.source, again.source);
}

#[test]
fn binding_with_scale_round_trips() {
    let yaml = r#"
source: { kind: cc, channel: 1, controller: 7 }
command: SetChainVolume
args: { chain: "rig:guitar" }
scale: { min: 0.0, max: 200.0, into: value }
"#;
    let b: Binding = serde_yaml::from_str(yaml).unwrap();
    assert!(b.scale.is_some());
    let s = b.scale.as_ref().unwrap();
    assert!((s.min - 0.0).abs() < 1e-9);
    assert!((s.max - 200.0).abs() < 1e-9);
    assert_eq!(s.into, "value");

    let back = serde_yaml::to_string(&b).unwrap();
    let again: Binding = serde_yaml::from_str(&back).unwrap();
    assert_eq!(b.command, again.command);
    assert_eq!(b.source, again.source);
    assert_eq!(
        b.scale.as_ref().unwrap().into,
        again.scale.as_ref().unwrap().into
    );
}

#[test]
fn scale_apply_is_linear_full_range() {
    let s = Scale {
        min: 0.0,
        max: 100.0,
        into: "value".into(),
    };
    assert!((s.apply(0) - 0.0).abs() < 1e-9);
    assert!((s.apply(127) - 100.0).abs() < 1e-9);
    assert!((s.apply(64) - 50.39).abs() < 0.5);
}

#[test]
fn source_is_continuous_only_for_cc() {
    assert!(!Source::NoteOn {
        channel: 1,
        note: 60
    }
    .is_continuous());
    assert!(!Source::NoteOff {
        channel: 1,
        note: 60
    }
    .is_continuous());
    assert!(!Source::ProgramChange { program: 0 }.is_continuous());
    assert!(Source::Cc {
        channel: 1,
        controller: 7
    }
    .is_continuous());
}
