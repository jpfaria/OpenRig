//! `DeleteLooperTake` removes a saved take from the
//! app-wide library, and a chain whose DI has that take loaded stops playing
//! it and unloads it — it never keeps pointing at a file that is gone.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use domain::ids::ChainId;
use engine::DiPcm;
use project::chain::Chain;
use project::project::Project;

use crate::command::{Command, LooperCommand};
use crate::di_loader::DiLoopSource;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::looper_take_library::{save_take, TakeDeleteError};
use crate::runtime_control::RuntimeControl;

#[derive(Default)]
struct SpyRuntimeControl {
    calls: Rc<RefCell<Vec<String>>>,
}

impl RuntimeControl for SpyRuntimeControl {
    fn disarm_di_stream(&self, chain: &ChainId) {
        self.calls.borrow_mut().push(format!("disarm {}", chain.0));
    }
}

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
        disabled_endpoints: Default::default(),
    }
}

fn dispatcher(takes: &Path) -> (LocalDispatcher, Rc<RefCell<Vec<String>>>) {
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain("rig:a"), chain("rig:b"), chain("rig:c")],
        midi: None,
    })));
    dispatcher.attach_looper_takes_path(Some(takes.to_path_buf()));
    let calls = Rc::new(RefCell::new(Vec::new()));
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
    }));
    (dispatcher, calls)
}

/// `chain` has `source` loaded as its DI, exactly as the decode task installs it.
fn load(dispatcher: &LocalDispatcher, chain: &str, source: DiLoopSource) {
    dispatcher.di_loop_state.borrow_mut().insert(
        ChainId(chain.into()),
        (source, Arc::new(DiPcm::new(vec![0.0; 8], 48_000, 1))),
    );
}

fn take(dir: &Path, name: &str) -> PathBuf {
    save_take(dir, name, &[0.25; 8], 48_000).expect("save")
}

fn delete(dispatcher: &LocalDispatcher, name: &str) -> anyhow::Result<Vec<Event>> {
    dispatcher.dispatch(Command::Looper(LooperCommand::DeleteLooperTake {
        name: name.into(),
    }))
}

#[test]
fn deleting_a_take_removes_it_from_the_library_and_says_which() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = take(tmp.path(), "verse");
    let (d, _) = dispatcher(tmp.path());

    let events = delete(&d, "verse.wav").expect("delete");

    assert!(!path.exists());
    assert!(events.contains(&Event::LooperTakeDeleted { path }));
}

#[test]
fn a_chain_playing_the_deleted_take_stops_and_unloads_it() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = take(tmp.path(), "riff");
    let (d, calls) = dispatcher(tmp.path());
    load(&d, "rig:a", DiLoopSource::File(path.clone()));

    let events = delete(&d, "riff.wav").expect("delete");

    let a = ChainId("rig:a".into());
    assert_eq!(*calls.borrow(), vec!["disarm rig:a".to_string()]);
    assert_eq!(d.di_loop_source_for_chain(&a), None);
    assert!(events.contains(&Event::ChainDiLoopEnabledChanged {
        chain: a.clone(),
        enabled: false,
    }));
    assert!(events.contains(&Event::ChainDiLoopSourceChanged { chain: a }));
}

#[test]
fn chains_with_another_source_are_left_alone() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let doomed = take(tmp.path(), "doomed");
    let kept = take(tmp.path(), "kept");
    let (d, calls) = dispatcher(tmp.path());
    load(&d, "rig:a", DiLoopSource::File(doomed));
    load(&d, "rig:b", DiLoopSource::File(kept.clone()));
    load(&d, "rig:c", DiLoopSource::Bundled("funk".into()));

    let events = delete(&d, "doomed").expect("delete");

    assert_eq!(*calls.borrow(), vec!["disarm rig:a".to_string()]);
    assert_eq!(
        d.di_loop_source_for_chain(&ChainId("rig:b".into())),
        Some(DiLoopSource::File(kept))
    );
    assert_eq!(
        d.di_loop_source_for_chain(&ChainId("rig:c".into())),
        Some(DiLoopSource::Bundled("funk".into()))
    );
    for other in ["rig:b", "rig:c"] {
        assert!(
            events
                .iter()
                .all(|e| e.chain().map(|c| c.0.as_str()) != Some(other)),
            "{other} must not be told anything changed"
        );
    }
}

#[test]
fn every_chain_holding_the_take_is_unloaded() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = take(tmp.path(), "loop");
    let (d, calls) = dispatcher(tmp.path());
    load(&d, "rig:a", DiLoopSource::File(path.clone()));
    load(&d, "rig:b", DiLoopSource::File(path));

    delete(&d, "loop.wav").expect("delete");

    let mut seen = calls.borrow().clone();
    seen.sort();
    assert_eq!(seen, vec!["disarm rig:a", "disarm rig:b"]);
}

fn refusal(result: anyhow::Result<Vec<Event>>) -> TakeDeleteError {
    result
        .expect_err("the delete must be refused")
        .downcast::<TakeDeleteError>()
        .expect("the refusal says why, typed")
}

#[test]
fn a_name_outside_the_library_is_refused_and_touches_nothing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let lib = tmp.path().join("lib");
    let outside = take(tmp.path(), "outside");
    let (d, calls) = dispatcher(&lib);
    load(&d, "rig:a", DiLoopSource::File(outside.clone()));

    assert_eq!(
        refusal(delete(&d, "../outside.wav")),
        TakeDeleteError::InvalidName
    );
    assert!(outside.exists());
    assert!(calls.borrow().is_empty());
    assert!(d
        .di_loop_source_for_chain(&ChainId("rig:a".into()))
        .is_some());
}

#[test]
fn a_take_that_is_not_there_is_refused() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (d, _) = dispatcher(tmp.path());

    assert_eq!(
        refusal(delete(&d, "ghost")),
        TakeDeleteError::NotFound("ghost.wav".into())
    );
}
