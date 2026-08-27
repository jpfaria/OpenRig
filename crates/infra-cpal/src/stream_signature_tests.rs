//! #913 — the endpoint ORDER a chain's stream signature is recorded in.
//!
//! The signature is zipped against the device vectors `resolve_chain_inputs` /
//! `resolve_chain_outputs` build, so it must list the same endpoints in the
//! same order: the chain's own bindings first, then one entry per enabled,
//! both-sides-bound insert. #881: a signature blind to the insert read a
//! removed insert as "I/O unchanged", the streams were never rebuilt, and the
//! orphan send stream stayed open as a second writer on the device.

#![cfg(not(all(target_os = "linux", feature = "jack")))]

use super::resolve_chain_io_with_inserts;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;

fn binding(id: &str, device: &str) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.into(),
        inputs: vec![IoEndpoint {
            name: "In 1".into(),
            device_id: DeviceId(format!("{device}-in")),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Out 1".into(),
            device_id: DeviceId(format!("{device}-out")),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}

fn insert_block(id: &str, io: &str, enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "external_loop".into(),
            io: io.into(),
        }),
    }
}

fn chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("chain:sig".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io-main".into()],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

/// The chain's own head/tail come from `io_binding_ids` (model A, #716) — a
/// bound chain carries no synthesized Input/Output block.
fn head_and_tail() -> Vec<AudioBlock> {
    Vec::new()
}

fn devices(entries: &[DeviceId]) -> Vec<String> {
    entries.iter().map(|d| d.0.clone()).collect()
}

#[test]
fn a_chain_with_no_insert_signs_only_its_own_binding() {
    let registry = vec![binding("io-main", "main")];
    let (inputs, outputs) = resolve_chain_io_with_inserts(&chain(head_and_tail()), &registry);
    assert_eq!(
        devices(
            &inputs
                .iter()
                .map(|e| e.device_id.clone())
                .collect::<Vec<_>>()
        ),
        vec!["main-in"]
    );
    assert_eq!(
        devices(
            &outputs
                .iter()
                .map(|e| e.device_id.clone())
                .collect::<Vec<_>>()
        ),
        vec!["main-out"]
    );
}

#[test]
fn an_enabled_insert_appends_its_return_and_send_after_the_chains_own_io() {
    let registry = vec![binding("io-main", "main"), binding("io-fx", "fx")];
    let mut blocks = head_and_tail();
    blocks.push(insert_block("fx", "io-fx", true));
    let (inputs, outputs) = resolve_chain_io_with_inserts(&chain(blocks), &registry);
    assert_eq!(
        devices(
            &inputs
                .iter()
                .map(|e| e.device_id.clone())
                .collect::<Vec<_>>()
        ),
        vec!["main-in", "fx-in"],
        "the insert's RETURN is a stream this chain really opens"
    );
    assert_eq!(
        devices(
            &outputs
                .iter()
                .map(|e| e.device_id.clone())
                .collect::<Vec<_>>()
        ),
        vec!["main-out", "fx-out"],
        "the insert's SEND likewise — #881"
    );
}

#[test]
fn a_disabled_insert_is_not_part_of_the_signature() {
    let registry = vec![binding("io-main", "main"), binding("io-fx", "fx")];
    let mut blocks = head_and_tail();
    blocks.push(insert_block("fx", "io-fx", false));
    let (inputs, outputs) = resolve_chain_io_with_inserts(&chain(blocks), &registry);
    assert_eq!(inputs.len(), 1);
    assert_eq!(outputs.len(), 1);
}

#[test]
fn an_insert_whose_binding_is_missing_opens_no_stream_to_sign() {
    let registry = vec![binding("io-main", "main")];
    let mut blocks = head_and_tail();
    blocks.push(insert_block("fx", "io-gone", true));
    let (inputs, outputs) = resolve_chain_io_with_inserts(&chain(blocks), &registry);
    assert_eq!(inputs.len(), 1, "an unbound insert is not a stream");
    assert_eq!(outputs.len(), 1);
}

#[test]
fn two_inserts_are_appended_in_block_order() {
    let registry = vec![
        binding("io-main", "main"),
        binding("io-fx-a", "a"),
        binding("io-fx-b", "b"),
    ];
    let mut blocks = head_and_tail();
    blocks.push(insert_block("fx-a", "io-fx-a", true));
    blocks.push(insert_block("fx-b", "io-fx-b", true));
    let (inputs, _) = resolve_chain_io_with_inserts(&chain(blocks), &registry);
    assert_eq!(
        devices(
            &inputs
                .iter()
                .map(|e| e.device_id.clone())
                .collect::<Vec<_>>()
        ),
        vec!["main-in", "a-in", "b-in"],
        "the device vectors are built in this order, so the signature must match"
    );
}
