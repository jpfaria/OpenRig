//! #127 (Task 13), red-first: saving a project writes its recorded loops.
//!
//! A loop is audio, so it travels as a wav sidecar under `<project>.loops/`
//! and the chain only remembers the file name. That export used to run in the
//! GUI's Save callback (`looper_persist::save_chain_loops`), immediately
//! BEFORE it dispatched `SaveProject` — so an MCP or gRPC client dispatching
//! `SaveProject` serialized a project whose loops were never written, and
//! reopening it found silence.
//!
//! The PCM crosses as an `Arc<LoopPcm>` handle, never as samples copied
//! through a reading.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use domain::ids::ChainId;
use engine::LoopPcm;
use project::chain::{Chain, LooperConfig};
use project::project::Project;

use crate::command::{Command, ProjectCommand};
use crate::dispatcher::CommandDispatcher;
use crate::local_dispatcher::LocalDispatcher;
use crate::runtime_control::RuntimeControl;

/// A runtime whose store holds exactly `loops`. `None` ⇒ nothing is hosted
/// (the rig is stopped), which is NOT the same as "nothing was recorded".
struct StoreRuntimeControl {
    loops: Option<Vec<(u64, Vec<f32>, u32)>>,
}

impl RuntimeControl for StoreRuntimeControl {
    fn export_chain_loops(&self, _chain: &Chain) -> Option<Vec<(u64, Arc<LoopPcm>)>> {
        self.loops.as_ref().map(|loops| {
            loops
                .iter()
                .map(|(uid, pcm, rate)| (*uid, Arc::new(LoopPcm::new(pcm.clone(), *rate))))
                .collect()
        })
    }
}

fn chain_with(uids: &[u64], audio_file: Option<&str>) -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: uids
            .iter()
            .map(|uid| LooperConfig {
                audio_file: audio_file.map(str::to_string),
                ..LooperConfig::new(*uid)
            })
            .collect(),
    }
}

/// A dispatcher saving into a fresh temp dir, with `loops` in its store.
fn dispatcher_saving_into(
    dir: &std::path::Path,
    chain: Chain,
    loops: Option<Vec<(u64, Vec<f32>, u32)>>,
) -> LocalDispatcher {
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain],
        midi: None,
    })));
    dispatcher.attach_project_path(dir.join("song.openrig"));
    dispatcher.attach_runtime_control(Rc::new(StoreRuntimeControl { loops }));
    dispatcher
}

fn audio_file_of(dispatcher: &LocalDispatcher, uid: u64) -> Option<String> {
    dispatcher.project.borrow().chains[0]
        .loopers
        .iter()
        .find(|l| l.uid == uid)
        .and_then(|l| l.audio_file.clone())
}

#[test]
fn saving_the_project_writes_every_recorded_loop_beside_it() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dispatcher = dispatcher_saving_into(
        tmp.path(),
        chain_with(&[1], None),
        Some(vec![(1, vec![0.25; 64], 44_100)]),
    );

    dispatcher
        .dispatch(Command::Project(ProjectCommand::SaveProject))
        .expect("save");

    let name = audio_file_of(&dispatcher, 1)
        .expect("the chain must remember the sidecar the save just wrote");
    let path = crate::looper_audio::loops_dir(&tmp.path().join("song.openrig")).join(&name);
    assert!(
        path.exists(),
        "an MCP/gRPC save must write the loop wav too — otherwise the project \
         is serialized pointing at audio nobody ever wrote: {path:?}"
    );
    let (pcm, rate) = crate::looper_audio::read_loop_wav(&tmp.path().join("song.openrig"), &name)
        .expect("read back");
    assert_eq!(
        (pcm.len(), rate),
        (64, 44_100),
        "the sidecar must carry the loop's OWN recorded rate, not some other \
         stream's — the handle carries it with the audio"
    );
}

#[test]
fn a_looper_with_nothing_recorded_forgets_its_stale_sidecar() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dispatcher = dispatcher_saving_into(
        tmp.path(),
        chain_with(&[1], Some("stale.wav")),
        // Hosted, and this looper holds no material.
        Some(vec![]),
    );

    dispatcher
        .dispatch(Command::Project(ProjectCommand::SaveProject))
        .expect("save");

    assert_eq!(
        audio_file_of(&dispatcher, 1),
        None,
        "a loop that was cleared must not keep pointing at the wav it used to \
         have, or reopening the project brings the erased audio back"
    );
}

/// The distinction the tri-state exists for: "no store is hosted" must never
/// be read as "nothing was recorded". Saving a stopped rig has to leave the
/// pointers exactly as they are — anything else erases the user's loops from
/// the project file.
#[test]
fn saving_with_the_rig_stopped_leaves_the_saved_loops_alone() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dispatcher = dispatcher_saving_into(
        tmp.path(),
        chain_with(&[1], Some("recorded-earlier.wav")),
        None,
    );

    dispatcher
        .dispatch(Command::Project(ProjectCommand::SaveProject))
        .expect("save");

    assert_eq!(
        audio_file_of(&dispatcher, 1).as_deref(),
        Some("recorded-earlier.wav"),
        "a save with nothing running must not wipe the loops recorded in an \
         earlier session"
    );
}
