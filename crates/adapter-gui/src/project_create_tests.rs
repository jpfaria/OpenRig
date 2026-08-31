//! #913 — creating a project that only exists in memory.
//!
//! The one that matters: no clean snapshot is taken. An in-memory project is
//! dirty from its first frame, so closing it without saving prompts the user
//! instead of throwing the work away silently.

use super::create_project;
use crate::runtime_analyzers::AnalyzerSessions;
use crate::runtime_lifecycle::RuntimeAttach;
use crate::state::ProjectSession;
use crate::ProjectChainItem;
use slint::{Model, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

struct Harness {
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_chains: Rc<VecModel<ProjectChainItem>>,
    runtime_attach: RuntimeAttach,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
    _dir: tempfile::TempDir,
    config_path: PathBuf,
}

impl Harness {
    fn new() -> Self {
        infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("config.yaml");
        Self {
            project_session: Rc::new(RefCell::new(None)),
            project_chains: Rc::new(VecModel::from(Vec::<ProjectChainItem>::new())),
            runtime_attach: RuntimeAttach::new(
                &Rc::new(RefCell::new(None)),
                &AnalyzerSessions::detached(),
            ),
            saved_project_snapshot: Rc::new(RefCell::new(Some("stale".to_string()))),
            _dir: dir,
            config_path,
        }
    }

    fn create(&self, name: &str) {
        create_project(
            name,
            &self.config_path,
            &self.project_session,
            &self.project_chains,
            &self.runtime_attach,
            &self.saved_project_snapshot,
            &[],
            &[],
        );
    }

    fn name(&self) -> Option<String> {
        self.project_session
            .borrow()
            .as_ref()?
            .project
            .borrow()
            .name
            .clone()
    }
}

#[test]
fn the_new_project_becomes_this_sessions_project() {
    let harness = Harness::new();
    harness.create("Studio");
    assert!(harness.project_session.borrow().is_some());
}

#[test]
fn the_project_carries_the_name_the_user_typed() {
    let harness = Harness::new();
    harness.create("Studio Rig");
    assert_eq!(harness.name().as_deref(), Some("Studio Rig"));
}

#[test]
fn no_clean_snapshot_is_taken_so_the_project_starts_dirty() {
    let harness = Harness::new();
    harness.create("Studio");
    assert!(
        harness.saved_project_snapshot.borrow().is_none(),
        "an in-memory project must prompt on close, not be discarded silently"
    );
}

#[test]
fn the_chain_rows_are_published_for_the_empty_project() {
    let harness = Harness::new();
    harness.create("Studio");
    assert_eq!(
        harness.project_chains.row_count(),
        0,
        "a fresh project has no chains, and the screen must say so rather than \
         keep the previous project's rows"
    );
}

#[test]
fn creating_a_second_project_replaces_the_first() {
    let harness = Harness::new();
    harness.create("First");
    harness.create("Second");
    assert_eq!(harness.name().as_deref(), Some("Second"));
}
