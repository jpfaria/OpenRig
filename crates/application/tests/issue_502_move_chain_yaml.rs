//! Regression test for issue #502 (regression of #246): reordering chains via
//! `Command::MoveChainUp` / `MoveChainDown` must mutate the live `Project`
//! such that re-serialising it to YAML reflects the new order.
//!
//! This integration test pins the dispatcher contract from the application
//! side. The matching GUI-layer regression lives in
//! `crates/adapter-gui/tests/issue_502_move_chain_session.rs` and covers the
//! pure handler that the Slint callback must route through.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;

fn make_chain(id: &str, description: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some(description.into()),
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: Vec::new(),
        di_output: None,
        loopers: vec![],
    }
}

fn project_with_chains(rows: &[(&str, &str)]) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: Some("issue-502".into()),
        device_settings: Vec::new(),
        chains: rows.iter().map(|(id, desc)| make_chain(id, desc)).collect(),
        midi: None,
    }))
}

// The YAML format does NOT store `ChainId` — it is reassigned on deserialise.
// We therefore identify chains across a round-trip by `description`, which is
// the user-facing stable identity persisted in YAML.
fn chain_descriptions_in_yaml(yaml: &str) -> Vec<String> {
    let reloaded: Project = serde_yaml::from_str(yaml).expect("yaml roundtrip");
    reloaded
        .chains
        .iter()
        .map(|c| c.description.clone().unwrap_or_default())
        .collect()
}

#[test]
fn move_chain_up_mutates_project_so_yaml_reflects_new_order() {
    let project = project_with_chains(&[("chain_a", "alpha"), ("chain_b", "beta")]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::MoveChainUp {
            chain: ChainId("chain_b".into()),
        })
        .expect("MoveChainUp must succeed");

    let yaml = serde_yaml::to_string(&*project.borrow()).expect("serialise project");
    let descs = chain_descriptions_in_yaml(&yaml);
    assert_eq!(
        descs,
        vec!["beta".to_string(), "alpha".to_string()],
        "YAML round-trip must reflect the new chain order after MoveChainUp"
    );
}

#[test]
fn move_chain_down_mutates_project_so_yaml_reflects_new_order() {
    let project = project_with_chains(&[("chain_a", "alpha"), ("chain_b", "beta")]);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::MoveChainDown {
            chain: ChainId("chain_a".into()),
        })
        .expect("MoveChainDown must succeed");

    let yaml = serde_yaml::to_string(&*project.borrow()).expect("serialise project");
    let descs = chain_descriptions_in_yaml(&yaml);
    assert_eq!(
        descs,
        vec!["beta".to_string(), "alpha".to_string()],
        "YAML round-trip must reflect the new chain order after MoveChainDown"
    );
}
