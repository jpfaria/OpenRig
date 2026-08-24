//! #826 — the whole round trip on a RIG project: record, save, close, reopen
//! FROM DISK, and the audio is still there.
//!
//! `runtime_loopers_tests` already covered record → save → restore, but its
//! session carries `rig: None` and its "reopen" reuses the in-memory project.
//! The app's real project IS a rig (`project.yaml` with `inputs:`), what hits
//! disk is built from the rig, and reopening re-reads the file. Every loss the
//! owner reported lived in that blind spot: the chain editor blanking the
//! chain's loopers, and the `audio_file` pointer being stamped after the rig
//! was already captured.
//!
//! These tests close it. Nothing is faked but the audio device: the real
//! store, the real `RuntimeControl`, the real save, the real
//! `load_rig_and_project` off disk, the real restore.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use application::command::{ChainCommand, Command, LooperCommand, ProjectCommand};
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use engine::LooperState;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::rig::{RigInput, RigPreset, RigProject};

use super::restore_chain_loops;
use crate::state::ProjectSession;

const CHAIN: &str = "rig:in";

type Runtime = Rc<RefCell<Option<ProjectRuntimeController>>>;

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

/// A rig with one input, bound to the registry above — the shape of the real
/// `project.yaml`.
fn rig() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".into(),
        RigPreset::from_legacy_blocks(Vec::new(), 100.0),
    );
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".into());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".into(),
        RigInput {
            label: Some("GUITARRA".into()),
            bank,
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
            io_binding_ids: vec!["io".into()],
            loopers: Vec::new(),
        },
    );
    RigProject {
        name: Some("project".into()),
        inputs,
        presets,
        outputs: BTreeMap::new(),
        chain_order: Vec::new(),
        midi: None,
    }
}

/// A session over a rig, exactly as `load_project_session` assembles one.
fn rig_session(project_path: &Path, rig: RigProject) -> ProjectSession {
    let project = engine::rig_runtime::rig_to_legacy_project(&rig, &Default::default());
    let mut session = ProjectSession::new(
        project,
        Some(project_path.to_path_buf()),
        None,
        PathBuf::from("./presets"),
    );
    let rig = Rc::new(RefCell::new(rig));
    session.dispatcher.attach_rig(Rc::clone(&rig));
    session.rig = Some(rig);
    *session.io_bindings.borrow_mut() = registry();
    session
}

/// A controller for the session's chain, with no audio device opened.
fn controller(session: &ProjectSession) -> Runtime {
    let chain = session.project.borrow().chains[0].clone();
    let mut chains = HashMap::new();
    chains.insert(
        (chain.id.clone(), 0usize),
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[256], &registry()).unwrap()),
    );
    let mut controller =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, 48_000);
    controller.set_io_bindings(registry());
    Rc::new(RefCell::new(Some(controller)))
}

fn tick(runtime: &Runtime, level: f32) {
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

/// Record `callbacks` callbacks of a steady signal into looper `uid` (128
/// frames each) and close it.
fn record_loop(runtime: &Runtime, chain: &Chain, uid: u64, callbacks: usize) {
    {
        let borrow = runtime.borrow();
        let c = borrow.as_ref().unwrap();
        c.looper_create(&chain.id, uid);
        c.looper_tap_record(&chain.id, uid);
        c.drain_looper_recording(chain);
    }
    for _ in 0..callbacks {
        tick(runtime, 0.5);
        let borrow = runtime.borrow();
        let c = borrow.as_ref().unwrap();
        c.drain_looper_recording(chain);
    }
    let borrow = runtime.borrow();
    let c = borrow.as_ref().unwrap();
    c.looper_tap_record(&chain.id, uid);
}

/// Add a looper through the bus and record into it. Returns its uid.
fn add_and_record(session: &ProjectSession, runtime: &Runtime, callbacks: usize) -> u64 {
    session
        .dispatcher
        .dispatch(Command::Looper(LooperCommand::AddChainLooper {
            chain: ChainId(CHAIN.into()),
        }))
        .expect("add looper");
    let uid = session.project.borrow().chains[0].loopers[0].uid;
    let chain = session.project.borrow().chains[0].clone();
    record_loop(runtime, &chain, uid, callbacks);
    uid
}

/// Save the way the app saves — attach the real `RuntimeControl`, dispatch,
/// and wait for the persist worker so the bytes are really on disk.
fn save(session: &ProjectSession, runtime: &Runtime) {
    crate::runtime_lifecycle::attach_runtime_control(
        runtime,
        &crate::runtime_analyzers::AnalyzerSessions::detached(),
        session,
    );
    session
        .dispatcher
        .dispatch(Command::Project(ProjectCommand::SaveProject))
        .expect("save");
    application::persist_worker::flush();
}

/// Close the app and open it again: read the project back off disk and give a
/// brand-new runtime its loops. Returns the reopened session + runtime.
fn reopen(project_path: &Path) -> (ProjectSession, Runtime) {
    let (rig, project) =
        crate::project_ops::load_rig_and_project(project_path).expect("reopen the project");
    let mut session = ProjectSession::new(
        project,
        Some(project_path.to_path_buf()),
        None,
        PathBuf::from("./presets"),
    );
    let rig = Rc::new(RefCell::new(rig));
    session.dispatcher.attach_rig(Rc::clone(&rig));
    session.rig = Some(rig);
    *session.io_bindings.borrow_mut() = registry();
    let runtime = controller(&session);
    restore_chain_loops(&session, &runtime, project_path);
    (session, runtime)
}

/// What the reopened looper holds: its state and its length in frames.
fn restored(runtime: &Runtime, uid: u64) -> (LooperState, usize) {
    tick(runtime, 0.0);
    let status = runtime
        .borrow()
        .as_ref()
        .unwrap()
        .chain_looper_status(&ChainId(CHAIN.into()), uid)
        .expect("the looper is back");
    (status.state, status.len_frames)
}

#[test]
fn a_recorded_loop_survives_closing_and_reopening_the_app() {
    let dir = tempfile::tempdir().expect("temp dir");
    let project_path = dir.path().join("project.yaml");
    let session = rig_session(&project_path, rig());
    let runtime = controller(&session);

    let uid = add_and_record(&session, &runtime, 1);
    save(&session, &runtime);

    let (_reopened, runtime) = reopen(&project_path);
    assert_eq!(
        restored(&runtime, uid),
        (LooperState::Stopped, 128),
        "the loop recorded before the app closed must come back with its audio"
    );
}

#[test]
fn renaming_the_chain_does_not_cost_the_loop() {
    // The owner's sequence: record, then open the chain editor and rename.
    let dir = tempfile::tempdir().expect("temp dir");
    let project_path = dir.path().join("project.yaml");
    let session = rig_session(&project_path, rig());
    let runtime = controller(&session);

    let uid = add_and_record(&session, &runtime, 1);

    let existing = session.project.borrow().chains[0].clone();
    let mut draft = crate::chain_editor::chain_draft_from_chain(0, &existing);
    draft.name = "GUITARRA - TONES".into();
    let renamed = crate::chain_editor::chain_from_draft(&draft, Some(&existing));
    session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::SaveChain { chain: renamed }))
        .expect("save the chain");

    save(&session, &runtime);

    let (reopened, runtime) = reopen(&project_path);
    assert_eq!(
        reopened.project.borrow().chains[0].description.as_deref(),
        Some("GUITARRA - TONES"),
        "the rename is what the editor was for"
    );
    assert_eq!(
        restored(&runtime, uid),
        (LooperState::Stopped, 128),
        "renaming the chain must not cost the loop it recorded"
    );
}

#[test]
fn a_waveform_edit_survives_closing_and_reopening_the_app() {
    let dir = tempfile::tempdir().expect("temp dir");
    let project_path = dir.path().join("project.yaml");
    let session = rig_session(&project_path, rig());
    let runtime = controller(&session);

    let uid = add_and_record(&session, &runtime, 4);
    // The editor works on a stopped loop; keep only its first half. The edit
    // reports the length it produced — a seam is blended in, so it is not
    // simply `end - start`, and THAT is the length that has to come back.
    let edited = {
        let borrow = runtime.borrow();
        let c = borrow.as_ref().unwrap();
        c.looper_stop(&ChainId(CHAIN.into()), uid);
        c.looper_apply_edit(
            &ChainId(CHAIN.into()),
            uid,
            engine::loop_edit::LoopEditOp::Keep,
            0,
            256,
        )
        .expect("keep the first half")
    };
    assert!(
        edited < 512,
        "the edit has to actually shorten the take, or this proves nothing"
    );

    save(&session, &runtime);

    let (_reopened, runtime) = reopen(&project_path);
    assert_eq!(
        restored(&runtime, uid),
        (LooperState::Stopped, edited),
        "the loop must come back EDITED, not as the take before the edit"
    );
}
