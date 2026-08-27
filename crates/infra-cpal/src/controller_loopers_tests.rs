//! #323 phase 2 — the loop plays through its LINKED preset, not the chain's
//! current preset. `looper_playback_chain` is the pure block-swap that makes
//! that true while keeping the chain's identity and routing (invariant #4).

use super::looper_playback_chain;
use domain::ids::ChainId;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;

fn block(id: &str) -> AudioBlock {
    AudioBlock {
        id: domain::ids::BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "clean".into(),
            params: project::param::ParameterSet::default(),
        }),
    }
}

fn chain() -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["scarlett".into()],
        blocks: vec![block("live-amp")],
        di_output: None,
        loopers: vec![],
    }
}

#[test]
fn linked_preset_blocks_replace_the_chains_processing_but_keep_identity() {
    let c = chain();
    let swapped = looper_playback_chain(&c, Some(vec![block("lead-drive"), block("lead-verb")]));

    // The tone comes from the linked preset...
    let ids: Vec<_> = swapped.blocks.iter().map(|b| b.id.0.clone()).collect();
    assert_eq!(ids, vec!["lead-drive", "lead-verb"]);
    // ...while identity and routing stay the chain's, so the isolated stream
    // still resolves the same output and remains isolated (invariant #4).
    assert_eq!(swapped.id, c.id);
    assert_eq!(swapped.io_binding_ids, c.io_binding_ids);
}

#[test]
fn no_linked_preset_plays_through_the_chains_own_blocks() {
    let c = chain();
    let unchanged = looper_playback_chain(&c, None);
    let ids: Vec<_> = unchanged.blocks.iter().map(|b| b.id.0.clone()).collect();
    assert_eq!(
        ids,
        vec!["live-amp"],
        "no link ⇒ the chain's current preset"
    );
    assert_eq!(unchanged.id, c.id);
}
