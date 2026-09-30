//! #979 — the chain runtime runs the seam detector on the channels its input
//! actually reads, and reports it per runtime: the recorded broken In 1 marks
//! the chain that reads it, and nothing else — not a clean input, not a
//! channel the input does not read, not another chain's runtime.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;

use std::sync::Arc;

use crate::runtime::{build_chain_runtime_state, process_input_f32, ChainRuntimeState};

const RATE: f32 = 44_100.0;
const BUFFER: usize = 64;
const STEPPED: &[u8] = include_bytes!("../tests/fixtures/issue_979/in1_stepped.f32");
const CLEAN: &[u8] = include_bytes!("../tests/fixtures/issue_979/in1_clean.f32");

fn samples(raw: &[u8]) -> Vec<f32> {
    raw.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        mix: Default::default(),
    }
}

fn registry(input_channel: usize) -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("quantum".into()),
            mode: ChannelMode::Mono,
            channels: vec![input_channel],
        }],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId("quantum".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn runtime(id: &str, input_channel: usize) -> Arc<ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(&chain(id), RATE, &[BUFFER], &registry(input_channel))
            .expect("a one-input chain must build a runtime"),
    )
}

/// Drives the runtime like the device callback: two interleaved channels,
/// `BUFFER` frames per call.
fn play(runtime: &Arc<ChainRuntimeState>, channel_0: &[f32], channel_1: &[f32]) {
    let interleaved: Vec<f32> = channel_0
        .iter()
        .zip(channel_1)
        .flat_map(|(a, b)| [*a, *b])
        .collect();
    for buffer in interleaved.chunks_exact(BUFFER * 2) {
        process_input_f32(runtime, 0, buffer, 2);
    }
}

#[test]
fn the_recorded_broken_input_marks_the_chain_that_reads_it() {
    let rt = runtime("rig:guitar", 0);
    play(&rt, &samples(STEPPED), &samples(CLEAN));
    assert!(rt.input_stepped(), "the broken In 1 did not mark its chain");
}

#[test]
fn a_clean_input_leaves_the_chain_unmarked() {
    let rt = runtime("rig:guitar", 0);
    play(&rt, &samples(CLEAN), &samples(CLEAN));
    assert!(!rt.input_stepped());
}

#[test]
fn a_broken_channel_the_input_does_not_read_is_ignored() {
    let rt = runtime("rig:guitar", 1);
    play(&rt, &samples(STEPPED), &samples(CLEAN));
    assert!(
        !rt.input_stepped(),
        "marked by a channel the input does not read"
    );
}

#[test]
fn a_broken_chain_does_not_mark_another_chain() {
    let broken = runtime("rig:guitar-1", 0);
    let other = runtime("rig:guitar-2", 1);
    let stepped = samples(STEPPED);
    let clean = samples(CLEAN);
    play(&broken, &stepped, &clean);
    play(&other, &stepped, &clean);
    assert!(broken.input_stepped());
    assert!(
        !other.input_stepped(),
        "one chain's broken input marked another chain"
    );
}
