//! THE RULE (issue #580 follow-up, broader scope)
//! ═══════════════════════════════════════════════
//!
//! Under sustained polling from non-audio threads (GUI timer, MCP poll,
//! observability tick), the audio thread's `process_input_f32` /
//! `process_output_f32` callbacks **must keep producing audio every
//! single callback**. Zero silenced buffers, zero gaps.
//!
//! Concretely: `process_input_f32` takes the `processing` Mutex via
//! `try_lock` (engine/runtime.rs around line 102). When `try_lock`
//! fails, the function returns early — no samples are pushed into the
//! per-route SPSC, the output stage's next `pop` returns silence, and
//! the user hears a click. The same failure mode covers any OTHER
//! lock / allocation / syscall the audio path may grow over time.
//!
//! This test exercises the failure path indirectly without instrumenting
//! production code: it drives N audio callbacks with a non-zero input
//! signal while a parallel "GUI thread" hammers every `ChainRuntimeState`
//! accessor we know the live GUI calls at sustained rate. After a brief
//! warm-up (so the elastic buffer + fade-in are at steady state) any
//! single missed `process_input_f32` will leave the SPSC route empty,
//! and the subsequent `process_output_f32` will emit a buffer of zeros.
//!
//! - If zero silenced buffers: the audio callback survives sustained
//!   GUI pressure for every accessor enumerated below. Any future click
//!   regression is then **not** in this class — look elsewhere
//!   (allocations on hot path, processor cost, OS scheduling).
//! - If silenced buffers > 0: at least one accessor in `gui_thread`
//!   below still contends with the audio thread. Identify it (binary
//!   search by commenting accessors out, or instrument production with
//!   a missed-callback counter) and fix at the source — never relax
//!   this assertion.
//!
//! `#[ignore]` by default for the same reason as the buffer = 32
//! deadline pair in `audio_deadline_tests.rs`: a tight loop with a
//! background thread is sensitive to the full test suite's parallel
//! scheduler load and would false-positive. Invoke serially:
//!
//! ```text
//! cargo test -p engine --release --lib audio_under_gui_pressure \
//!   -- --ignored --test-threads=1
//! ```

use crate::runtime::{
    build_chain_runtime_state, process_input_f32, process_output_f32, DEFAULT_ELASTIC_TARGET,
};
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{IoBinding, IoEndpoint};
use project::chain::Chain;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn chain() -> Chain {
    Chain {
        id: ChainId("issue580-stress".into()),
        description: Some("issue #580 broader pressure test".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

#[test]
#[ignore = "issue #580 follow-up: stress test sensitive to parallel \
 scheduler load — run with `cargo test -p engine --release --lib \
 audio_under_gui_pressure -- --ignored --test-threads=1`. Pins the \
 broader invariant that NO ChainRuntimeState accessor called from a \
 non-audio thread at sustained rate may silence an audio callback."]
fn audio_callback_not_silenced_under_sustained_gui_polling() {
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain(),
            48_000.0_f32,
            &[DEFAULT_ELASTIC_TARGET],
            &registry(),
        )
        .expect("runtime should build"),
    );

    // Spawn the "GUI thread" — hammer every ChainRuntimeState accessor
    // the live GUI calls at sustained rate. New accessor surfaces should
    // be added here as they ship. The yield_now keeps the loop tight
    // without monopolising one core.
    let stop = Arc::new(AtomicBool::new(false));
    let stop_gui = Arc::clone(&stop);
    let runtime_gui = Arc::clone(&runtime);
    let gui_thread = std::thread::spawn(move || {
        while !stop_gui.load(Ordering::Relaxed) {
            // Issue #580: this was the contended path pre-fix.
            let _ = runtime_gui.stream_count();
            // Cheap atomic reads — included so any future change that
            // accidentally swaps an atomic for a locked structure is
            // caught.
            let _ = runtime_gui.is_output_muted();
            let _ = runtime_gui.volume_pct();
            std::thread::yield_now();
        }
    });

    let buffer_frames = 32_usize;
    let input_total_channels = 2_usize;
    let output_total_channels = 2_usize;
    let input_buf = vec![0.5_f32; buffer_frames * input_total_channels];
    let mut output_buf = vec![0.0_f32; buffer_frames * output_total_channels];

    // Warm-up: drive enough callbacks for the fade-in ramp and elastic
    // pre-fill to settle. The route SPSC is then in steady state — any
    // subsequent silenced input callback shows up as zero output.
    for _ in 0..256 {
        process_input_f32(&runtime, 0, &input_buf, input_total_channels);
        process_output_f32(&runtime, 0, &mut output_buf, output_total_channels);
    }

    // Measure: count callbacks where the output is fully silent. Under
    // the test scenario (constant non-zero input, no fade-in remaining,
    // route SPSC primed) the only way a buffer comes out silent is if
    // `process_input_f32` returned early and the SPSC pop found nothing.
    let iterations = 10_000_usize;
    let mut silenced_buffers = 0_usize;
    let started = Instant::now();
    for _ in 0..iterations {
        process_input_f32(&runtime, 0, &input_buf, input_total_channels);
        process_output_f32(&runtime, 0, &mut output_buf, output_total_channels);
        if output_buf.iter().all(|&s| s.abs() < 1e-6) {
            silenced_buffers += 1;
        }
    }
    let elapsed = started.elapsed();

    stop.store(true, Ordering::Relaxed);
    gui_thread.join().expect("gui thread should join cleanly");

    eprintln!(
        "[audio_under_gui_pressure] {iterations} callbacks under sustained \
         GUI polling: {silenced_buffers} silenced buffers in {elapsed:?}"
    );

    assert_eq!(
        silenced_buffers, 0,
        "issue #580 broader rule: {silenced_buffers} of {iterations} audio \
         callbacks emitted a silent buffer while a parallel thread polled \
         ChainRuntimeState accessors at sustained rate. Some accessor in \
         the GUI thread above still contends with the audio path's \
         try_lock — find which one and mirror its value into an atomic / \
         ArcSwap updated at the rare write sites in runtime_graph.rs. \
         Never relax this assertion: every silenced buffer is an audible \
         click in production."
    );

    // Sanity bound on test duration — if the GUI thread starved the
    // audio thread for many seconds we want a louder signal than the
    // assert above. 10k callbacks at buffer=32 @ 48k = ~6.67 s of
    // audio; allow 10× slack for the real wall clock of a busy CI box.
    assert!(
        elapsed < Duration::from_secs(70),
        "test took {elapsed:?} — likely starvation/deadlock between the \
         audio loop and the GUI poller; investigate the lock graph"
    );
}
