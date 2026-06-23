//! Per-binding stream enumeration (issue #716, Task 9).
//!
//! Pins the routing rule: for each binding referenced in a chain, the engine
//! enumerates `(input port, output port)` pairs with `inputPos <= outputPos`,
//! and each pair becomes one isolated stream running ONLY the effect blocks
//! strictly between the two ports.
//!
//! Worked examples over chain `A,B,C,D,E`:
//!
//! `input_offset` — io XYZ in {ch1@0, ch2@afterA}, out {ch3,4@end} produces two
//! streams: ch1 over `A..E` and ch2 over `B..E`, both to ch3,4.
//!
//! `output_offset` — io XYZ in {ch1@0}, out {ch3@end, ch4@afterC} produces two
//! streams: ch1 over `A..E` to ch3, and ch1 over `A..C` to ch4.

use super::resolve_chain_streams;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;

/// Five inert gain blocks A,B,C,D,E with stable ids so a test can name the
/// block range a stream runs.
fn effect(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn bound_input(id: &str, io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: io.into(),
            endpoint: endpoint.into(),
            entries: Vec::new(),
        }),
    }
}

fn bound_output(id: &str, io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: io.into(),
            endpoint: endpoint.into(),
            entries: Vec::new(),
        }),
    }
}

fn endpoint(name: &str) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![0],
    }
}

/// One binding XYZ with input endpoints ch1, ch2 and output endpoints ch3, ch4.
fn xyz_binding() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "xyz".into(),
        name: "XYZ".into(),
        inputs: vec![endpoint("ch1"), endpoint("ch2")],
        outputs: vec![endpoint("ch3"), endpoint("ch4")],
    }]
}

/// Map a stream's block indices to the block ids it runs (the effect blocks).
fn block_ids(chain: &Chain, indices: &[usize]) -> Vec<String> {
    indices
        .iter()
        .filter_map(|&i| chain.blocks.get(i))
        .map(|b| b.id.0.clone())
        .collect()
}

// ── input-offset example ──────────────────────────────────────────────────

/// Chain layout: ch1-in, A, ch2-in, B, C, D, E, ch3,4-out.
/// ch1's input port sits at the head; ch2's input port sits after A.
fn input_offset_chain() -> Chain {
    Chain {
        id: ChainId("input_offset".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            bound_input("ch1-in", "xyz", "ch1"),
            effect("A"),
            bound_input("ch2-in", "xyz", "ch2"),
            effect("B"),
            effect("C"),
            effect("D"),
            effect("E"),
            bound_output("ch34-out", "xyz", "ch3"),
        ],
    }
}

#[test]
fn input_offset_produces_two_streams_over_expected_block_ranges() {
    let chain = input_offset_chain();
    let streams = resolve_chain_streams(&chain, &xyz_binding());

    assert_eq!(
        streams.len(),
        2,
        "expected exactly 2 streams (ch1→out, ch2→out), got {}",
        streams.len()
    );

    // Stream from ch1 (head) runs A,B,C,D,E.
    let ch1_stream = streams
        .iter()
        .find(|s| s.input_endpoint == "ch1")
        .expect("ch1 stream must exist");
    assert_eq!(
        block_ids(&chain, &ch1_stream.block_indices),
        vec!["A", "B", "C", "D", "E"],
        "ch1 stream must run all five blocks A..E"
    );

    // Stream from ch2 (after A) runs B,C,D,E only.
    let ch2_stream = streams
        .iter()
        .find(|s| s.input_endpoint == "ch2")
        .expect("ch2 stream must exist");
    assert_eq!(
        block_ids(&chain, &ch2_stream.block_indices),
        vec!["B", "C", "D", "E"],
        "ch2 stream (input after A) must run only B..E"
    );
}

// ── output-offset example ─────────────────────────────────────────────────

/// Chain layout: ch1-in, A, B, C, ch4-out, D, E, ch3-out.
/// ch4's output port sits after C; ch3's output port sits at the end.
fn output_offset_chain() -> Chain {
    Chain {
        id: ChainId("output_offset".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            bound_input("ch1-in", "xyz", "ch1"),
            effect("A"),
            effect("B"),
            effect("C"),
            bound_output("ch4-out", "xyz", "ch4"),
            effect("D"),
            effect("E"),
            bound_output("ch3-out", "xyz", "ch3"),
        ],
    }
}

#[test]
fn output_offset_produces_two_streams_over_expected_block_ranges() {
    let chain = output_offset_chain();
    let streams = resolve_chain_streams(&chain, &xyz_binding());

    assert_eq!(
        streams.len(),
        2,
        "expected exactly 2 streams (ch1→ch3, ch1→ch4), got {}",
        streams.len()
    );

    // ch1 → ch3 (end) runs A,B,C,D,E.
    let to_ch3 = streams
        .iter()
        .find(|s| s.output_endpoint == "ch3")
        .expect("ch1→ch3 stream must exist");
    assert_eq!(to_ch3.input_endpoint, "ch1");
    assert_eq!(
        block_ids(&chain, &to_ch3.block_indices),
        vec!["A", "B", "C", "D", "E"],
        "ch1→ch3 stream must run all five blocks A..E"
    );

    // ch1 → ch4 (after C) runs A,B,C only.
    let to_ch4 = streams
        .iter()
        .find(|s| s.output_endpoint == "ch4")
        .expect("ch1→ch4 stream must exist");
    assert_eq!(to_ch4.input_endpoint, "ch1");
    assert_eq!(
        block_ids(&chain, &to_ch4.block_indices),
        vec!["A", "B", "C"],
        "ch1→ch4 stream (output after C) must run only A..C"
    );
}

// ── cross-binding pairing guard ───────────────────────────────────────────

/// Two bindings, each in→out. The resolver must NOT pair io_a's input with
/// io_b's output (that is the structural isolation the routing rule enforces).
#[test]
fn streams_never_cross_binding_boundaries() {
    let chain = Chain {
        id: ChainId("two_binding".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            bound_input("in:a", "io_a", "in_a"),
            bound_input("in:b", "io_b", "in_b"),
            bound_output("out:a", "io_a", "out_a"),
            bound_output("out:b", "io_b", "out_b"),
        ],
    };
    let registry = vec![
        IoBinding {
            id: "io_a".into(),
            name: "A".into(),
            inputs: vec![endpoint("in_a")],
            outputs: vec![endpoint("out_a")],
        },
        IoBinding {
            id: "io_b".into(),
            name: "B".into(),
            inputs: vec![endpoint("in_b")],
            outputs: vec![endpoint("out_b")],
        },
    ];
    let streams = resolve_chain_streams(&chain, &registry);

    assert_eq!(
        streams.len(),
        2,
        "expected exactly 2 same-binding streams, got {}",
        streams.len()
    );
    for s in &streams {
        assert_eq!(
            s.input_binding, s.output_binding,
            "stream pairs input binding '{}' with output binding '{}' — \
             cross-binding routing is forbidden",
            s.input_binding, s.output_binding
        );
    }
}
