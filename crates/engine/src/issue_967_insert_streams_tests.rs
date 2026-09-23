//! #967 — an insert keeps its streams when it is switched; only its DSP cut
//! follows the switch.
//!
//! A BOUND insert owns a send and a return stream whether it is on or off, so
//! the engine reserves their route and input indices in both states — the
//! numbering the opened streams were built against never changes. A disabled
//! insert still cuts nothing: every head plays straight through to its own
//! E/S's outputs, exactly as before #967. These pin both halves.

use std::sync::Arc;

use super::tests::{insert_chain, insert_registry, tuner_block};
use super::{build_chain_runtime_state, process_input_f32, process_output_f32};
use crate::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io};
use crate::runtime_state::ChainRuntimeState;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;

const FRAMES: usize = 128;
const INSERT: &str = "insert:0";

fn build(chain: &Chain, registry: &[IoBinding]) -> Arc<ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], registry)
            .expect("the chain must build a runtime"),
    )
}

fn level(samples: &[f32]) -> f32 {
    samples.iter().map(|s| s.abs()).sum::<f32>() / samples.len().max(1) as f32
}

/// Two E/S on one interface — A (in 0 → out 0/1), B (in 1 → out 2/3) — plus a
/// loop on the same interface (return in 4, send out 5).
fn two_heads_registry() -> Vec<IoBinding> {
    let ep = |name: &str, mode: ChannelMode, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode,
        channels,
    };
    vec![
        IoBinding {
            id: "a".into(),
            name: "A".into(),
            inputs: vec![ep("in", ChannelMode::Mono, vec![0])],
            outputs: vec![ep("out", ChannelMode::Stereo, vec![0, 1])],
        },
        IoBinding {
            id: "b".into(),
            name: "B".into(),
            inputs: vec![ep("in", ChannelMode::Mono, vec![1])],
            outputs: vec![ep("out", ChannelMode::Stereo, vec![2, 3])],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![ep("ret", ChannelMode::Mono, vec![4])],
            outputs: vec![ep("snd", ChannelMode::Mono, vec![5])],
        },
    ]
}

fn two_heads_chain(insert_enabled: bool) -> Chain {
    Chain {
        id: ChainId("two-heads".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["a".into(), "b".into()],
        blocks: vec![AudioBlock {
            id: BlockId(INSERT.into()),
            enabled: insert_enabled,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "fx".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }
}

/// Switched off, the loop is out of the picture: A's guitar reaches A's
/// outputs and B's reaches B's, each at unity — never both heads summed into
/// every output (+6 dB on the owner's two-E/S-on-one-channel rig).
#[test]
fn a_disabled_insert_leaves_every_head_on_its_own_outputs() {
    const CHANNELS: usize = 8;
    let runtime = build(&two_heads_chain(false), &two_heads_registry());
    let mut input = vec![0.0_f32; FRAMES * CHANNELS];
    for frame in input.chunks_mut(CHANNELS) {
        frame[0] = 0.2;
        frame[1] = 0.3;
    }
    let (mut a, mut b) = (
        vec![0.0_f32; FRAMES * CHANNELS],
        vec![0.0_f32; FRAMES * CHANNELS],
    );
    for _ in 0..256 {
        process_input_f32(&runtime, 0, &input, CHANNELS);
        a.fill(0.0);
        b.fill(0.0);
        process_output_f32(&runtime, 0, &mut a, CHANNELS);
        process_output_f32(&runtime, 1, &mut b, CHANNELS);
    }
    let a_out: Vec<f32> = a.chunks(CHANNELS).map(|f| f[0]).collect();
    let b_out: Vec<f32> = b.chunks(CHANNELS).map(|f| f[2]).collect();

    assert!(
        (level(&a_out) - 0.2).abs() < 0.02 && (level(&b_out) - 0.3).abs() < 0.02,
        "A's outputs carry A (0.2), B's carry B (0.3) — got {} and {}",
        level(&a_out),
        level(&b_out)
    );
}

/// The send route and the return input keep their numbers whether the insert
/// is on or off — the streams were opened against that numbering and a switch
/// must never renumber them.
#[test]
fn switching_the_insert_keeps_the_route_and_input_numbering() {
    let registry = two_heads_registry();
    let numbering = |enabled| {
        let chain = two_heads_chain(enabled);
        let (ins, outs) = resolve_chain_io(&chain, &registry);
        let (eff_in, cpal, _, _) = effective_inputs(&chain, &ins, &registry);
        let eff_out = effective_outputs(&chain, &outs, &registry);
        (
            eff_in
                .iter()
                .map(|e| (e.device_id.clone(), e.channels.clone()))
                .collect::<Vec<_>>(),
            cpal,
            eff_out
                .iter()
                .map(|e| (e.device_id.clone(), e.channels.clone()))
                .collect::<Vec<_>>(),
        )
    };
    let on = numbering(true);
    assert_eq!(on.2.len(), 3, "two tails and the send: {:?}", on.2);
    assert_eq!(
        numbering(false),
        on,
        "a disabled insert keeps its return and send in the numbering"
    );
}

/// With the loop OFF nothing cuts the chain, so two E/S are two isolated
/// runtimes (#703) — merging them because the chain merely OWNS a loop's
/// streams left the second interface's output with no runtime and put both
/// device callbacks on one processing lock. With the loop ON the cut is one
/// pipeline across both heads, one runtime.
///
/// cpal only: linux+JACK groups runtimes per DEVICE, not per input entry
/// (#703's cfg law — the JACK-direct client binds one runtime per device), so
/// two E/S on one interface are one runtime there whether or not a loop cuts
/// the chain.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[test]
fn a_disabled_insert_keeps_every_head_in_its_own_runtime() {
    let registry = two_heads_registry();
    assert_eq!(
        crate::runtime_graph::input_group_ids(&two_heads_chain(false), &registry),
        vec![0, 1],
        "loop off: one isolated runtime per E/S"
    );
}

/// On every platform, the loop's cut is one pipeline across every head: one
/// runtime.
#[test]
fn an_enabled_insert_makes_the_heads_one_runtime() {
    let registry = two_heads_registry();
    assert_eq!(
        crate::runtime_graph::input_group_ids(&two_heads_chain(true), &registry),
        vec![0],
        "loop on: the cut is one pipeline"
    );
}

/// A single-E/S chain — the owner's layout — is one runtime whether its loop is
/// on or off, so the switch is a DSP rebuild into the slot its streams feed.
#[test]
fn a_single_head_chain_keeps_its_runtime_grouping_across_the_switch() {
    let registry = insert_registry();
    let chain = |enabled| {
        let mut chain = insert_chain();
        for block in chain.blocks.iter_mut() {
            if block.id.0 == INSERT {
                block.enabled = enabled;
            }
        }
        chain
    };
    assert_eq!(
        crate::runtime_graph::input_group_ids(&chain(false), &registry),
        crate::runtime_graph::input_group_ids(&chain(true), &registry),
    );
}

/// While the loop is off nothing reads its return. Its stream is still open
/// and still calls in with whatever the gear sends — that must feed no
/// segment at all, not the segment that happens to share its number.
#[test]
fn the_return_of_a_disabled_insert_feeds_nothing() {
    const CHANNELS: usize = 2;
    let mut chain = insert_chain();
    chain
        .blocks
        .retain(|b| !matches!(b.kind, AudioBlockKind::Core(_)));
    for block in chain.blocks.iter_mut() {
        if block.id.0 == INSERT {
            block.enabled = false;
        }
    }
    let runtime = build(&chain, &insert_registry());
    let silence = vec![0.0_f32; FRAMES * CHANNELS];
    let loud_return = vec![0.9_f32; FRAMES * CHANNELS];
    let mut tail = vec![0.0_f32; FRAMES * CHANNELS];
    let mut peak = 0.0_f32;
    for _ in 0..256 {
        process_input_f32(&runtime, 0, &silence, CHANNELS);
        process_input_f32(&runtime, 1, &loud_return, CHANNELS);
        tail.fill(0.0);
        process_output_f32(&runtime, 0, &mut tail, CHANNELS);
        peak = peak.max(tail.iter().fold(0.0_f32, |m, s| m.max(s.abs())));
    }
    assert!(
        peak < 1e-4,
        "the disabled loop's return leaked into the tail (peak {peak})"
    );
}

/// The delay natives register once per test process.
fn register_delay() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        block_delay::register_natives();
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins");
        plugin_loader::registry::init(&root);
    });
}

/// A chain holding a VST3 is updated IN PLACE on a live edit (#779), and a
/// switch of its insert moves blocks between segments. A block that only
/// changed segment keeps its processor — here a delay whose echo, fed before
/// the switch, must still ring after it.
#[test]
fn an_in_place_update_keeps_the_processors_that_changed_segment() {
    const CHANNELS: usize = 2;
    register_delay();
    // A stereo head, like the stereo return: the echo sees the same kind of
    // signal on either side of the cut (a mono head would legitimately
    // rebuild it as a mono-content processor, #588).
    let mut registry = insert_registry();
    registry[0].inputs[0].mode = ChannelMode::Stereo;
    registry[0].inputs[0].channels = vec![0, 1];
    let chain_with = |insert_enabled: bool| {
        let mut chain = insert_chain();
        chain
            .blocks
            .retain(|b| !matches!(b.kind, AudioBlockKind::Core(_)));
        let mut echo = tuner_block("echo:0", 50.0);
        if let AudioBlockKind::Core(core) = &mut echo.kind {
            core.params.insert("feedback", ParameterValue::Float(0.0));
            core.params.insert("mix", ParameterValue::Float(50.0));
            core.params.insert("tone", ParameterValue::Float(100.0));
        }
        // input → insert → echo → output: the echo sits AFTER the cut.
        let insert_at = chain
            .blocks
            .iter()
            .position(|b| b.id.0 == INSERT)
            .expect("insert");
        chain.blocks.insert(insert_at + 1, echo);
        for block in chain.blocks.iter_mut() {
            if block.id.0 == INSERT {
                block.enabled = insert_enabled;
            }
        }
        chain
    };
    let runtime = build(&chain_with(true), &registry);
    let silence = vec![0.0_f32; FRAMES * CHANNELS];
    let impulse = vec![0.5_f32; FRAMES * CHANNELS];
    let mut tail = vec![0.0_f32; FRAMES * CHANNELS];
    for _ in 0..64 {
        process_input_f32(&runtime, 0, &silence, CHANNELS);
        process_input_f32(&runtime, 1, &silence, CHANNELS);
        process_output_f32(&runtime, 0, &mut tail, CHANNELS);
    }
    // The gear answers with a click; the echo after the loop stores it.
    process_input_f32(&runtime, 0, &silence, CHANNELS);
    process_input_f32(&runtime, 1, &impulse, CHANNELS);
    process_output_f32(&runtime, 0, &mut tail, CHANNELS);

    crate::runtime::update_chain_runtime_state(
        &runtime,
        &chain_with(false),
        48_000.0,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &registry,
    )
    .expect("the in-place update must apply");

    let mut peak = 0.0_f32;
    for _ in 0..40 {
        process_input_f32(&runtime, 0, &silence, CHANNELS);
        tail.fill(0.0);
        process_output_f32(&runtime, 0, &mut tail, CHANNELS);
        peak = peak.max(tail.iter().fold(0.0_f32, |m, s| m.max(s.abs())));
    }
    assert!(
        peak > 0.05,
        "the echo's processor was rebuilt when the insert was switched — its \
         tail is gone (peak {peak})"
    );
}

/// A head split over two channels is two segments; the disabled loop's return
/// comes in on a stream numbered past the head's. It must not fall back onto
/// "the segment with that number" — here the second split-mono sibling.
#[test]
fn the_return_of_a_disabled_insert_feeds_no_split_mono_sibling() {
    const CHANNELS: usize = 2;
    let mut registry = insert_registry();
    registry[0].inputs[0].channels = vec![0, 1];
    let mut chain = insert_chain();
    chain
        .blocks
        .retain(|b| !matches!(b.kind, AudioBlockKind::Core(_)));
    for block in chain.blocks.iter_mut() {
        if block.id.0 == INSERT {
            block.enabled = false;
        }
    }
    let runtime = build(&chain, &registry);
    let silence = vec![0.0_f32; FRAMES * CHANNELS];
    let loud_return = vec![0.9_f32; FRAMES * CHANNELS];
    let mut tail = vec![0.0_f32; FRAMES * CHANNELS];
    let mut peak = 0.0_f32;
    for _ in 0..256 {
        process_input_f32(&runtime, 0, &silence, CHANNELS);
        process_input_f32(&runtime, 1, &loud_return, CHANNELS);
        tail.fill(0.0);
        process_output_f32(&runtime, 0, &mut tail, CHANNELS);
        peak = peak.max(tail.iter().fold(0.0_f32, |m, s| m.max(s.abs())));
    }
    assert!(
        peak < 1e-4,
        "the disabled loop's return was processed as a guitar sibling (peak {peak})"
    );
}

/// The disabled loop's return stream (on its own interface) has nothing to
/// process, so its callback must not even take the runtime's processing lock:
/// every take of that lock is a period the guitar's own callback may lose to
/// `try_lock` — crackle on a chain whose loop is switched off. Taking the lock
/// is observable: the callback that holds it drains the queued block toggles.
#[test]
fn the_return_of_a_disabled_insert_does_not_take_the_audio_lock() {
    const CHANNELS: usize = 2;
    let mut chain = insert_chain();
    chain
        .blocks
        .retain(|b| !matches!(b.kind, AudioBlockKind::Core(_)));
    for block in chain.blocks.iter_mut() {
        if block.id.0 == INSERT {
            block.enabled = false;
        }
    }
    let runtime = build(&chain, &insert_registry());
    crate::runtime::set_block_enabled(&runtime, &BlockId("output:0".into()), true)
        .expect("queue a toggle");
    process_input_f32(&runtime, 1, &vec![0.9_f32; FRAMES * CHANNELS], CHANNELS);
    assert_eq!(
        runtime.pending_block_toggles.len(),
        1,
        "the idle return callback took the processing lock (it drained the toggle queue)"
    );
}

/// Two loops, the first switched off: the chain is cut only at the second, and
/// the cut must use the SECOND loop's send and return — the first one's are
/// still reserved in the numbering, so counting only the cutting inserts would
/// send the guitar into the wrong loop.
#[test]
fn a_cut_after_a_disabled_insert_uses_its_own_send_and_return() {
    let ep = |name: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels,
    };
    let registry = vec![
        IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![ep("in", vec![0])],
            outputs: vec![ep("out", vec![0])],
        },
        IoBinding {
            id: "fx-a".into(),
            name: "A".into(),
            inputs: vec![ep("ret", vec![2])],
            outputs: vec![ep("snd", vec![2])],
        },
        IoBinding {
            id: "fx-b".into(),
            name: "B".into(),
            inputs: vec![ep("ret", vec![3])],
            outputs: vec![ep("snd", vec![3])],
        },
    ];
    let insert = |id: &str, io: &str, enabled: bool| AudioBlock {
        id: BlockId(id.into()),
        enabled,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".into(),
            io: io.into(),
        }),
    };
    let chain = Chain {
        id: ChainId("two-loops".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![insert("a", "fx-a", false), insert("b", "fx-b", true)],
        di_output: None,
        loopers: vec![],
    };
    let (ins, outs) = resolve_chain_io(&chain, &registry);
    let (eff_in, cpal, split, groups) = effective_inputs(&chain, &ins, &registry);
    let eff_out = effective_outputs(&chain, &outs, &registry);
    let segments = crate::runtime_segments::split_chain_into_segments(
        &chain, &eff_in, &cpal, &split, &groups, &eff_out, &registry,
    );

    assert_eq!(segments.len(), 2, "cut once, at B");
    let send = segments[0].output_route_indices.clone();
    assert_eq!(
        send.iter()
            .map(|&r| eff_out[r].channels.clone())
            .collect::<Vec<_>>(),
        vec![vec![3]],
        "the guitar goes out B's send, not A's"
    );
    assert_eq!(
        segments[1].input.channels,
        vec![3],
        "and comes back on B's return"
    );
}

/// An E/S that has only an input so far (its output not added yet) plus a loop
/// switched off: the chain has no output of its own. The loop's reserved send
/// is NOT an output of the chain — sending the processed guitar out a loop the
/// user switched off would feed the gear anyway.
#[test]
fn a_chain_with_no_output_never_plays_out_a_disabled_inserts_send() {
    let ep = |name: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels,
    };
    let registry = vec![
        IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![ep("in", vec![0])],
            outputs: vec![],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![ep("ret", vec![2])],
            outputs: vec![ep("snd", vec![2])],
        },
    ];
    let chain = Chain {
        id: ChainId("input-only".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![AudioBlock {
            id: BlockId(INSERT.into()),
            enabled: false,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "fx".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };
    let (ins, outs) = resolve_chain_io(&chain, &registry);
    let (eff_in, cpal, split, groups) = effective_inputs(&chain, &ins, &registry);
    let eff_out = effective_outputs(&chain, &outs, &registry);
    let segments = crate::runtime_segments::split_chain_into_segments(
        &chain, &eff_in, &cpal, &split, &groups, &eff_out, &registry,
    );
    let send_route = eff_out
        .iter()
        .position(|o| o.channels == vec![2])
        .expect("the send is reserved");

    assert!(
        segments
            .iter()
            .all(|s| !s.output_route_indices.contains(&send_route)),
        "the switched-off loop's send carries the chain: {:?}",
        segments
            .iter()
            .map(|s| s.output_route_indices.clone())
            .collect::<Vec<_>>()
    );
}

/// A VST3 chain is updated IN PLACE (#779). Switching its loop ON makes the
/// return write a tail nothing wrote before — an output-only E/S on another
/// interface running at another rate. That route must be built at ITS
/// device's rate (and resampled into), not at the runtime's: at the wrong
/// rate a 48 kHz device drains a 44.1 kHz feed and underruns without end.
#[test]
fn an_in_place_update_builds_a_new_tail_at_its_devices_rate() {
    let ep = |name: &str, dev: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode: ChannelMode::Mono,
        channels,
    };
    let registry = vec![
        IoBinding {
            id: "a".into(),
            name: "A".into(),
            inputs: vec![ep("in", "scarlett", vec![0])],
            outputs: vec![ep("out", "scarlett", vec![0])],
        },
        IoBinding {
            id: "b".into(),
            name: "B".into(),
            inputs: vec![],
            outputs: vec![ep("out", "teyun", vec![0])],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![ep("ret", "scarlett", vec![3])],
            outputs: vec![ep("snd", "scarlett", vec![3])],
        },
    ];
    let chain = |loop_on: bool| Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["a".into(), "b".into()],
        blocks: vec![AudioBlock {
            id: BlockId(INSERT.into()),
            enabled: loop_on,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "fx".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };
    let rates: std::collections::HashMap<DeviceId, f32> = [
        (DeviceId("scarlett".into()), 44_100.0),
        (DeviceId("teyun".into()), 48_000.0),
    ]
    .into_iter()
    .collect();
    let runtime = Arc::new(
        crate::runtime_graph::build_chain_runtime_state_with_device_rates(
            &chain(false),
            44_100.0,
            &rates,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("builds"),
    );
    assert!(!runtime.writes_output(1), "loop off: nothing writes B yet");

    crate::runtime::update_chain_runtime_state_at_device_rates(
        &runtime,
        &chain(true),
        &rates,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &registry,
    )
    .expect("the in-place update applies");

    let routes = runtime.output_routes.load();
    let b = routes[1]
        .as_ref()
        .expect("loop on: the return writes B's tail");
    assert_eq!(
        b.sample_rate, 48_000.0,
        "#967: B's new route runs at the TEYUN's rate"
    );
}

/// The loop on a VST3 chain switched ON, OFF and ON again: each ON builds B's
/// cross-rate route fresh. The resampler the callback cached for the route it
/// fed before must not survive into the new one — its history would put a few
/// samples of the audio from when the loop went off in front of the new
/// signal.
#[test]
fn a_route_rebuilt_in_place_starts_with_a_fresh_resampler() {
    let ep = |name: &str, dev: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode: ChannelMode::Mono,
        channels,
    };
    let registry = vec![
        IoBinding {
            id: "a".into(),
            name: "A".into(),
            inputs: vec![ep("in", "scarlett", vec![0])],
            outputs: vec![ep("out", "scarlett", vec![0])],
        },
        IoBinding {
            id: "b".into(),
            name: "B".into(),
            inputs: vec![],
            outputs: vec![ep("out", "teyun", vec![0])],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![ep("ret", "scarlett", vec![3])],
            outputs: vec![ep("snd", "scarlett", vec![3])],
        },
    ];
    let chain = |loop_on: bool| Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["a".into(), "b".into()],
        blocks: vec![AudioBlock {
            id: BlockId(INSERT.into()),
            enabled: loop_on,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "fx".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };
    let rates: std::collections::HashMap<DeviceId, f32> = [
        (DeviceId("scarlett".into()), 44_100.0),
        (DeviceId("teyun".into()), 48_000.0),
    ]
    .into_iter()
    .collect();
    let runtime = Arc::new(
        crate::runtime_graph::build_chain_runtime_state_with_device_rates(
            &chain(true),
            44_100.0,
            &rates,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("builds"),
    );
    const CHANNELS: usize = 4;
    let play = |runtime: &Arc<ChainRuntimeState>| {
        let input = vec![0.3_f32; FRAMES * CHANNELS];
        for _ in 0..8 {
            process_input_f32(runtime, 0, &input, CHANNELS);
        }
    };
    play(&runtime);
    let update = |loop_on: bool| {
        crate::runtime::update_chain_runtime_state_at_device_rates(
            &runtime,
            &chain(loop_on),
            &rates,
            false,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("the in-place update applies");
    };
    update(false);
    play(&runtime);
    update(true);

    let processing = runtime.processing.lock().expect("processing lock");
    assert!(
        processing
            .input_scratches
            .iter()
            .all(|scratch| !scratch.route_resamplers.contains_key(&1)),
        "#967: the rebuilt route 1 inherited the old route's resampler history"
    );
}
