//! THE RULE (issue #580, pinned forever)
//! ═══════════════════════════════════════
//!
//! Any method on `ChainRuntimeState` that is callable from a non-audio
//! thread at sustained rate (GUI timer, MCP poll, observability tick)
//! **MUST NOT acquire `processing` (or any other Mutex shared with the
//! audio thread)**, not even with `try_lock`. Use a lock-free atomic
//! mirror updated at the rare write sites in `runtime_graph.rs`
//! (`build_chain_runtime_state` + the rebuild step that swaps
//! `input_states`) instead.
//!
//! If a new accessor needs a value that lives behind the Mutex, mirror
//! it into an atomic (or `ArcSwap`) at the write sites — do not push
//! the cost onto the read path. CLAUDE.md invariant #8 ("zero lock on
//! the audio thread") covers this in spirit: a Mutex the audio thread
//! `try_lock`s is still a synchronisation point, and *every* contention
//! window the read side opens is a callback the audio thread skips.
//!
//! Why this is load-bearing
//! ────────────────────────
//! The audio thread enters `process_input_f32` and calls
//! `runtime.processing.try_lock()` (engine/runtime.rs around line 102).
//! When the lock is held by another thread, the call returns Err and the
//! callback **emits no samples for that buffer**. The latency-probe
//! comment in `process_input_f32` documents that as tolerable "during a
//! config rebuild in flight" — i.e. a write event that fires once per
//! preset switch / block add-remove / rig-nav. Any *read* accessor that
//! takes the same lock at high rate breaks the rare-event assumption,
//! producing audible clicks at small buffer sizes.
//!
//! The symptom is buffer-size dependent: at buffer = 32 frames @ 48 kHz
//! (callback period ≈ 666 µs), even a single OS preemption that holds
//! the GUI thread mid-`lock()` blows the window — the missed input
//! callback is felt at output because the elastic SPSC ring between the
//! two stages only buffers `buffer_size` samples of upstream headroom.
//! At buffer = 256 (≈ 5.3 ms) the window is 8× larger AND the ring
//! holds 8× more headroom, so misses are absorbed silently. Offline
//! single-threaded deadline tests **cannot reproduce this** — they need
//! the contention between threads that only this kind of test
//! simulates.
//!
//! Case study — the regression this test pins
//! ──────────────────────────────────────────
//! Pre-fix, `ChainRuntimeState::stream_count()` was implemented as
//! `processing.lock().map(|p| p.input_states.len())` (runtime_state.rs).
//! The meter polling timer
//! `adapter-gui/src/meter_wiring.rs::start_meter_polling` (wired from
//! `desktop_app.rs:353`) calls `controller.stream_count(&chain.id)` at
//! 30 Hz from app startup, **per chain, regardless of any visualisation
//! window being open**. Two chains = 60 blocking lock acquisitions per
//! second on the GUI thread, sustained for the life of the session.
//! Users hit dropouts at buffer = 32 that disappeared at buffer = 256
//! — exact 8× ratio matching the window/headroom analysis above.
//!
//! Fix: `stream_count` reads an `AtomicUsize` mirror updated by
//! `build_chain_runtime_state` and the rebuild path in
//! `runtime_graph.rs` (both rare). The accessor is lock-free.
//!
//! How this test enforces the rule
//! ───────────────────────────────
//! The test thread holds `runtime.processing` for the duration of the
//! check (standing in for the audio thread mid-callback). A second
//! thread calls `stream_count()` and tries to deliver its result back
//! through a channel; the test thread expects the result inside a tight
//! timeout. If `stream_count()` ever tries to acquire the same Mutex
//! (whether `.lock()` blocking OR `.try_lock()` followed by a fallback
//! that depends on the lock being free), the channel never receives and
//! the timeout fires with the error message below — pointing the next
//! contributor directly at the rule.
//!
//! This test fires red on the regression pattern, regardless of which
//! specific accessor introduced it. If a future change adds another
//! GUI-callable read on `ChainRuntimeState` that takes `processing`,
//! adapt this test to cover the new accessor too (one-test-per-pattern
//! is fine; do not delete the existing one to "save lines").

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
            io: String::new(),
            endpoint: String::new(),
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
            io: String::new(),
            endpoint: String::new(),
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
        "THE RULE (issue #580): a non-audio-thread accessor on \
         ChainRuntimeState — `stream_count` here, but the rule applies \
         to every GUI / MCP / observability poll — must NOT acquire the \
         `processing` Mutex, not even with try_lock. The audio thread \
         takes that Mutex via try_lock in process_input_f32 and skips \
         the entire callback (emitting silence) whenever the try_lock \
         fails. At 30 Hz from the meter polling timer the GUI thread \
         opens enough contention windows that buffer=32 @48k cannot \
         stay glitch-free, while buffer=256 hides it via elastic-buffer \
         headroom. Fix: mirror the needed value into an AtomicUsize / \
         ArcSwap updated at the rare write sites in runtime_graph.rs \
         (build + rebuild). See the module docstring for the full \
         pattern."
    );

    let count = received.unwrap();
    assert_eq!(
        count, 1,
        "single-stream stereo chain should report exactly 1 stream"
    );
}
