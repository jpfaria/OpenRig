//! Bug: every time the user switches scene/preset (or renames the active
//! preset) on a rig-backed chain, the synthetic legacy chain is re-projected
//! from the rig and the user's configured **output endpoint** (and inputs,
//! though the live symptom was the output) disappears. The MCP/rig nav path
//! ends up calling `rig_to_legacy_project` which rebuilds the chain from rig
//! state alone; I/O endpoints that were committed to the synthetic chain
//! (e.g. via `save_chain_output_endpoints`) live in the legacy project but
//! NOT in the rig, so the rebuild drops them.
//!
//! Live repro (today on `develop`):
//! 1. `add_chain` (CABELINHO) → empty chain.
//! 2. `save_chain_input_endpoints` + `save_chain_output_endpoints` → input/output added.
//! 3. `add_block` × N (build the tone).
//! 4. `rename_rig_preset` → ChainReloaded fires, chain re-projects → **output
//!    block vanishes from the persisted YAML**, audio engine loses its sink,
//!    output meter freezes at -120 dBFS.
//!
//! These RED tests pin the contract: rig-nav commands (RenameRigPreset,
//! ApplyRigNav) MUST preserve the chain's Input and Output blocks across the
//! re-projection.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{AudioBlock, AudioBlockKind, InputEntry, OutputBlock, OutputEntry};
use project::chain::{ChainInputMode, ChainOutputMode};
use project::rig::{RigInput, RigPreset, RigProject};

use application::command::{Command, RigNavKind};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;

const CHAIN_ID: &str = "rig:in";
const OUTPUT_BLOCK_ID: &str = "rig:in:out";
const DEVICE: &str = "test:device";

fn user_output_block() -> AudioBlock {
    AudioBlock {
        id: BlockId(OUTPUT_BLOCK_ID.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId(DEVICE.into()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    }
}

/// A rig with one input, two presets so we can switch between them, no
/// outputs configured at the rig layer (output endpoints live in the legacy
/// project only, just like the live MCP flow today).
fn rig_with_two_presets() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".to_string(),
        RigPreset::from_legacy_blocks(Vec::new(), 100.0),
    );
    presets.insert(
        "p2".to_string(),
        RigPreset::from_legacy_blocks(Vec::new(), 100.0),
    );
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".to_string());
    bank.insert(2, "p2".to_string());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".to_string(),
        RigInput {
            label: None,
            sources: vec![InputEntry {
                device_id: DeviceId(DEVICE.into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
            bank,
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
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

fn dispatcher_with_user_output() -> (
    LocalDispatcher,
    Rc<RefCell<project::project::Project>>,
    Rc<RefCell<RigProject>>,
) {
    let rig = Rc::new(RefCell::new(rig_with_two_presets()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    // The live MCP flow: user committed an output endpoint that lives only in
    // the legacy project (the rig has no `outputs` entry for this chain).
    for c in project.borrow_mut().chains.iter_mut() {
        if c.id.0 == CHAIN_ID {
            c.blocks
                .retain(|b| !matches!(b.kind, AudioBlockKind::Output(_)));
            c.blocks.push(user_output_block());
        }
    }
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    (dispatcher, project, rig)
}

fn output_blocks_on(
    project: &Rc<RefCell<project::project::Project>>,
    chain_id: &str,
) -> Vec<AudioBlock> {
    project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id.0 == chain_id)
        .map(|c| {
            c.blocks
                .iter()
                .filter(|b| matches!(b.kind, AudioBlockKind::Output(_)))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn rename_rig_preset_must_preserve_user_output_endpoint() {
    let (dispatcher, project, _rig) = dispatcher_with_user_output();
    assert_eq!(
        output_blocks_on(&project, CHAIN_ID).len(),
        1,
        "precondition: chain has the user's output block before rename"
    );

    dispatcher
        .dispatch(Command::RenameRigPreset {
            chain: ChainId(CHAIN_ID.into()),
            name: "Coldplay - Clocks".into(),
        })
        .expect("dispatch ok");

    let outputs_after = output_blocks_on(&project, CHAIN_ID);
    assert_eq!(
        outputs_after.len(),
        1,
        "RenameRigPreset re-projects the chain; the user's output endpoint \
         must survive the projection. The audio engine loses its sink \
         otherwise (meter freezes at -120 dBFS)."
    );
    assert_eq!(
        outputs_after[0].id.0, OUTPUT_BLOCK_ID,
        "the exact output block (same id) must be the one preserved"
    );
}

#[test]
fn apply_rig_nav_scene_switch_must_preserve_user_output_endpoint() {
    let (dispatcher, project, _rig) = dispatcher_with_user_output();
    assert_eq!(output_blocks_on(&project, CHAIN_ID).len(), 1);

    dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: ChainId(CHAIN_ID.into()),
            kind: RigNavKind::StepScene(1),
        })
        .expect("dispatch ok");

    assert_eq!(
        output_blocks_on(&project, CHAIN_ID).len(),
        1,
        "ApplyRigNav (scene step) must not wipe the user's output endpoint"
    );
}

#[test]
fn apply_rig_nav_preset_step_must_preserve_user_output_endpoint() {
    let (dispatcher, project, _rig) = dispatcher_with_user_output();
    assert_eq!(output_blocks_on(&project, CHAIN_ID).len(), 1);

    dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: ChainId(CHAIN_ID.into()),
            kind: RigNavKind::StepPreset(1),
        })
        .expect("dispatch ok");

    assert_eq!(
        output_blocks_on(&project, CHAIN_ID).len(),
        1,
        "ApplyRigNav (preset step) must not wipe the user's output endpoint"
    );
}
