//! #913 — installing a project file as this session's project.
//!
//! Both the launcher's recent list and the file dialog open through here, so
//! the sequence holds for either. What must be true afterwards: the session IS
//! the loaded project, its rows are on screen, the clean snapshot is taken so
//! it does not open already dirty, and it is recorded as recent. A load that
//! fails must leave every one of those untouched.

use super::{open_project_at, OpenProjectCtx};
use crate::runtime_analyzers::AnalyzerSessions;
use crate::runtime_lifecycle::RuntimeAttach;
use crate::state::ProjectSession;
use crate::{ProjectChainItem, RecentProjectItem};
use infra_filesystem::AppConfig;
use slint::{Model, VecModel};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

const PROJECT_YAML: &str = r#"
name: Studio
chains:
  - description: Guitar
    blocks:
      - type: gain
        enabled: true
        model: volume
"#;

fn project_file(dir: &tempfile::TempDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, PROJECT_YAML).expect("write project");
    std::fs::canonicalize(&path).expect("canonicalize")
}

struct Harness {
    app_config: Rc<RefCell<AppConfig>>,
    recent_projects: Rc<VecModel<RecentProjectItem>>,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
    project_chains: Rc<VecModel<ProjectChainItem>>,
    runtime_attach: RuntimeAttach,
    saved_project_snapshot: Rc<RefCell<Option<String>>>,
}

impl Harness {
    fn new() -> Self {
        infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
        Self {
            app_config: Rc::new(RefCell::new(AppConfig::default())),
            recent_projects: Rc::new(VecModel::from(Vec::<RecentProjectItem>::new())),
            project_session: Rc::new(RefCell::new(None)),
            project_chains: Rc::new(VecModel::from(Vec::<ProjectChainItem>::new())),
            runtime_attach: RuntimeAttach::new(
                &Rc::new(RefCell::new(None)),
                &AnalyzerSessions::detached(),
            ),
            saved_project_snapshot: Rc::new(RefCell::new(None)),
        }
    }

    fn open(&self, path: &PathBuf) -> Result<super::OpenedProject, String> {
        open_project_at(
            &OpenProjectCtx {
                app_config: &self.app_config,
                recent_projects: &self.recent_projects,
                project_session: &self.project_session,
                project_chains: &self.project_chains,
                runtime_attach: &self.runtime_attach,
                saved_project_snapshot: &self.saved_project_snapshot,
                input_chain_devices: &[],
                output_chain_devices: &[],
                search: "",
            },
            path,
        )
    }
}

#[test]
fn opening_installs_the_file_as_the_sessions_project() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = project_file(&dir, "studio.yaml");
    let harness = Harness::new();

    let opened = harness.open(&path).expect("open");

    assert!(harness.project_session.borrow().is_some());
    assert_eq!(opened.canonical_path, path);
    assert!(!opened.title.is_empty());
}

#[test]
fn opening_publishes_the_rows_and_takes_the_clean_snapshot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = project_file(&dir, "studio.yaml");
    let harness = Harness::new();
    harness.open(&path).expect("open");
    assert_eq!(harness.project_chains.row_count(), 1);
    assert!(harness.saved_project_snapshot.borrow().is_some());
}

#[test]
fn opening_records_the_project_as_recent_and_republishes_the_list() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = project_file(&dir, "studio.yaml");
    let harness = Harness::new();
    harness.open(&path).expect("open");
    assert_eq!(harness.app_config.borrow().recent_projects.len(), 1);
    assert_eq!(harness.recent_projects.row_count(), 1);
}

#[test]
fn a_file_that_cannot_be_read_changes_nothing() {
    let harness = Harness::new();
    let missing = PathBuf::from("/nonexistent/openrig-913/studio.yaml");

    assert!(harness.open(&missing).is_err());

    assert!(harness.project_session.borrow().is_none());
    assert_eq!(harness.project_chains.row_count(), 0);
    assert!(harness.saved_project_snapshot.borrow().is_none());
    assert!(harness.app_config.borrow().recent_projects.is_empty());
}

#[test]
fn a_file_that_is_not_a_project_changes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("garbage.yaml");
    std::fs::write(&path, "this: is: not: a: project: {[}\n").expect("write");
    let harness = Harness::new();
    assert!(harness.open(&path).is_err());
    assert!(harness.project_session.borrow().is_none());
}

#[test]
fn opening_a_second_project_replaces_the_first() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = project_file(&dir, "first.yaml");
    let second = project_file(&dir, "second.yaml");
    let harness = Harness::new();

    harness.open(&first).expect("first");
    let opened = harness.open(&second).expect("second");

    assert_eq!(opened.canonical_path, second);
    assert_eq!(
        harness.project_chains.row_count(),
        1,
        "the rows are replaced, not appended"
    );
    assert_eq!(
        harness.app_config.borrow().recent_projects.len(),
        2,
        "both are remembered"
    );
}
