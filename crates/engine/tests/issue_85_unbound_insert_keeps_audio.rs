//! Issue #85 — an `Insert` block the user just added, before picking its E/S,
//! silences the whole chain: the input meter drops to -120 dBFS and comes back
//! only when the block is removed. Found while validating the mid ports over
//! MCP.
//!
//! An unbound port has nowhere to send and nowhere to return from, so it cannot
//! do its job — but it must not take the chain down with it. The rest of the
//! chain has to keep playing, exactly as a bypassed block does, until the user
//! finishes wiring it.

use std::sync::{Arc, Once};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use engine::runtime_graph::build_chain_runtime_state;
use engine::runtime_state::ChainRuntimeState;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InsertBlock};
use project::chain::Chain;
use project::param::ParameterSet;

const SR: f32 = 48_000.0;
const BUF: usize = 64;
const DEVICE_CHANNELS: usize = 2;

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        block_dyn::register_natives();
    });
}

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn wire(id: &str) -> AudioBlock {
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

/// The block as the add flow creates it: no `io` picked yet.
fn unbound_insert() -> AudioBlock {
    AudioBlock {
        id: BlockId("issue85:insert".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".into(),
            io: String::new(),
        }),
    }
}

fn chain_with(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("issue-85-unbound-insert".into()),
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

/// Peak reaching the chain's tail output.
fn tail_peak(chain: &Chain) -> f32 {
    let runtime: Arc<ChainRuntimeState> = Arc::new(
        build_chain_runtime_state(chain, SR, &[DEFAULT_ELASTIC_TARGET], &registry())
            .expect("runtime must build"),
    );
    const CALLBACKS: usize = 400;
    const WARMUP: usize = 300;
    let mut input = vec![0.0_f32; BUF];
    let mut output = vec![0.0_f32; BUF * DEVICE_CHANNELS];
    let mut peak = 0.0_f32;
    let mut phase = 0.0_f32;
    let step = 2.0 * std::f32::consts::PI * 440.0 / SR;
    for cb in 0..CALLBACKS {
        for s in input.iter_mut() {
            *s = 0.5 * phase.sin();
            phase += step;
        }
        process_input_f32(&runtime, 0, &input, 1);
        output.iter_mut().for_each(|s| *s = 0.0);
        process_output_f32(&runtime, 0, &mut output, DEVICE_CHANNELS);
        if cb < WARMUP {
            continue;
        }
        for frame in output.chunks_exact(DEVICE_CHANNELS) {
            peak = peak.max(frame[0].abs()).max(frame[1].abs());
        }
    }
    peak
}

/// Baseline: the same chain without the port plays.
#[test]
fn the_chain_plays_without_the_insert() {
    init_registry();
    let peak = tail_peak(&chain_with(vec![wire("A"), wire("B")]));
    assert!(peak > 0.1, "the probe chain is silent (peak {peak})");
}

#[test]
fn an_unbound_insert_does_not_silence_the_chain() {
    init_registry();
    let peak = tail_peak(&chain_with(vec![wire("A"), unbound_insert(), wire("B")]));
    assert!(
        peak > 0.1,
        "#85: an Insert with no E/S picked yet silenced the whole chain \
         (peak {peak}) — an unwired port must sit out, not take the rig down"
    );
}
