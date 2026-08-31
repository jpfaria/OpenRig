//! #913 — opening the project named on the command line.
//!
//! Two outcomes only, and the failing one matters most: a bad path on the
//! command line must leave the app usable on the launcher, with NOTHING half
//! applied — no chain rows from a project that is not loaded, no recent entry
//! for a file that could not be read.

use super::load_cli_project;
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

fn project_file() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("project.yaml");
    std::fs::write(&path, PROJECT_YAML).expect("write project");
    (dir, path)
}

struct Session {
    project: Rc<RefCell<Option<ProjectSession>>>,
    chains: Rc<VecModel<ProjectChainItem>>,
    snapshot: Rc<RefCell<Option<String>>>,
    config: Rc<RefCell<AppConfig>>,
    recents: Rc<VecModel<RecentProjectItem>>,
}

impl Session {
    fn empty() -> Self {
        // The snapshot walks the asset paths, which panic until startup set
        // them. Defaults point at directories that do not exist in a test run.
        infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
        Self {
            project: Rc::new(RefCell::new(None)),
            chains: Rc::new(VecModel::from(Vec::<ProjectChainItem>::new())),
            snapshot: Rc::new(RefCell::new(None)),
            config: Rc::new(RefCell::new(AppConfig::default())),
            recents: Rc::new(VecModel::from(Vec::<RecentProjectItem>::new())),
        }
    }

    fn open(&self, path: &PathBuf) -> Option<super::CliOpened> {
        load_cli_project(
            path,
            &self.project,
            &self.chains,
            &[],
            &[],
            &self.snapshot,
            &self.config,
            &self.recents,
        )
    }
}

#[test]
fn a_project_named_on_the_command_line_becomes_this_sessions_project() {
    let (_guard, path) = project_file();
    let session = Session::empty();

    let opened = session.open(&path).expect("the file is a valid project");

    assert!(session.project.borrow().is_some());
    assert!(
        opened.title.contains("Studio") || !opened.title.is_empty(),
        "unexpected title: {}",
        opened.title
    );
    assert!(opened.canonical_path.ends_with("project.yaml"));
}

#[test]
fn opening_publishes_the_projects_chain_rows() {
    let (_guard, path) = project_file();
    let session = Session::empty();
    session.open(&path).expect("open");
    assert_eq!(
        session.chains.row_count(),
        1,
        "the chains screen shows what was loaded"
    );
}

#[test]
fn opening_takes_the_clean_snapshot_so_the_project_does_not_start_dirty() {
    let (_guard, path) = project_file();
    let session = Session::empty();
    session.open(&path).expect("open");
    assert!(
        session.snapshot.borrow().is_some(),
        "with no snapshot the first dirty check would flag an untouched project"
    );
}

#[test]
fn opening_records_the_project_as_recent() {
    let (_guard, path) = project_file();
    let session = Session::empty();
    session.open(&path).expect("open");
    assert_eq!(session.config.borrow().recent_projects.len(), 1);
    assert_eq!(
        session.recents.row_count(),
        1,
        "the launcher list is republished, not just the config"
    );
}

#[test]
fn a_path_that_does_not_exist_leaves_the_session_on_the_launcher() {
    let session = Session::empty();
    let missing = PathBuf::from("/nonexistent/openrig-913/project.yaml");

    assert!(session.open(&missing).is_none());

    assert!(session.project.borrow().is_none(), "nothing was loaded");
    assert_eq!(session.chains.row_count(), 0, "no rows from a failed open");
    assert!(session.snapshot.borrow().is_none());
    assert!(
        session.config.borrow().recent_projects.is_empty(),
        "a file that could not be read must not enter the recents"
    );
}

#[test]
fn a_file_that_is_not_a_project_leaves_the_session_untouched() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("garbage.yaml");
    std::fs::write(&path, "this: is: not: a: project: {[}\n").expect("write");
    let session = Session::empty();

    assert!(session.open(&path).is_none());
    assert!(session.project.borrow().is_none());
    assert_eq!(session.chains.row_count(), 0);
}

#[test]
fn opening_a_second_project_replaces_the_first() {
    let (_a, first) = project_file();
    let (_b, second) = project_file();
    let session = Session::empty();
    session.open(&first).expect("first");
    session.open(&second).expect("second");
    assert_eq!(
        session.chains.row_count(),
        1,
        "the rows are replaced, not appended"
    );
    assert!(session.project.borrow().is_some());
}
