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
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::runtime_graph::{build_chain_runtime_state, update_chain_runtime_state};

const SR: f32 = 48_000.0;

/// Registry mirroring the legacy mono-in / stereo-out endpoints. The chain's
/// head input and tail output are now resolved from this binding (#716).
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
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
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
        io_binding_ids: vec!["io".into()],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

fn first_output_buffer_len(chain: &Chain, buffer: usize) -> usize {
    let rt =
        build_chain_runtime_state(chain, SR, &[buffer], &registry()).expect("chain runtime builds");
    rt.output_routes.load()[0]
        .as_ref()
        .expect("route 0")
        .buffer
        .len()
}

#[test]
fn ir_chain_primes_output_elastic_buffer_on_initial_build() {
    // buffer 64 → without priming the output elastic buffer is empty (len
    // 0) and an IR chain's cold start could underrun it. It is primed with
    // its cushion. #965: on the producer's own clock (this registry: one
    // device in and out) that cushion is the route's own target — 64 here.
    // The old 512-frame floor sat above the target and the drift guard cut
    // it ~186 ms later (a skip after every edit and DI render); the #617
    // uniform convolver removed the spike it covered, and the real stack
    // measured the lean cushion clean at 64 frames (cold start, a minute,
    // live edits, a cab added live under full CPU load — see
    // `infra-cpal/tests/issue_965_insert_on_the_owners_interface.rs`). A
    // route on ANOTHER clock keeps the 512 cushion (`route_cushion`).
    let conv_chain = chain("issue-592-ir", vec![cab("cab")]);
    let primed = first_output_buffer_len(&conv_chain, 64);
    assert_eq!(
        primed, 64,
        "BUG #592: a chain with a convolution (IR/cab) block must be born \
         with its output cushion filled — exactly its own target on its \
         producer's clock. Got len {primed}.",
    );
}

#[test]
fn non_convolution_chain_is_not_primed() {
    // A plain input→output chain has no FFT spike; it must keep the lean
    // (unprimed) start — no added latency for non-IR chains.
    let plain = chain("issue-592-plain", vec![]);
    let primed = first_output_buffer_len(&plain, 64);
    assert_eq!(
        primed, 0,
        "a chain without a convolution block must NOT be primed (no extra \
         latency for non-IR chains); got len {primed}"
    );
}

#[test]
fn ir_chain_rebuild_preserves_cushion_without_repriming() {
    // The #592 intent: an edit (rebuild) must not INJECT new silence —
    // re-priming on every knob turn would add a fresh cushion (latency/gap)
    // per edit. The original assertion (`len == 0` after rebuild) encoded
    // the old implementation — a brand-new EMPTY buffer — which was itself
    // the #670 edit-click bug: it discarded the in-flight audio and left the
    // chain permanently fragile (the cushion never refills in lockstep).
    // The rebuild now REUSES the route, so the fill must be exactly what it
    // was before the edit: not reset to 0, not re-primed upward.
    let conv_chain = chain("issue-592-ir-edit", vec![cab("cab")]);
    let rt = Arc::new(
        build_chain_runtime_state(&conv_chain, SR, &[64], &registry()).expect("initial build"),
    );
    // Drain part of the initial prime so "preserved" and "re-primed" differ.
    let before_route = rt.output_routes.load()[0].clone().expect("route 0");
    for _ in 0..32 {
        let _ = before_route.buffer.pop();
    }
    let before_edit = rt.output_routes.load()[0]
        .as_ref()
        .expect("route 0")
        .buffer
        .len();
    assert!(
        before_edit > 0,
        "test setup: drained prime should remain > 0"
    );
    update_chain_runtime_state(&rt, &conv_chain, SR, false, &[64], &registry()).expect("rebuild");
    let after_edit = rt.output_routes.load()[0]
        .as_ref()
        .expect("route 0")
        .buffer
        .len();
    assert_eq!(
        after_edit, before_edit,
        "a rebuild/edit must PRESERVE the output buffer exactly — no re-prime \
         (would add latency per edit) and no reset to empty (discards \
         in-flight audio and leaves the chain fragile: the #670 edit click); \
         got len {after_edit} after a no-op edit (was {before_edit})"
    );
}
