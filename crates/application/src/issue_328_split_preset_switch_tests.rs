//! #328 — a split belongs to the PRESET, not to the chain.
//!
//! `merge_preserved_ports` keeps the chain's own ports in their slots across a
//! preset/scene switch. A split is one of the preset's blocks: switching preset
//! must bring THAT preset's split (or none), never keep the old one.

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, OutputBlock, SplitBlock, SplitEnd};
use project::param::ParameterSet;

use super::merge_preserved_ports;

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

fn split(id: &str, end: SplitEnd, a: Vec<AudioBlock>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            a,
            ..SplitBlock::new(end)
        }),
    }
}

fn aux_send(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "Send".into(),
        }),
    }
}

fn ids(blocks: &[AudioBlock]) -> Vec<&str> {
    blocks.iter().map(|b| b.id.0.as_str()).collect()
}

#[test]
fn switching_to_a_preset_brings_its_own_split() {
    let current = vec![
        effect("drive"),
        split("old-split", SplitEnd::Mix, vec![effect("amp-old")]),
        aux_send("aux"),
    ];
    let rebuilt = vec![
        effect("comp"),
        split("new-split", SplitEnd::Mix, vec![effect("amp-new")]),
    ];
    let merged = merge_preserved_ports(&current, rebuilt);
    assert_eq!(
        ids(&merged),
        vec!["comp", "new-split", "aux"],
        "#328: the new preset's split replaces the old one; the aux send keeps its slot"
    );
}

#[test]
fn switching_to_a_preset_brings_its_own_y_split() {
    // A Y split is routing (`is_routing() == true`), yet it still travels with
    // the preset — the merge keeps ports only, never every routing block.
    let current = vec![
        split("old-y", SplitEnd::Y, vec![effect("amp-old")]),
        aux_send("aux"),
    ];
    let rebuilt = vec![split("new-y", SplitEnd::Y, vec![effect("amp-new")])];
    let merged = merge_preserved_ports(&current, rebuilt);
    assert_eq!(ids(&merged), vec!["new-y", "aux"]);
}

#[test]
fn switching_to_a_preset_without_a_split_drops_the_old_one() {
    let current = vec![
        split("old-split", SplitEnd::Y, vec![effect("amp-old")]),
        aux_send("aux"),
    ];
    let rebuilt = vec![effect("comp")];
    let merged = merge_preserved_ports(&current, rebuilt);
    assert_eq!(ids(&merged), vec!["comp", "aux"]);
}
