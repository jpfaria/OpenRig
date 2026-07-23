//! Audio-alloc invariant tests — real-rig (#670/#781) (issue #792 split).
//! Shared allocator instrumentation + fixtures live in the base file.
#![allow(unused_imports)]
use super::*;
use super::audio_alloc_invariant::*;

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
        io_binding_ids: vec!["io".into()],
        blocks: vec![
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
        ],
        di_output: None,
        loopers: vec![],
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
            &registry_mono_in_stereo_out(),
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
        p.insert(format!("band{b}_enabled"), P670Val::Bool(true));
        p.insert(format!("band{b}_freq"), P670Val::Float(1000.0));
        p.insert(format!("band{b}_gain"), P670Val::Float(0.0));
        p.insert(format!("band{b}_q"), P670Val::Float(1.0));
        p.insert(format!("band{b}_type"), P670Val::String("peak".into()));
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
        io_binding_ids: vec!["io".into()],
        blocks: vec![block],
        di_output: None,
        loopers: vec![],
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
            &registry_mono_in_stereo_out(),
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
            &registry_mono_in_stereo_out(),
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

// ── Issue #781: the RESIDUAL underrun on the user's real two-interface rig.
// After the #743 stream-isolation fix removed the 12800 flood, the user still
// hears 128-192 sporadic underruns "em qualquer situação" (even ONE chain, one
// worker, on an idle M4) with the VST3 DEACTIVATED. The worker trace shows the
// stall is OFF-CPU (low thread-cpu, high wall) DURING a block's process — the
// hallmark of a per-callback allocation grabbing the process allocator lock
// (the exact mechanism the #670 tests above pin, and the user's own words:
// "openrig engasgando por algum processo síncrono"). The existing battery
// covers eq8, ir_cab, and eq+NAM+limiter — but NOT the blocks that are actually
// in the user's rig:input-1: gate_basic, TWO NAM instances, and eq8 TOGETHER.
// These tests close that gap: any RED here IS the synchronous off-CPU cause.

fn p781_gate() -> AudioBlock {
    p670_core(
        "gate",
        "dynamics",
        "gate_basic",
        p670_floats(&[
            ("threshold", -60.0),
            ("attack_ms", 1.0),
            ("release_ms", 100.0),
            ("hold_ms", 150.0),
            ("hysteresis_db", 6.0),
        ]),
    )
}

#[test]
#[ignore = "issue #781: run serially in release — `cargo test -p engine \
 --release --lib audio_callback_does_not_allocate -- --ignored --test-threads=1`"]
fn audio_callback_does_not_allocate_with_gate_basic() {
    p670_init_registry();
    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(
            &p670_isolated(p781_gate()),
            48_000.0_f32,
            &[DEFAULT_ELASTIC_TARGET],
            &registry_mono_in_stereo_out(),
        )
        .expect("gate runtime should build"),
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
    eprintln!("[#781 alloc] gate_basic @64: {allocs} allocations / 1000 callbacks");
    assert_eq!(
        allocs, 0,
        "BUG #781: gate_basic allocates {allocs}x on the audio thread — grabs the \
         allocator lock and stalls the worker off-CPU (the residual underrun)."
    );
}

#[test]
#[ignore = "issue #781: run serially in release"]
fn audio_callback_does_not_allocate_with_two_nam_instances() {
    p670_init_registry();
    let chain = Chain {
        id: ChainId("issue781-two-nam".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![
            p670_nam("amp1", "nam_marshall_plexi", "angus"),
            p670_nam("amp2", "nam_marshall_plexi", "angus"),
        ],
        di_output: None,
        loopers: vec![],
    };
    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0_f32,
            &[DEFAULT_ELASTIC_TARGET],
            &registry_mono_in_stereo_out(),
        )
        .expect("two-NAM runtime should build"),
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
    eprintln!("[#781 alloc] two NAM instances @64: {allocs} allocations / 1000 callbacks");
    assert_eq!(
        allocs, 0,
        "BUG #781: two NAM instances allocate {allocs}x on the audio thread in \
         1000 callbacks — the second instance's process path is not alloc-free."
    );
}

#[test]
#[ignore = "issue #781: run serially in release"]
fn audio_callback_does_not_allocate_with_user_rig_input1() {
    p670_init_registry();
    let chain = Chain {
        id: ChainId("issue781-rig-input1".into()),
        description: Some("issue #781 user's real rig:input-1 chain".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![
            p781_gate(),
            p670_nam("amp1", "nam_marshall_plexi", "angus"),
            p670_nam("amp2", "nam_marshall_plexi", "angus"),
            p670_eq8(),
        ],
        di_output: None,
        loopers: vec![],
    };
    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0_f32,
            &[DEFAULT_ELASTIC_TARGET],
            &registry_mono_in_stereo_out(),
        )
        .expect("user rig:input-1 runtime should build"),
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
    eprintln!(
        "[#781 alloc] user rig:input-1 (gate + 2 NAM + eq8) @64: {allocs} allocations / 1000 callbacks"
    );
    assert_eq!(
        allocs, 0,
        "BUG #781: the user's real rig:input-1 chain allocates {allocs}x on the \
         audio thread in 1000 callbacks — an allocation grabs the process \
         allocator lock and stalls the worker off-CPU. This is the residual \
         underrun 'em qualquer situação' (synchronous, even single-chain)."
    );
}
