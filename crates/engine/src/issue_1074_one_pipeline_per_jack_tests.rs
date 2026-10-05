//! #1074 — one physical jack is ONE pipeline per output device.
//!
//! The owner's DIGITAL chain carries `guitarra-1` (In 1 → MAIN + FRFR) and
//! `guitarra-1-syn5050` (In 1 → SYN-5050), all on the Quantum HD 8. The engine
//! built one pipeline per (input × output) pair, so the same guitar ran the
//! whole chain three times: three NAM instances, three taps showing the same
//! signal in the tuner and the spectrum, three different latencies on the
//! outputs. Pipelines that run the same blocks on the same jack now fold into
//! one that fans out to every output on that device.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io};
use crate::runtime_segments::{split_chain_into_segments, ChainSegment};

fn ep(device: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: format!("{device}-{channels:?}"),
        device_id: DeviceId(device.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn binding(id: &str, input: &[usize], outputs: Vec<IoEndpoint>) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.into(),
        inputs: vec![ep("hd8", ChannelMode::Mono, input)],
        outputs,
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

fn chain(bindings: &[&str]) -> Chain {
    Chain {
        id: ChainId("rig:input-4".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: bindings.iter().map(|b| b.to_string()).collect(),
        blocks: vec![effect("amp"), effect("cab")],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

/// The owner's rig: two E/S on In 1, three outputs on the same interface.
fn owner_registry(syn_device: &str) -> Vec<IoBinding> {
    vec![
        binding(
            "guitarra-1",
            &[0],
            vec![
                ep("hd8", ChannelMode::Stereo, &[0, 1]),
                ep("hd8", ChannelMode::Stereo, &[24, 25]),
            ],
        ),
        binding(
            "guitarra-1-syn5050",
            &[0],
            vec![ep(syn_device, ChannelMode::Stereo, &[4, 5])],
        ),
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

/// Output channels each pipeline feeds, one entry per pipeline.
fn fan_out(chain: &Chain, registry: &[IoBinding]) -> Vec<Vec<Vec<usize>>> {
    let (segs, outs) = segments(chain, registry);
    segs.iter()
        .map(|s| {
            s.output_route_indices
                .iter()
                .map(|&r| outs[r].clone())
                .collect()
        })
        .collect()
}

#[test]
fn one_jack_on_two_bindings_runs_one_pipeline_for_every_output_on_the_device() {
    let registry = owner_registry("hd8");
    let chain = chain(&["guitarra-1", "guitarra-1-syn5050"]);

    assert_eq!(
        fan_out(&chain, &registry),
        vec![vec![vec![0, 1], vec![24, 25], vec![4, 5]]],
        "#1074: In 1 is one pipeline feeding MAIN, FRFR and SYN-5050 — not three"
    );
}

#[test]
fn one_jack_on_one_binding_with_two_outputs_is_one_pipeline() {
    let registry = owner_registry("hd8");
    let chain = chain(&["guitarra-1"]);

    assert_eq!(
        fan_out(&chain, &registry),
        vec![vec![vec![0, 1], vec![24, 25]]],
        "#1074: one jack, two outputs on one interface = one pipeline"
    );
}

#[test]
fn an_output_on_another_device_keeps_its_own_pipeline() {
    let registry = owner_registry("other-interface");
    let chain = chain(&["guitarra-1", "guitarra-1-syn5050"]);

    assert_eq!(
        fan_out(&chain, &registry),
        vec![vec![vec![0, 1], vec![24, 25]], vec![vec![4, 5]]],
        "#1074: another interface has its own clock, so it keeps its own pipeline"
    );
}

#[test]
fn two_jacks_never_fold_into_one_pipeline() {
    let registry = vec![
        binding("g1", &[0], vec![ep("hd8", ChannelMode::Stereo, &[0, 1])]),
        binding("g2", &[1], vec![ep("hd8", ChannelMode::Stereo, &[0, 1])]),
    ];
    let chain = chain(&["g1", "g2"]);

    assert_eq!(
        fan_out(&chain, &registry).len(),
        2,
        "#1074: two guitars are two isolated pipelines even on one output"
    );
}
