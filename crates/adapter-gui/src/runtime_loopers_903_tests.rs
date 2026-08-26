//! #903 — opening a project gives its saved loops back, with every chain off.
//!
//! A project opens with ALL its chains disabled ("nothing auto-starts"), and
//! the loop lives in the controller's store, which only exists while a
//! controller does. The restore hung off runtime creation, so a freshly opened
//! project showed every looper as EMPTY until the user enabled a chain — and
//! the loop reappeared after enabling and disabling it again. Opening is the
//! moment the loops must come back: through the bus, so MCP's `load_project`
//! gets the same restore the GUI's open does.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, ProjectCommand};
use domain::ids::ChainId;
use engine::LooperState;
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, LooperConfig};
use project::project::Project;

use crate::runtime_lifecycle::attach_runtime_control;
use crate::state::ProjectSession;

const UID: u64 = 1;
const RATE: u32 = 48_000;
const FRAMES: usize = 1_000;

fn chain_id() -> ChainId {
    ChainId("chain-903-looper".into())
}

/// A disabled chain carrying one looper that points at `audio_file`.
fn project_with_saved_loop(audio_file: String) -> Project {
    let mut looper = LooperConfig::new(UID);
    looper.audio_file = Some(audio_file);
    Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
            loopers: vec![looper],
        }],
        midi: None,
    }
}

/// A project on disk whose looper has a non-silent sidecar wav beside it.
/// Lives in a temp dir the test owns — never the user's own files.
fn saved_project(dir: &std::path::Path) -> (std::path::PathBuf, Project) {
    std::fs::create_dir_all(dir).expect("test dir");
    let project_path = dir.join("song.openrig");
    let pcm = vec![0.5_f32; FRAMES * 2];
    let file =
        application::looper_audio::write_loop_wav(&project_path, &chain_id(), UID, &pcm, RATE)
            .expect("the sidecar wav is written");
    (project_path.clone(), project_with_saved_loop(file))
}

#[test]
fn opening_a_project_restores_its_saved_loops_with_every_chain_disabled() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let dir = tmp.path().to_path_buf();
    let (project_path, project) = saved_project(&dir);

    let session = ProjectSession::new(
        project.clone(),
        Some(project_path.clone()),
        None,
        dir.join("presets"),
    );
    let project_runtime: Rc<RefCell<Option<ProjectRuntimeController>>> =
        Rc::new(RefCell::new(None));
    attach_runtime_control(
        &project_runtime,
        &crate::runtime_analyzers::AnalyzerSessions::detached(),
        &session,
    );

    assert!(
        project_runtime.borrow().is_none(),
        "precondition: a project opens with every chain disabled, so nothing is running"
    );

    session
        .dispatcher
        .dispatch(Command::Project(ProjectCommand::LoadProject {
            project,
            path: project_path,
        }))
        .expect("opening a project is not an error");

    let runtime = project_runtime.borrow();
    let controller = runtime
        .as_ref()
        .expect("the restore needs a store to put the loop in");
    let status = controller
        .chain_looper_status(&chain_id(), UID)
        .expect("the looper exists");
    assert_ne!(
        status.state,
        LooperState::Empty,
        "a project that carries a recorded loop must not open showing EMPTY"
    );
    assert!(
        status.len_frames > 0,
        "the saved take must come back with its length — got {} frames",
        status.len_frames
    );
}
