//! Red-first tests for in-memory chain creation. The user reported:
//! "criei uma chain nova e não veio com o preset preenchido nem a
//! scene 1." That means after `Command::AddChain` runs (and BEFORE
//! any save/reload), the session's `RigProject` must already have
//! the input + preset + scene 1 so the GUI's preset combobox is
//! populated immediately.
//!
//! Each test exercises the path the GUI follows: build a chain via
//! `chain_factory::build_default_chain`, dispatch `Command::AddChain`,
//! then assert on `session.rig` directly. No save, no reload.

use crate::project_ops::create_new_project_session;
use crate::state::ProjectSession;
use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use application::command::Command;
use application::dispatcher::CommandDispatcher;
use project::chain::Chain;
use std::path::PathBuf;
use tempfile::TempDir;

fn new_session(tmp: &TempDir) -> ProjectSession {
    let cfg: PathBuf = tmp.path().join("config.yaml");
    let path: PathBuf = tmp.path().join("project.yaml");
    let mut session = create_new_project_session(&cfg);
    session.project_path = Some(path);
    session.config_path = Some(cfg);
    session
}

fn chain_with(session: &ProjectSession, desc: &str, dev: &str) -> Chain {
    build_default_chain(DefaultChainParams {
        project: &session.project.borrow(),
        instrument: "electric_guitar",
        description: Some(desc.into()),
        input: EndpointSpec {
            device_id: Some(dev),
            channels: vec![0],
        },
        output: EndpointSpec {
            device_id: Some("test-out"),
            channels: vec![0, 1],
        },
    })
}

// ────────────────────────────────────────────────────────────────────
// 1. New session already exposes a rig the GUI can render from
// ────────────────────────────────────────────────────────────────────

#[test]
fn new_project_session_has_a_rig_attached() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    assert!(
        session.rig.is_some(),
        "the preset/scene UI binds to `session.rig`; a NEW session must \
         already have one so the GUI has something to render against"
    );
}

// ────────────────────────────────────────────────────────────────────
// 2. AddChain populates the rig with input + preset + scene 1
// ────────────────────────────────────────────────────────────────────

#[test]
fn add_chain_command_creates_a_rig_input() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    session
        .dispatcher
        .dispatch(Command::AddChain { chain })
        .expect("AddChain");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    assert_eq!(
        rig.inputs.len(),
        1,
        "AddChain must add a corresponding input to the rig in memory; \
         got inputs={:?}",
        rig.inputs.keys().collect::<Vec<_>>()
    );
    let (_, input) = rig.inputs.iter().next().unwrap();
    assert_eq!(input.label.as_deref(), Some("Chain 1"));
}

#[test]
fn add_chain_command_creates_a_preset_named_preset_1() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    session
        .dispatcher
        .dispatch(Command::AddChain { chain })
        .expect("AddChain");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    let preset_names: Vec<Option<String>> =
        rig.presets.values().map(|p| p.name.clone()).collect();
    assert!(
        preset_names.iter().any(|n| n.as_deref() == Some("Preset 1")),
        "AddChain must create a default 'Preset 1' in the rig; got {preset_names:?}"
    );
    assert!(
        !preset_names.iter().any(|n| n.as_deref() == Some("Chain 1")),
        "preset.name must not duplicate the chain title"
    );
}

#[test]
fn add_chain_command_creates_scene_1_in_the_new_preset() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    session
        .dispatcher
        .dispatch(Command::AddChain { chain })
        .expect("AddChain");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    for (preset_name, preset) in &rig.presets {
        assert!(
            preset.scenes.contains_key(&1),
            "preset {preset_name:?} should have scene 1 already created"
        );
    }
}

// ────────────────────────────────────────────────────────────────────
// 3. The active preset/scene are pointed at slot 1 so the UI's
//    combobox lights up "Preset 1" by default
// ────────────────────────────────────────────────────────────────────

#[test]
fn add_chain_active_preset_and_scene_default_to_1() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    session
        .dispatcher
        .dispatch(Command::AddChain { chain })
        .expect("AddChain");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    let (_, input) = rig.inputs.iter().next().expect("one input");
    assert_eq!(input.active_preset, 1, "active preset defaults to slot 1");
    assert_eq!(input.active_scene, 1, "active scene defaults to 1");
    assert!(
        input.bank.contains_key(&1),
        "bank slot 1 holds the default preset key"
    );
}

// ────────────────────────────────────────────────────────────────────
// 4. Two chains with the same source produce two independent rig inputs
// ────────────────────────────────────────────────────────────────────

#[test]
fn two_add_chain_calls_with_same_source_produce_two_inputs() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let a = chain_with(&session, "Chain 1", "shared-dev");
    let b = chain_with(&session, "Chain 2", "shared-dev");
    session
        .dispatcher
        .dispatch(Command::AddChain { chain: a })
        .expect("AddChain A");
    session
        .dispatcher
        .dispatch(Command::AddChain { chain: b })
        .expect("AddChain B");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    assert_eq!(
        rig.inputs.len(),
        2,
        "two AddChain dispatches with the same source must produce two \
         inputs (got {:?})",
        rig.inputs.keys().collect::<Vec<_>>()
    );
    let labels: Vec<Option<String>> =
        rig.inputs.values().map(|i| i.label.clone()).collect();
    assert!(
        labels.contains(&Some("Chain 1".to_string()))
            && labels.contains(&Some("Chain 2".to_string())),
        "both labels survive; got {labels:?}"
    );
}
