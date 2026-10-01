//! #827, red-first: `SaveChainLooperTake` keeps a recorded loop as a named
//! take in the app-wide library, and the take plays as a DI source on ANY
//! chain — the looper mixdown and the DI's PCM are one and the same file.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use domain::ids::ChainId;
use engine::LoopPcm;
use project::chain::{Chain, LooperConfig};
use project::project::Project;

use crate::command::{ChainCommand, Command, LooperCommand};
use crate::di_loader::DiLoopSource;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::looper_take_library::TakeSaveError;
use crate::runtime_control::RuntimeControl;

/// A runtime whose looper store holds exactly `loops`; `None` ⇒ no store is
/// hosted (the rig is stopped).
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

fn chain(id: &str, looper_uids: &[u64]) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: looper_uids
            .iter()
            .map(|uid| LooperConfig::new(*uid))
            .collect(),
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

fn dispatcher(takes: &Path, loops: Option<Vec<(u64, Vec<f32>, u32)>>) -> LocalDispatcher {
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain("rig:guitar", &[1]), chain("rig:other", &[])],
        midi: None,
    })));
    dispatcher.attach_looper_takes_path(Some(takes.to_path_buf()));
    dispatcher.attach_runtime_control(Rc::new(StoreRuntimeControl { loops }));
    dispatcher
}

fn save(dispatcher: &LocalDispatcher, name: &str) -> anyhow::Result<Vec<Event>> {
    dispatcher.dispatch(Command::Looper(LooperCommand::SaveChainLooperTake {
        chain: ChainId("rig:guitar".into()),
        looper: 1,
        name: name.into(),
    }))
}

fn saved_path(events: &[Event]) -> PathBuf {
    events
        .iter()
        .find_map(|e| match e {
            Event::ChainLooperTakeSaved { path, .. } => Some(path.clone()),
            _ => None,
        })
        .expect("a save must announce where the take landed")
}

#[test]
fn saving_a_take_writes_the_looper_mixdown_into_the_library() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let pcm: Vec<f32> = (0..64).map(|i| i as f32 / 128.0).collect();
    let d = dispatcher(tmp.path(), Some(vec![(1, pcm.clone(), 44_100)]));

    let events = save(&d, "verse").expect("save");

    let path = saved_path(&events);
    assert_eq!(path, tmp.path().join("verse.wav"));
    let wav = adapter_render::wav::read_wav(&path).expect("read back");
    assert_eq!(
        (wav.samples, wav.sample_rate_hz),
        (pcm, 44_100),
        "the take is the looper's own mixdown at its own recorded rate"
    );
}

#[test]
fn a_saved_take_loads_as_the_di_source_of_another_chain() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let d = dispatcher(tmp.path(), Some(vec![(1, vec![0.25; 256], 48_000)]));
    let path = saved_path(&save(&d, "riff").expect("save"));

    let other = ChainId("rig:other".into());
    d.dispatch(Command::Chain(ChainCommand::SetChainDiLoopSource {
        chain: other.clone(),
        source: DiLoopSource::File(path.clone()),
    }))
    .expect("a saved take is a valid DI source");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while d.di_loop_source_for_chain(&other).is_none() && std::time::Instant::now() < deadline {
        let _ = d.poll_async_results();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        d.di_loop_source_for_chain(&other),
        Some(DiLoopSource::File(path))
    );
}

fn refusal(result: anyhow::Result<Vec<Event>>) -> TakeSaveError {
    result
        .expect_err("the save must be refused")
        .downcast::<TakeSaveError>()
        .expect("the refusal says why, typed")
}

#[test]
fn a_looper_with_nothing_recorded_saves_nothing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let d = dispatcher(tmp.path(), Some(vec![]));

    assert_eq!(refusal(save(&d, "empty")), TakeSaveError::NothingRecorded);
    assert!(!tmp.path().join("empty.wav").exists());
}

#[test]
fn a_stopped_rig_has_nothing_to_save() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let d = dispatcher(tmp.path(), None);

    assert_eq!(refusal(save(&d, "ghost")), TakeSaveError::NothingRecorded);
}

#[test]
fn a_name_already_in_the_library_is_refused() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let d = dispatcher(tmp.path(), Some(vec![(1, vec![0.25; 8], 48_000)]));
    save(&d, "take").expect("first save");

    assert_eq!(
        refusal(save(&d, "take")),
        TakeSaveError::NameTaken("take.wav".into())
    );
}
