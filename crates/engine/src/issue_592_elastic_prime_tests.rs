//! Issue #592 — RED-first: a chain that contains a convolution (IR/cab)
//! block must build its output elastic buffer **primed** with a silence
//! cushion, so the first-stream-start at small device buffers (32/64)
//! survives the IR convolver's periodic per-partition FFT spike instead
//! of underrunning ("xiado"/distortion until a warm rebuild).
//!
//! Root cause (confirmed in code): the IR convolver
//! (`ir::FftBlockConvolver`) ran a full FFT inline every
//! `ir::PARTITION_SIZE` samples, so at buffer 64 one callback in
//! eight was far heavier than the rest. (Issue #617 later eliminated that
//! spike at the source by shrinking the partition to 64 so the work is
//! uniform per callback; this cold-start cushion is now decoupled — see
//! `IR_COLD_START_CUSHION_FRAMES` — and kept for producer warmup jitter.)
//! The elastic buffer that decouples
//! the DSP producer from the output consumer starts EMPTY
//! (`ElasticBuffer::new` → len 0) and its `target_level` was dead code —
//! there was no jitter cushion. Cold-start (slow first callbacks) drains
//! it to silence on the FFT spike. Priming the buffer to one partition of
//! silence on the initial build gives the cushion immediately, before the
//! producer warms up.
//!
//! Scope: only chains WITH a convolution block are primed, and only on the
//! INITIAL build (a rebuild/edit runs warm, so it refills naturally — no
//! re-prime, no latency creep). Non-IR chains are untouched.

use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;

use crate::runtime_graph::{build_chain_runtime_state, update_chain_runtime_state};

const SR: f32 = 48_000.0;

fn input() -> AudioBlock {
    AudioBlock {
        id: BlockId("in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }
}

fn output() -> AudioBlock {
    AudioBlock {
        id: BlockId("out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    }
}

/// A convolution (cab/IR) block. The model need not resolve to a real
/// plugin: the block faults to a bypass node, but the chain still
/// *declares* a convolution block, which is what drives the priming.
fn cab(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_CAB.to_string(),
            model: "ir_test_fake".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn chain(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks,
    }
}

fn first_output_buffer_len(chain: &Chain, buffer: usize) -> usize {
    let rt = build_chain_runtime_state(chain, SR, &[buffer]).expect("chain runtime builds");
    rt.output_routes.load()[0].buffer.len()
}

#[test]
fn ir_chain_primes_output_elastic_buffer_on_initial_build() {
    // buffer 64 → without priming the output elastic buffer is empty (len
    // 0) and the IR convolver's per-partition FFT spike underruns it on a
    // cold start. With the fix it is primed to a real cushion (>= 256).
    let conv_chain = chain("issue-592-ir", vec![input(), cab("cab"), output()]);
    let primed = first_output_buffer_len(&conv_chain, 64);
    assert!(
        primed >= 256,
        "BUG #592: a chain with a convolution (IR/cab) block must prime its \
         output elastic buffer with a silence cushion on the initial build so \
         cold-start at buffer 64 survives the IR FFT spike. Got len {primed} \
         (expected >= 256). Without it, the freshly loaded IR preset \
         underruns/distorts until a warm rebuild.",
    );
}

#[test]
fn non_convolution_chain_is_not_primed() {
    // A plain input→output chain has no FFT spike; it must keep the lean
    // (unprimed) start — no added latency for non-IR chains.
    let plain = chain("issue-592-plain", vec![input(), output()]);
    let primed = first_output_buffer_len(&plain, 64);
    assert_eq!(
        primed, 0,
        "a chain without a convolution block must NOT be primed (no extra \
         latency for non-IR chains); got len {primed}"
    );
}

#[test]
fn ir_chain_rebuild_does_not_reprime() {
    // The initial build primes; a subsequent in-place edit (rebuild) runs
    // warm and must NOT re-prime — re-priming on every knob turn would add
    // a silence cushion (latency/gap) on each edit.
    let conv_chain = chain("issue-592-ir-edit", vec![input(), cab("cab"), output()]);
    let rt = Arc::new(build_chain_runtime_state(&conv_chain, SR, &[64]).expect("initial build"));
    update_chain_runtime_state(&rt, &conv_chain, SR, false, &[64]).expect("rebuild");
    let after_edit = rt.output_routes.load()[0].buffer.len();
    assert_eq!(
        after_edit, 0,
        "a rebuild/edit must not re-prime the output buffer (it runs warm); \
         got len {after_edit} after a no-op edit"
    );
}
