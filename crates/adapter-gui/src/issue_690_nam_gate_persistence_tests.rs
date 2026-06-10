//! Issue #690 — red-first repro: the user toggles the noise gate on a
//! NAM block in the block editor, saves the project, reopens it, and the
//! gate is back to its previous state. The contract under test is the
//! same as the rest of the persistence matrix: **what was dispatched and
//! saved must survive a `save_project_session` ↔ `load_project_session`
//! round-trip** — including for NAM-backed blocks, whose params are
//! seeded from the plugin manifest (#675) and must NOT be re-seeded over
//! the user's saved values on load.

use crate::project_ops::{create_new_project_session, load_project_session, save_project_session};
use crate::state::ProjectSession;
use application::block_factory::{build_default_block, resolve_effect_type_for_model};
use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use application::command::Command;
use application::dispatcher::CommandDispatcher;
use domain::ids::{BlockId, ChainId};
use project::block::AudioBlockKind;
use project::param::ParameterSet;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture_plugins_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../application/tests/fixtures/plugins")
}

struct Sandbox {
    _tmp: TempDir,
    path: PathBuf,
    cfg: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("project.yaml");
        let cfg = tmp.path().join("config.yaml");
        // The load path resolves NAM packages through the configured
        // plugins root — point it at the same fixture tree the registry
        // is initialised from.
        std::fs::write(
            &cfg,
            format!("plugins_root: {}\n", fixture_plugins_root().display()),
        )
        .expect("write config with plugins_root");
        Self {
            _tmp: tmp,
            path,
            cfg,
        }
    }

    fn new_session(&self) -> ProjectSession {
        let mut session = create_new_project_session(&self.cfg);
        session.project_path = Some(self.path.clone());
        session.config_path = Some(self.cfg.clone());
        session
    }

    fn save(&self, session: &ProjectSession) {
        save_project_session(session, &self.path).expect("save");
    }

    fn reload(&self) -> ProjectSession {
        load_project_session(&self.path, &self.cfg).expect("reload")
    }
}

fn add_chain(session: &ProjectSession) -> ChainId {
    let chain = build_default_chain(DefaultChainParams {
        project: &session.project.borrow(),
        instrument: "electric_guitar",
        description: Some("X".into()),
        input: EndpointSpec {
            device_id: Some("dev"),
            channels: vec![0],
        },
        output: EndpointSpec {
            device_id: Some("test-out"),
            channels: vec![0, 1],
        },
    });
    session
        .dispatcher
        .dispatch(Command::SaveChain { chain })
        .expect("SaveChain");
    session.project.borrow().chains[0].id.clone()
}

/// Insert the fixture NAM grid pedal the same way the GUI block editor
/// does: a default block built from the manifest, via InsertPrebuiltBlock.
fn insert_nam_block(session: &ProjectSession, chain: &ChainId, id: &str) -> BlockId {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| plugin_loader::registry::init_many(&[fixture_plugins_root()]));
    let effect_type =
        resolve_effect_type_for_model("nam_ts9_grid").expect("effect type for the grid pedal");
    let block = build_default_block(BlockId(id.into()), &effect_type, "nam_ts9_grid")
        .expect("default NAM block");
    let position = session
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id == *chain)
        .map(|c| c.blocks.len() - 1)
        .expect("chain present");
    session
        .dispatcher
        .dispatch(Command::InsertPrebuiltBlock {
            chain: chain.clone(),
            block,
            position,
        })
        .expect("InsertPrebuiltBlock");
    BlockId(id.into())
}

fn block_params(session: &ProjectSession, block: &BlockId) -> ParameterSet {
    session
        .project
        .borrow()
        .chains
        .iter()
        .flat_map(|c| c.blocks.iter())
        .find(|b| b.id == *block)
        .map(|b| match &b.kind {
            AudioBlockKind::Nam(nam) => nam.params.clone(),
            AudioBlockKind::Core(core) => core.params.clone(),
            other => panic!("expected a NAM-backed block, got {}", other.label()),
        })
        .expect("block present after reload")
}

#[test]
fn nam_noise_gate_toggle_survives_save_and_reload() {
    let s = Sandbox::new();
    let session = s.new_session();
    let chain_id = add_chain(&session);
    let block = insert_nam_block(&session, &chain_id, "nam1");

    // Sanity: the fixture manifest seeds the gate ON (#675).
    let seeded = block_params(&session, &block);
    assert_eq!(
        seeded.get_bool("noise_gate.enabled"),
        Some(true),
        "fixture must seed the gate enabled — repro precondition"
    );

    // First save+reload turns the chain into a rig-projected one
    // (`rig:input-N`) — the user's project is always in this state by
    // the time they edit an existing block.
    s.save(&session);
    let session = s.reload();
    let chain_id = session.project.borrow().chains[0].id.clone();
    assert!(
        chain_id.0.starts_with("rig:"),
        "after a save+reload the chain must be rig-projected — repro precondition"
    );

    // The user flips the gate in the block editor.
    session
        .dispatcher
        .dispatch(Command::SetBlockParameterBool {
            chain: chain_id.clone(),
            block: block.clone(),
            path: "noise_gate.enabled".into(),
            value: false,
        })
        .expect("SetBlockParameterBool");
    s.save(&session);

    let reloaded = s.reload();
    let params = block_params(&reloaded, &block);
    assert_eq!(
        params.get_bool("noise_gate.enabled"),
        Some(false),
        "BUG #690: the noise-gate toggle on a NAM block must survive \
         save + reload — it reverted to the pre-save state"
    );
}

#[test]
fn nam_noise_gate_threshold_survives_save_and_reload() {
    let s = Sandbox::new();
    let session = s.new_session();
    let chain_id = add_chain(&session);
    let block = insert_nam_block(&session, &chain_id, "nam1");

    // Sanity: the fixture capture seeds threshold_db = -55 (#675).
    let seeded = block_params(&session, &block);
    assert_eq!(
        seeded.get_f32("noise_gate.threshold_db"),
        Some(-55.0),
        "fixture must seed the threshold — repro precondition"
    );

    // Same rig-projected state as the toggle repro.
    s.save(&session);
    let session = s.reload();
    let chain_id = session.project.borrow().chains[0].id.clone();

    session
        .dispatcher
        .dispatch(Command::SetBlockParameterNumber {
            chain: chain_id.clone(),
            block: block.clone(),
            path: "noise_gate.threshold_db".into(),
            value: -23.5,
        })
        .expect("SetBlockParameterNumber");
    s.save(&session);

    let reloaded = s.reload();
    let params = block_params(&reloaded, &block);
    assert_eq!(
        params.get_f32("noise_gate.threshold_db"),
        Some(-23.5),
        "BUG #690: the noise-gate threshold on a NAM block must survive \
         save + reload — it reverted to the manifest-seeded value"
    );
}
