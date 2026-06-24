//! #716 RED — a binding-bound chain must be NORMALIZED to the new format on
//! load: its I/O is the system binding (io_binding_ids); it must carry NO I/O
//! blocks. Legacy leftovers (an old Input/Output block on the chain) are the
//! bug — they must be dropped, not kept.
//!
//! User repro: project TESTE chain 1 has io_binding_ids:[io-1-50c4] AND a
//! leftover Output block on the same Scarlett the binding outputs to. That
//! duplicate output starves the device (absurd latency + underruns) and skews
//! the runtime topology. Keep the chain in the new format only.

use std::collections::{BTreeMap, BTreeSet};

use domain::ids::{BlockId, DeviceId};
use engine::rig_runtime::rig_to_legacy_project;
use project::block::{
    AudioBlock, AudioBlockKind, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{ChainInputMode, ChainOutputMode};
use project::rig::{RigInput, RigPreset, RigProject};

fn legacy_output_block() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:in:out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![OutputEntry {
                device_id: DeviceId("scarlett".into()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    }
}

#[test]
fn bound_chain_drops_legacy_io_blocks_on_load() {
    // A bound input whose preset still carries a legacy Output block.
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".to_string(),
        RigPreset::from_legacy_blocks(vec![legacy_output_block()], 100.0),
    );
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".to_string());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".to_string(),
        RigInput {
            label: None,
            sources: vec![InputEntry {
                device_id: DeviceId("scarlett".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
            bank,
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
            io_binding_ids: vec!["main".to_string()],
        },
    );
    let rig = RigProject {
        name: None,
        inputs,
        presets,
        outputs: BTreeMap::new(),
        chain_order: Vec::new(),
        midi: None,
    };

    let project = rig_to_legacy_project(&rig, &BTreeSet::new());
    let chain = project.chains.first().expect("one chain");

    let io_blocks = chain
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
        .count();
    assert_eq!(
        io_blocks, 0,
        "a binding-bound chain (io_binding_ids) must be normalized to the new format with NO \
         I/O blocks — its I/O is the system binding. Got {io_blocks} legacy I/O block(s); a \
         leftover output block duplicates the binding's output and starves the device."
    );
}
