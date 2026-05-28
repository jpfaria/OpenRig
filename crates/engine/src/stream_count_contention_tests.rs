//! Issue #580: `ChainRuntimeState::stream_count` must not block on the
//! `processing` Mutex.
//!
//! Why this is load-bearing
//! ────────────────────────
//! The audio thread enters `process_input_f32` and calls
//! `runtime.processing.try_lock()` (engine/runtime.rs around line 102).
//! If the lock is held, the callback returns early — **no samples are
//! pushed to any output route for that buffer**. The Latency-probe
//! comment in `process_input_f32` already documents this as a tolerable
//! event "during a config rebuild in flight" — i.e. very rare.
//!
//! `ChainRuntimeState::stream_count` taking a *blocking* `.lock()` on the
//! same Mutex breaks that "rare event" assumption. The meter polling
//! timer (`adapter-gui/src/meter_wiring.rs::start_meter_polling`, wired
//! from `desktop_app.rs:353`) calls it **once per chain at 30 Hz** —
//! steady state, for the life of the session, regardless of whether any
//! visualisation window is open.
//!
//! Whenever the GUI thread is holding `processing` to read its length,
//! the audio thread's `try_lock` fails and the callback emits silence.
//! At buffer = 32 frames @ 48 kHz (callback period ≈ 666 µs), any
//! OS-level preemption holding the GUI mid-lock blows the window. At
//! buffer = 256 (≈ 5.3 ms), the window is 8× larger and the elastic
//! output buffer absorbs the dropouts — which is exactly the regression
//! reported in #580 ("had to raise the buffer from 32 to 256 to get
//! clean audio after the per-stream meters landed").
//!
//! `input_states.len()` only changes on runtime build / rebuild
//! (`runtime_graph::build_chain_runtime_state` and the rebuild path in
//! the same file). Both are infrequent. So `stream_count` does not need
//! the lock at all — it can read an `AtomicUsize` mirror updated at
//! those two sites.
//!
//! This test holds the `processing` Mutex on the test thread (simulating
//! the audio thread mid-callback) and asserts that another thread can
//! call `stream_count()` without blocking. The current implementation
//! fails this — the call blocks forever and the channel timeout fires.
//! After the fix lands, it passes immediately.

use crate::runtime::{build_chain_runtime_state, DEFAULT_ELASTIC_TARGET};
use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

fn input_stereo(channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("input:0".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainInputMode::Stereo,
                channels,
            }],
        }),
    }
}

fn output_stereo(channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("output:0".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainOutputMode::Stereo,
                channels,
            }],
        }),
    }
}

fn chain() -> Chain {
    Chain {
        id: ChainId("issue580-contention".into()),
        description: Some("issue #580 stream_count contention test".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![input_stereo(vec![0, 1]), output_stereo(vec![0, 1])],
    }
}

#[test]
fn stream_count_does_not_block_on_processing_lock() {
    let runtime = Arc::new(
        build_chain_runtime_state(&chain(), 48_000.0_f32, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime should build"),
    );

    // Simulate the audio thread mid-callback: hold `processing`.
    let _guard = runtime
        .processing
        .lock()
        .expect("processing lock should be held cleanly for the test");

    // Now call `stream_count()` from another thread. If it takes the
    // same Mutex (current behaviour, pre-fix), it blocks forever and
    // the channel `recv_timeout` returns `Err`. After the fix lands —
    // reading an `AtomicUsize` mirror updated by build / rebuild — it
    // returns immediately.
    let runtime_clone = Arc::clone(&runtime);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let count = runtime_clone.stream_count();
        let _ = tx.send(count);
    });

    let received = rx.recv_timeout(Duration::from_millis(200));
    assert!(
        received.is_ok(),
        "issue #580: stream_count() blocked on the processing Mutex. \
         The meter polling timer calls it at 30 Hz on the GUI thread; \
         while it holds the lock the audio thread's process_input_f32 \
         try_lock fails and the callback emits silence. stream_count() \
         must read a lock-free atomic mirror so it never contends with \
         the audio callback."
    );

    let count = received.unwrap();
    assert_eq!(
        count, 1,
        "single-stream stereo chain should report exactly 1 stream"
    );
}
