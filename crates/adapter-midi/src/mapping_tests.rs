use super::*;

fn write_tmp(name: &str, body: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("openrig-midimap-{name}.yaml"));
    std::fs::write(&p, body).unwrap();
    p
}

#[test]
fn parses_all_source_kinds_and_optional_input() {
    let p = write_tmp(
        "kinds",
        r#"
input: Chocolate
bindings:
  - source: { kind: note_on, channel: 1, note: 60 }
    command: ToggleBlockEnabled
    args: { chain: "chain:a", block: "block:b" }
  - source: { kind: program_change, program: 5 }
    command: SaveProject
  - source: { kind: cc, channel: 1, controller: 7 }
    command: SetBlockParameterNumber
    args: { chain: "chain:a", block: "block:b", path: gain }
    scale: { min: 0.0, max: 100.0 }
"#,
    );
    let map = MidiMap::load(&p).unwrap();
    assert_eq!(map.input.as_deref(), Some("Chocolate"));
    assert_eq!(map.bindings.len(), 3);
    assert_eq!(
        map.bindings[0].source,
        Source::NoteOn {
            channel: 1,
            note: 60
        }
    );
    assert!(map.bindings[2].source.is_continuous());
    assert_eq!(map.bindings[2].scale.as_ref().unwrap().into, "value");
}

#[test]
fn default_input_is_none() {
    let p = write_tmp("noinput", "bindings: []\n");
    let map = MidiMap::load(&p).unwrap();
    assert!(map.input.is_none());
    assert!(map.bindings.is_empty());
}

#[test]
fn load_rejects_unknown_command() {
    let p = write_tmp(
        "unknown",
        r#"
bindings:
  - source: { kind: note_on, channel: 1, note: 60 }
    command: NotARealCommand
"#,
    );
    let err = MidiMap::load(&p).unwrap_err().to_string();
    assert!(err.contains("binding #0"), "{err}");
}

#[test]
fn load_rejects_args_violating_command_schema() {
    // ToggleBlockEnabled needs string ids; a number fails the schema.
    let p = write_tmp(
        "badargs",
        r#"
bindings:
  - source: { kind: note_on, channel: 1, note: 60 }
    command: ToggleBlockEnabled
    args: { chain: 0, block: 1 }
"#,
    );
    let err = MidiMap::load(&p).unwrap_err().to_string();
    assert!(err.contains("binding #0"), "{err}");
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
fn scaled_continuous_binding_validates_with_probe_value() {
    // Args omit `value`; validation must inject the scaled probe so the
    // schema check passes (the value arrives at runtime from the pedal).
    let p = write_tmp(
        "scaled",
        r#"
bindings:
  - source: { kind: cc, channel: 1, controller: 7 }
    command: SetBlockParameterNumber
    args: { chain: "chain:a", block: "block:b", path: gain }
    scale: { min: 0.0, max: 100.0 }
"#,
    );
    assert!(MidiMap::load(&p).is_ok());
}
