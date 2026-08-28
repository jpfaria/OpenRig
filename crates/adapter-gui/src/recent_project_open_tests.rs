//! #913 — opening a project from the launcher's recent list.
//!
//! The list outlives the files it points at: a project can be renamed, moved or
//! deleted between sessions. So the two failure paths matter as much as the
//! happy one — an entry already flagged invalid is refused without touching the
//! session, and one that fails to load NOW is flagged so the user can clean it
//! up instead of clicking a dead row forever.

use super::{open_recent, OpenRecentError};
use crate::project_open::OpenProjectCtx;
use crate::runtime_analyzers::AnalyzerSessions;
use crate::runtime_lifecycle::RuntimeAttach;
use crate::state::ProjectSession;
use crate::{ProjectChainItem, RecentProjectItem};
use infra_filesystem::{AppConfig, RecentProjectEntry};
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

/// The recents store canonical paths, so the fixture does too — otherwise the
/// open registers the canonical form as a SECOND entry and the indexes shift
/// under the test for a reason production never sees.
fn project_file(dir: &tempfile::TempDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, PROJECT_YAML).expect("write project");
    std::fs::canonicalize(&path).expect("canonicalize")
}

fn entry(path: &PathBuf, valid: bool, reason: Option<&str>) -> RecentProjectEntry {
    RecentProjectEntry {
        project_path: path.to_string_lossy().into_owned(),
        project_name: "Studio".into(),
        is_valid: valid,
        invalid_reason: reason.map(str::to_string),
    }
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
    /// Where `path` currently sits in the recent list. Opening a project moves
    /// it to the front, so an index taken before an open is stale.
    fn index_of(&self, path: &PathBuf) -> usize {
        self.app_config
            .borrow()
            .recent_projects
            .iter()
            .position(|e| e.project_path == path.to_string_lossy())
            .unwrap_or_else(|| panic!("{} is not in the recent list", path.display()))
    }

    fn new(entries: Vec<RecentProjectEntry>) -> Self {
        // The open walks the asset paths, which panic until startup set them.
        infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
        let mut config = AppConfig::default();
        config.recent_projects = entries;
        let project_session = Rc::new(RefCell::new(None));
        Self {
            app_config: Rc::new(RefCell::new(config)),
            recent_projects: Rc::new(VecModel::from(Vec::<RecentProjectItem>::new())),
            runtime_attach: RuntimeAttach::new(
                &Rc::new(RefCell::new(None)),
                &AnalyzerSessions::detached(),
            ),
            project_session,
            project_chains: Rc::new(VecModel::from(Vec::<ProjectChainItem>::new())),
            saved_project_snapshot: Rc::new(RefCell::new(None)),
        }
    }

    fn open(&self, index: usize) -> Result<crate::project_open::OpenedProject, OpenRecentError> {
        open_recent(
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
            index,
        )
    }
}

#[test]
fn opening_a_recent_row_installs_that_project_as_the_session() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = project_file(&dir, "studio.yaml");
    let harness = Harness::new(vec![entry(&path, true, None)]);

    let opened = harness.open(0).expect("the file is there and valid");

    assert!(harness.project_session.borrow().is_some());
    assert!(opened.canonical_path.ends_with("studio.yaml"));
    assert!(!opened.title.is_empty());
}

#[test]
fn opening_publishes_the_chain_rows_and_the_clean_snapshot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = project_file(&dir, "studio.yaml");
    let harness = Harness::new(vec![entry(&path, true, None)]);

    harness.open(0).expect("open");

    assert_eq!(harness.project_chains.row_count(), 1);
    assert!(
        harness.saved_project_snapshot.borrow().is_some(),
        "without the snapshot the project would open already dirty"
    );
}

#[test]
fn opening_refreshes_the_recent_list_it_was_launched_from() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = project_file(&dir, "studio.yaml");
    let harness = Harness::new(vec![entry(&path, true, None)]);
    harness.open(0).expect("open");
    assert_eq!(harness.recent_projects.row_count(), 1);
}

#[test]
fn a_row_index_that_is_no_longer_there_is_refused() {
    let harness = Harness::new(Vec::new());
    assert_eq!(harness.open(0), Err(OpenRecentError::NoSuchEntry));
    assert!(harness.project_session.borrow().is_none());
}

#[test]
fn an_entry_already_flagged_invalid_is_refused_with_its_recorded_reason() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = project_file(&dir, "studio.yaml");
    let harness = Harness::new(vec![entry(&path, false, Some("file was moved"))]);

    assert_eq!(
        harness.open(0),
        Err(OpenRecentError::AlreadyInvalid(Some(
            "file was moved".to_string()
        ))),
        "the reason recorded earlier is what the user is shown"
    );
    assert!(
        harness.project_session.borrow().is_none(),
        "a known-bad entry must not disturb the open project"
    );
}

#[test]
fn a_project_that_no_longer_loads_is_flagged_so_the_user_can_clean_it_up() {
    let missing = PathBuf::from("/nonexistent/openrig-913/gone.yaml");
    let harness = Harness::new(vec![entry(&missing, true, None)]);

    assert_eq!(harness.open(0), Err(OpenRecentError::LoadFailed));

    let config = harness.app_config.borrow();
    assert!(
        !config.recent_projects[0].is_valid,
        "clicking a dead row forever is what the flag prevents"
    );
    assert!(config.recent_projects[0].invalid_reason.is_some());
}

#[test]
fn a_failed_open_leaves_the_previously_open_project_alone() {
    let dir = tempfile::tempdir().expect("tempdir");
    let good = project_file(&dir, "studio.yaml");
    let missing = PathBuf::from("/nonexistent/openrig-913/gone.yaml");
    let harness = Harness::new(vec![entry(&good, true, None), entry(&missing, true, None)]);

    harness
        .open(harness.index_of(&good))
        .expect("open the good one");
    assert!(harness.project_session.borrow().is_some());

    let stale = harness.index_of(&missing);
    assert_eq!(harness.open(stale), Err(OpenRecentError::LoadFailed));
    assert!(
        harness.project_session.borrow().is_some(),
        "a failed open must not close what the user had open"
    );
    assert_eq!(
        harness.project_chains.row_count(),
        1,
        "nor empty its chain rows"
    );
}

#[test]
fn opening_a_second_project_replaces_the_first() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = project_file(&dir, "first.yaml");
    let second = project_file(&dir, "second.yaml");
    let harness = Harness::new(vec![entry(&first, true, None), entry(&second, true, None)]);

    harness.open(harness.index_of(&first)).expect("first");
    let opened = harness.open(harness.index_of(&second)).expect("second");

    assert!(opened.canonical_path.ends_with("second.yaml"));
    assert_eq!(
        harness.project_chains.row_count(),
        1,
        "the rows are replaced, not appended"
    );
}
