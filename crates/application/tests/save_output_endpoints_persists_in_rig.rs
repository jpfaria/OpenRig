//! Bug repro (user): "I fix the output on the Coldplay - Clocks preset
//! of CABELINHO and it never persists" -- the user edits the chain's
//! output endpoint, the legacy chain reflects the edit, but as soon as
//! the chain re-projects from the rig (reload, scene switch, runtime
//! resync), the edit is gone.
//!
//! Cause: `Command::SaveChainOutputEndpoints` (and the matching Input /
//! SaveChainIo handlers) only writes the I/O blocks into the legacy
//! `Project`. `rig.outputs` stays empty for the chain. Any subsequent
//! `rig_to_legacy_project` rebuild therefore drops the edited output.
//! The dispatcher's `ApplyRigNav` step-4 fix preserves I/O in memory,
//! but the next save+reload (or any code path that re-projects from
//! the rig) still loses it because the persistence layer keeps the
//! rig as source of truth and the rig never learned about the output.
//!
//! Pin the contract: a successful `SaveChainOutputEndpoints` on a
//! rig-backed chain must propagate the endpoint into `rig.outputs` so
//! the next `rig_to_legacy_project` produces the same chain. Symmetric
//! for `SaveChainInputEndpoints` (already works through
//! `rig.inputs.sources`) and `SaveChainIo`.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{ChainInputMode, ChainOutputMode};
use project::rig::{RigInput, RigPreset, RigProject};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;

const INPUT_NAME: &str = "in";
const CHAIN_ID: &str = "rig:in";
const DEVICE: &str = "test:device";

fn user_input() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:in:in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId(DEVICE.into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }
}

fn user_output() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:in:out".into()),
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

fn rig_with_input() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".into(),
        RigPreset::from_legacy_blocks(Vec::new(), 100.0),
    );
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".into());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        INPUT_NAME.into(),
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
            instrument: "electric_guitar".to_string(),
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

fn outputs_count_on(chain: &project::chain::Chain) -> usize {
    chain
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, AudioBlockKind::Output(_)))
        .count()
}

#[test]
fn save_chain_output_endpoints_persists_through_rig_reload() {
    // Setup: rig with one input, no outputs. Project projected from
    // rig -- chain `rig:in` exists with input only.
    let rig = Rc::new(RefCell::new(rig_with_input()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    // 1) User dispatches SaveChainOutputEndpoints with their output.
    let res = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: ChainId(CHAIN_ID.into()),
        output_blocks: vec![user_output()],
    });
    assert!(
        res.is_ok(),
        "SaveChainOutputEndpoints dispatch failed: {:?}",
        res.err()
    );

    let in_memory = project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id.0 == CHAIN_ID)
        .map(outputs_count_on)
        .unwrap_or(0);
    assert_eq!(
        in_memory, 1,
        "precondition: in-memory chain gained the output"
    );

    // 2) Simulate reload: rebuild the synthetic chain from the rig.
    //    This is what `load_rig_and_project` and every rig-driven
    //    re-projection path do. The output must survive.
    let reloaded = engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    let chain = reloaded
        .chains
        .iter()
        .find(|c| c.id.0 == CHAIN_ID)
        .expect("reloaded project has the rig-backed chain");
    assert_eq!(
        outputs_count_on(chain),
        1,
        "SaveChainOutputEndpoints must propagate the output into rig.outputs \
         so the next rig→legacy projection (reload, scene switch, runtime \
         resync) keeps the user's sink. Today rig.outputs stays empty, so \
         the output disappears after every projection."
    );
}

#[test]
fn save_chain_io_persists_both_endpoints_through_rig_reload() {
    let rig = Rc::new(RefCell::new(rig_with_input()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    // SaveChainIo expects both blocks to already exist on the chain
    // (it replaces in place). Seed an initial Output to satisfy that
    // precondition; the test then asserts the *replacement* persists
    // through reload.
    for c in project.borrow_mut().chains.iter_mut() {
        if c.id.0 == CHAIN_ID {
            c.blocks.push(user_output());
        }
    }
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    let res = dispatcher.dispatch(Command::SaveChainIo {
        chain: ChainId(CHAIN_ID.into()),
        input_block: user_input(),
        output_block: user_output(),
    });
    assert!(res.is_ok(), "SaveChainIo dispatch failed: {:?}", res.err());

    let reloaded = engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    let chain = reloaded
        .chains
        .iter()
        .find(|c| c.id.0 == CHAIN_ID)
        .expect("reloaded project has the rig-backed chain");
    let inputs = chain
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, AudioBlockKind::Input(_)))
        .count();
    let outputs = outputs_count_on(chain);
    assert_eq!(
        outputs, 1,
        "SaveChainIo output must persist through rig→legacy projection"
    );
    assert_eq!(
        inputs, 1,
        "SaveChainIo input must persist through rig→legacy projection"
    );
}
