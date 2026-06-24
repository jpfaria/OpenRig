//! #716 RED — after relating a binding and reopening the project, the chain
//! must still build an audio runtime so block edits take effect.
//!
//! User repro: a running chain doesn't respond to add/disable block, and logs
//! "block '…' not found in any input runtime of the chain". Root: the binding
//! selection is persisted as `io_binding_ids`, which the rig drops on reopen
//! (rig_sync.rs:144). The reopened chain is then UNBOUND → `build_chain_runtime`
//! produces NO runtime → there is no input runtime for any block to land in.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_cpal::{build_chain_runtime, BuildRequest};
use project::block::InputEntry;
use project::chain::ChainInputMode;
use project::rig::{RigInput, RigPreset, RigProject};

fn rig_with_chain() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert("p1".into(), RigPreset::from_legacy_blocks(Vec::new(), 100.0));
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".into());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".into(),
        RigInput {
            label: None,
            sources: vec![InputEntry {
                device_id: DeviceId("hw:0,0".to_string()),
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
            io_binding_ids: Vec::new(),
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

fn binding_main() -> IoBinding {
    IoBinding {
        id: "main".into(),
        name: "Main".into(),
        inputs: vec![IoEndpoint {
            name: "Guitar In".into(),
            device_id: DeviceId("hw:0,0".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Monitor Out".into(),
            device_id: DeviceId("hw:0,0".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}

#[test]
fn reopened_checklist_chain_still_builds_a_runtime() {
    let rig = Rc::new(RefCell::new(rig_with_chain()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    // User relates binding "main" via the editor checklist.
    dispatcher
        .dispatch(Command::SetChainIoBindings {
            chain: ChainId("rig:in".to_string()),
            binding_ids: vec!["main".to_string()],
        })
        .expect("SetChainIoBindings must succeed");

    // Reopen: project rebuilt from the persisted rig.
    let reopened =
        engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    let chain = reopened
        .chains
        .into_iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("chain must exist after reopen");

    // Start it: build the live runtime with the binding registry present.
    let req = BuildRequest {
        chain,
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024],
        io_bindings: vec![binding_main()],
    };
    let runtimes = build_chain_runtime(&req).expect("build must not error");

    assert!(
        !runtimes.is_empty(),
        "after relating binding 'main' and reopening, the chain must still build a runtime; \
         got 0 — the selection was lost (io_binding_ids dropped by the rig), the chain reopened \
         UNBOUND, so no input runtime exists and block edits report \
         'block not found in any input runtime of the chain'"
    );
}

#[test]
fn reopened_chain_runtime_owns_a_live_input() {
    let rig = Rc::new(RefCell::new(rig_with_chain()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    dispatcher
        .dispatch(Command::SetChainIoBindings {
            chain: ChainId("rig:in".to_string()),
            binding_ids: vec!["main".to_string()],
        })
        .expect("SetChainIoBindings must succeed");

    let reopened =
        engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    let chain = reopened
        .chains
        .into_iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("chain must exist after reopen");

    let req = BuildRequest {
        chain,
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024],
        io_bindings: vec![binding_main()],
    };
    let runtimes = build_chain_runtime(&req).expect("build must not error");

    let owns_input = runtimes
        .iter()
        .any(|(_, rt)| rt.input_cpal_index().is_some() && !rt.is_draining());
    assert!(
        owns_input,
        "after reopen the chain must own a live (non-draining) input runtime wired to a cpal \
         input; got {} runtime(s) — none own an input, so block edits land nowhere",
        runtimes.len()
    );
}
