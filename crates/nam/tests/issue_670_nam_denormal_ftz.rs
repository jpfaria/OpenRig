//! Issue #670 — RED-first repro: NAM inference must not denormal-stall when
//! the thread's flush-to-zero (FZ) bit is off.
//!
//! The user hears intermittent crackle at buffer 64 from a SINGLE light
//! chain. Instrumentation (the #670 probe) pinned it to the NAM amp block
//! spiking ~7-30x in compute, inside the C++ inference, on aarch64. The
//! suspected mechanism: when a preceding block leaves the FPU FZ bit
//! cleared, the neural net's tiny activations on a quiet/decaying signal go
//! subnormal and the FPU enters the gradual-underflow slow path (50-100x
//! per op), blowing the 64-frame deadline.
//!
//! `engine::runtime_dsp::ensure_flush_to_zero` sets FZ once per callback at
//! the TOP of `process_input_f32` — but anything between there and the NAM
//! call (e.g. an LV2 plugin's process) can clear it, and then the NAM runs
//! unprotected. The fix is for the NAM processor to assert FZ itself, right
//! before inference, so it can never run denormal-stalled.
//!
//! This test reproduces the stall deterministically: it feeds the real
//! (fixture) NAM a quiet signal that drives internal activations subnormal,
//! and times the inference with FZ ON vs FZ OFF. With FZ off the inference
//! must NOT be dramatically slower — i.e. the processor must protect itself.
//! RED before the fix (FZ off → slow), GREEN after.
//!
//! aarch64 + release only: FZ is an aarch64 FPCR bit, and the denormal
//! slowdown is only measurable in optimized code. Other arches/debug skip.

#![cfg(all(target_arch = "aarch64", not(debug_assertions)))]

use block_core::MonoProcessor;
use nam::processor::{NamProcessor, DEFAULT_PLUGIN_PARAMS};
use std::time::Instant;

fn fixture_model() -> String {
    format!(
        "{}/../engine/tests/fixtures/plugins/nam/marshall_plexi/captures/angus_nano.nam",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// Set or clear the FPCR flush-to-zero (FZ) bit on this thread.
unsafe fn set_flush_to_zero(on: bool) {
    let mut fpcr: u64;
    core::arch::asm!("mrs {}, fpcr", out(reg) fpcr);
    if on {
        fpcr |= 1 << 24;
    } else {
        fpcr &= !(1u64 << 24);
    }
    core::arch::asm!("msr fpcr, {}", in(reg) fpcr);
}

/// Median nanoseconds to process one 64-frame buffer of a denormal-prone
/// signal, with the FZ bit forced to `fz` for every call.
fn median_block_ns(proc: &mut NamProcessor, fz: bool) -> u128 {
    const FRAMES: usize = 64;
    const ITERS: usize = 400;
    // Feed TRUE subnormals (< f32 subnormal threshold 1.18e-38): every FP op
    // on these enters the gradual-underflow slow path unless FZ flushes them.
    // A noise gate is disabled in the params below so they reach the net.
    let subnormal = f32::from_bits(0x0000_0002); // ~2.8e-45, deeply subnormal
    let quiet: Vec<f32> = (0..FRAMES)
        .map(|i| if i % 2 == 0 { subnormal } else { -subnormal })
        .collect();

    // Warm up (fill the model's receptive field with the quiet signal).
    for _ in 0..64 {
        let mut buf = quiet.clone();
        unsafe { set_flush_to_zero(fz) };
        proc.process_block(&mut buf);
    }

    let mut samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let mut buf = quiet.clone();
        unsafe { set_flush_to_zero(fz) };
        let t0 = Instant::now();
        proc.process_block(&mut buf);
        samples.push(t0.elapsed().as_nanos());
    }
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[test]
fn nam_inference_does_not_denormal_stall_when_flush_to_zero_is_off() {
    let model = fixture_model();
    let mut params = DEFAULT_PLUGIN_PARAMS;
    params.noise_gate_enabled = false; // let the quiet signal reach the net
    params.eq_enabled = false;
    let mut proc =
        NamProcessor::new(&model, None, params, 48_000.0).expect("fixture NAM model must load");

    // Baseline: inference with flush-to-zero ON (the protected path).
    let with_fz = median_block_ns(&mut proc, true);
    // The bug: inference with flush-to-zero OFF. If the net denormal-stalls,
    // this is many times slower than the protected baseline.
    let without_fz = median_block_ns(&mut proc, false);

    eprintln!(
        "[#670] NAM inference per 64-frame buffer: FZ_on={}us  FZ_off={}us  ratio={:.1}x",
        with_fz / 1_000,
        without_fz / 1_000,
        without_fz as f64 / with_fz.max(1) as f64,
    );

    assert!(
        without_fz < with_fz * 3,
        "BUG #670: NAM inference is {:.1}x slower with flush-to-zero OFF \
         ({}us vs {}us per 64-frame buffer) — the neural net denormal-stalls \
         on a quiet signal when a preceding block left FZ cleared. The NAM \
         processor must assert flush-to-zero itself before inference so it is \
         never deadline-blown by an upstream block. (FZ_off should be ~= \
         FZ_on once the processor protects itself.)",
        without_fz as f64 / with_fz.max(1) as f64,
        without_fz / 1_000,
        with_fz / 1_000,
    );
}
