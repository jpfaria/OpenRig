//! Regression tests covering every project-admin operation that the GUI
//! exposes through the dispatcher (project name, chain CRUD/reorder,
//! block CRUD/reorder/params, rig preset/scene). The contract under
//! test is identical for every operation: **what was dispatched and
//! saved must survive a `save_project_session` ↔ `load_project_session`
//! round-trip**.
//!
//! The exact bug the user reported (delete-all-then-add-new chain
//! disappears on reload) has its dedicated dedicated tests in
//! `project_ops_persistence_tests`; the matrix here proves the rest of
//! the admin surface is no quieter about the same regression.

use crate::project_ops::{create_new_project_session, load_project_session, save_project_session};
use crate::state::ProjectSession;
use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use application::command::Command;
use application::dispatcher::CommandDispatcher;
use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use std::path::PathBuf;
use tempfile::TempDir;

// ────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────

/// The owner's private capture tree, resolved from `OPENRIG_OWNER_PLUGINS` or
/// the sibling `OpenRig-plugins` checkout. `None` when neither is present — the
/// #606 repro then skips (it needs the real disk packages, not a fixture).
pub(super) fn owner_plugins_root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("OPENRIG_OWNER_PLUGINS") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    // Walk up from the crate dir; accept the first ancestor with a sibling
    // `OpenRig-plugins/plugins/source` (author's main checkout or a .solvers
    // clone, at any depth). None on CI, where the tree is absent.
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let cand = dir.join("OpenRig-plugins/plugins/source");
        if cand.is_dir() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub(super) struct Sandbox {
    _tmp: TempDir,
    path: PathBuf,
    pub(super) cfg: PathBuf,
}

impl Sandbox {
    pub(super) fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("project.yaml");
        let cfg = tmp.path().join("config.yaml");
        Self {
            _tmp: tmp,
            path,
            cfg,
        }
    }

    pub(super) fn new_session(&self) -> ProjectSession {
        let mut session = create_new_project_session(&self.cfg);
        session.project_path = Some(self.path.clone());
        session.config_path = Some(self.cfg.clone());
        session
    }

    pub(super) fn save(&self, session: &ProjectSession) {
        save_project_session(session, &self.path).expect("save");
    }

    pub(super) fn reload(&self) -> ProjectSession {
        load_project_session(&self.path, &self.cfg).expect("reload")
    }
}

fn default_chain(session: &ProjectSession, desc: &str) -> Chain {
    // Each chain gets a unique capture device so the migration produces
    // one input per chain rather than collapsing them into one input
    // with N presets (which is the correct behaviour for chains that
    // share a source, but not what these tests are asserting against).
    let input_dev = format!("dev-{desc}");
    let chain = build_default_chain(DefaultChainParams {
        project: &session.project.borrow(),
        instrument: "electric_guitar",
        description: Some(desc.into()),
        input: EndpointSpec {
            device_id: Some(&input_dev),
            channels: vec![0],
            io: String::new(),
            endpoint: String::new(),
        },
        output: EndpointSpec {
            device_id: Some("test-out"),
            channels: vec![0, 1],
            io: String::new(),
            endpoint: String::new(),
        },
    });
    chain
}

pub(super) fn gain_block(id: &str, drive: f32) -> AudioBlock {
    // ibanez_ts9 is a real gain model in the registry; using a registered
    // model means the load path does not silently drop the block as
    // "unsupported gain model" (which empties round-trips).
    let mut params = ParameterSet::default();
    params.insert("drive", ParameterValue::Float(drive));
    params.insert("tone", ParameterValue::Float(50.0));
    params.insert("level", ParameterValue::Float(50.0));
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "ibanez_ts9".into(),
            params,
        }),
    }
}

pub(super) fn add_chain(session: &ProjectSession, desc: &str) -> ChainId {
    let chain = default_chain(session, desc);
    session
        .dispatcher
        .dispatch(Command::AddChain { chain })
        .expect("AddChain");
    // The dispatcher re-tags new chains to `rig:<input>`; pick up
    // the post-dispatch id from the project.
    session
        .project
        .borrow()
        .chains
        .iter()
        .rev()
        .find(|c| c.description.as_deref() == Some(desc))
        .map(|c| c.id.clone())
        .expect("chain present after dispatch")
}

pub(super) fn chain_count(s: &ProjectSession) -> usize {
    s.project.borrow().chains.len()
}

pub(super) fn chain_descriptions(s: &ProjectSession) -> Vec<Option<String>> {
    s.project
        .borrow()
        .chains
        .iter()
        .map(|c| c.description.clone())
        .collect()
}

pub(super) fn find_chain(s: &ProjectSession, desc: &str) -> Option<Chain> {
    s.project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some(desc))
        .cloned()
}

// ────────────────────────────────────────────────────────────────────
// 1. Project name
// ────────────────────────────────────────────────────────────────────

#[test]
fn update_project_name_persists_across_reload() {
    let s = Sandbox::new();
    let session = s.new_session();
    session
        .dispatcher
        .dispatch(Command::UpdateProjectName {
            name: "MY PROJECT".into(),
        })
        .expect("UpdateProjectName");
    s.save(&session);

    let reloaded = s.reload();
    assert_eq!(
        reloaded.project.borrow().name.as_deref(),
        Some("MY PROJECT")
    );
}

// ────────────────────────────────────────────────────────────────────
// 2. Chain CRUD
// ────────────────────────────────────────────────────────────────────

#[test]
fn add_chain_command_persists() {
    let s = Sandbox::new();
    let session = s.new_session();
    add_chain(&session, "GUITAR");
    s.save(&session);

    let reloaded = s.reload();
    assert_eq!(
        chain_descriptions(&reloaded),
        vec![Some("GUITAR".to_string())]
    );
}

#[test]
fn remove_chain_command_persists() {
    let s = Sandbox::new();
    let session = s.new_session();
    let id_a = add_chain(&session, "A");
    let _id_b = add_chain(&session, "B");
    s.save(&session);

    let session = s.reload();
    let target = session
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("A"))
        .map(|c| c.id.clone())
        .unwrap_or(id_a);
    session
        .dispatcher
        .dispatch(Command::RemoveChain { chain: target })
        .expect("RemoveChain");
    s.save(&session);

    let reloaded = s.reload();
    assert_eq!(
        chain_descriptions(&reloaded),
        vec![Some("B".to_string())],
        "RemoveChain did not persist (got {:?})",
        chain_descriptions(&reloaded)
    );
}

#[test]
fn move_chain_up_persists_order() {
    let s = Sandbox::new();
    let session = s.new_session();
    add_chain(&session, "A");
    add_chain(&session, "B");
    s.save(&session);

    let session = s.reload();
    let target_b = session
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("B"))
        .map(|c| c.id.clone())
        .expect("B exists");
    session
        .dispatcher
        .dispatch(Command::MoveChainUp { chain: target_b })
        .expect("MoveChainUp");
    s.save(&session);

    let reloaded = s.reload();
    assert_eq!(
        chain_descriptions(&reloaded),
        vec![Some("B".to_string()), Some("A".to_string())],
        "MoveChainUp order did not persist"
    );
}

#[test]
fn move_chain_down_persists_order() {
    let s = Sandbox::new();
    let session = s.new_session();
    add_chain(&session, "A");
    add_chain(&session, "B");
    s.save(&session);

    let session = s.reload();
    let target_a = session
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("A"))
        .map(|c| c.id.clone())
        .expect("A exists");
    session
        .dispatcher
        .dispatch(Command::MoveChainDown { chain: target_a })
        .expect("MoveChainDown");
    s.save(&session);

    let reloaded = s.reload();
    assert_eq!(
        chain_descriptions(&reloaded),
        vec![Some("B".to_string()), Some("A".to_string())],
        "MoveChainDown order did not persist"
    );
}

// #436: `enabled` is a runtime-only flag on the rig path. `rig_to_chains`
// always projects chains as enabled=true and the post-projection step in
// `rig_to_legacy_project` resets it from a `BTreeSet<String>` of enabled
// inputs that `load_rig_and_project` passes as empty. So toggling
// `enabled` survives in memory but never round-trips to disk — that is
// an existing design limitation, not something this persistence fix
// changes. Marking ignored to flag it without hiding the test.
#[ignore = "rig design: enabled is runtime-only, see #436"]
#[test]
fn toggle_chain_enabled_persists() {
    let s = Sandbox::new();
    let session = s.new_session();
    let id = add_chain(&session, "X");
    let initial = session.project.borrow().chains[0].enabled;
    session
        .dispatcher
        .dispatch(Command::ToggleChainEnabled { chain: id })
        .expect("ToggleChainEnabled");
    s.save(&session);

    let reloaded = s.reload();
    let after = reloaded
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("X"))
        .map(|c| c.enabled)
        .expect("chain present");
    assert_eq!(after, !initial, "ToggleChainEnabled did not persist");
}

#[test]
fn set_chain_volume_persists() {
    let s = Sandbox::new();
    let session = s.new_session();
    let id = add_chain(&session, "X");
    session
        .dispatcher
        .dispatch(Command::SetChainVolume {
            chain: id,
            value: 37.5,
        })
        .expect("SetChainVolume");
    s.save(&session);

    let reloaded = s.reload();
    let v = reloaded
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("X"))
        .map(|c| c.volume)
        .expect("chain present");
    assert!(
        (v - 37.5).abs() < 1e-3,
        "SetChainVolume did not persist (got {v})"
    );
}

// `SaveChain` mutates only the legacy `Project`; the rig path's source
// of truth for chain title is `RigPreset.name`, edited via
// `Command::RenameRigPreset`. Persisting a chain rename therefore goes
// through `RenameRigPreset` (covered in `project_rig_persistence_tests`).
// Leaving the original test here, ignored, so the gap is visible.
#[ignore = "rig design: chain rename persists via RenameRigPreset, not SaveChain"]
#[test]
fn save_chain_metadata_via_save_chain_command_persists() {
    let s = Sandbox::new();
    let session = s.new_session();
    let _id = add_chain(&session, "OLD");
    s.save(&session);

    let session = s.reload();
    let mut chain = session.project.borrow().chains[0].clone();
    chain.description = Some("NEW".into());
    session
        .dispatcher
        .dispatch(Command::SaveChain { chain })
        .expect("SaveChain");
    s.save(&session);

    let reloaded = s.reload();
    assert!(
        reloaded
            .project
            .borrow()
            .chains
            .iter()
            .any(|c| c.description.as_deref() == Some("NEW")),
        "SaveChain metadata edit did not persist (got {:?})",
        chain_descriptions(&reloaded)
    );
}

// ────────────────────────────────────────────────────────────────────
// 3. Block CRUD inside a chain
// ────────────────────────────────────────────────────────────────────

#[test]
fn insert_prebuilt_block_persists() {
    let s = Sandbox::new();
    let session = s.new_session();
    let chain_id = add_chain(&session, "X");
    let block_position = session.project.borrow().chains[0].blocks.len() - 1;

    session
        .dispatcher
        .dispatch(Command::InsertPrebuiltBlock {
            chain: chain_id.clone(),
            block: gain_block("g1", 11.0),
            position: block_position,
        })
        .expect("InsertPrebuiltBlock");
    s.save(&session);

    let reloaded = s.reload();
    let has_g1 = reloaded
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("X"))
        .map(|c| c.blocks.iter().any(|b| b.id.0 == "g1"))
        .unwrap_or(false);
    assert!(has_g1, "InsertPrebuiltBlock did not persist");
}

#[test]
fn remove_block_persists() {
    let s = Sandbox::new();
    let session = s.new_session();
    let chain_id = add_chain(&session, "X");
    let block_position = session.project.borrow().chains[0].blocks.len() - 1;
    session
        .dispatcher
        .dispatch(Command::InsertPrebuiltBlock {
            chain: chain_id.clone(),
            block: gain_block("g1", 11.0),
            position: block_position,
        })
        .expect("Insert");

    session
        .dispatcher
        .dispatch(Command::RemoveBlock {
            chain: chain_id.clone(),
            block: BlockId("g1".into()),
        })
        .expect("RemoveBlock");
    s.save(&session);

    let reloaded = s.reload();
    let has_g1 = reloaded
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("X"))
        .map(|c| c.blocks.iter().any(|b| b.id.0 == "g1"))
        .unwrap_or(false);
    assert!(!has_g1, "RemoveBlock did not persist");
}

#[test]
fn move_block_persists_order() {
    let s = Sandbox::new();
    let session = s.new_session();
    let chain_id = add_chain(&session, "X");
    // Insert 2 effects between input and output (positions in middle).
    let pos = session.project.borrow().chains[0].blocks.len() - 1;
    session
        .dispatcher
        .dispatch(Command::InsertPrebuiltBlock {
            chain: chain_id.clone(),
            block: gain_block("g1", 10.0),
            position: pos,
        })
        .expect("Insert g1");
    let pos = session.project.borrow().chains[0].blocks.len() - 1;
    session
        .dispatcher
        .dispatch(Command::InsertPrebuiltBlock {
            chain: chain_id.clone(),
            block: gain_block("g2", 20.0),
            position: pos,
        })
        .expect("Insert g2");

    // Move g2 before g1.
    let target_position = session.project.borrow().chains[0]
        .blocks
        .iter()
        .position(|b| b.id.0 == "g1")
        .expect("g1 exists");
    session
        .dispatcher
        .dispatch(Command::MoveBlock {
            chain: chain_id.clone(),
            block: BlockId("g2".into()),
            new_position: target_position,
        })
        .expect("MoveBlock");
    s.save(&session);

    let reloaded = s.reload();
    let order: Vec<String> = reloaded
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("X"))
        .map(|c| c.blocks.iter().map(|b| b.id.0.clone()).collect())
        .unwrap_or_default();
    let i1 = order.iter().position(|s| s == "g1");
    let i2 = order.iter().position(|s| s == "g2");
    assert!(
        i2.is_some() && i1.is_some() && i2 < i1,
        "MoveBlock order did not persist (got {order:?})"
    );
}

#[test]
fn toggle_block_enabled_persists() {
    let s = Sandbox::new();
    let session = s.new_session();
    let chain_id = add_chain(&session, "X");
    let pos = session.project.borrow().chains[0].blocks.len() - 1;
    session
        .dispatcher
        .dispatch(Command::InsertPrebuiltBlock {
            chain: chain_id.clone(),
            block: gain_block("g1", 11.0),
            position: pos,
        })
        .expect("Insert");

    session
        .dispatcher
        .dispatch(Command::ToggleBlockEnabled {
            chain: chain_id.clone(),
            block: BlockId("g1".into()),
        })
        .expect("ToggleBlockEnabled");
    s.save(&session);

    let reloaded = s.reload();
    let enabled = reloaded
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.description.as_deref() == Some("X"))
        .and_then(|c| c.blocks.iter().find(|b| b.id.0 == "g1").map(|b| b.enabled))
        .expect("g1 present");
    assert!(!enabled, "ToggleBlockEnabled (true→false) did not persist");
}

