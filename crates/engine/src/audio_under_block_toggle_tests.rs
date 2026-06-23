//! THE RULE (issue #580 follow-up)
//! ════════════════════════════════
//!
//! Toggling a block (enable / disable) must NEVER silence the audio
//! callback. The user-facing symptom of this rule being broken is a
//! click in the output the moment the block is toggled — exactly the
//! artefact reported as residual on top of the #580 atomic-mirror fix.
//!
//! Concretely: `set_block_enabled` is the fast-path API that flips a
//! `BlockRuntimeNode`'s `FadeState` in place (issue #522). If it
//! holds the `processing` Mutex blocking while doing so, the audio
//! thread's `try_lock` in `process_input_f32` (runtime.rs:102) fails
//! for any in-flight callback — that buffer comes out silent
//! (audible click at buffer = 32 @ 48 kHz). The fix, when this test
//! fires red, is to make the toggle path lock-free with respect to
//! the audio thread (atomic state transition + memory barrier; the
//! audio thread reads the state under its existing `try_lock` of the
//! per-input scratch and respects the transition naturally).
//!
//! How this test enforces the rule
//! ───────────────────────────────
//! - Build a runtime with a real DSP block (Core reverb / room) in
//!   the chain. The block is the one the toggle thread flips.
//! - Spawn a "toggle thread" that calls `set_block_enabled(..., true)`
//!   / `(..., false)` in a tight loop — far higher rate than the
//!   user can possibly click in the UI, to compress the contention
//!   window into a short test.
//! - On the test thread, drive 10k buffer=32 audio callbacks with a
//!   constant non-zero input. After a warm-up so the elastic SPSC is
//!   at steady state, any single silenced output buffer is conclusive
//!   evidence that an audio callback was skipped (the try_lock failed
//!   because the toggle was holding the lock at that instant).
//!
//! `#[ignore]` for the same reason as the other buffer=32 stress
//! tests — sensitive to parallel-test scheduler load. Run serially:
//!
//! ```text
//! cargo test -p engine --release --lib audio_under_block_toggle \
//!   -- --ignored --test-threads=1
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry,
    OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;

use super::{
    build_chain_runtime_state, process_input_f32, process_output_f32, set_block_enabled,
    DEFAULT_ELASTIC_TARGET,
};

const SR: f32 = 48_000.0;
const BLOCK_ID: &str = "issue580:toggle-target";

fn neutral_params(effect_type: &str, model: &str) -> ParameterSet {
    let schema =
        schema_for_block_model(effect_type, model).expect("schema must exist for test model");
    ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults must normalize")
}

fn toggle_chain() -> Chain {
    Chain {
        id: ChainId("issue580-toggle-stress".into()),
        description: Some("issue #580 block-toggle contention".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![
            AudioBlock {
                id: BlockId("test:in".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    io: String::new(),
                    endpoint: String::new(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId(BLOCK_ID.into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "reverb".into(),
                    model: "room".into(),
                    params: neutral_params("reverb", "room"),
                }),
            },
            AudioBlock {
                id: BlockId("test:out".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    io: String::new(),
                    endpoint: String::new(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    }
}

#[test]
#[ignore = "issue #580 follow-up: block-toggle contention. Sensitive to \
 parallel-test scheduler load. Run serially: `cargo test -p engine \
 --release --lib audio_under_block_toggle -- --ignored --test-threads=1`."]
fn audio_callback_not_silenced_during_repeated_block_toggle() {
    let runtime = Arc::new(
        build_chain_runtime_state(&toggle_chain(), SR, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime should build"),
    );
    let block_id = BlockId(BLOCK_ID.into());

    let stop = Arc::new(AtomicBool::new(false));
    let stop_toggle = Arc::clone(&stop);
    let runtime_toggle = Arc::clone(&runtime);
    let block_for_toggle = block_id.clone();

    let toggle_thread = std::thread::spawn(move || {
        let mut enabled = false;
        while !stop_toggle.load(Ordering::Relaxed) {
            // The user-visible "click on toggle" trace: every call to
            // `set_block_enabled` is a potential contention window with
            // the audio thread's `try_lock` on `processing`.
            let _ = set_block_enabled(&runtime_toggle, &block_for_toggle, enabled);
            enabled = !enabled;
            std::thread::yield_now();
        }
    });

    let buffer_frames = 32_usize;
    let input_total_channels = 1_usize;
    let output_total_channels = 2_usize;
    let input_buf = vec![0.5_f32; buffer_frames * input_total_channels];
    let mut output_buf = vec![0.0_f32; buffer_frames * output_total_channels];

    // Warm-up so the elastic buffer + fade-in are at steady state. The
    // measurement loop afterwards trusts that any silent output buffer
    // is the direct consequence of an audio-thread try_lock failure.
    for _ in 0..256 {
        process_input_f32(&runtime, 0, &input_buf, input_total_channels);
        process_output_f32(&runtime, 0, &mut output_buf, output_total_channels);
    }

    let iterations = 10_000_usize;
    let mut silenced = 0_usize;
    for _ in 0..iterations {
        process_input_f32(&runtime, 0, &input_buf, input_total_channels);
        process_output_f32(&runtime, 0, &mut output_buf, output_total_channels);
        if output_buf.iter().all(|&s| s.abs() < 1e-6) {
            silenced += 1;
        }
    }

    stop.store(true, Ordering::Relaxed);
    toggle_thread.join().expect("toggle thread joins cleanly");

    eprintln!(
        "[audio_under_block_toggle] {iterations} callbacks under repeated \
         set_block_enabled storm: {silenced} silenced buffers"
    );

    assert_eq!(
        silenced, 0,
        "issue #580 residual: {silenced} of {iterations} audio callbacks \
         emitted silence while `set_block_enabled` was being called \
         repeatedly from another thread. The block-toggle fast path is \
         still contending with the audio thread's processing.try_lock. \
         Make the toggle path lock-free w.r.t. the audio thread — the \
         FadeState transition can be an atomic store, the audio thread \
         reads it under its existing per-callback try_lock and respects \
         the transition naturally. Every silenced callback in this test \
         is a click the user hears when toggling a block in the UI."
    );
}
