//! #967 — a footswitch pressed while an off-thread rebuild is in flight must
//! not be lost when the rebuilt runtime goes live.
//!
//! A scene switch (or any live edit) builds the chain's new runtime on the
//! control worker from a snapshot of the chain. The toggle is applied in place
//! on the runtime that is live NOW; the build that lands a moment later was
//! made from the older snapshot. Publishing it without replaying the toggle
//! puts the old insert state back while the UI shows the new one.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{
    build_chain_runtime_state, process_input_f32, process_output_f32, RuntimeGraph,
};
use infra_cpal::ProjectRuntimeController;
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;

const INSERT: &str = "issue-967:insert";
const CHANNELS: usize = 4;
const FRAMES: usize = 128;

fn registry() -> Vec<IoBinding> {
    let ep = |name: &str, mode: ChannelMode, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode,
        channels,
    };
    vec![
        IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![ep("in0", ChannelMode::Mono, vec![0])],
            outputs: vec![ep("out0", ChannelMode::Stereo, vec![0, 1])],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![ep("ret", ChannelMode::Mono, vec![2])],
            outputs: vec![ep("snd", ChannelMode::Mono, vec![3])],
        },
    ]
}

fn chain(insert_enabled: bool) -> Chain {
    Chain {
        id: ChainId("issue-967-race".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![AudioBlock {
            id: BlockId(INSERT.into()),
            enabled: insert_enabled,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "fx".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }
}

/// Tail level with the guitar at 0.5 on ch 0 and the gear answering 0.2 on ch 2.
fn tail_level(controller: &ProjectRuntimeController, chain_id: &ChainId) -> f32 {
    let runtime = controller.chain_runtime(chain_id).expect("runtime");
    let mut input = vec![0.0_f32; FRAMES * CHANNELS];
    for frame in input.chunks_mut(CHANNELS) {
        frame[0] = 0.5;
        frame[2] = 0.2;
    }
    let mut output = vec![0.0_f32; FRAMES * CHANNELS];
    for _ in 0..256 {
        process_input_f32(&runtime, 0, &input, CHANNELS);
        output.fill(0.0);
        process_output_f32(&runtime, 0, &mut output, CHANNELS);
    }
    output.chunks(CHANNELS).map(|f| f[0].abs()).sum::<f32>() / FRAMES as f32
}

#[test]
fn a_toggle_made_while_a_rebuild_is_in_flight_survives_the_rebuild() {
    let on = chain(true);
    let chain_id = on.id.clone();
    let initial = Arc::new(build_chain_runtime_state(&on, 48_000.0, &[1024], &registry()).unwrap());
    let mut chains = HashMap::new();
    chains.insert((chain_id.clone(), 0_usize), initial);
    let mut controller = ProjectRuntimeController::for_testing(RuntimeGraph { chains });
    controller.set_io_bindings(registry());

    // A live edit schedules a rebuild from the chain as it is now (loop ON)…
    controller.schedule_chain_rebuild(&on, 48_000.0, HashMap::new(), vec![1024]);
    // …and the footswitch switches the loop OFF before that build lands.
    controller
        .toggle_block_enabled_live(&chain(false), &BlockId(INSERT.into()), false)
        .expect("the live toggle must apply");

    let deadline = Instant::now() + Duration::from_secs(10);
    while controller.poll_pending_rebuilds() == 0 {
        assert!(Instant::now() < deadline, "the rebuild never landed");
        std::thread::yield_now();
    }

    let level = tail_level(&controller, &chain_id);
    assert!(
        (level - 0.5).abs() < 0.02,
        "#967: the rebuilt runtime must keep the loop the footswitch switched off \
         (dry guitar 0.5), not the snapshot's state (gear 0.2) — tail was {level}"
    );
}
