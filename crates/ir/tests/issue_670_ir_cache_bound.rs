//! Issue #670: the partitioned IR convolution is CACHE-BOUND. Its
//! multiply-accumulate walks the frequency-delay-line with a MODULAR
//! (non-sequential) index, so its ~130 KB working set must stay hot in cache
//! to be fast. Isolated it is ~4 us — but the LIVE probe measured ~270 us PER
//! BUFFER (67x). The mechanism: the OS context-switches the audio thread to
//! the UI thread (Slint render + spectrum FFT) and back; the audio thread
//! resumes with a COLD cache, and the convolver's cache-hostile access then
//! reloads its whole working set from memory every buffer.
//!
//! This reproduces it OFFLINE with no live app: time process_block with a
//! HOT cache vs with the cache EVICTED on the same thread right before each
//! call (the post-context-switch state). RED while the FDL access is
//! cache-hostile; GREEN once it is walked sequentially (prefetch-friendly).
#![cfg(not(debug_assertions))]

use block_core::MonoProcessor;
use ir::MonoIrProcessor;
use std::time::Instant;

fn cabinet_ir(len: usize) -> Vec<f32> {
    (0..len)
        .map(|n| {
            let t = n as f32 / 48_000.0;
            (-t * 25.0).exp() * (2.0 * std::f32::consts::PI * 1500.0 * t).sin()
        })
        .collect()
}

/// Median process_block time over many buffers. If `pollute` is non-empty,
/// sweep it on THIS thread before each call to evict the convolver's working
/// set — the cold-cache state the audio thread is left in after the OS runs
/// the UI thread on its core and switches back.
fn median_ns(proc: &mut MonoIrProcessor, pollute: &mut [u8]) -> u128 {
    let mut s = Vec::with_capacity(1200);
    for k in 0..1200 {
        let mut buf: Vec<f32> = (0..64)
            .map(|i| {
                0.3 * (2.0 * std::f32::consts::PI * 220.0 * (k * 64 + i) as f32 / 48_000.0).sin()
            })
            .collect();
        if !pollute.is_empty() {
            let mut x = 1u8;
            let mut i = 0;
            while i < pollute.len() {
                pollute[i] = pollute[i].wrapping_add(x);
                x = x.wrapping_add(13);
                i += 64;
            }
            std::hint::black_box(&pollute);
        }
        let t0 = Instant::now();
        proc.process_block(&mut buf);
        s.push(t0.elapsed().as_nanos());
    }
    s.sort_unstable();
    // Low percentile, not the median: it reflects the cache behaviour with the
    // least OS/parallel-test contention noise (the median is inflated when
    // other test binaries run concurrently).
    s[s.len() / 20]
}

#[test]
fn ir_convolution_is_not_cache_bound() {
    let mut proc = MonoIrProcessor::new(cabinet_ir(8192)).unwrap();
    for _ in 0..128 {
        let mut b = vec![0.1f32; 64];
        proc.process_block(&mut b);
    }

    let hot = median_ns(&mut proc, &mut []);
    // 64 MB >> any per-core cache: sweeping it evicts the convolver's state.
    let mut big = vec![0u8; 64 * 1024 * 1024];
    let cold = median_ns(&mut proc, &mut big);

    eprintln!(
        "[#670 IR] hot-cache median={hot}ns  cold-cache median={cold}ns  ratio={:.1}x",
        cold as f64 / hot.max(1) as f64
    );
    assert!(
        cold < hot * 2,
        "BUG #670: the IR convolution is {:.1}x slower with a cold cache \
         (hot={hot}ns vs cold={cold}ns) — its modular frequency-delay-line \
         access reloads its whole ~130 KB working set from memory after the OS \
         context-switches the audio thread to the UI. That is the live ~270us \
         spike. Walk the FDL SEQUENTIALLY so a cold-cache reload is a cheap \
         linear prefetch, not a miss storm.",
        cold as f64 / hot.max(1) as f64
    );
}
