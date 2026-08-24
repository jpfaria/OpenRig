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
