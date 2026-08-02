//! #85 / LAW (stream isolation) — a stream is one (input × output) pair, and
//! each one is an INDEPENDENT pipeline.
//!
//! The owner's rule, in his words: "se eu tenho 1 input e dois outputs eu tenho
//! dois streams; se eu tenho 2 inputs e 2 outputs eu tenho 4 streams — a cadeia
//! de efeito muda de acordo com a posição do input e do output".
//!
//! So a mid `Output` is NOT a tap riding another pipeline: it is the end of its
//! own. And a mid `Input` starts its own. Each pair runs exactly the blocks
//! between its input's position and its output's — nothing before, nothing
//! after.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io};
use crate::runtime_segments::split_chain_into_segments;

fn endpoint(name: &str, device: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

/// `main` is the chain's own E/S; `aux` is the second interface a mid port uses.
fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId("scarlett".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![endpoint("main-out", "scarlett", vec![0, 1])],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: vec![IoEndpoint {
                name: "aux-in".into(),
                device_id: DeviceId("teyun".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![endpoint("aux-out", "teyun", vec![2, 3])],
        },
    ]
}

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

fn mid_output() -> AudioBlock {
    AudioBlock {
        id: BlockId("mid-out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "aux-out".into(),
        }),
    }
}

fn mid_input() -> AudioBlock {
    AudioBlock {
        id: BlockId("mid-in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "aux-in".into(),
        }),
    }
}

fn chain_with(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("issue-85-pairs".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

/// The block ids each segment runs, one entry per segment.
fn segment_blocks(chain: &Chain) -> Vec<Vec<String>> {
    let registry = registry();
    let (resolved_in, resolved_out) = resolve_chain_io(chain, &registry);
    let (eff_in, cpal_indices, split_positions, entry_groups) =
        effective_inputs(chain, &resolved_in, &registry);
    let eff_out = effective_outputs(chain, &resolved_out, &registry);
    split_chain_into_segments(
        chain,
        &eff_in,
        &cpal_indices,
        &split_positions,
        &entry_groups,
        &eff_out,
        &registry,
    )
    .iter()
    .map(|segment| {
        segment
            .block_indices
            .iter()
            .filter_map(|&i| chain.blocks.get(i))
            .map(|b| b.id.0.clone())
            .collect()
    })
    .collect()
}

/// One input, two outputs (the tail plus a mid `Output` after block A) → TWO
/// independent pipelines: A alone into the mid output, A+B into the tail.
#[test]
fn one_input_and_two_outputs_are_two_streams() {
    let chain = chain_with(vec![effect("A"), mid_output(), effect("B")]);
    let mut segments = segment_blocks(&chain);
    segments.sort();

    assert_eq!(
        segments,
        vec![
            vec!["A".to_string()],
            vec!["A".to_string(), "B".to_string()],
        ],
        "#85: a mid Output ends its OWN pipeline — one stream per (input × output)"
    );
}

/// Two inputs, two outputs → FOUR pipelines, each running exactly the blocks
/// between its own input and its own output.
#[test]
fn two_inputs_and_two_outputs_are_four_streams() {
    // [A, mid-in, B, mid-out, C]: the mid input enters after A, the mid output
    // ends after B.
    let chain = chain_with(vec![
        effect("A"),
        mid_input(),
        effect("B"),
        mid_output(),
        effect("C"),
    ]);
    let mut segments = segment_blocks(&chain);
    segments.sort();

    assert_eq!(
        segments,
        vec![
            // head input → mid output
            vec!["A".to_string(), "B".to_string()],
            // head input → tail output
            vec!["A".to_string(), "B".to_string(), "C".to_string()],
            // mid input → mid output
            vec!["B".to_string()],
            // mid input → tail output
            vec!["B".to_string(), "C".to_string()],
        ],
        "#85: 2 inputs × 2 outputs = 4 independent pipelines, each running only \
         the blocks between its own input and its own output"
    );
}
