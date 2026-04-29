//! Stream-isolation contract tests (issue #350).
//!
//! Codifies the CLAUDE.md non-regression invariant #4: each `InputBlock`
//! must produce a TOTALLY isolated parallel stream. Two InputBlocks —
//! whether grouped under the same YAML chain or split across chains —
//! must NEVER share runtime state. No shared buffer, lock, scratch,
//! cache line, route, tap, or any Arc'd mutable state. CPU or buffer
//! contention from one stream must NOT affect the other's callback.
//!
//! Today the engine groups N inputs of one YAML chain into a single
//! `ChainRuntimeState` with shared `output_routes` (SPSC violation),
//! shared `input_taps`, shared `output_taps`, shared `processing`
//! mutex. The tests below assert the post-fix contract; until #350
//! is implemented they are `#[ignore]` (and will FAIL when run with
//! `--ignored`). When the fix lands in this branch, the `#[ignore]`
//! markers are dropped and the tests must pass.

use super::*;
use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::project::Project;
use std::collections::HashMap;
use std::sync::Arc;

fn input_block(id: &str, device: &str, channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId(device.into()),
                mode: ChainInputMode::Mono,
                channels,
            }],
        }),
    }
}

fn output_block(id: &str, device: &str, channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId(device.into()),
                mode: ChainOutputMode::Mono,
                channels,
            }],
        }),
    }
}

/// Chain with N InputBlocks all routed to one OutputBlock. The user-
/// visible "two guitars in the same chain" scenario that triggered #350.
fn dual_input_chain() -> Chain {
    Chain {
        id: ChainId("dual_input".into()),
        description: Some("two guitars, one chain".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            input_block("dual_input:input:0", "guitar_a", vec![0]),
            input_block("dual_input:input:1", "guitar_b", vec![0]),
            output_block("dual_input:output:0", "main_out", vec![0]),
        ],
    }
}

fn build_dual_input_graph() -> RuntimeGraph {
    let chain = dual_input_chain();
    let project = Project {
        name: Some("stream_isolation_test".into()),
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
    };
    let mut sample_rates = HashMap::new();
    sample_rates.insert(chain.id.clone(), 48000.0_f32);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();
    build_runtime_graph(&project, &sample_rates, &elastic_targets)
        .expect("dual_input chain must build")
}

// ─────────────────────────────────────────────────────────────────────
// Contract: one runtime per InputBlock
// ─────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "PENDING #350 — engine groups all InputBlocks of a chain into ONE ChainRuntimeState"]
fn two_input_blocks_in_same_chain_produce_two_independent_runtimes() {
    let graph = build_dual_input_graph();

    assert!(
        graph.chains.len() >= 2,
        "expected one ChainRuntimeState per InputBlock (≥2), got {}",
        graph.chains.len()
    );
}

// ─────────────────────────────────────────────────────────────────────
// Contract: zero shared Arc'd state between streams
// ─────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "PENDING #350 — output_routes are currently shared across InputBlocks of the same chain"]
fn two_input_blocks_must_not_share_output_routes_arc() {
    let graph = build_dual_input_graph();
    let runtimes: Vec<&Arc<ChainRuntimeState>> = graph.chains.values().collect();
    assert!(
        runtimes.len() >= 2,
        "fixture failed: dual_input_chain produced <2 runtimes"
    );

    for i in 0..runtimes.len() {
        for j in (i + 1)..runtimes.len() {
            let r0 = runtimes[i].output_routes.load_full();
            let r1 = runtimes[j].output_routes.load_full();
            assert!(
                !Arc::ptr_eq(&r0, &r1),
                "runtimes #{i} and #{j} share output_routes Vec Arc — violates isolation invariant"
            );
            for (k, (route0, route1)) in r0.iter().zip(r1.iter()).enumerate() {
                assert!(
                    !Arc::ptr_eq(route0, route1),
                    "runtimes #{i}/#{j} share OutputRoutingState Arc at index {k}"
                );
            }
        }
    }
}

#[test]
#[ignore = "PENDING #350 — input_taps Vec is currently shared across InputBlocks of the same chain"]
fn two_input_blocks_must_not_share_input_taps_arc() {
    let graph = build_dual_input_graph();
    let runtimes: Vec<&Arc<ChainRuntimeState>> = graph.chains.values().collect();
    assert!(runtimes.len() >= 2, "fixture failed");

    for i in 0..runtimes.len() {
        for j in (i + 1)..runtimes.len() {
            let r0 = runtimes[i].input_taps.load_full();
            let r1 = runtimes[j].input_taps.load_full();
            assert!(
                !Arc::ptr_eq(&r0, &r1),
                "runtimes #{i} and #{j} share input_taps Vec Arc — violates isolation invariant"
            );
        }
    }
}

#[test]
#[ignore = "PENDING #350 — processing scratch is currently shared across InputBlocks of the same chain"]
fn two_input_blocks_must_not_share_processing_state() {
    let graph = build_dual_input_graph();
    let runtimes: Vec<&Arc<ChainRuntimeState>> = graph.chains.values().collect();
    assert!(runtimes.len() >= 2, "fixture failed");

    // The processing Mutex protects per-input scratch, segment maps, and
    // input_states. Sharing the Mutex itself between streams means a slow
    // input contends with another's callback. Address-equality of the
    // Mutex object is sufficient evidence of a shared lock.
    for i in 0..runtimes.len() {
        for j in (i + 1)..runtimes.len() {
            let p0: *const _ = &runtimes[i].processing;
            let p1: *const _ = &runtimes[j].processing;
            assert!(
                p0 != p1,
                "runtimes #{i} and #{j} reference the same processing Mutex — \
                 contention from one input's callback can stall the other"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Contract: ElasticBuffer obeys SPSC — each output buffer has exactly
// one producer (one InputBlock).
// ─────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "PENDING #350 — output route ElasticBuffer is currently a single instance shared by N InputBlocks pushing concurrently"]
fn each_output_route_buffer_has_exactly_one_producer() {
    // The ElasticBuffer field of OutputRoutingState is declared SPSC
    // (`crates/engine/src/runtime.rs:90-99`). With multiple InputBlocks
    // routing to the same OutputBlock, today the engine has each input's
    // process_input_f32 call `route.buffer.push()` on the SAME ElasticBuffer.
    // Two producers on an SPSC ring is undefined behavior + cache contention.
    //
    // Post-#350 the architecture is: each InputBlock owns its own
    // OutputRoutingState (and its own ElasticBuffer). If a user asks for
    // "two guitars merging into one device output", the merge happens at
    // the cpal/JACK backend level — not by stuffing two producers into
    // one of OUR rings.
    let graph = build_dual_input_graph();
    let runtimes: Vec<&Arc<ChainRuntimeState>> = graph.chains.values().collect();
    assert!(runtimes.len() >= 2, "fixture failed");

    // Each runtime must have its own distinct ElasticBuffer instance
    // (not just a different Arc<OutputRoutingState> wrapper around the
    // same buffer — different ElasticBuffers).
    let buffers: Vec<*const _> = runtimes
        .iter()
        .flat_map(|r| {
            let routes = r.output_routes.load_full();
            (0..routes.len())
                .map(|i| {
                    let r = routes[i].clone();
                    let p: *const _ = &r.buffer;
                    p
                })
                .collect::<Vec<_>>()
        })
        .collect();

    for i in 0..buffers.len() {
        for j in (i + 1)..buffers.len() {
            assert!(
                buffers[i] != buffers[j],
                "ElasticBuffer instance shared across output routes — \
                 two producers on a single SPSC ring violates SPSC contract"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Regression tests (issue #350) — every effective input MUST receive data.
// Pinned here so the "channel 2 silent" class of bug never lands again.
// ─────────────────────────────────────────────────────────────────────────

/// 1 InputBlock with `mode: mono, channels: [0, 1]` is split by the engine
/// into 2 effective inputs (one per channel). For every effective input the
/// runtime MUST register at least one segment that fires when its CPAL
/// callback is dispatched. If any segment is missing, that channel is
/// silent — exactly the regression the previous broken iteration shipped.
#[test]
fn every_effective_input_index_has_at_least_one_segment() {
    let chain = Chain {
        id: ChainId("regression:every-input-has-segment".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            // 1 InputBlock, 2 channels, mono mode — the user's "duas
            // guitarras na mesma chain" config.
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("scarlett".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0, 1],
                    }],
                }),
            },
            output_block("output:0", "main_out", vec![0]),
        ],
    };

    let project = Project {
        name: Some("regression".into()),
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
    };
    let mut sample_rates = HashMap::new();
    sample_rates.insert(chain.id.clone(), 48_000.0_f32);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();
    let graph = build_runtime_graph(&project, &sample_rates, &elastic_targets)
        .expect("regression chain must build");

    let runtime = graph.chains.values().next().expect("expected 1 runtime");
    let processing = runtime.processing.lock().expect("lock poisoned");

    // Engine's effective_inputs splits mono multi-channel into one entry
    // per channel — so the user's chain produces ≥2 effective inputs.
    let (eff_inputs, cpal_indices, _) = effective_inputs(&chain);
    assert!(
        eff_inputs.len() >= 2,
        "fixture invariant: 1-InputBlock mono multi-channel must split into ≥2 effective inputs, got {}",
        eff_inputs.len()
    );

    // Invariante: cada `cpal_index` que o engine declara como destino de
    // callback DEVE ter pelo menos um segment registrado em
    // `input_to_segments`. Quem declara uma stream cpal mas não registra
    // segment garante silêncio naquele callback — exatamente o que a
    // tentativa de fix anterior produziu pro canal 2.
    let unique_cpal_indices: std::collections::HashSet<usize> =
        cpal_indices.iter().copied().collect();
    for cpal_idx in unique_cpal_indices {
        let segments_for_idx = processing
            .input_to_segments
            .get(cpal_idx)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        assert!(
            !segments_for_idx.is_empty(),
            "input_to_segments[{}] is empty — engine declared cpal_index {} for a \
             callback but no segment is registered to receive its data; that \
             channel will be silent",
            cpal_idx,
            cpal_idx
        );
    }
}

/// Belt-and-suspenders: for every InputProcessingState the engine creates,
/// at least one entry in `input_to_segments` references it. If a segment
/// is orphaned (no callback dispatches to it), it never processes audio.
#[test]
fn no_segment_is_orphaned_from_input_dispatch() {
    let chain = Chain {
        id: ChainId("regression:no-orphan-segment".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("scarlett".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0, 1, 2],
                    }],
                }),
            },
            output_block("output:0", "main_out", vec![0]),
        ],
    };
    let project = Project {
        name: Some("regression".into()),
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
    };
    let mut sample_rates = HashMap::new();
    sample_rates.insert(chain.id.clone(), 48_000.0_f32);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();
    let graph = build_runtime_graph(&project, &sample_rates, &elastic_targets)
        .expect("regression chain must build");

    let runtime = graph.chains.values().next().expect("expected 1 runtime");
    let processing = runtime.processing.lock().expect("lock poisoned");
    let input_states_len = processing.input_states.len();
    let input_to_segments_count = processing.input_to_segments.len();

    // Walk every segment index and assert SOME entry in input_to_segments
    // references it.
    for seg_idx in 0..input_states_len {
        let mut found = false;
        for input_idx in 0..input_to_segments_count {
            if processing.input_to_segments[input_idx].contains(&seg_idx) {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "segment {} is orphaned — no input_to_segments entry references it; \
             its audio is never processed",
            seg_idx
        );
    }
}

/// The output side mirror of the input regression: every output route the
/// engine creates MUST have at least one segment writing to it (i.e. some
/// `output_buffers` Arc points at it). An output route with no producer
/// is silent forever.
#[test]
fn every_output_route_has_at_least_one_producer_segment() {
    let chain = Chain {
        id: ChainId("regression:every-output-has-producer".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            input_block("input:0", "scarlett", vec![0]),
            output_block("output:0", "main_out", vec![0]),
        ],
    };
    let project = Project {
        name: Some("regression".into()),
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
    };
    let mut sample_rates = HashMap::new();
    sample_rates.insert(chain.id.clone(), 48_000.0_f32);
    let elastic_targets: HashMap<ChainId, Vec<usize>> = HashMap::new();
    let graph = build_runtime_graph(&project, &sample_rates, &elastic_targets)
        .expect("regression chain must build");

    let runtime = graph.chains.values().next().expect("expected 1 runtime");
    let routes = runtime.output_routes.load_full();
    let processing = runtime.processing.lock().expect("lock poisoned");

    for (route_idx, _route) in routes.iter().enumerate() {
        // A producer is any InputProcessingState whose `output_route_indices`
        // names this route_idx — that segment will push frames to the route.
        let producers: Vec<usize> = processing
            .input_states
            .iter()
            .enumerate()
            .filter(|(_, state)| state.output_route_indices.contains(&route_idx))
            .map(|(i, _)| i)
            .collect();
        assert!(
            !producers.is_empty(),
            "output_routes[{}] has no producer segment — output is silent forever",
            route_idx
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Cross-talk / interference detection (issue #350) — measurable signal
// isolation between concurrent streams of the same device. THESE are the
// tests the user explicitly demanded ("garantir que streams sao isolados").
// ─────────────────────────────────────────────────────────────────────────

/// Two channels of the same device must NOT cancel each other in the
/// output. Send +0.5 on channel 0 and −0.5 on channel 1 of one
/// 2-channel-mono InputBlock; the output of a passthrough chain MUST
/// still carry both signals (in a stereo output) or, if the chain
/// architecture mixes them, the sum MUST not be zero (which would mean
/// total cancellation = total interference).
///
/// This test currently FAILS on the post-revert architecture: both
/// segments upmix Mono→Stereo by broadcasting (Stereo([s, s])), then
/// the engine sums into a single shared output buffer, producing
/// Stereo([s_ch0 + s_ch1, s_ch0 + s_ch1]) — which is silence when the
/// two inputs are equal-and-opposite. That is the user-visible "channel
/// 2 interfering with channel 1" bug, exposed mathematically.
///
/// When we fix the architecture so each split-mono segment writes only
/// to its own output channel position, this test PASSES: ch0 in left,
/// ch1 in right, both preserved.
#[test]
#[ignore = "PENDING #350 phase 2 — current arch broadcasts mono and sums, cancelling opposite-phase signals"]
fn two_channel_mono_input_must_not_cancel_in_output() {
    use crate::runtime::{process_input_f32, process_output_f32};

    let chain = Chain {
        id: ChainId("isolation:no-cancel".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("scarlett".into()),
                        mode: ChainInputMode::Mono,
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
                        device_id: DeviceId("monitor".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    };

    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256]).expect("passthrough chain must build"),
    );

    let frames = 64usize;
    // Stereo interleaved: ch0 = +0.5 every frame, ch1 = −0.5 every frame.
    let data: Vec<f32> = (0..frames).flat_map(|_| [0.5_f32, -0.5_f32]).collect();

    // Fire the cpal callback for input_index 0 (the only one — both
    // segments share the index since they hit the same physical device).
    process_input_f32(&runtime, 0, &data, 2);

    // Drain the output: stereo, 2 channels.
    let mut out = vec![0.0_f32; frames * 2];
    process_output_f32(&runtime, 0, &mut out, 2);

    // Energy invariant: at least ONE of the two output channels must
    // carry a non-trivial signal. If BOTH channels are below the noise
    // floor, the signals cancelled — total interference.
    let abs_energy_left: f32 = out.iter().step_by(2).map(|s| s.abs()).sum();
    let abs_energy_right: f32 = out.iter().skip(1).step_by(2).map(|s| s.abs()).sum();
    let total_energy = abs_energy_left + abs_energy_right;
    assert!(
        total_energy > 1e-3,
        "output channels are silent (left={:.6}, right={:.6}) — the two input \
         signals cancelled each other. Streams are not isolated.",
        abs_energy_left,
        abs_energy_right
    );
}

/// Inverse of the cancellation test: send +0.5 on BOTH channels and
/// verify the output is NOT saturated (above 0.95) by the sum (1.0)
/// hitting tanh saturation. If isolated, each channel keeps its own
/// 0.5 signal in its own output channel — no clipping. If summed
/// architecture, both output channels carry tanh(1.0) ≈ 0.76, audible
/// as soft distortion.
#[test]
#[ignore = "PENDING #350 phase 2 — same-phase signals must be carried separately, not summed and limited"]
fn two_channel_mono_input_must_not_saturate_when_both_loud() {
    use crate::runtime::{process_input_f32, process_output_f32};

    let chain = Chain {
        id: ChainId("isolation:no-sat".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("scarlett".into()),
                        mode: ChainInputMode::Mono,
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
                        device_id: DeviceId("monitor".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    };

    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256]).expect("passthrough chain must build"),
    );

    let frames = 64usize;
    // Both channels at +0.5 (well below clip if isolated).
    let data: Vec<f32> = (0..frames).flat_map(|_| [0.5_f32, 0.5_f32]).collect();

    process_input_f32(&runtime, 0, &data, 2);

    let mut out = vec![0.0_f32; frames * 2];
    process_output_f32(&runtime, 0, &mut out, 2);

    // Each output channel must carry ~0.5 (its own input channel) — NOT
    // tanh(1.0)≈0.76 from summed-and-limited mixing.
    for (i, &sample) in out.iter().enumerate() {
        let channel = i % 2;
        assert!(
            (sample - 0.5).abs() < 0.05,
            "out[{}] (channel {}) = {:.4}; expected ~0.5 (the matching input \
             channel). A larger value means the engine summed two channels \
             and the limiter saturated — streams not isolated.",
            i,
            channel,
            sample
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// "Stream is ALWAYS stereo internally" invariant (issue #350).
// ─────────────────────────────────────────────────────────────────────────
//
// Project rule (CLAUDE.md non-regression invariant #5): every internal
// stream processes on a STEREO bus when the chain output is stereo —
// regardless of input mode. Mono input upmixes by broadcasting
// (Stereo([s, s])); two split-mono siblings are TWO separate stereo
// streams (each broadcast), summed at fan-out with 1/N gain to avoid
// limiter saturation. Auto-panning, forcing Mono bus on a stereo
// chain, or sending one guitar to one ear is FORBIDDEN.
//
// The tests below pin the rule. Reintroducing the Mono-bus override or
// auto-pan will break them.

/// `processing_layout` of every InputProcessingState in a chain whose
/// OutputBlock is stereo MUST be Stereo — split-mono siblings included.
/// This catches the regression where a previous fix forced Mono bus
/// for split-mono segments and the user heard each guitar in only one
/// ear (auto-pan effect via partial broadcast).
#[test]
fn split_mono_segments_keep_stereo_processing_when_output_is_stereo() {
    let chain = Chain {
        id: ChainId("isolation:always-stereo".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            // 1 InputBlock, 2 channels, mono mode → 2 effective entries
            // (one per channel) — the user's "duas guitarras na mesma
            // input" config.
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("scarlett".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0, 1],
                    }],
                }),
            },
            // Stereo output → bus must stay stereo for every stream.
            AudioBlock {
                id: BlockId("output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("monitor".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    };

    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256])
            .expect("split-mono / stereo-output chain must build"),
    );
    let processing = runtime.processing.lock().expect("lock poisoned");

    assert!(
        processing.input_states.len() >= 2,
        "fixture invariant: split-mono with 2 channels must produce ≥2 segments, got {}",
        processing.input_states.len()
    );

    for (i, state) in processing.input_states.iter().enumerate() {
        assert!(
            matches!(state.processing_layout, AudioChannelLayout::Stereo),
            "segment {} processing_layout = {:?}; must be Stereo when chain \
             output is stereo, even for split-mono entries. Forcing Mono bus \
             here breaks the 'stream is always stereo internally' rule and \
             produces auto-pan / one-ear-only output.",
            i,
            state.processing_layout
        );
    }
}

/// DualMono input + stereo output: also Stereo bus (the DualMono variant
/// is flattened to a Stereo layout at the buffer level; L/R independence
/// is preserved internally by `AudioProcessor::DualMono`).
#[test]
fn dual_mono_segment_keeps_stereo_processing() {
    let chain = Chain {
        id: ChainId("isolation:dualmono-stereo".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("scarlett".into()),
                        mode: ChainInputMode::DualMono,
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
                        device_id: DeviceId("monitor".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    };

    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256]).expect("dualmono chain must build"),
    );
    let processing = runtime.processing.lock().expect("lock poisoned");

    for (i, state) in processing.input_states.iter().enumerate() {
        assert!(
            matches!(state.processing_layout, AudioChannelLayout::Stereo),
            "DualMono segment {} processing_layout = {:?}; must be Stereo. \
             DualMono is flattened to a Stereo bus at the buffer level, with \
             internal L/R independence preserved by AudioProcessor::DualMono.",
            i,
            state.processing_layout
        );
    }
}

/// Mono input + Mono OUTPUT: Mono bus is correct. The "always stereo"
/// rule applies WHEN OUTPUT IS STEREO. If the user explicitly configures
/// a mono output, we don't force a useless upmix.
#[test]
fn mono_input_with_mono_output_stays_mono() {
    let chain = Chain {
        id: ChainId("isolation:mono-mono".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            input_block("input:0", "scarlett", vec![0]),
            output_block("output:0", "monitor", vec![0]),
        ],
    };
    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256]).expect("mono-only chain must build"),
    );
    let processing = runtime.processing.lock().expect("lock poisoned");

    for (i, state) in processing.input_states.iter().enumerate() {
        assert!(
            matches!(state.processing_layout, AudioChannelLayout::Mono),
            "segment {} processing_layout = {:?}; mono in + mono out must \
             stay Mono (no useless upmix).",
            i,
            state.processing_layout
        );
    }
}
