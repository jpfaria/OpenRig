//! Red-first regression test for the live bug: editing a chain's
//! name in the chain editor, hitting save, and watching the UI snap
//! back to the previous name ("Chain Reborn"). The chain editor
//! dispatches `ChainCommand::SaveChain` (upsert path) with a fresh
//! description. The handler replaces the legacy `chain` entry in
//! place, but the rig's `RigInput.label` is what feeds
//! `rig_to_chains` -> `chain.description` on every re-projection.
//! If we don't push the new name into the rig too, the very next
//! `ChainReloaded` re-projection wipes the rename.

use crate::project_ops::create_new_project_session;
use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use application::command::{ChainCommand, Command, SelectionCommand};
use application::dispatcher::CommandDispatcher;
use std::path::PathBuf;
use tempfile::TempDir;

fn session_with_one_rig_chain() -> (TempDir, crate::state::ProjectSession) {
    let tmp = TempDir::new().unwrap();
    let cfg: PathBuf = tmp.path().join("config.yaml");
    let path: PathBuf = tmp.path().join("project.yaml");
    let mut session = create_new_project_session(&cfg);
    session.project_path = Some(path);
    session.config_path = Some(cfg);
    let chain = build_default_chain(DefaultChainParams {
        project: &session.project.borrow(),
        instrument: "electric_guitar",
        description: Some("Chain Reborn".into()),
        input: EndpointSpec {
            device_id: Some("dev-A"),
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
    session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::SaveChain { chain }))
        .expect("seed SaveChain");
    (tmp, session)
}

#[test]
fn save_chain_rename_propagates_to_rig_input_label() {
    let (_tmp, session) = session_with_one_rig_chain();

    // The editor dispatches SaveChain with the same chain id but a
    // new description.
    let mut renamed = session.project.borrow().chains[0].clone();
    renamed.description = Some("Lead Tone".into());
    session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::SaveChain { chain: renamed }))
        .expect("SaveChain rename");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    let label = rig.inputs.values().next().and_then(|i| i.label.clone());
    assert_eq!(
        label.as_deref(),
        Some("Lead Tone"),
        "after renaming via SaveChain, the rig's `input.label` must \
         reflect the new name; otherwise the next re-projection \
         restores the old description"
    );
}

#[test]
fn rename_survives_a_re_projection() {
    let (_tmp, session) = session_with_one_rig_chain();
    let mut renamed = session.project.borrow().chains[0].clone();
    renamed.description = Some("Lead Tone".into());
    session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::SaveChain { chain: renamed }))
        .expect("rename");

    // Force a re-projection (any rig nav command emits ChainReloaded
    // which the GUI reacts to by re-projecting). After that, the
    // legacy `Project` must still show the new description.
    let chain_id = session.project.borrow().chains[0].id.clone();
    session
        .dispatcher
        .dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
            chain: chain_id,
            kind: application::command::RigNavKind::StepPreset(0),
        }))
        .expect("noop nav to trigger reproject");

    let desc = session.project.borrow().chains[0].description.clone();
    assert_eq!(
        desc.as_deref(),
        Some("Lead Tone"),
        "after a re-projection, the chain description must still \
         carry the renamed value (came back as {desc:?})"
    );
}
