//! The processing lock as the audio side takes it (#987): the device callback
//! never waits, the per-input DSP worker waits a bounded moment for an edit's
//! swap, and neither ever panics on the lock.

use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ids::ChainId;
use project::chain::Chain;

use super::try_lock_processing;
use crate::runtime::build_chain_runtime_state;
use crate::runtime_state::ChainRuntimeState;

fn runtime() -> Arc<ChainRuntimeState> {
    let chain = Chain {
        id: ChainId("processing-lock".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    };
    Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[256], &[]).expect("empty chain builds"))
}

/// Hold `runtime`'s processing lock on another thread, the way an edit's swap
/// does, until `hold` has passed. Returns once the lock is held.
fn edit_holding_the_lock(
    runtime: &Arc<ChainRuntimeState>,
    hold: Duration,
) -> std::thread::JoinHandle<()> {
    let runtime = Arc::clone(runtime);
    let (held, is_held) = mpsc::channel();
    let edit = std::thread::spawn(move || {
        let _guard = runtime.processing.lock().expect("lock");
        held.send(()).expect("test waits");
        std::thread::sleep(hold);
    });
    is_held.recv().expect("the edit holds the lock");
    edit
}

#[test]
fn the_device_callback_never_waits_for_the_lock() {
    let runtime = runtime();
    let edit = edit_holding_the_lock(&runtime, Duration::from_millis(200));
    let started = Instant::now();
    assert!(try_lock_processing(&runtime, 0).is_none());
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "patience 0 is a plain try_lock: the buffer is skipped at once"
    );
    edit.join().expect("edit");
}

#[test]
fn a_patient_worker_gets_the_lock_an_edit_releases_within_its_patience() {
    let runtime = runtime();
    let edit = edit_holding_the_lock(&runtime, Duration::from_millis(5));
    let patience = Duration::from_secs(5);
    assert!(
        try_lock_processing(&runtime, patience.as_nanos() as u64).is_some(),
        "the worker must wait out a short swap instead of dropping the buffer"
    );
    edit.join().expect("edit");
}

#[test]
fn a_patient_worker_gives_up_once_its_patience_runs_out() {
    let runtime = runtime();
    let edit = edit_holding_the_lock(&runtime, Duration::from_millis(500));
    let patience = Duration::from_millis(2);
    let started = Instant::now();
    assert!(try_lock_processing(&runtime, patience.as_nanos() as u64).is_none());
    let waited = started.elapsed();
    assert!(
        waited >= patience && waited < Duration::from_millis(250),
        "the worker waits its patience and no longer; waited {waited:?}"
    );
    edit.join().expect("edit");
}

#[test]
fn a_poisoned_lock_skips_the_buffer_instead_of_panicking_the_audio_thread() {
    let runtime = runtime();
    let poisoner = Arc::clone(&runtime);
    let _ = std::thread::spawn(move || {
        let _guard = poisoner.processing.lock().expect("lock");
        panic!("an edit panicking under the lock poisons it");
    })
    .join();
    assert!(runtime.processing.is_poisoned());
    assert!(try_lock_processing(&runtime, 0).is_none());
    assert!(try_lock_processing(&runtime, 1_000_000).is_none());
}
