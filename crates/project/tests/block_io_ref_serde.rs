//! Task 5 — chain Input/Output blocks gain `io` + `endpoint` reference fields.
//!
//! Two tests:
//!   1. `new_schema_round_trips` — a block using the new `io`/`endpoint` fields
//!      serializes and deserializes back equal (serde round-trip).
//!   2. `legacy_entries_still_deserialize` — clean break (#716): `entries` is
//!      NEVER serialized (new projects persist only `io`/`endpoint`), but an
//!      old YAML that still carries `entries:` deserializes without error so a
//!      legacy project loads (it simply opens unbound).

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};

// ---------------------------------------------------------------------------
// Test 1: new-schema round-trip
// ---------------------------------------------------------------------------

#[test]
fn new_schema_round_trips() {
    let block = AudioBlock {
        id: BlockId("chain:in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "main".into(),
            endpoint: "In1".into(),
            entries: vec![],
        }),
    };

    let yaml = serde_yaml::to_string(&block).expect("serialize");
    let back: AudioBlock = serde_yaml::from_str(&yaml).expect("deserialize");

    assert_eq!(block, back, "round-trip must produce an identical block");

    // The new fields appear in the serialised form.
    assert!(
        yaml.contains("io: main"),
        "io field must be present: {yaml}"
    );
    assert!(
        yaml.contains("endpoint: In1"),
        "endpoint field must be present: {yaml}"
    );

    // Legacy `entries` must NOT be emitted for a block that uses the new schema.
    assert!(
        !yaml.contains("entries:"),
        "entries must be absent when empty: {yaml}"
    );
}

#[test]
fn new_schema_round_trips_output() {
    let block = AudioBlock {
        id: BlockId("chain:out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "main".into(),
            endpoint: "Out1".into(),
            entries: vec![],
        }),
    };

    let yaml = serde_yaml::to_string(&block).expect("serialize");
    let back: AudioBlock = serde_yaml::from_str(&yaml).expect("deserialize");

    assert_eq!(
        block, back,
        "round-trip must produce an identical output block"
    );
    assert!(
        yaml.contains("io: main"),
        "io field must be present: {yaml}"
    );
    assert!(
        yaml.contains("endpoint: Out1"),
        "endpoint field must be present: {yaml}"
    );
}

// ---------------------------------------------------------------------------
// Test 2: clean break (#716) — `entries` never serializes, but an old YAML
// that still carries `entries:` deserializes so a legacy project loads.
// ---------------------------------------------------------------------------

/// An InputBlock with legacy `entries` must NOT emit `entries:` on serialize
/// (new projects persist only `io`/`endpoint`), yet an old YAML carrying
/// `entries:` must still deserialize without error.
#[test]
fn legacy_entries_still_deserialize() {
    use domain::ids::DeviceId;
    use project::block::InputEntry;
    use project::chain::ChainInputMode;

    let legacy = InputBlock {
        model: "standard".into(),
        io: String::new(),
        endpoint: String::new(),
        entries: vec![InputEntry {
            device_id: DeviceId("coreaudio:default".into()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        }],
    };
    let block = AudioBlock {
        id: BlockId("chain:in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(legacy),
    };

    // Clean break: `entries` is NEVER serialized.
    let yaml = serde_yaml::to_string(&block).expect("block must serialise");
    assert!(
        !yaml.contains("entries:"),
        "entries must NOT be serialized (clean break #716): {yaml}"
    );

    // An OLD YAML still carrying `entries:` must deserialize so a legacy
    // project loads — the values are ignored for routing (chain opens unbound).
    let old_yaml = "\
id: chain:in
enabled: true
kind: !Input
  model: standard
  entries:
  - device_id: coreaudio:default
    mode: mono
    channels:
    - 0
";
    let back: AudioBlock = serde_yaml::from_str(old_yaml).expect("legacy YAML must deserialise");
    let AudioBlockKind::Input(ref ib) = back.kind else {
        panic!("expected Input block, got {:?}", back.kind);
    };
    assert!(
        ib.io.is_empty(),
        "legacy block has no binding — opens unbound"
    );
    assert_eq!(
        ib.entries.len(),
        1,
        "legacy entries deserialize so the project still loads"
    );
    assert_eq!(
        ib.entries[0].device_id,
        DeviceId("coreaudio:default".into())
    );
}

#[test]
fn legacy_output_entries_still_deserialize() {
    use domain::ids::DeviceId;
    use project::block::OutputEntry;
    use project::chain::ChainOutputMode;

    let legacy = OutputBlock {
        model: "standard".into(),
        io: String::new(),
        endpoint: String::new(),
        entries: vec![OutputEntry {
            device_id: DeviceId("coreaudio:default".into()),
            mode: ChainOutputMode::Stereo,
            channels: vec![0, 1],
        }],
    };
    let block = AudioBlock {
        id: BlockId("chain:out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(legacy),
    };

    let yaml = serde_yaml::to_string(&block).expect("block must serialise");
    assert!(
        !yaml.contains("entries:"),
        "entries must NOT be serialized (clean break #716): {yaml}"
    );

    let old_yaml = "\
id: chain:out
enabled: true
kind: !Output
  model: standard
  entries:
  - device_id: coreaudio:default
    mode: stereo
    channels:
    - 0
    - 1
";
    let back: AudioBlock =
        serde_yaml::from_str(old_yaml).expect("legacy output YAML must deserialise");
    let AudioBlockKind::Output(ref ob) = back.kind else {
        panic!("expected Output block, got {:?}", back.kind);
    };
    assert!(
        ob.io.is_empty(),
        "legacy block has no binding — opens unbound"
    );
    assert_eq!(
        ob.entries.len(),
        1,
        "legacy output entries deserialize so the project still loads"
    );
    assert_eq!(
        ob.entries[0].device_id,
        DeviceId("coreaudio:default".into())
    );
}
