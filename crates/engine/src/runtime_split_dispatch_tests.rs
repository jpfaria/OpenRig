//! #328: a split node runs wherever a block node runs — the live callback
//! and the offline render — and reports its longer path's latency.

use crossbeam_queue::ArrayQueue;
use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::split_param_keys::mix_polarity;
use project::block::split_params::{self, default_split_params};

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_process_segment::apply_block_processor;
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::latency::node_latency;
use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_split::test_support::{delay_node, gain_node};
use crate::runtime_state::{BlockError, BlockRuntimeNode, RuntimeProcessor};

fn split_node(
    a: Vec<BlockRuntimeNode>,
    b: Vec<BlockRuntimeNode>,
    invert: bool,
) -> BlockRuntimeNode {
    let mut params = default_split_params(2);
    if invert {
        params.insert(
            &mix_polarity(1),
            ParameterValue::String(split_params::POLARITY_INVERT.into()),
        );
    }
    let mut node = gain_node("split", 1.0);
    node.processor = RuntimeProcessor::Split(SplitRuntimeState::new(
        true,
        vec![a, b],
        SplitKnobs::from_params(&params, 2),
        &BlockId("split".into()),
    ));
    node
}

fn peak(frames: &[AudioFrame]) -> f32 {
    frames
        .iter()
        .map(|f| match f {
            AudioFrame::Stereo([l, r]) => l.abs().max(r.abs()),
            AudioFrame::Mono(s) => s.abs(),
        })
        .fold(0.0_f32, f32::max)
}

#[test]
fn the_live_callback_runs_a_split_node() {
    let mut node = split_node(vec![delay_node("ir", 64)], vec![], true);
    let queue = ArrayQueue::<BlockError>::new(8);
    let mut frames: Vec<AudioFrame> = (0..256)
        .map(|i| {
            let s = 0.5 * (i as f32 * 0.05).sin();
            AudioFrame::Stereo([s, s])
        })
        .collect();
    apply_block_processor(&mut node, &mut frames, &queue);
    assert!(
        peak(&frames) < 1e-6,
        "aligned, inverted paths cancel; peak {}",
        peak(&frames)
    );
}

#[test]
fn the_offline_render_runs_a_split_node() {
    let mut nodes = vec![split_node(vec![delay_node("ir", 64)], vec![], true)];
    let input: Vec<[f32; 2]> = (0..512)
        .map(|i| {
            let s = 0.5 * (i as f32 * 0.05).sin();
            [s, s]
        })
        .collect();
    let out = crate::offline::render_nodes_masked(&mut nodes, &input, 128, 0, &[true]);
    let loudest = out
        .iter()
        .map(|[l, r]| l.abs().max(r.abs()))
        .fold(0.0_f32, f32::max);
    assert!(
        loudest < 1e-6,
        "offline split must cancel too; peak {loudest}"
    );
}

#[test]
fn a_split_node_reports_its_longer_path() {
    let node = split_node(vec![delay_node("ir", 64)], vec![delay_node("os", 7)], false);
    assert_eq!(node_latency(&node), 64);
}
