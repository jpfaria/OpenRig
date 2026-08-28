//! #913 — Save As re-points everything that derives from the project's path.
//!
//! Four things follow the file the user chose, and each has been a bug when it
//! did not: where the YAML is written (#555), where its sidecar config goes,
//! where presets are saved and loaded from, and where a cold start restores the
//! recorded loops from (#127).

use super::bind_project_path;
use crate::runtime_analyzers::AnalyzerSessions;
use crate::runtime_lifecycle::RuntimeAttach;
use crate::state::ProjectSession;
use project::project::Project;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

fn session() -> ProjectSession {
    ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        None,
        None,
        PathBuf::from("/tmp/openrig-913-old/presets"),
    )
}

fn attach() -> RuntimeAttach {
    RuntimeAttach::new(&Rc::new(RefCell::new(None)), &AnalyzerSessions::detached())
}

#[test]
fn the_session_points_at_the_chosen_file() {
    let mut session = session();
    let chosen = PathBuf::from("/tmp/openrig-913-new/studio.yaml");
    bind_project_path(&mut session, chosen.clone(), &attach());
    assert_eq!(session.project_path, Some(chosen));
}

#[test]
fn the_presets_folder_follows_the_project_into_its_new_home() {
    let mut session = session();
    bind_project_path(
        &mut session,
        PathBuf::from("/tmp/openrig-913-new/studio.yaml"),
        &attach(),
    );
    assert_eq!(
        session.presets_path,
        PathBuf::from("/tmp/openrig-913-new/presets"),
        "presets must land next to the file the user just chose"
    );
}

#[test]
fn the_config_sidecar_is_resolved_for_the_new_path() {
    let mut session = session();
    bind_project_path(
        &mut session,
        PathBuf::from("/tmp/openrig-913-new/studio.yaml"),
        &attach(),
    );
    let config = session.config_path.clone().expect("a config path");
    assert!(
        config.starts_with("/tmp/openrig-913-new"),
        "the sidecar must not stay in the old folder: {}",
        config.display()
    );
}

#[test]
fn a_bare_filename_with_no_parent_still_resolves_a_presets_folder() {
    let mut session = session();
    bind_project_path(&mut session, PathBuf::from("studio.yaml"), &attach());
    assert_eq!(
        session.presets_path,
        PathBuf::from("presets"),
        "a relative presets folder beside the file, never an empty path"
    );
}

#[test]
fn saving_as_a_second_time_moves_everything_again() {
    let mut session = session();
    let attach = attach();
    bind_project_path(
        &mut session,
        PathBuf::from("/tmp/openrig-913-a/studio.yaml"),
        &attach,
    );
    bind_project_path(
        &mut session,
        PathBuf::from("/tmp/openrig-913-b/studio.yaml"),
        &attach,
    );
    assert_eq!(
        session.project_path,
        Some(PathBuf::from("/tmp/openrig-913-b/studio.yaml"))
    );
    assert_eq!(
        session.presets_path,
        PathBuf::from("/tmp/openrig-913-b/presets")
    );
}
