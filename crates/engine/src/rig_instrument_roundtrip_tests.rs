//! Tests for chain instrument persistence across the rig round-trip (#627).
//!
//! Verifies that setting a non-default instrument on a rig chain (e.g. acoustic_guitar)
//! survives: legacy-chain mutation → sync_synthetic_into_rig → rig_to_legacy_project.

use std::collections::{BTreeMap, BTreeSet};

use domain::ids::DeviceId;
use project::block::InputEntry;
use project::chain::ChainInputMode;
use project::rig::{RigInput, RigPreset, RigProject};

use crate::rig_runtime::rig_to_legacy_project;
use project::rig_sync::sync_synthetic_into_rig;

fn simple_rig() -> RigProject {
    RigProject {
        name: Some("Test".into()),
        inputs: [(
            "input-1".to_string(),
            RigInput {
                label: None,
                sources: vec![InputEntry {
                    device_id: DeviceId("sc".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                }],
                bank: BTreeMap::from([(1usize, "p".to_string())]),
                active_preset: 1,
                active_scene: 1,
                routing: vec![],
                instrument: block_core::DEFAULT_INSTRUMENT.to_string(),
            },
        )]
        .into_iter()
        .collect(),
        outputs: Default::default(),
        presets: [(
            "p".to_string(),
            RigPreset::from_legacy_blocks(vec![], 100.0),
        )]
        .into_iter()
        .collect(),
        midi: None,
        chain_order: vec![],
    }
}

/// Mutating chain.instrument to "acoustic_guitar" and calling sync_synthetic_into_rig
/// must capture the new value into RigInput.instrument.
/// Re-projecting via rig_to_legacy_project must emit the acoustic instrument (not electric).
#[test]
fn instrument_survives_sync_and_reprojection() {
    let mut rig = simple_rig();

    // Project to legacy chains
    let mut proj = rig_to_legacy_project(&rig, &BTreeSet::new());

    // Simulate user changing the instrument on the projected chain
    let chain = proj
        .chains
        .iter_mut()
        .find(|c| c.id.0 == "rig:input-1")
        .unwrap();
    chain.instrument = block_core::INST_ACOUSTIC_GUITAR.to_string();

    // Sync legacy state back into the rig model
    sync_synthetic_into_rig(&mut rig, &proj);

    // Re-project: instrument must survive
    let proj2 = rig_to_legacy_project(&rig, &BTreeSet::new());
    let c = proj2
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:input-1")
        .unwrap();
    assert_eq!(
        c.instrument,
        block_core::INST_ACOUSTIC_GUITAR,
        "instrument must survive sync→reprojection (was reverted to electric_guitar before fix)"
    );
}
