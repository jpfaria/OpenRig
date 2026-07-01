//! Task 7 — Reshape SaveChainInput/OutputEndpoints + O3 delete-referenced rejection.
//!
//! New command shapes (replacing the old block-list variants):
//!
//!   SaveChainInputEndpoints  { chain, block_index, io, endpoint }
//!   SaveChainOutputEndpoints { chain, block_index, io, endpoint }
//!   SaveChainIo              { chain, input_block_index, output_block_index, io, endpoint }
//!
//! Each sets block.io = io and block.endpoint = endpoint on the target block
//! (identified by index inside the chain). The edit persists in the rig and
//! survives a rig→legacy reload.
//!
//! DeleteIoBinding must REJECT when any chain block has block.io == id.
//!
//! ## Test isolation
//!
//! Every test uses `TempDir` + `attach_config_path`. No `set_var("HOME")`.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{BlockId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
use project::project::Project;
use project::rig::{RigInput, RigPreset, RigProject};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn empty_project() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    }))
}

fn make_binding(id: &str, name: &str) -> IoBinding {
    IoBinding {
        id: id.to_string(),
        name: name.to_string(),
        inputs: vec![IoEndpoint {
            name: "Guitar In".to_string(),
            device_id: DeviceId("hw:0,0".to_string()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![],
    }
}

fn rig_with_chain() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".into(),
        RigPreset::from_legacy_blocks(Vec::new(), 100.0),
    );
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".into());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".into(),
        RigInput {
            label: None,
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

fn input_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

fn output_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

// ---------------------------------------------------------------------------
// save_input_endpoints_sets_reference
//
// Dispatch SaveChainInputEndpoints with the new shape { chain, block_index,
// io, endpoint } → the block at block_index gains io/endpoint in memory AND
// the mutation survives a rig→legacy reload.
// ---------------------------------------------------------------------------

#[test]
fn save_input_endpoints_sets_reference() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let cfg_path = tmp.path().join("config.yaml");

    let rig = Rc::new(RefCell::new(rig_with_chain()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));

    // Add the input block so block_index 0 is available.
    let chain_id_str = "rig:in";
    for c in project.borrow_mut().chains.iter_mut() {
        if c.id.0 == chain_id_str {
            // Input block is already seeded by rig_to_legacy_project at index 0.
            // If absent, push it.
            if c.blocks.iter().all(|b| !matches!(b.kind, AudioBlockKind::Input(_))) {
                c.blocks.insert(0, input_block("rig:in:in"));
            }
        }
    }

    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    dispatcher.attach_config_path(Some(cfg_path.clone()));

    // Dispatch the NEW-shape command.
    let res = dispatcher.dispatch(Command::SaveChainInputEndpoints {
        chain: domain::ids::ChainId(chain_id_str.to_string()),
        block_index: 0,
        io: "main".to_string(),
        endpoint: "Guitar In".to_string(),
    });
    assert!(
        res.is_ok(),
        "SaveChainInputEndpoints (new shape) dispatch failed: {:?}",
        res.err()
    );

    // In-memory: the block at index 0 must have io/endpoint set.
    let project_ref = project.borrow();
    let chain = project_ref
        .chains
        .iter()
        .find(|c| c.id.0 == chain_id_str)
        .expect("chain must exist");
    let input_blk = chain
        .blocks
        .iter()
        .find(|b| matches!(b.kind, AudioBlockKind::Input(_)))
        .expect("input block must exist");
    if let AudioBlockKind::Input(ib) = &input_blk.kind {
        assert_eq!(
            ib.io, "main",
            "input block io must be 'main' after SaveChainInputEndpoints"
        );
        assert_eq!(
            ib.endpoint, "Guitar In",
            "input block endpoint must be 'Guitar In' after SaveChainInputEndpoints"
        );
    } else {
        panic!("expected Input block");
    }
    drop(project_ref);

    // Reload: mutation must survive rig→legacy projection.
    let reloaded = engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    let chain = reloaded
        .chains
        .iter()
        .find(|c| c.id.0 == chain_id_str)
        .expect("reloaded chain must exist");
    let input_blk = chain
        .blocks
        .iter()
        .find(|b| matches!(b.kind, AudioBlockKind::Input(_)))
        .expect("input block must exist after reload");
    if let AudioBlockKind::Input(ib) = &input_blk.kind {
        assert_eq!(
            ib.io, "main",
            "input block io must survive rig→legacy reload"
        );
        assert_eq!(
            ib.endpoint, "Guitar In",
            "input block endpoint must survive rig→legacy reload"
        );
    } else {
        panic!("expected Input block after reload");
    }
}

// ---------------------------------------------------------------------------
// save_output_endpoints_sets_reference
//
// Same as above but for the output block.
// ---------------------------------------------------------------------------

#[test]
fn save_output_endpoints_sets_reference() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let cfg_path = tmp.path().join("config.yaml");

    let rig = Rc::new(RefCell::new(rig_with_chain()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));

    let chain_id_str = "rig:in";

    // Seed an output block on the chain (index 1, after the input at 0).
    for c in project.borrow_mut().chains.iter_mut() {
        if c.id.0 == chain_id_str {
            // Ensure there's an output block somewhere.
            if c.blocks.iter().all(|b| !matches!(b.kind, AudioBlockKind::Output(_))) {
                c.blocks.push(output_block("rig:in:out"));
            }
        }
    }

    // Find the output block index.
    let output_idx = {
        let p = project.borrow();
        let chain = p.chains.iter().find(|c| c.id.0 == chain_id_str).unwrap();
        chain
            .blocks
            .iter()
            .position(|b| matches!(b.kind, AudioBlockKind::Output(_)))
            .expect("output block must be present")
    };

    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    dispatcher.attach_config_path(Some(cfg_path));

    let res = dispatcher.dispatch(Command::SaveChainOutputEndpoints {
        chain: domain::ids::ChainId(chain_id_str.to_string()),
        block_index: output_idx,
        io: "main".to_string(),
        endpoint: "Monitor Out".to_string(),
    });
    assert!(
        res.is_ok(),
        "SaveChainOutputEndpoints (new shape) dispatch failed: {:?}",
        res.err()
    );

    // In-memory check.
    {
        let project_ref = project.borrow();
        let chain = project_ref
            .chains
            .iter()
            .find(|c| c.id.0 == chain_id_str)
            .expect("chain must exist");
        let output_blk = chain
            .blocks
            .iter()
            .find(|b| matches!(b.kind, AudioBlockKind::Output(_)))
            .expect("output block must exist");
        if let AudioBlockKind::Output(ob) = &output_blk.kind {
            assert_eq!(
                ob.io, "main",
                "output block io must be 'main' after SaveChainOutputEndpoints"
            );
            assert_eq!(
                ob.endpoint, "Monitor Out",
                "output block endpoint must be 'Monitor Out'"
            );
        } else {
            panic!("expected Output block");
        }
    }

    // Reload check — must survive rig→legacy projection.
    let reloaded = engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    let chain = reloaded
        .chains
        .iter()
        .find(|c| c.id.0 == chain_id_str)
        .expect("reloaded chain must exist");
    let output_blk = chain
        .blocks
        .iter()
        .find(|b| matches!(b.kind, AudioBlockKind::Output(_)))
        .expect("output block must exist after reload");
    if let AudioBlockKind::Output(ob) = &output_blk.kind {
        assert_eq!(
            ob.io, "main",
            "output block io must survive rig→legacy reload"
        );
        assert_eq!(
            ob.endpoint, "Monitor Out",
            "output block endpoint must survive rig→legacy reload"
        );
    } else {
        panic!("expected Output block after reload");
    }
}

// ---------------------------------------------------------------------------
// delete_referenced_binding_rejected
//
// When any chain block has block.io == id, DeleteIoBinding must return Err
// (not remove the binding). The error message must name the referencing chain.
// ---------------------------------------------------------------------------

#[test]
fn delete_referenced_binding_rejected() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let cfg_path = tmp.path().join("config.yaml");

    let dispatcher = LocalDispatcher::new(empty_project());
    dispatcher.attach_config_path(Some(cfg_path.clone()));

    // Create the binding first.
    dispatcher
        .dispatch(Command::CreateIoBinding {
            binding: make_binding("main", "Main"),
        })
        .expect("CreateIoBinding ok");

    // Create a project with a chain whose input block references "main".
    let mut chain = project::chain::Chain {
        id: domain::ids::ChainId("my-chain".to_string()),
        description: Some("My Chain".to_string()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
    };
    let mut blk = input_block("my-chain:in");
    if let AudioBlockKind::Input(ref mut ib) = blk.kind {
        ib.io = "main".to_string();
        ib.endpoint = "Guitar In".to_string();
    }
    chain.blocks.push(blk);

    // Re-create a dispatcher backed by a project with the referencing chain.
    let project_with_ref = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain],
        midi: None,
    }));
    let dispatcher2 = LocalDispatcher::new(Rc::clone(&project_with_ref));
    dispatcher2.attach_config_path(Some(cfg_path.clone()));

    // Also pre-seed the binding so it exists in config.
    dispatcher2
        .dispatch(Command::CreateIoBinding {
            binding: make_binding("main", "Main"),
        })
        .expect("seed binding ok");
    application::persist_worker::flush();

    // Now attempt to delete the referenced binding.
    let result = dispatcher2.dispatch(Command::DeleteIoBinding {
        id: "main".to_string(),
    });

    assert!(
        result.is_err(),
        "DeleteIoBinding must be rejected when a chain block references the binding"
    );
    let err_msg = format!("{:?}", result.err().unwrap());
    assert!(
        err_msg.contains("my-chain") || err_msg.contains("referenced"),
        "error must mention the referencing chain or 'referenced'; got: {err_msg}"
    );

    // Binding must still be present in config.
    application::persist_worker::flush();
    if cfg_path.exists() {
        let raw = std::fs::read_to_string(&cfg_path).expect("read config.yaml");
        assert!(
            raw.contains("main"),
            "binding 'main' must still be present in config.yaml after rejected delete; got:\n{raw}"
        );
    }
}

// ---------------------------------------------------------------------------
// delete_unreferenced_binding_ok
//
// When no chain block references the binding, DeleteIoBinding must succeed.
// ---------------------------------------------------------------------------

#[test]
fn delete_unreferenced_binding_ok() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let cfg_path = tmp.path().join("config.yaml");

    // Project with a chain whose input block references "other", NOT "main".
    let mut chain = project::chain::Chain {
        id: domain::ids::ChainId("my-chain".to_string()),
        description: Some("My Chain".to_string()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
    };
    let mut blk = input_block("my-chain:in");
    if let AudioBlockKind::Input(ref mut ib) = blk.kind {
        ib.io = "other".to_string();
        ib.endpoint = "Guitar In".to_string();
    }
    chain.blocks.push(blk);

    let project_with_ref = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project_with_ref));
    dispatcher.attach_config_path(Some(cfg_path.clone()));

    dispatcher
        .dispatch(Command::CreateIoBinding {
            binding: make_binding("main", "Main"),
        })
        .expect("CreateIoBinding ok");

    // No chain references "main", so delete must succeed.
    let result = dispatcher.dispatch(Command::DeleteIoBinding {
        id: "main".to_string(),
    });
    assert!(
        result.is_ok(),
        "DeleteIoBinding must succeed when no chain block references it; got: {:?}",
        result.err()
    );

    application::persist_worker::flush();
    if cfg_path.exists() {
        let raw = std::fs::read_to_string(&cfg_path).expect("read config.yaml");
        assert!(
            !raw.contains("\"main\"") && !raw.contains("id: main"),
            "deleted unreferenced binding must be absent from config.yaml; got:\n{raw}"
        );
    }
}
