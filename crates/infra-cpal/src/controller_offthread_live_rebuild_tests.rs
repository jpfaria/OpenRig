//! #328: a VST3 inside a split path must take the #779 in-place live rebuild.
//! A fresh off-thread build would call `createInstance` while the audio thread
//! is inside the old instance's `process()` — the JUCE SIGSEGV #779 fixed.

use domain::ids::BlockId;
use project::block::split_params::default_split_params;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd};
use project::param::ParameterSet;

use super::chain_contains_vst3;
use crate::controller_live_edit_replicates_user_report_tests::gain_chain;

fn vst3_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_VST3.into(),
            model: "vst3:Missing:Missing".into(),
            params: ParameterSet::default(),
        }),
    }
}

#[test]
fn a_vst3_inside_a_split_path_takes_the_in_place_rebuild() {
    let mut chain = gain_chain(100.0);
    chain.blocks.push(AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Mix,
            params: default_split_params(),
            a: vec![],
            b: vec![vst3_block("amp_b")],
        }),
    });
    assert!(
        chain_contains_vst3(&chain),
        "#779: a VST3 in path B must be updated in place, never re-instantiated off-thread"
    );
}
