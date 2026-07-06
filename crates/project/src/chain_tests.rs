//! Tests for `project::chain`. Lifted from `chain.rs` so the production file
//! stays under the size cap. Re-attached via `#[cfg(test)] #[path] mod tests;`,
//! so every `super::*` reference resolves unchanged.

use super::*;
use crate::block::{
    schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InsertBlock,
    OutputBlock,
};
use crate::param::ParameterSet;
use domain::ids::{BlockId, ChainId};

/// A binding-bound Input block (model A, #716): no device endpoints, only a
/// binding reference. Pass empty `io` for an unbound block.
fn make_input_block(id: &str, io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            io: io.to_string(),
            endpoint: endpoint.to_string(),
        }),
    }
}

/// A binding-bound Output block (model A, #716).
fn make_output_block(id: &str, io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            io: io.to_string(),
            endpoint: endpoint.to_string(),
        }),
    }
}

fn make_insert_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".to_string(),
            io: "fx".to_string(),
        }),
    }
}

fn make_delay_block(id: &str) -> AudioBlock {
    let model = block_delay::supported_models().first().unwrap();
    let schema = schema_for_block_model("delay", model).unwrap();
    let params = ParameterSet::default().normalized_against(&schema).unwrap();
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: model.to_string(),
            params,
        }),
    }
}

fn make_chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("chain:0".to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
    }
}

// #716 domain rule: a chain has I/O only when it references a binding
// (`io_binding_ids`) or carries an I/O block bound to a binding (`io` set).
// `has_io` is the predicate the dispatcher enforces to forbid enabling an
// I/O-less chain.
#[test]
fn has_io_false_when_no_binding_and_no_input() {
    assert!(!make_chain(vec![]).has_io());
}

#[test]
fn has_io_false_when_input_block_is_unbound() {
    // An Input block with no binding reference is not I/O.
    let chain = make_chain(vec![make_input_block("in", "", "")]);
    assert!(!chain.has_io());
}

#[test]
fn has_io_true_when_io_binding_selected() {
    let mut chain = make_chain(vec![]);
    chain.io_binding_ids = vec!["scarlet".to_string()];
    assert!(chain.has_io());
}

#[test]
fn has_io_true_when_input_block_is_binding_bound() {
    let chain = make_chain(vec![make_input_block("in", "scarlet", "in1")]);
    assert!(chain.has_io());
}

// --- processing_layout tests ---

#[test]
fn processing_layout_mono_input_mono_output() {
    let layout = processing_layout(&[0], &[0], ChainInputMode::Mono);
    assert_eq!(layout, ProcessingLayout::Mono);
}

#[test]
fn processing_layout_mono_input_stereo_output() {
    let layout = processing_layout(&[0], &[0, 1], ChainInputMode::Mono);
    assert_eq!(layout, ProcessingLayout::Stereo);
}

#[test]
fn processing_layout_stereo_input_mono_output() {
    let layout = processing_layout(&[0, 1], &[0], ChainInputMode::Stereo);
    assert_eq!(layout, ProcessingLayout::Stereo);
}

#[test]
fn processing_layout_stereo_input_stereo_output() {
    let layout = processing_layout(&[0, 1], &[0, 1], ChainInputMode::Stereo);
    assert_eq!(layout, ProcessingLayout::Stereo);
}

#[test]
fn processing_layout_dual_mono_two_inputs() {
    let layout = processing_layout(&[0, 1], &[0, 1], ChainInputMode::DualMono);
    assert_eq!(layout, ProcessingLayout::DualMono);
}

#[test]
fn processing_layout_dual_mono_single_input_falls_to_mono() {
    // DualMono with only 1 input channel => not enough for DualMono, falls through
    let layout = processing_layout(&[0], &[0], ChainInputMode::DualMono);
    // With 1 input and DualMono mode, in_count < 2, so it skips DualMono check
    // Not stereo mode, so it goes to out_count match: 1 => Mono
    assert_eq!(layout, ProcessingLayout::Mono);
}

#[test]
fn processing_layout_mono_input_empty_output() {
    let layout = processing_layout(&[0], &[], ChainInputMode::Mono);
    assert_eq!(layout, ProcessingLayout::Mono);
}

#[test]
fn processing_layout_stereo_mode_single_channel_still_stereo() {
    // Stereo mode overrides channel count
    let layout = processing_layout(&[0], &[0], ChainInputMode::Stereo);
    assert_eq!(layout, ProcessingLayout::Stereo);
}

// --- Chain::input_blocks / output_blocks / insert_blocks ---

#[test]
fn input_blocks_returns_all_inputs_with_indices() {
    let chain = make_chain(vec![
        make_input_block("in:0", "b1", "in1"),
        make_delay_block("fx:0"),
        make_input_block("in:1", "b2", "in1"),
        make_output_block("out:0", "b1", "out1"),
    ]);
    let inputs = chain.input_blocks();
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[0].0, 0);
    assert_eq!(inputs[1].0, 2);
}

#[test]
fn output_blocks_returns_all_outputs_with_indices() {
    let chain = make_chain(vec![
        make_input_block("in:0", "b1", "in1"),
        make_output_block("out:0", "b1", "out1"),
        make_delay_block("fx:0"),
        make_output_block("out:1", "b2", "out1"),
    ]);
    let outputs = chain.output_blocks();
    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0].0, 1);
    assert_eq!(outputs[1].0, 3);
}

#[test]
fn insert_blocks_returns_all_inserts_with_indices() {
    let chain = make_chain(vec![
        make_input_block("in:0", "b1", "in1"),
        make_insert_block("ins:0"),
        make_delay_block("fx:0"),
        make_insert_block("ins:1"),
        make_output_block("out:0", "b1", "out1"),
    ]);
    let inserts = chain.insert_blocks();
    assert_eq!(inserts.len(), 2);
    assert_eq!(inserts[0].0, 1);
    assert_eq!(inserts[1].0, 3);
}

#[test]
fn input_blocks_empty_chain_returns_empty() {
    let chain = make_chain(vec![]);
    assert!(chain.input_blocks().is_empty());
}

#[test]
fn output_blocks_empty_chain_returns_empty() {
    let chain = make_chain(vec![]);
    assert!(chain.output_blocks().is_empty());
}

#[test]
fn insert_blocks_no_inserts_returns_empty() {
    let chain = make_chain(vec![
        make_input_block("in:0", "b1", "in1"),
        make_output_block("out:0", "b1", "out1"),
    ]);
    assert!(chain.insert_blocks().is_empty());
}

// --- Chain::first_input / last_output ---

#[test]
fn first_input_returns_first_input_block() {
    let chain = make_chain(vec![
        make_input_block("in:0", "b1", "in1"),
        make_delay_block("fx:0"),
        make_input_block("in:1", "b2", "in1"),
        make_output_block("out:0", "b1", "out1"),
    ]);
    let first = chain.first_input().expect("should have first input");
    assert_eq!(first.io, "b1");
}

#[test]
fn first_input_empty_chain_returns_none() {
    let chain = make_chain(vec![]);
    assert!(chain.first_input().is_none());
}

#[test]
fn first_input_no_input_blocks_returns_none() {
    let chain = make_chain(vec![
        make_delay_block("fx:0"),
        make_output_block("out:0", "b1", "out1"),
    ]);
    assert!(chain.first_input().is_none());
}

#[test]
fn last_output_returns_last_output_block() {
    let chain = make_chain(vec![
        make_input_block("in:0", "b1", "in1"),
        make_output_block("out:0", "b1", "out1"),
        make_delay_block("fx:0"),
        make_output_block("out:1", "b2", "out1"),
    ]);
    let last = chain.last_output().expect("should have last output");
    assert_eq!(last.io, "b2");
}

#[test]
fn last_output_empty_chain_returns_none() {
    let chain = make_chain(vec![]);
    assert!(chain.last_output().is_none());
}

#[test]
fn last_output_no_output_blocks_returns_none() {
    let chain = make_chain(vec![
        make_input_block("in:0", "b1", "in1"),
        make_delay_block("fx:0"),
    ]);
    assert!(chain.last_output().is_none());
}

// --- ChainInputMode / ChainOutputMode defaults ---

#[test]
fn chain_input_mode_default_is_mono() {
    assert_eq!(ChainInputMode::default(), ChainInputMode::Mono);
}

#[test]
fn chain_output_mode_default_is_stereo() {
    assert_eq!(ChainOutputMode::default(), ChainOutputMode::Stereo);
}

#[test]
fn chain_output_mixdown_default_is_average() {
    assert_eq!(ChainOutputMixdown::default(), ChainOutputMixdown::Average);
}
