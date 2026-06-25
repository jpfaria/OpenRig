//! Issue #545 — red-first guard for "toggling a chain off keeps the
//! streams alive underneath". The user disables a chain from the
//! Chains screen; the audio they hear silences, but the chain's tap
//! and meter keep moving and CPU does not drop. The controller pauses
//! the chain by calling `runtime.set_draining()`, and the existing
//! pause-chain controller tests confirm the flag flips on the
//! `Arc<ChainRuntimeState>`. They do not verify the contract that
//! `controller_pause_chain_tests.rs` actually claims: "the audio
//! callbacks short-circuit on `is_draining()` to emit silence with
//! zero processor work".
//!
//! This test pins exactly that contract at the engine level. It builds
//! a passthrough runtime (input → output, no DSP blocks), drives a
//! sine through `process_input_f32` / `process_output_f32` long
//! enough to clear the warmup fade, captures the output peak as the
//! baseline (must be audible), calls `set_draining()`, drives the
//! same sine for the same window, and asserts the output is silent.
//!
//! RED today means the user's symptom: `set_draining()` flips the
//! flag but `process_output_f32` still copies processed audio out, so
//! the tap and meter keep observing signal and CPU stays at the
//! running-chain baseline.

use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

const SR: f32 = 48_000.0;
const FRAMES: usize = 64;
const ELASTIC: usize = 1024;
const WARMUP_CALLBACKS: usize = 80;
const MEASURE_CALLBACKS: usize = 120;

fn registry() -> Vec<domain::io_binding::IoBinding> {
    vec![domain::io_binding::IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![domain::io_binding::IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Mono,
            channels: vec![0],
        }],
    }]
}

fn passthrough_chain() -> Chain {
    let _ = ParameterSet::default; // silence unused-import if ParameterSet not needed
    let _ = Project {
        // ensures `project::project::Project` import is exercised even though we don't construct it here
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    Chain {
        id: ChainId("chain:545:passthrough".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
    }
}

fn drive_capture_peak(
    runtime: &Arc<engine::runtime::ChainRuntimeState>,
    callbacks: usize,
    measure_after: usize,
    phase_offset: usize,
) -> f32 {
    let mut in_buf = vec![0.0_f32; FRAMES];
    let mut out_buf = vec![0.0_f32; FRAMES];
    let mut peak = 0.0_f32;
    for cb in 0..callbacks {
        for i in 0..FRAMES {
            let n = (phase_offset + cb * FRAMES + i) as f32;
            in_buf[i] = 0.5 * (2.0 * std::f32::consts::PI * 220.0 * n / SR).sin();
        }
        engine::runtime::process_input_f32(runtime, 0, &in_buf, 1);
        engine::runtime::process_output_f32(runtime, 0, &mut out_buf, 1);
        if cb >= measure_after {
            for &s in &out_buf {
                peak = peak.max(s.abs());
            }
        }
    }
    peak
}

/// Drain on a chain with one input must silence the output.
#[test]
fn issue_545_set_draining_makes_process_output_emit_silence() {
    let chain = passthrough_chain();
    let runtime = Arc::new(
        engine::runtime::build_chain_runtime_state(&chain, SR, &[ELASTIC], &registry())
            .expect("passthrough runtime must build"),
    );

    let peak_running = drive_capture_peak(&runtime, MEASURE_CALLBACKS, WARMUP_CALLBACKS, 0);
    assert!(
        peak_running > 0.05,
        "test setup wrong: passthrough chain produced peak {peak_running:.4} \
         while running (expected > 0.05 from the sine input)"
    );

    runtime.set_draining();
    assert!(runtime.is_draining(), "set_draining must flip the flag");

    let phase_offset = MEASURE_CALLBACKS * FRAMES;
    let peak_drained =
        drive_capture_peak(&runtime, MEASURE_CALLBACKS, WARMUP_CALLBACKS, phase_offset);

    assert!(
        peak_drained < 1.0e-4,
        "REGRESSION: paused chain (set_draining() == true) emitted peak \
         {peak_drained:.6} — must be ≤ 1e-4 (effectively silent). \
         Baseline running peak was {peak_running:.4}."
    );
}

fn drain_ring(ring: &engine::spsc::SpscRing<f32>) -> usize {
    let mut n = 0;
    while ring.pop().is_some() {
        n += 1;
    }
    n
}

/// Per the user's symptom on issue #545, the bug shows up on the
/// stream taps (the source the GUI meter reads from) even when the
/// output is silent. After `set_draining()`, the stream tap rings
/// must stop accumulating frames; if they keep receiving processed
/// audio the meter keeps moving and the chain looks "alive" from the
/// outside.
#[test]
fn issue_545_set_draining_stops_stream_tap_pushes() {
    let chain = passthrough_chain();
    let runtime = Arc::new(
        engine::runtime::build_chain_runtime_state(&chain, SR, &[ELASTIC], &registry())
            .expect("runtime must build"),
    );

    // Subscribe a stream tap on input index 0 — same path the GUI
    // meter takes.
    let [l_ring, r_ring] = runtime.subscribe_stream_tap(0, 16_384);

    // Drive while running so the tap fills with real audio first.
    let _ = drive_capture_peak(&runtime, MEASURE_CALLBACKS, WARMUP_CALLBACKS, 0);

    // Drain the rings so we are measuring only frames pushed AFTER
    // set_draining().
    let baseline_l = drain_ring(&l_ring);
    let baseline_r = drain_ring(&r_ring);
    assert!(
        baseline_l > 0,
        "test setup wrong: stream tap received no frames while chain was \
         running (got L={baseline_l}, R={baseline_r})"
    );

    runtime.set_draining();
    assert!(runtime.is_draining());

    let phase_offset = MEASURE_CALLBACKS * FRAMES;
    let _ = drive_capture_peak(&runtime, MEASURE_CALLBACKS, WARMUP_CALLBACKS, phase_offset);

    let pushed_l = drain_ring(&l_ring);
    let pushed_r = drain_ring(&r_ring);

    assert!(
        pushed_l == 0 && pushed_r == 0,
        "REGRESSION: stream tap kept receiving frames after `set_draining()` — \
         pushed L={pushed_l} R={pushed_r} (baseline running pushed L={baseline_l} \
         R={baseline_r}). The GUI meter polls these rings, which is why the \
         user sees the meter keep moving after toggling the chain off."
    );
}
