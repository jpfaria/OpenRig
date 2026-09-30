//! #328 — every place the stream layer decides whether a chain gets streams
//! must read ONE rule, `engine::runtime_graph::chain_plays`: switched on and
//! the endpoint checklist leaves it an input and an output. A site still
//! gating on `chain.enabled` alone tries to open a chain whose every output is
//! unchecked — and one such chain fails the sync of the whole project.

use std::path::Path;

const GATE_FILES: &[&str] = &[
    "src/chain_resolve.rs",
    "src/validation.rs",
    "src/stream_builder_project.rs",
    "src/controller_sync.rs",
    "src/controller_upsert.rs",
];

const BARE_GATES: &[&str] = &[
    "if !chain.enabled {",
    "if chain.enabled {",
    "|c| c.enabled",
    "c.enabled && c.id",
];

#[test]
fn every_stream_gate_reads_chain_plays() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for file in GATE_FILES {
        let source = std::fs::read_to_string(root.join(file))
            .unwrap_or_else(|e| panic!("read {file}: {e}"));
        for gate in BARE_GATES {
            assert!(
                !source.contains(gate),
                "#328: {file} still gates on `{gate}` — use `engine::runtime_graph::chain_plays(chain, registry)`"
            );
        }
        assert!(
            source.contains("chain_plays("),
            "#328: {file} must decide through `chain_plays`"
        );
    }
}
