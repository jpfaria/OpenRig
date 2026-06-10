//! Issue #693 — the app is effectively single-threaded: every `Command`
//! executes its side-effects INLINE on the calling thread (GUI callback
//! or MCP handler). One slow side-effect freezes the whole UI; pressing
//! a button while another action runs waits for it.
//!
//! Contract under test: dispatching a `Command` must return to the
//! caller immediately even when the side-effect's I/O is stuck — the
//! heavy work belongs to a worker ("goroutine": dedicated thread +
//! channel), never to the caller thread.
//!
//! Worst-case sink: `project_path` is a FIFO with no reader, so any
//! direct write from the dispatching thread hangs forever. Unix-only
//! (mkfifo) — PR CI runs Linux.
#![cfg(unix)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use project::project::Project;

#[test]
fn issue_693_dispatch_returns_immediately_even_when_side_effect_io_is_stuck() {
    let dir = std::env::temp_dir().join(format!("issue_693_fifo_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create tmp dir");
    let fifo = dir.join("project.yaml");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("run mkfifo");
    assert!(status.success(), "mkfifo failed");

    // The spawned thread plays the GUI thread: it dispatches and must
    // come back instantly regardless of how stuck the side-effect is.
    let (done_tx, done_rx) = mpsc::channel();
    let fifo_for_caller = fifo.clone();
    std::thread::spawn(move || {
        let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
            name: Some("issue 693".into()),
            device_settings: Vec::new(),
            chains: Vec::new(),
            midi: None,
        })));
        dispatcher.attach_project_path(fifo_for_caller);
        let t0 = Instant::now();
        let _ = dispatcher.dispatch(Command::SaveProject);
        let _ = done_tx.send(t0.elapsed());
    });

    let result = done_rx.recv_timeout(Duration::from_secs(2));
    // Best-effort unblock of the FIFO so the test process can exit
    // cleanly even while the contract is still broken.
    let _ = std::fs::OpenOptions::new().read(true).open(&fifo);
    let _ = std::fs::remove_dir_all(&dir);

    let elapsed = result.expect(
        "dispatch(SaveProject) is stuck on the side-effect's I/O: commands run \
         inline on the calling (UI) thread — the app is single-threaded (issue #693)",
    );
    assert!(
        elapsed < Duration::from_millis(200),
        "dispatch took {elapsed:?} with a stuck sink — the caller thread must only \
         enqueue; side-effects belong to the command worker (issue #693)"
    );
}
