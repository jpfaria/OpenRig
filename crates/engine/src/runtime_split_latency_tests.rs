//! #328 spec §4.3: each path's latency is the sum of what its enabled
//! blocks report; the ceiling keeps room for blocks switched off now.

use domain::ids::BlockId;

use super::{
    node_latency, node_latency_ceiling, path_latency, path_latency_ceiling, processor_latency,
};
use crate::runtime_audio_frame::AudioProcessor;
use crate::runtime_split::test_support::{delay_node, gain_node, FixedDelay};
use crate::runtime_state::{RuntimeProcessor, SelectRuntimeState};

#[test]
fn an_enabled_block_adds_what_its_processor_reports() {
    assert_eq!(node_latency(&delay_node("ir", 64)), 64);
}

#[test]
fn a_switched_off_block_adds_nothing_but_keeps_its_room() {
    let mut node = delay_node("ir", 64);
    node.block_snapshot.enabled = false;
    assert_eq!(node_latency(&node), 0, "a bypassed block does not process");
    assert_eq!(
        node_latency_ceiling(&node),
        64,
        "it may be switched back on"
    );
}

#[test]
fn a_faulted_block_adds_nothing() {
    let mut node = delay_node("ir", 64);
    node.faulted = true;
    assert_eq!((node_latency(&node), node_latency_ceiling(&node)), (0, 0));
}

#[test]
fn a_dual_mono_pair_reports_its_channels() {
    let pair = AudioProcessor::DualMono {
        left: Box::new(FixedDelay::new(7)),
        right: Box::new(FixedDelay::new(7)),
    };
    assert_eq!(processor_latency(&pair), 7);
}

#[test]
fn a_select_adds_what_its_selected_option_adds() {
    let mut node = gain_node("select", 1.0);
    node.processor = RuntimeProcessor::Select(SelectRuntimeState {
        selected_block_id: BlockId("slow".into()),
        options: vec![delay_node("fast", 5), delay_node("slow", 9)],
    });
    assert_eq!(node_latency(&node), 9);
}

#[test]
fn a_path_sums_its_blocks() {
    let mut off = delay_node("off", 100);
    off.block_snapshot.enabled = false;
    let path = vec![
        delay_node("ir", 64),
        gain_node("amp", 0.5),
        delay_node("os", 7),
        off,
    ];
    assert_eq!(path_latency(&path), 71);
    assert_eq!(path_latency_ceiling(&path), 171);
}
