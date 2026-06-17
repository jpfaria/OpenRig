//! Task 5 — chain Input/Output blocks gain `io` + `endpoint` reference fields.
//!
//! Two tests:
//!   1. `new_schema_round_trips` — a block using the new `io`/`endpoint` fields
//!      serializes and deserializes back equal (serde round-trip).
//!   2. `legacy_entries_still_deserialize` — a block YAML that only has the old
//!      `entries` field (no `io`/`endpoint`) deserializes without error and the
//!      entries are accessible for the Task-6 migration.

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
    assert!(yaml.contains("io: main"), "io field must be present: {yaml}");
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

    assert_eq!(block, back, "round-trip must produce an identical output block");
    assert!(yaml.contains("io: main"), "io field must be present: {yaml}");
    assert!(
        yaml.contains("endpoint: Out1"),
        "endpoint field must be present: {yaml}"
    );
}

// ---------------------------------------------------------------------------
// Test 2: legacy `entries` still deserialise (back-compat for Task-6 migration)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Helpers: build legacy blocks using the old struct shape and serialise them
// so the YAML format is always valid (avoids hard-coding the serde repr).
// ---------------------------------------------------------------------------

/// Serialise a legacy InputBlock (with only `entries`, no `io`/`endpoint`)
/// and confirm it round-trips back, exposing entries for the Task-6 migration.
#[test]
fn legacy_entries_still_deserialize() {
    use domain::ids::DeviceId;
    use project::block::{InputEntry};
    use project::chain::ChainInputMode;

    // Construct directly via the struct, leaving io/endpoint as defaults.
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

    // Serialise to YAML — `io`/`endpoint` are skipped (empty), `entries` appears.
    let yaml = serde_yaml::to_string(&block).expect("legacy block must serialise");
    assert!(
        yaml.contains("entries:"),
        "legacy YAML must contain entries: {yaml}"
    );

    // Deserialise back and verify entries are present.
    let back: AudioBlock = serde_yaml::from_str(&yaml).expect("legacy YAML must deserialise");
    let AudioBlockKind::Input(ref ib) = back.kind else {
        panic!("expected Input block, got {:?}", back.kind);
    };
    assert_eq!(
        ib.entries.len(),
        1,
        "legacy entries must be accessible for migration"
    );
    assert_eq!(ib.entries[0].device_id, DeviceId("coreaudio:default".into()));
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

    let yaml = serde_yaml::to_string(&block).expect("legacy output block must serialise");
    assert!(yaml.contains("entries:"), "entries must appear: {yaml}");

    let back: AudioBlock = serde_yaml::from_str(&yaml).expect("legacy output YAML must deserialise");
    let AudioBlockKind::Output(ref ob) = back.kind else {
        panic!("expected Output block, got {:?}", back.kind);
    };
    assert_eq!(
        ob.entries.len(),
        1,
        "legacy output entries must be accessible for migration"
    );
    assert_eq!(ob.entries[0].device_id, DeviceId("coreaudio:default".into()));
}
