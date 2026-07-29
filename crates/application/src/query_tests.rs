use super::*;
use domain::ids::{BlockId, ChainId};
use project::block::types::{AudioBlock, AudioBlockKind, InputBlock};
use project::chain::Chain;

fn input_block(id: &str, enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled,
        kind: AudioBlockKind::Input(InputBlock {
            model: "default".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

fn chain(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

fn project(chains: Vec<Chain>) -> Project {
    Project {
        name: Some("My Rig".to_string()),
        device_settings: vec![],
        chains,
        midi: None,
    }
}

#[test]
fn empty_project_reports_no_chains() {
    let out = list_ids(&Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    });
    assert!(out.contains("project: (unnamed)"), "{out}");
    assert!(out.contains("(no chains)"), "{out}");
}

#[test]
fn lists_full_ids_for_chains_and_blocks() {
    let p = project(vec![chain(
        "chain:abc",
        vec![input_block("chain:abc:block:def", true)],
    )]);
    let out = list_ids(&p);
    assert!(out.contains("project: My Rig"), "{out}");
    assert!(
        out.contains("chain chain:abc  instrument=guitar  enabled"),
        "{out}"
    );
    assert!(
        out.contains("  block chain:abc:block:def  input  enabled"),
        "{out}"
    );
    assert!(out.contains("(chains: 1)"), "{out}");
}

#[test]
fn marks_disabled_block_and_empty_chain() {
    let p = project(vec![
        chain("chain:x", vec![input_block("chain:x:block:y", false)]),
        chain("chain:empty", vec![]),
    ]);
    let out = list_ids(&p);
    assert!(
        out.contains("block chain:x:block:y  input  disabled"),
        "{out}"
    );
    assert!(out.contains("  (no blocks)"), "{out}");
    assert!(out.contains("(chains: 2)"), "{out}");
}
