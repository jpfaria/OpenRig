//! Red-first regression: when a chain is moved up/down on the chains
//! screen, the preset/scene selector in that row keeps showing the
//! OLD neighbour's state until something else triggers a refresh
//! (save+reload, click a different chain, …). Reproduces by asking
//! `chain_rig_nav::rig_nav_rows` what the UI would render — that is
//! the same function `refresh_chain_rig_nav` pushes into Slint, so
//! if the rows it produces after `MoveChainUp` align 1:1 with the
//! new chain order, then the fix is just calling that function from
//! the move callbacks (no model change). If they DON'T align, the
//! bug is deeper than the UI refresh.

use crate::chain_rig_nav::rig_nav_rows;
use crate::project_ops::create_new_project_session;
use crate::state::ProjectSession;
use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use application::command::Command;
use application::dispatcher::CommandDispatcher;
use domain::ids::ChainId;
use std::path::PathBuf;
use tempfile::TempDir;

fn new_session() -> (TempDir, ProjectSession) {
    let tmp = TempDir::new().unwrap();
    let cfg: PathBuf = tmp.path().join("config.yaml");
    let path: PathBuf = tmp.path().join("project.yaml");
    let mut session = create_new_project_session(&cfg);
    session.project_path = Some(path);
    session.config_path = Some(cfg);
    (tmp, session)
}

/// Add a chain with a uniquely-named preset so reorder mistakes are
/// observable through the preset label.
fn add_chain(session: &ProjectSession, desc: &str, dev: &str, preset_name: &str) -> ChainId {
    let chain = build_default_chain(DefaultChainParams {
        project: &session.project.borrow(),
        instrument: "electric_guitar",
        description: Some(desc.into()),
        input: EndpointSpec {
            device_id: Some(dev),
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
        .dispatch(Command::SaveChain { chain })
        .expect("SaveChain");
    let id = session
        .project
        .borrow()
        .chains
        .iter()
        .rev()
        .find(|c| c.description.as_deref() == Some(desc))
        .map(|c| c.id.clone())
        .expect("chain present");
    session
        .dispatcher
        .dispatch(Command::RenameRigPreset {
            chain: id.clone(),
            name: preset_name.into(),
        })
        .expect("rename");
    id
}

#[test]
fn move_chain_up_brings_its_preset_label_along() {
    let (_tmp, session) = new_session();
    add_chain(&session, "A", "dev-A", "AAA");
    add_chain(&session, "B", "dev-B", "BBB");
    let c = add_chain(&session, "C", "dev-C", "CCC");

    // Sanity: initial order is A, B, C.
    let rig_rc = session.rig.as_ref().expect("rig").clone();
    {
        let rig = rig_rc.borrow();
        let rows = rig_nav_rows(&rig, &session.project.borrow());
        let labels: Vec<&str> = rows
            .iter()
            .map(|r| r.preset_labels.first().map(String::as_str).unwrap_or(""))
            .collect();
        assert_eq!(labels, vec!["AAA", "BBB", "CCC"], "baseline preset order");
    }

    // Move C up to index 1.
    session
        .dispatcher
        .dispatch(Command::MoveChainUp { chain: c })
        .expect("MoveChainUp");

    // After the dispatch, the preset label at index 1 in
    // `rig_nav_rows` MUST be "CCC" — the one that travelled with C.
    // The live bug: the GUI's `chain_rig_nav` model never refreshed,
    // so it kept showing "BBB" at index 1. This test guards the
    // contract `refresh_chain_rig_nav` is supposed to maintain.
    let rig = rig_rc.borrow();
    let rows = rig_nav_rows(&rig, &session.project.borrow());
    let labels: Vec<&str> = rows
        .iter()
        .map(|r| r.preset_labels.first().map(String::as_str).unwrap_or(""))
        .collect();
    assert_eq!(
        labels,
        vec!["AAA", "CCC", "BBB"],
        "after MoveChainUp(C), rig_nav_rows must align with the new chain order"
    );
}

#[test]
fn move_chain_down_brings_its_preset_label_along() {
    let (_tmp, session) = new_session();
    let a = add_chain(&session, "A", "dev-A", "AAA");
    add_chain(&session, "B", "dev-B", "BBB");
    add_chain(&session, "C", "dev-C", "CCC");

    session
        .dispatcher
        .dispatch(Command::MoveChainDown { chain: a })
        .expect("MoveChainDown");

    let rig = session.rig.as_ref().expect("rig").borrow();
    let rows = rig_nav_rows(&rig, &session.project.borrow());
    let labels: Vec<&str> = rows
        .iter()
        .map(|r| r.preset_labels.first().map(String::as_str).unwrap_or(""))
        .collect();
    assert_eq!(
        labels,
        vec!["BBB", "AAA", "CCC"],
        "after MoveChainDown(A), rig_nav_rows must align with the new chain order"
    );
}
