//! #716: a chain references I/O bindings (by id) instead of carrying its own
//! input/output endpoints. The engine is unchanged — only discovery moves:
//! a chain's input/output endpoints are resolved from its selected bindings.
//!
//! RED-first: `io_binding_ids` does not exist yet.

use project::chain::Chain;

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
