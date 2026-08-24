//! #826 RED — the wav is written, but the project on disk does not point at it.
//!
//! Owner repro, second round: "fechei o programa e o áudio gravado se perdeu".
//! Forensics on the real project after that session: a fresh
//! `rig-input-1-looper-1.wav` sits in `project.loops/`, and `project.yaml`
//! carries the looper — with NO `audio_file` key. The audio was saved; the
//! project simply forgot where it is, so reopening finds an empty looper.
//!
//! The order in `save_project_to_disk` is the bug: `CaptureRigEdits` copies the
//! project's loopers into the rig FIRST, then `export_project_loops` writes the
//! wavs and stamps `audio_file` onto the project's chains — but what hits disk
//! is built from the RIG, which was captured a step too early.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use application::command::{Command, ProjectCommand};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use application::runtime_control::RuntimeControl;
use engine::LoopPcm;
use project::chain::{Chain, LooperConfig};
use project::rig::{RigInput, RigPreset, RigProject};

/// A rig hosting one recorded loop on its only input.
struct RecordedLoop;

impl RuntimeControl for RecordedLoop {
    fn export_chain_loops(&self, chain: &Chain) -> Option<Vec<(u64, Arc<LoopPcm>)>> {
        Some(
            chain
                .loopers
                .iter()
                .map(|cfg| {
                    (
                        cfg.uid,
                        Arc::new(LoopPcm::new([0.25f32, -0.25].repeat(128).to_vec(), 48_000)),
                    )
                })
                .collect(),
        )
    }
}

fn rig_with_a_looper() -> RigProject {
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
            label: None,
            bank,
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
            io_binding_ids: Vec::new(),
            loopers: vec![LooperConfig::new(1)],
        },
    );
    RigProject {
        name: None,
        inputs,
        presets,
        outputs: BTreeMap::new(),
        chain_order: Vec::new(),
        midi: None,
    }
}

#[test]
fn the_saved_project_points_at_the_loop_it_just_wrote() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let project_path = tmp.path().join("project.yaml");

    let rig = Rc::new(RefCell::new(rig_with_a_looper()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    dispatcher.attach_project_path(project_path.clone());
    dispatcher.attach_runtime_control(Rc::new(RecordedLoop));

    dispatcher
        .dispatch(Command::Project(ProjectCommand::SaveProject))
        .expect("save");
    application::persist_worker::flush();

    let wav = tmp.path().join("project.loops").join("rig-in-looper-1.wav");
    assert!(
        wav.exists(),
        "the loop's audio must be written beside the project"
    );

    let yaml = std::fs::read_to_string(&project_path).expect("the project must be on disk");
    assert!(
        yaml.contains("audio_file: rig-in-looper-1.wav"),
        "the saved project must point at the wav it just wrote, or reopening \
         finds an empty looper next to its own audio; got:\n{yaml}"
    );
}

/// The whole point of the pointer: reopening the saved project finds the loop
/// and its audio, not an empty looper.
#[test]
fn reopening_the_saved_project_finds_the_recorded_audio() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let project_path = tmp.path().join("project.yaml");

    let rig = Rc::new(RefCell::new(rig_with_a_looper()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    dispatcher.attach_project_path(project_path.clone());
    dispatcher.attach_runtime_control(Rc::new(RecordedLoop));

    dispatcher
        .dispatch(Command::Project(ProjectCommand::SaveProject))
        .expect("save");
    application::persist_worker::flush();

    // Reopen exactly the way the app does: read the rig back, project it.
    let yaml = std::fs::read_to_string(&project_path).expect("the project must be on disk");
    let reopened: RigProject = serde_yaml::from_str::<serde_yaml::Value>(&yaml)
        .ok()
        .and_then(|v| serde_yaml::from_value(v.get("project")?.clone()).ok())
        .expect("the saved project must deserialize as a rig");
    let chains = engine::rig_runtime::rig_to_legacy_project(&reopened, &BTreeSet::new());
    let looper = &chains
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("chain")
        .loopers[0];

    let file = looper
        .audio_file
        .as_deref()
        .expect("the reopened looper must know where its audio is");
    let (pcm, rate) =
        application::looper_audio::read_loop_wav(&project_path, file).expect("read the loop back");
    assert_eq!(rate, 48_000);
    assert_eq!(pcm.len(), 256, "the recorded loop comes back whole");
}
