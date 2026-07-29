//! Issue #717 — the chain's chosen DI-loop output persists per-chain in the
//! project (ADR 0003: it travels with the `.openrig`). A chain with
//! `di_output = Some(..)` round-trips through YAML unchanged; a legacy chain
//! without the field deserializes to `None` (existing projects unaffected).

use domain::ids::ChainId;
use project::chain::{Chain, DiOutputRef};

fn base_chain() -> Chain {
    Chain {
        id: ChainId("c".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

#[test]
fn di_output_round_trips_through_yaml() {
    let mut chain = base_chain();
    chain.di_output = Some(DiOutputRef {
        binding_id: "io".into(),
        endpoint: "out0".into(),
    });

    let yaml = serde_yaml::to_string(&chain).unwrap();
    let back: Chain = serde_yaml::from_str(&yaml).unwrap();

    assert_eq!(back.di_output, chain.di_output);
}

#[test]
fn legacy_chain_without_di_output_deserializes_to_none() {
    let yaml = "\
instrument: electric_guitar
enabled: true
volume: 100.0
io_binding_ids: []
blocks: []
";
    let chain: Chain = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(chain.di_output, None);
}
