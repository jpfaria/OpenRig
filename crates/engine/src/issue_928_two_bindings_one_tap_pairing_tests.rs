//! #928 — two E/S may read the SAME capture point and feed different outputs
//! (the owner's GUITARRA 1 MAIN + SYN5050, both input ch 0). The segment
//! builder paired a head input with "its" binding by tap equality, so the
//! second binding's input was taken for the first one's: both streams fed
//! MAIN and the SYN5050 output had no pipeline at all. A head input belongs to
//! the binding it was resolved FROM (its position), never to whichever binding
//! happens to read the same channel.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io};
use crate::runtime_segments::{split_chain_into_segments, ChainSegment};

fn ep(mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: "e".into(),
        device_id: DeviceId("quantum".into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn binding(id: &str, input: &[usize], out: &[usize]) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.into(),
        inputs: vec![ep(ChannelMode::Mono, input)],
        outputs: vec![ep(ChannelMode::Stereo, out)],
    }
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

fn insert(io: &str, enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId(io.into()),
        enabled,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".into(),
            io: io.into(),
        }),
    }
}

fn chain(bindings: &[&str], blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("rig:input-2".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: bindings.iter().map(|b| b.to_string()).collect(),
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        binding("main", &[0], &[0, 1]),
        binding("syn5050", &[0], &[16, 17]),
        binding("syn2", &[17, 18], &[10]),
    ]
}

fn segments(chain: &Chain, registry: &[IoBinding]) -> (Vec<ChainSegment>, Vec<Vec<usize>>) {
    let (ri, ro) = resolve_chain_io(chain, registry);
    let (ei, ci, sp, eg) = effective_inputs(chain, &ri, registry);
    let eo = effective_outputs(chain, &ro, registry);
    let segs = split_chain_into_segments(chain, &ei, &ci, &sp, &eg, &eo, registry);
    let outs = eo.iter().map(|o| o.channels.clone()).collect();
    (segs, outs)
}

/// `(entry group, output channels)` per segment — the pairing, readably.
fn pairing(chain: &Chain, registry: &[IoBinding]) -> Vec<(usize, Vec<Vec<usize>>)> {
    let (segs, outs) = segments(chain, registry);
    segs.iter()
        .map(|s| {
            (
                s.entry_group,
                s.output_route_indices
                    .iter()
                    .map(|&r| outs[r].clone())
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn each_binding_on_the_shared_tap_feeds_its_own_output() {
    let registry = registry();
    let chain = chain(
        &["main", "syn5050"],
        vec![effect("gate"), insert("syn2", false)],
    );

    assert_eq!(
        pairing(&chain, &registry),
        vec![(0, vec![vec![0, 1]]), (1, vec![vec![16, 17]])],
        "#928: the first binding's input feeds MAIN, the second's feeds SYN5050 — \
         not both into MAIN with SYN5050 left unfed"
    );
}

#[test]
fn binding_order_does_not_change_who_feeds_what() {
    let registry = registry();
    let chain = chain(&["syn5050", "main"], vec![effect("gate")]);

    assert_eq!(
        pairing(&chain, &registry),
        vec![(0, vec![vec![16, 17]]), (1, vec![vec![0, 1]])],
        "#928: swapping the selection order swaps the rows, never the routing"
    );
}

#[test]
fn a_shared_tap_next_to_an_insert_still_sends_both_heads_into_the_loop() {
    let registry = registry();
    let chain = chain(
        &["main", "syn5050"],
        vec![effect("gate"), insert("syn2", true), effect("delay")],
    );

    assert_eq!(
        pairing(&chain, &registry),
        vec![
            (0, vec![vec![10]]),
            (1, vec![vec![10]]),
            (2, vec![vec![0, 1], vec![16, 17]]),
        ],
        "with the loop on: both heads send into it, the return feeds both tails"
    );
}
