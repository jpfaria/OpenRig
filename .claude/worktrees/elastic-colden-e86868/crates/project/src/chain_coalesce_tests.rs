//! Tests for `Chain::coalesce_endpoint_blocks` (issue #377 migration).
//! Split out of `chain_tests.rs` so neither file exceeds the 600 LOC cap.

use super::*;
use crate::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, InsertBlock, InsertEndpoint, OutputBlock,
    OutputEntry,
};
use domain::ids::{BlockId, ChainId, DeviceId};

fn make_input_block(
    id: &str,
    device: &str,
    channels: Vec<usize>,
    mode: ChainInputMode,
) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            entries: vec![InputEntry {
                device_id: DeviceId(device.into()),
                mode,
                channels,
            }],
        }),
    }
}

fn make_output_block(
    id: &str,
    device: &str,
    channels: Vec<usize>,
    mode: ChainOutputMode,
) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            entries: vec![OutputEntry {
                device_id: DeviceId(device.into()),
                mode,
                channels,
            }],
        }),
    }
}

fn make_insert_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".to_string(),
            send: InsertEndpoint {
                device_id: DeviceId("send-dev".into()),
                mode: ChainInputMode::Stereo,
                channels: vec![0, 1],
            },
            return_: InsertEndpoint {
                device_id: DeviceId("return-dev".into()),
                mode: ChainInputMode::Stereo,
                channels: vec![0, 1],
            },
        }),
    }
}

fn empty_chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: Vec::new(),
    }
}

#[test]
fn coalesce_merges_two_consecutive_input_blocks_at_chain_head() {
    let mut chain = empty_chain("c1");
    chain.blocks = vec![
        make_input_block("c1:input:0", "dev-A", vec![0], ChainInputMode::Mono),
        make_input_block("c1:input:1", "dev-B", vec![1], ChainInputMode::Mono),
        make_output_block("c1:output:0", "out", vec![0, 1], ChainOutputMode::Stereo),
    ];

    chain.coalesce_endpoint_blocks();

    assert_eq!(chain.blocks.len(), 2, "head run merged from 2 to 1");
    let AudioBlockKind::Input(ib) = &chain.blocks[0].kind else {
        panic!("expected InputBlock at head");
    };
    assert_eq!(ib.entries.len(), 2, "both devices preserved as entries");
    assert_eq!(ib.entries[0].device_id.0, "dev-A");
    assert_eq!(ib.entries[1].device_id.0, "dev-B");
    assert_eq!(
        chain.blocks[0].id.0, "c1:input:0",
        "first block id survives"
    );
}

#[test]
fn coalesce_merges_two_consecutive_output_blocks_at_chain_tail() {
    let mut chain = empty_chain("c1");
    chain.blocks = vec![
        make_input_block("c1:input:0", "in", vec![0], ChainInputMode::Mono),
        make_output_block("c1:output:0", "out-A", vec![0, 1], ChainOutputMode::Stereo),
        make_output_block("c1:output:1", "out-B", vec![0], ChainOutputMode::Mono),
    ];

    chain.coalesce_endpoint_blocks();

    assert_eq!(chain.blocks.len(), 2, "tail run merged from 2 to 1");
    let AudioBlockKind::Output(ob) = &chain.blocks[1].kind else {
        panic!("expected OutputBlock at tail");
    };
    assert_eq!(ob.entries.len(), 2, "both devices preserved as entries");
    assert_eq!(ob.entries[0].device_id.0, "out-A");
    assert_eq!(ob.entries[1].device_id.0, "out-B");
    assert_eq!(
        chain.blocks[1].id.0, "c1:output:1",
        "last block id survives"
    );
}

#[test]
fn coalesce_is_noop_for_already_normalized_chain() {
    let mut chain = empty_chain("c1");
    chain.blocks = vec![
        make_input_block("c1:input:0", "in", vec![0], ChainInputMode::Mono),
        make_output_block("c1:output:0", "out", vec![0, 1], ChainOutputMode::Stereo),
    ];
    let before = chain.blocks.clone();

    chain.coalesce_endpoint_blocks();

    assert_eq!(
        chain.blocks, before,
        "chain with single I/O blocks is unchanged"
    );
}

#[test]
fn coalesce_handles_both_head_and_tail_runs_in_same_chain() {
    let mut chain = empty_chain("c1");
    chain.blocks = vec![
        make_input_block("c1:input:0", "in-A", vec![0], ChainInputMode::Mono),
        make_input_block("c1:input:1", "in-B", vec![1], ChainInputMode::Mono),
        make_input_block("c1:input:2", "in-C", vec![0, 1], ChainInputMode::Stereo),
        make_output_block("c1:output:0", "out-A", vec![0, 1], ChainOutputMode::Stereo),
        make_output_block("c1:output:1", "out-B", vec![0], ChainOutputMode::Mono),
    ];

    chain.coalesce_endpoint_blocks();

    assert_eq!(
        chain.blocks.len(),
        2,
        "3 inputs + 2 outputs collapse to 1 + 1"
    );
    let AudioBlockKind::Input(ib) = &chain.blocks[0].kind else {
        panic!("expected InputBlock at head");
    };
    assert_eq!(ib.entries.len(), 3);
    let AudioBlockKind::Output(ob) = &chain.blocks[1].kind else {
        panic!("expected OutputBlock at tail");
    };
    assert_eq!(ob.entries.len(), 2);
}

#[test]
fn coalesce_does_not_merge_non_consecutive_input_blocks() {
    let mut chain = empty_chain("c1");
    chain.blocks = vec![
        make_input_block("c1:input:0", "in", vec![0], ChainInputMode::Mono),
        make_insert_block("c1:fx"),
        make_input_block("c1:input:1", "in2", vec![1], ChainInputMode::Mono),
        make_output_block("c1:output:0", "out", vec![0, 1], ChainOutputMode::Stereo),
    ];

    chain.coalesce_endpoint_blocks();

    assert_eq!(
        chain.blocks.len(),
        4,
        "Input blocks separated by non-Input blocks are intentionally untouched"
    );
}

#[test]
fn coalesce_handles_empty_chain() {
    let mut chain = empty_chain("c1");
    chain.coalesce_endpoint_blocks();
    assert!(chain.blocks.is_empty());
}
