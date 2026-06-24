//! #716: a chain references I/O bindings (by id) instead of carrying its own
//! input/output endpoints. The engine is unchanged — only discovery moves:
//! a chain's input/output endpoints are resolved from its selected bindings.
//!
//! RED-first: `io_binding_ids` does not exist yet.

use project::block::{InputBlock, OutputBlock};
use project::chain::Chain;

#[test]
fn io_block_is_a_pure_binding_reference_no_entries() {
    // #716 clean break: an I/O block references its endpoint purely by binding
    // (io + endpoint). It carries NO embedded device entries — the device list
    // lives in the per-machine binding registry, never in the chain.
    let input = InputBlock {
        model: "standard".into(),
        io: "scarlett".into(),
        endpoint: "in-1".into(),
    };
    let yaml = serde_yaml::to_string(&input).expect("serialize input block");
    assert!(yaml.contains("io: scarlett"), "io persisted: {yaml}");
    assert!(yaml.contains("endpoint: in-1"), "endpoint persisted: {yaml}");
    assert!(
        !yaml.contains("entries"),
        "the model must not carry legacy device entries: {yaml}"
    );

    let output = OutputBlock {
        model: "standard".into(),
        io: "scarlett".into(),
        endpoint: "out-main".into(),
    };
    let back: OutputBlock =
        serde_yaml::from_str(&serde_yaml::to_string(&output).unwrap()).expect("roundtrip output");
    assert_eq!(back.io, "scarlett");
    assert_eq!(back.endpoint, "out-main");
}

#[test]
fn chain_without_io_binding_ids_defaults_to_empty() {
    // Back-compat: an existing project YAML has no `io_binding_ids` key.
    let yaml = "instrument: guitar\nenabled: true\n";
    let chain: Chain = serde_yaml::from_str(yaml).expect("deserialize chain");
    assert!(
        chain.io_binding_ids.is_empty(),
        "a chain with no io_binding_ids must default to an empty list"
    );
}

#[test]
fn chain_carries_selected_io_binding_ids() {
    let yaml = "instrument: guitar\nenabled: true\nio_binding_ids:\n  - main\n  - mic\n";
    let chain: Chain = serde_yaml::from_str(yaml).expect("deserialize chain");
    assert_eq!(
        chain.io_binding_ids,
        vec!["main".to_string(), "mic".to_string()]
    );
}
