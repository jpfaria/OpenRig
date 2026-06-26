//! Task 5 — chain Input/Output blocks reference a binding via `io` +
//! `endpoint` (model A, #716). Device endpoints (`entries`) were removed from
//! the data model entirely; a block now serde round-trips on `io`/`endpoint`.
//! Legacy chain-block YAML no longer deserializes at the block level — old
//! projects are upgraded by the `Project` → `RigProject` migration, not by
//! per-block back-compat (that is covered in `migrate_tests`).

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

    // Legacy `entries` no longer exists in the model and must never appear.
    assert!(
        !yaml.contains("entries:"),
        "entries must never be emitted (removed in #716): {yaml}"
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
