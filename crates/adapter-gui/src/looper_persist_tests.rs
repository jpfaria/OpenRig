//! #323 — a recorded loop survives closing and reopening the project.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use engine::{LooperOp, LooperState};
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::project::Project;

use super::{restore_chain_loops, save_chain_loops};
use crate::state::ProjectSession;

const CHAIN: &str = "chain:loop-persist";

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn chain() -> Chain {
    Chain {
        id: ChainId(CHAIN.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn session(project_path: PathBuf) -> ProjectSession {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain()],
        midi: None,
    }));
    let dispatcher = Rc::new(LocalDispatcher::new(Rc::clone(&project)));
    ProjectSession {
        project,
        dispatcher,
        project_path: Some(project_path),
        config_path: None,
        presets_path: PathBuf::from("./presets"),
        rig: None,
        io_bindings: Rc::new(RefCell::new(registry())),
    }
}

fn controller() -> Rc<RefCell<Option<ProjectRuntimeController>>> {
    let mut chains = HashMap::new();
    chains.insert(
        (ChainId(CHAIN.into()), 0usize),
        Arc::new(build_chain_runtime_state(&chain(), 48_000.0, &[256], &registry()).unwrap()),
    );
    let mut controller =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, 48_000);
    controller.set_io_bindings(registry());
    Rc::new(RefCell::new(Some(controller)))
}

fn tick(runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>, level: f32) {
    let borrow = runtime.borrow();
    let Some(c) = borrow.as_ref() else { return };
    let frames = 128usize;
    let input = vec![level; frames * 2];
    let mut output = vec![0.0f32; frames * 2];
    for rt in c.runtimes_for_chain(&ChainId(CHAIN.into())) {
        engine::runtime::process_input_f32(&rt, 0, &input, 2);
        engine::runtime::process_output_f32(&rt, 0, &mut output, 2);
    }
}

/// Record one callback of a steady signal into looper `uid` and close it.
fn record_loop(runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>, uid: u64) {
    {
        let borrow = runtime.borrow();
        let c = borrow.as_ref().unwrap();
        let id = ChainId(CHAIN.into());
        c.push_chain_looper_op(&id, |_| Some(LooperOp::Create { uid }));
        c.push_chain_looper_op(&id, |rt| {
            Some(LooperOp::TapRecord {
                uid,
                buffer: Some(vec![0.0f32; rt.looper_max_frames() * 2].into_boxed_slice()),
            })
        });
    }
    tick(runtime, 0.5);
    {
        let borrow = runtime.borrow();
        let c = borrow.as_ref().unwrap();
        c.push_chain_looper_op(&ChainId(CHAIN.into()), |_| {
            Some(LooperOp::TapRecord { uid, buffer: None })
        });
    }
    tick(runtime, 0.0);
}

#[test]
fn a_recorded_loop_is_written_beside_the_project_and_comes_back_on_reopen() {
    let dir = tempfile::tempdir().expect("temp dir");
    let project_path = dir.path().join("song.openrig");
    let session = session(project_path.clone());
    let runtime = controller();

    session
        .dispatcher
        .dispatch(Command::AddChainLooper {
            chain: ChainId(CHAIN.into()),
        })
        .expect("add");
    let uid = session.project.borrow().chains[0].loopers[0].uid;
    record_loop(&runtime, uid);

    save_chain_loops(&session, &runtime, &project_path);

    let saved = session.project.borrow().chains[0].loopers[0]
        .audio_file
        .clone()
        .expect("the chain remembers the file it saved");
    assert!(
        dir.path().join("song.loops").join(&saved).exists(),
        "the loop audio lives beside the project"
    );

    // Reopen: fresh runtimes, same project.
    let reopened = controller();
    restore_chain_loops(&session, &reopened, &project_path);
    tick(&reopened, 0.0);

    let status = reopened
        .borrow()
        .as_ref()
        .unwrap()
        .chain_looper_status(&ChainId(CHAIN.into()), uid)
        .expect("the looper is back");
    assert_eq!(
        status.state,
        LooperState::Stopped,
        "a restored loop waits for the user instead of playing on open"
    );
    assert_eq!(status.len_frames, 128, "the recorded length came back");
}

#[test]
fn an_empty_looper_saves_no_file_and_clears_a_stale_pointer() {
    let dir = tempfile::tempdir().expect("temp dir");
    let project_path = dir.path().join("song.openrig");
    let session = session(project_path.clone());
    let runtime = controller();

    session
        .dispatcher
        .dispatch(Command::AddChainLooper {
            chain: ChainId(CHAIN.into()),
        })
        .expect("add");
    let uid = session.project.borrow().chains[0].loopers[0].uid;
    session.project.borrow_mut().chains[0].loopers[0].audio_file = Some("stale.wav".into());

    save_chain_loops(&session, &runtime, &project_path);

    assert!(
        session.project.borrow().chains[0].loopers[0]
            .audio_file
            .is_none(),
        "an empty looper must not keep pointing at an old recording"
    );
    let _ = uid;
}

#[test]
fn a_missing_sidecar_does_not_break_opening_the_project() {
    let dir = tempfile::tempdir().expect("temp dir");
    let project_path = dir.path().join("song.openrig");
    let session = session(project_path.clone());
    session
        .dispatcher
        .dispatch(Command::AddChainLooper {
            chain: ChainId(CHAIN.into()),
        })
        .expect("add");
    let uid = session.project.borrow().chains[0].loopers[0].uid;
    session.project.borrow_mut().chains[0].loopers[0].audio_file = Some("gone.wav".into());

    let runtime = controller();
    restore_chain_loops(&session, &runtime, &project_path);
    tick(&runtime, 0.0);

    let status = runtime
        .borrow()
        .as_ref()
        .unwrap()
        .chain_looper_status(&ChainId(CHAIN.into()), uid)
        .expect("the looper slot still exists");
    assert_eq!(
        status.state,
        LooperState::Empty,
        "the looper is simply empty — the project still opens"
    );
}
