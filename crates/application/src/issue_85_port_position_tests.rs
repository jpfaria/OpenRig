//! #85 — switching preset/scene must not MOVE the user's mid ports.
//!
//! The rig-nav rebuild preserves the chain's `Input`/`Output` blocks across the
//! swap, but it rebuilt the list as `[all inputs] + [effects] + [all outputs]`.
//! That was harmless when the only I/O blocks were the head input and the tail
//! output; with #85 a port sits BETWEEN effects on purpose, and its position is
//! the whole point — an aux send after the cab must not jump to the end of the
//! chain, where it would emit the full chain instead of the cab.

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, OutputBlock};
use project::param::ParameterSet;

use crate::local_dispatcher_rig::merge_preserved_ports;

fn effect(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model: "digital_clean".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn port(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "Out 1".into(),
        }),
    }
}

#[test]
fn a_mid_port_keeps_its_position_across_a_preset_switch() {
    // Live chain: [cab, aux-send, delay]. The preset carries only its effects —
    // the ports belong to the chain and are merged back in.
    let current = vec![effect("cab"), port("aux"), effect("delay")];
    let rebuilt = vec![effect("cab"), effect("delay")];

    let merged = merge_preserved_ports(&current, rebuilt);
    let ids: Vec<&str> = merged.iter().map(|b| b.id.0.as_str()).collect();

    assert_eq!(
        ids,
        vec!["cab", "aux", "delay"],
        "#85: the aux send sits between the cab and the delay — a preset switch \
         must not push it to the end of the chain"
    );
}

#[test]
fn a_port_at_the_head_and_tail_still_lands_at_the_head_and_tail() {
    let current = vec![port("head"), effect("cab"), port("tail")];
    let rebuilt = vec![effect("cab")];

    let merged = merge_preserved_ports(&current, rebuilt);
    let ids: Vec<&str> = merged.iter().map(|b| b.id.0.as_str()).collect();

    assert_eq!(ids, vec!["head", "cab", "tail"]);
}

fn insert(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Insert(project::block::InsertBlock {
            model: "external_loop".into(),
            io: "fx".into(),
        }),
    }
}

/// #881 — an `Insert` is routing too: it splits the chain into the segment that
/// feeds the SEND and the one the RETURN comes back into, so its position is
/// the whole point, exactly like a mid port. Treating it as an effect slot made
/// a preset switch hand that slot to the next rebuilt effect and DROP the loop:
/// the pedal in front of the insert stopped colouring the send because there
/// was no send any more.
#[test]
fn an_insert_keeps_its_position_across_a_preset_switch() {
    // Live chain: [drive, synergy-loop, eq]. The preset carries its effects only.
    let current = vec![effect("drive"), insert("loop"), effect("eq")];
    let rebuilt = vec![effect("drive"), effect("eq")];

    let merged = merge_preserved_ports(&current, rebuilt);
    let ids: Vec<&str> = merged.iter().map(|b| b.id.0.as_str()).collect();

    assert_eq!(
        ids,
        vec!["drive", "loop", "eq"],
        "#881: the external loop must stay between the drive and the eq — a \
         preset switch must not consume its slot and drop it"
    );
}

fn insert_enabled(id: &str, enabled: bool) -> AudioBlock {
    AudioBlock {
        enabled,
        ..insert(id)
    }
}

/// #921 — a scene's `bypass` on an `Insert` reaches the chain. The merge keeps
/// the insert's SLOT from the current chain (#881), but the rebuilt block with
/// the same id already carries the scene-applied `enabled`; cloning the
/// current insert whole threw that away, so the loop kept whatever state it
/// had before the switch while every other block followed the scene.
#[test]
fn an_insert_takes_the_scene_enabled_from_the_rebuilt_block() {
    let current = vec![effect("drive"), insert_enabled("loop", true), effect("amp")];
    let rebuilt = vec![
        effect("drive"),
        insert_enabled("loop", false),
        effect("amp"),
    ];

    let merged = merge_preserved_ports(&current, rebuilt);
    let ids: Vec<&str> = merged.iter().map(|b| b.id.0.as_str()).collect();

    assert_eq!(ids, vec!["drive", "loop", "amp"], "slot preserved");
    assert!(
        !merged[1].enabled,
        "#921: the scene bypasses the insert — the merged chain must carry \
         enabled=false, not the stale enabled=true from before the switch"
    );
}

/// #921 — and back: a scene that does NOT bypass the insert re-enables it
/// even when the user had toggled it off by hand in the previous scene.
#[test]
fn an_insert_re_enabled_by_the_scene_comes_back_on() {
    let current = vec![
        effect("drive"),
        insert_enabled("loop", false),
        effect("amp"),
    ];
    let rebuilt = vec![effect("drive"), insert_enabled("loop", true), effect("amp")];

    let merged = merge_preserved_ports(&current, rebuilt);

    assert!(
        merged[1].enabled,
        "#921: the scene leaves the insert on — the manual off from the \
         previous scene must not survive the switch"
    );
}
