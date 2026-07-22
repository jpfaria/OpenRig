//! THE RULE (CLAUDE.md invariant #8, pinned)
//! ═══════════════════════════════════════════
//!
//! The audio callback path must perform **zero heap allocations**.
//! Every `Vec` it touches has been pre-grown to its maximum required
//! capacity at runtime build / rebuild time; every scratch buffer is
//! preallocated; every `Arc::clone` is reference-count-only.
//!
//! Why: an allocator call on the audio thread is a syscall path that
//! can grab a process-wide allocator lock, page in fresh memory from
//! the kernel, or stall the thread arbitrarily. At buffer = 32 frames
//! @ 48 kHz (callback period ≈ 666 µs) a single page fault wipes the
//! window. The symptom is an audible click that has no pattern from
//! the user's perspective — it depends on the allocator's internal
//! state, which depends on the rest of the process's heap traffic.
//!
//! How this test catches violations
//! ────────────────────────────────
//! - A custom `CountingAllocator` is installed as `#[global_allocator]`
//!   for the test binary (gated under `#[cfg(test)]` — production
//!   binaries keep the default `System` allocator). The wrapper
//!   increments a global atomic counter on every `alloc` / `realloc`
//!   call **but only when a thread-local guard is set**. Without the
//!   guard active, the wrapper is a transparent pass-through; tests
//!   that legitimately allocate during setup do not interfere.
//! - The test warms up the runtime, drains any first-callback
//!   allocations (fade-in scratch growth, cold caches), then sets the
//!   guard for a measurement window of N callbacks and asserts the
//!   counter delta is zero.
//!
//! If this test starts firing red, the most recent commit added code
//! that allocates on the per-callback path. Common culprits:
//!   - `Vec::push` past capacity (forgot to `Vec::with_capacity` at
//!     build time, or `Vec::clear` cleared but the next push exceeds
//!     the prior steady-state length).
//!   - `format!` / `to_string` / `String::from` (typically slipped
//!     into a log statement on the hot path).
//!   - `Box::new` / `Arc::new` (a fresh `Arc::new` allocates; cloning
//!     an existing `Arc` does not).
//!   - `clone` on a heap-backed type (`Vec`, `HashMap`, `String`).
//!   - A new processor crate that allocates inside `process` instead
//!     of inside `build`.
//!
//! Fix at the source. Never silence this assertion.
//!
//! `#[ignore]` by default — the allocator wrapper changes the process
//! allocator for the full test binary and the measurement is sensitive
//! to parallel-thread allocator pressure. Run serially:
//!
//! ```text
//! cargo test -p engine --release --lib audio_alloc_invariant \
//!   -- --ignored --test-threads=1
//! ```

pub(super) use crate::runtime::{
    build_chain_runtime_state, process_input_f32, process_output_f32, DEFAULT_ELASTIC_TARGET,
};
pub(super) use domain::ids::{BlockId, ChainId, DeviceId};
pub(super) use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
pub(super) use project::block::{AudioBlock, AudioBlockKind};
pub(super) use project::chain::Chain;
pub(super) use std::alloc::{GlobalAlloc, Layout, System};
pub(super) use std::cell::Cell;
pub(super) use std::sync::atomic::{AtomicUsize, Ordering};
pub(super) use std::sync::Arc;

// ── Allocator instrumentation ────────────────────────────────────────
//
// The wrapper counts `alloc` and `realloc` calls but only when the
// thread-local guard is set. `dealloc` is not counted: drops on the
// hot path that release pre-allocated buffers happen by design (e.g.
// when a frame Vec wraps around capacity); the invariant we care
// about is "no NEW allocation".

thread_local! {
    static ALLOC_GUARD: Cell<bool> = const { Cell::new(false) };
}

static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ALLOC_GUARD.with(|g| g.get()) {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ALLOC_GUARD.with(|g| g.get()) {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        System.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ALLOC_GUARD.with(|g| g.get()) {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

pub(super) fn measure_allocs<F: FnOnce()>(f: F) -> usize {
    ALLOC_COUNT.store(0, Ordering::Relaxed);
    ALLOC_GUARD.with(|g| g.set(true));
    f();
    ALLOC_GUARD.with(|g| g.set(false));
    ALLOC_COUNT.load(Ordering::Relaxed)
}

// ── Fixture ──────────────────────────────────────────────────────────

// Model A (#716): I/O lives in the binding registry, not in-chain blocks.

/// Registry: stereo input + stereo output on one device.
pub(super) fn registry_stereo() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

/// Registry: mono input + stereo output on one device.
pub(super) fn registry_mono_in_stereo_out() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

pub(super) fn chain() -> Chain {
    Chain {
        id: ChainId("issue580-alloc".into()),
        description: Some("issue #580 audio-thread alloc invariant".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
    }
}

#[test]
#[ignore = "CLAUDE.md invariant #8: audio callback must not allocate. \
 Allocator wrapper changes the process allocator and is sensitive to \
 parallel-test pressure — run serially: `cargo test -p engine \
 --release --lib audio_alloc_invariant -- --ignored --test-threads=1`."]
pub(super) fn audio_callback_does_not_allocate_at_buffer_32() {
    let runtime = Arc::new(
        build_chain_runtime_state(
            &chain(),
            48_000.0_f32,
            &[DEFAULT_ELASTIC_TARGET],
            &registry_stereo(),
        )
        .expect("runtime should build"),
    );

    let buffer_frames = 32_usize;
    let input_total_channels = 2_usize;
    let output_total_channels = 2_usize;
    let input_buf = vec![0.5_f32; buffer_frames * input_total_channels];
    let mut output_buf = vec![0.0_f32; buffer_frames * output_total_channels];

    // Warm-up OUTSIDE the measurement guard. The first callbacks grow
    // any frame buffers that were sized for `num_frames > capacity` and
    // settle the fade-in ramp. Anything that needs to grow / cache /
    // resolve handles must finish here — if a steady-state callback
    // still allocates, that is the violation we are pinning.
    for _ in 0..256 {
        process_input_f32(&runtime, 0, &input_buf, input_total_channels);
        process_output_f32(&runtime, 0, &mut output_buf, output_total_channels);
    }

    // Measurement window: 1000 callbacks of steady-state processing.
    let allocs = measure_allocs(|| {
        for _ in 0..1_000 {
            process_input_f32(&runtime, 0, &input_buf, input_total_channels);
            process_output_f32(&runtime, 0, &mut output_buf, output_total_channels);
        }
    });

    eprintln!("[audio_alloc_invariant] 1000 callbacks at buffer=32 @ 48k: {allocs} allocations");

    assert_eq!(
        allocs, 0,
        "CLAUDE.md invariant #8 broken: {allocs} heap allocations in \
         1000 steady-state audio callbacks. Every alloc on the audio \
         thread is a syscall-path / allocator-lock risk and is the \
         most plausible source of buffer=32 clicks once GUI-thread \
         contention (issue #580) is fixed. Read the module docstring \
         for the common culprits and fix at the source — never relax \
         this assertion."
    );
}

