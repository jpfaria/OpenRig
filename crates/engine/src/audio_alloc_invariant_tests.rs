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

use crate::runtime::{
    build_chain_runtime_state, process_input_f32, process_output_f32, DEFAULT_ELASTIC_TARGET,
};
use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// ── Allocator instrumentation ────────────────────────────────────────
//
// The wrapper counts `alloc` and `realloc` calls but only when the
// thread-local guard is set. `dealloc` is not counted: drops on the
// hot path that release pre-allocated buffers happen by design (e.g.
// when a frame Vec wraps around capacity); the invariant we care
// about is "no NEW allocation".

thread_local! {
    static ALLOC_GUARD: Cell<bool> = Cell::new(false);
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

fn measure_allocs<F: FnOnce()>(f: F) -> usize {
    ALLOC_COUNT.store(0, Ordering::Relaxed);
    ALLOC_GUARD.with(|g| g.set(true));
    f();
    ALLOC_GUARD.with(|g| g.set(false));
    ALLOC_COUNT.load(Ordering::Relaxed)
}

// ── Fixture ──────────────────────────────────────────────────────────

fn chain() -> Chain {
    Chain {
        id: ChainId("issue580-alloc".into()),
        description: Some("issue #580 audio-thread alloc invariant".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
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
#[ignore = "CLAUDE.md invariant #8: audio callback must not allocate. \
 Allocator wrapper changes the process allocator and is sensitive to \
 parallel-test pressure — run serially: `cargo test -p engine \
 --release --lib audio_alloc_invariant -- --ignored --test-threads=1`."]
fn audio_callback_does_not_allocate_at_buffer_32() {
    let runtime = Arc::new(
        build_chain_runtime_state(&chain(), 48_000.0_f32, &[DEFAULT_ELASTIC_TARGET])
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

// ── Issue #670: the pipe chain above never runs a DSP block. The user's
// buffer-64 crackle is an OFF-CPU stall during a real block's process (an
// allocation on the audio thread grabs the allocator lock → the callback
// blocks). This test drives the user's REAL native + NAM blocks and pins
// zero per-callback allocations through them.

use domain::value_objects::ParameterValue as P670Val;
use project::block::{CoreBlock, NamBlock};
use project::param::ParameterSet as P670Set;
use std::sync::Once as P670Once;

fn p670_init_registry() {
    static INIT: P670Once = P670Once::new();
    INIT.call_once(|| {
        nam::register_builder();
        ir::register_builder();
        block_dyn::register_natives();
        block_filter::register_natives();
        block_reverb::register_natives();
        block_gain::register_natives();
        block_amp::register_natives();
        block_preamp::register_natives();
        block_cab::register_natives();
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins");
        plugin_loader::registry::init(&root);
    });
}

fn p670_floats(pairs: &[(&str, f32)]) -> P670Set {
    let mut p = P670Set::default();
    for (k, v) in pairs {
        p.insert(*k, P670Val::Float(*v));
    }
    p
}

fn p670_core(id: &str, et: &str, model: &str, p: P670Set) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: et.into(),
            model: model.into(),
            params: p,
        }),
    }
}

fn p670_nam(id: &str, model: &str, preset: &str) -> AudioBlock {
    let mut p = P670Set::default();
    p.insert("preset", P670Val::String(preset.into()));
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Nam(NamBlock {
            model: model.into(),
            params: p,
        }),
    }
}

fn p670_real_rig_chain() -> Chain {
    Chain {
        id: ChainId("issue670-alloc-realrig".into()),
        description: Some("issue #670 real-block alloc invariant".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            p670_core(
                "eq",
                "filter",
                "native_guitar_eq",
                p670_floats(&[
                    ("high", 0.0),
                    ("high_mid", 0.0),
                    ("low", 0.0),
                    ("low_mid", 0.0),
                ]),
            ),
            p670_nam("amp", "nam_marshall_plexi", "angus"),
            p670_core(
                "limit",
                "dynamics",
                "limiter_brickwall",
                p670_floats(&[
                    ("ceiling", -0.1),
                    ("knee_db", 2.0),
                    ("lookahead_ms", 3.0),
                    ("release_ms", 100.0),
                    ("threshold", -1.0),
                ]),
            ),
            AudioBlock {
                id: BlockId("output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
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
#[ignore = "issue #670: run serially in release — `cargo test -p engine \
 --release --lib audio_callback_does_not_allocate_with_real_blocks -- \
 --ignored --test-threads=1`."]
fn audio_callback_does_not_allocate_with_real_blocks() {
    p670_init_registry();
    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(
            &p670_real_rig_chain(),
            48_000.0_f32,
            &[DEFAULT_ELASTIC_TARGET],
        )
        .expect("real-rig runtime should build"),
    );
    let input_buf = vec![0.3_f32; 64];
    let mut output_buf = vec![0.0_f32; 64 * 2];
    for _ in 0..256 {
        process_input_f32(&runtime, 0, &input_buf, 1);
        process_output_f32(&runtime, 0, &mut output_buf, 2);
    }
    let allocs = measure_allocs(|| {
        for _ in 0..1_000 {
            process_input_f32(&runtime, 0, &input_buf, 1);
            process_output_f32(&runtime, 0, &mut output_buf, 2);
        }
    });
    eprintln!("[#670 alloc] 1000 callbacks, real native+NAM blocks @64: {allocs} allocations");
    assert_eq!(
        allocs, 0,
        "BUG #670: {allocs} heap allocations in 1000 steady-state callbacks \
         through real DSP blocks (eq + NAM + limiter). An allocation on the \
         audio thread grabs the allocator lock and blocks the callback \
         off-CPU — exactly the buffer-64 stall the probe measured. The pipe \
         chain test misses this because it runs no DSP block."
    );
}

fn p670_eq8() -> AudioBlock {
    let mut p = P670Set::default();
    for b in 1..=8 {
        p.insert(&format!("band{b}_enabled"), P670Val::Bool(true));
        p.insert(&format!("band{b}_freq"), P670Val::Float(1000.0));
        p.insert(&format!("band{b}_gain"), P670Val::Float(0.0));
        p.insert(&format!("band{b}_q"), P670Val::Float(1.0));
        p.insert(&format!("band{b}_type"), P670Val::String("peak".into()));
    }
    p.insert("output_db", P670Val::Float(0.0));
    p670_core("eq8", "filter", "eq_eight_band_parametric", p)
}

fn p670_isolated(block: AudioBlock) -> Chain {
    Chain {
        id: ChainId("issue670-iso".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            block,
            AudioBlock {
                id: BlockId("output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
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
#[ignore = "issue #670: run serially in release"]
fn audio_callback_does_not_allocate_with_eq8() {
    p670_init_registry();
    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(
            &p670_isolated(p670_eq8()),
            48_000.0_f32,
            &[DEFAULT_ELASTIC_TARGET],
        )
        .expect("eq8 runtime should build"),
    );
    let input_buf = vec![0.3_f32; 64];
    let mut output_buf = vec![0.0_f32; 64 * 2];
    for _ in 0..256 {
        process_input_f32(&runtime, 0, &input_buf, 1);
        process_output_f32(&runtime, 0, &mut output_buf, 2);
    }
    let allocs = measure_allocs(|| {
        for _ in 0..1_000 {
            process_input_f32(&runtime, 0, &input_buf, 1);
            process_output_f32(&runtime, 0, &mut output_buf, 2);
        }
    });
    eprintln!("[#670 alloc] eq_eight_band_parametric @64: {allocs} allocations / 1000 callbacks");
    assert_eq!(
        allocs, 0,
        "BUG #670: eq_eight_band_parametric allocates {allocs}x on the audio thread"
    );
}

/// Issue #670: the user reports the rig gets MUCH worse with an IR/CAB in the
/// chain. The IR's per-buffer COMPUTE is tiny (~4us measured), so if it hurts
/// it must be doing something else on the audio thread — the prime suspect is
/// a per-callback heap allocation (FFT scratch), which is cheap alone but
/// serializes every audio thread on the allocator lock once several chains
/// run at once (the multi-chain crackle). This pins the IR convolution to
/// ZERO audio-thread allocations (CLAUDE.md invariant #8). Uses the real
/// bundled cab IR.
fn p670_ir_cab() -> AudioBlock {
    let mut p = P670Set::default();
    p.insert("preset", P670Val::String("big".into()));
    p670_core("ircab", "cab", "ir_fender_deluxe_reverb_oxford", p)
}

#[test]
#[ignore = "issue #670: run serially in release"]
fn audio_callback_does_not_allocate_with_ir_cab() {
    p670_init_registry();
    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(
            &p670_isolated(p670_ir_cab()),
            48_000.0_f32,
            &[DEFAULT_ELASTIC_TARGET],
        )
        .expect("ir cab runtime should build"),
    );
    let input_buf = vec![0.3_f32; 64];
    let mut output_buf = vec![0.0_f32; 64 * 2];
    for _ in 0..256 {
        process_input_f32(&runtime, 0, &input_buf, 1);
        process_output_f32(&runtime, 0, &mut output_buf, 2);
    }
    let allocs = measure_allocs(|| {
        for _ in 0..1_000 {
            process_input_f32(&runtime, 0, &input_buf, 1);
            process_output_f32(&runtime, 0, &mut output_buf, 2);
        }
    });
    eprintln!("[#670 alloc] ir_fender_deluxe_reverb_oxford cab @64: {allocs} allocations / 1000 callbacks");
    assert_eq!(
        allocs, 0,
        "BUG #670: the IR/CAB convolution allocates {allocs}x on the audio thread \
         in 1000 callbacks — cheap alone, but it serializes every audio thread on \
         the allocator lock once several chains run, which is why the rig gets \
         much worse with an IR in the chain."
    );
}
