//! Issue #670 — RED-first: the IR/CAB convolution must not denormal-stall
//! when the thread's flush-to-zero (FZ) bit is off.
//!
//! The user confirmed disabling the IR/CAB stops the crackle, yet the IR's
//! per-buffer cost is tiny on a steady tone. The reconciling mechanism:
//! while playing, the convolution's frequency-delay-line decays through the
//! SUBNORMAL float range on every note's tail. f32 ops on subnormals take the
//! FPU gradual-underflow slow path (tens of times slower) UNLESS flush-to-zero
//! is armed. `engine` arms FZ once per callback, but the C++ NAM inference
//! that runs just before the IR can leave FZ cleared — and then the IR's pure
//! Rust FFT convolution runs unprotected and blows the 64-frame deadline.
//! A steady test tone never reaches the subnormal regime, which is why the
//! offline cost looked fine.
//!
//! This feeds the convolver TRUE subnormals and times a buffer with FZ ON vs
//! OFF. With FZ off it must NOT be dramatically slower — i.e. the IR processor
//! must assert FZ itself. RED before the fix, GREEN after.
//!
//! aarch64 + release only (FZ is an aarch64 FPCR bit; the slowdown only shows
//! in optimized code).

#![cfg(all(target_arch = "aarch64", not(debug_assertions)))]

use block_core::MonoProcessor;
use ir::MonoIrProcessor;
use std::time::Instant;

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

/// A realistic cabinet-length IR (near the 8192-sample cap → ~128 partitions).
fn cabinet_ir(len: usize) -> Vec<f32> {
    (0..len)
        .map(|n| {
            let t = n as f32 / 48_000.0;
            (-t * 25.0).exp() * (2.0 * std::f32::consts::PI * 1500.0 * t).sin()
        })
        .collect()
}

fn median_block_ns(proc: &mut MonoIrProcessor, fz: bool) -> u128 {
    const FRAMES: usize = 64;
    const ITERS: usize = 600;
    let subnormal = f32::from_bits(0x0000_0002); // ~2.8e-45, deeply subnormal
    let quiet: Vec<f32> = (0..FRAMES)
        .map(|i| if i % 2 == 0 { subnormal } else { -subnormal })
        .collect();

    for _ in 0..128 {
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
fn ir_convolution_does_not_denormal_stall_when_flush_to_zero_is_off() {
    let mut proc = MonoIrProcessor::new(cabinet_ir(8192));

    let with_fz = median_block_ns(&mut proc, true);
    let without_fz = median_block_ns(&mut proc, false);

    eprintln!(
        "[#670] IR convolution per 64-frame buffer: FZ_on={}us  FZ_off={}us  ratio={:.1}x",
        with_fz / 1_000,
        without_fz / 1_000,
        without_fz as f64 / with_fz.max(1) as f64,
    );

    assert!(
        without_fz < with_fz * 3,
        "BUG #670: IR convolution is {:.1}x slower with flush-to-zero OFF \
         ({}us vs {}us per 64-frame buffer) — the FFT convolution \
         denormal-stalls on a decaying (subnormal) tail when an upstream \
         block (the C++ NAM) left FZ cleared. The IR processor must assert \
         flush-to-zero itself before processing so it is never deadline-blown.",
        without_fz as f64 / with_fz.max(1) as f64,
        without_fz / 1_000,
        with_fz / 1_000,
    );
}
