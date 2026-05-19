//! Tests for `RigProject.midi` — project-owned MIDI bindings (ADR 0003 / #499).
//!
//! Bindings live in the project so they travel with the `.openrig` file: same
//! setlist, same behavior on every machine. The system layer (device profile,
//! fallback bindings) is covered by `infra-filesystem` and the resolver.

use crate::midi::{Binding, RigProjectMidi, Source};
use crate::rig::RigProject;
use std::collections::BTreeMap;

fn empty_project() -> RigProject {
    RigProject {
        name: None,
        inputs: BTreeMap::new(),
        outputs: BTreeMap::new(),
        presets: BTreeMap::new(),
        midi: None,
    }
}

fn sample_binding() -> Binding {
    Binding {
        source: Source::NoteOn {
            channel: 1,
            note: 60,
        },
        command: "SaveProject".to_string(),
        args: serde_json::Value::Null,
        scale: None,
    }
}

#[test]
fn project_without_midi_field_serializes_without_key() {
    // A project that has no `midi:` field must round-trip without one — pre-#499
    // projects keep parsing unchanged and never grow a phantom `midi: null`.
    let project = empty_project();
    let yaml = serde_yaml::to_string(&project).unwrap();
    assert!(!yaml.contains("midi:"), "should not emit midi key: {yaml}");
}

#[test]
fn project_round_trips_with_midi_bindings() {
    let project = RigProject {
        midi: Some(RigProjectMidi {
            bindings: vec![sample_binding()],
        }),
        ..empty_project()
    };
    let yaml = serde_yaml::to_string(&project).unwrap();
    assert!(yaml.contains("midi:"), "must emit midi key: {yaml}");
    assert!(
        yaml.contains("command: SaveProject"),
        "must emit binding: {yaml}"
    );

    let back: RigProject = serde_yaml::from_str(&yaml).unwrap();
    let midi = back.midi.expect("midi survives round-trip");
    assert_eq!(midi.bindings.len(), 1);
    assert_eq!(midi.bindings[0].command, "SaveProject");
    assert_eq!(
        midi.bindings[0].source,
        Source::NoteOn {
            channel: 1,
            note: 60,
        }
    );
}

#[test]
fn pre_499_project_yaml_parses_with_midi_none() {
    // A YAML doc without any `midi:` key (every project file shipped before
    // #499) must still parse and yield `midi == None`. Backward-compat is a
    // hard requirement of ADR 0003.
    let yaml = r#"
name: Pre-499 Project
inputs: {}
outputs: {}
presets: {}
"#;
    let project: RigProject = serde_yaml::from_str(yaml).unwrap();
    assert!(project.midi.is_none());
}

#[test]
fn midi_bindings_field_round_trips_multiple_bindings() {
    let midi = RigProjectMidi {
        bindings: vec![
            sample_binding(),
            Binding {
                source: Source::Cc {
                    channel: 1,
                    controller: 7,
                },
                command: "SetChainVolume".to_string(),
                args: serde_json::json!({ "chain": "rig:guitar" }),
                scale: Some(crate::midi::Scale {
                    min: 0.0,
                    max: 200.0,
                    into: "value".to_string(),
                }),
            },
        ],
    };

    let yaml = serde_yaml::to_string(&midi).unwrap();
    let back: RigProjectMidi = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(back.bindings.len(), 2);
    assert_eq!(back.bindings[1].command, "SetChainVolume");
    assert!(back.bindings[1].scale.is_some());
}
