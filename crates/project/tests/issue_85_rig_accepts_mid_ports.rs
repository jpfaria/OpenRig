//! #85 — a mid `Input`/`Output` port must be storable in a preset.
//!
//! The rig format rejects ANY I/O block inside a preset ("presets are
//! processing-only"), which is why a mid port the user places never reaches
//! `project.yaml`: the block exists in memory, disappears on save. `Insert` is
//! already accepted there and is exactly the same kind of thing — a port the
//! user placed between effects — so `Input`/`Output` must be too.
//!
//! What the rule still has to reject is the LEGACY head/tail leftover: a port
//! bound to the very binding the chain already carries (#716) duplicates the
//! chain's own I/O and starves the device.

use std::collections::BTreeMap;

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, InsertBlock, OutputBlock};
use project::rig::{RigInput, RigPreset, RigProject};

fn output_port(io: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:in:aux".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: io.into(),
            endpoint: "Out 1".into(),
        }),
    }
}

fn insert_port() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:in:ins".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".into(),
            io: "aux".into(),
        }),
    }
}

fn rig_with(blocks: Vec<AudioBlock>) -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".to_string(),
        RigPreset::from_legacy_blocks(blocks, 100.0),
    );
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".to_string());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".to_string(),
        RigInput {
            label: None,
            bank,
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
            io_binding_ids: vec!["main".to_string()],
            loopers: Vec::new(),
        },
    );
    RigProject {
        name: None,
        inputs,
        presets,
        outputs: BTreeMap::new(),
        chain_order: Vec::new(),
        midi: None,
    }
}

#[test]
fn a_preset_may_carry_a_mid_output_port() {
    let rig = rig_with(vec![output_port("aux")]);
    assert!(
        rig.validate().is_ok(),
        "#85: a mid Output pointing at another E/S is a port the user placed — the \
         same thing an Insert is, and Insert is accepted. Rejecting it is why the \
         block never reaches project.yaml: {:?}",
        rig.validate()
    );
}

#[test]
fn a_preset_may_carry_an_insert_port() {
    // Baseline: the rule already accepts Insert. Pins that this test's rig shape
    // is otherwise valid, so the case above fails for the port rule alone.
    assert!(rig_with(vec![insert_port()]).validate().is_ok());
}
