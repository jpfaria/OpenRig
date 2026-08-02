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
