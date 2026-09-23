//! Golden tests for #454-T5 spillover. Deterministic — assert the
//! mechanism *contract* (state machine + SPSC-safe retention), not exact
//! DSP values: after a spillover switch the previous pipeline is retained
//! and decays to nothing over `SPILLOVER_FRAMES`, the non-spillover path is
//! byte-identical (no `outgoing`), and the audio output stays finite
//! (no click/NaN) across the whole window.

use super::{
    build_chain_runtime_state, process_input_f32, process_output_f32, update_chain_runtime_state,
    update_chain_runtime_state_spillover, ChainRuntimeState, DEFAULT_ELASTIC_TARGET,
};
use crate::runtime_state::{FADE_IN_FRAMES, SPILLOVER_FRAMES};
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use std::sync::Arc;

const SR: f32 = 48_000.0;

/// Per-machine registry mirroring the old head input (mono ch0) and tail
/// output (stereo ch0/1) device blocks the chain used to embed (#716). The
/// chain selects it via `io_binding_ids`.
fn registry() -> Vec<IoBinding> {
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

fn core(id: &str, effect_type: &str, model: &str) -> AudioBlock {
    let schema =
        schema_for_block_model(effect_type, model).expect("schema must exist for test model");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults must normalize");
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.to_string(),
            model: model.to_string(),
            params,
        }),
    }
}

fn chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: Some("spill".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

fn build(c: &Chain) -> Arc<ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(c, SR, &[DEFAULT_ELASTIC_TARGET], &registry())
            .expect("runtime builds"),
    )
}

fn drive_silence(rt: &Arc<ChainRuntimeState>, frames: usize) -> Vec<f32> {
    let data = vec![0.0_f32; frames]; // 1 input channel
    process_input_f32(rt, 0, &data, 1);
    let mut out = vec![0.0_f32; frames * 2];
    process_output_f32(rt, 0, &mut out, 2);
    out
}

fn outgoing_frames_remaining(rt: &Arc<ChainRuntimeState>) -> Option<usize> {
    let p = rt.processing.lock().expect("processing lock");
    p.input_states
        .first()
        .and_then(|is| is.outgoing.as_ref())
        .map(|t| t.frames_remaining)
}

fn delay_model() -> &'static str {
    block_delay::supported_models()
        .first()
        .expect("a delay model exists")
}

#[test]
fn spillover_retains_previous_pipeline_then_drops_it() {
    let a = chain(vec![core("d", "delay", delay_model())]);
    let rt = build(&a);
    // Warm the chain.
    let warm = vec![0.5_f32; 256];
    process_input_f32(&rt, 0, &warm, 1);

    // Switch preset (same I/O, different processing) WITH spillover.
    let b = chain(vec![]);
    update_chain_runtime_state_spillover(
        &rt,
        &b,
        SR,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &registry(),
    )
    .expect("spillover switch");

    // The previous pipeline is retained, full window pending.
    assert_eq!(
        outgoing_frames_remaining(&rt),
        Some(SPILLOVER_FRAMES),
        "old pipeline retained as a decaying tail"
    );

    // Drive silence: it must decay by exactly the callback frame count and
    // eventually be dropped — output stays finite the whole time.
    let mut last = SPILLOVER_FRAMES;
    let mut callbacks = 0;
    loop {
        let out = drive_silence(&rt, 256);
        assert!(
            out.iter().all(|s| s.is_finite()),
            "no NaN/inf during spillover"
        );
        callbacks += 1;
        match outgoing_frames_remaining(&rt) {
            Some(rem) => {
                assert!(rem < last, "tail must monotonically decay");
                assert_eq!(last - rem, 256, "decays by the callback frame count");
                last = rem;
            }
            None => break, // dropped
        }
        assert!(
            callbacks <= SPILLOVER_FRAMES / 256 + 2,
            "tail must terminate within the window"
        );
    }
    assert!(callbacks >= 1);
}

#[test]
fn non_spillover_switch_has_no_outgoing_byte_identical() {
    let a = chain(vec![core("d", "delay", delay_model())]);
    let rt = build(&a);
    let warm = vec![0.5_f32; 256];
    process_input_f32(&rt, 0, &warm, 1);

    let b = chain(vec![]);
    update_chain_runtime_state(&rt, &b, SR, false, &[DEFAULT_ELASTIC_TARGET], &registry())
        .expect("in-place switch");

    assert_eq!(
        outgoing_frames_remaining(&rt),
        None,
        "non-spillover path must NOT retain a tail (byte-identical)"
    );
}

/// `registry()` with the same E/S gaining a second stereo output on channels
/// 2/3: one input × two outputs = two pipelines (#85) where `registry()`
/// gives one.
fn registry_two_outputs() -> Vec<IoBinding> {
    let mut registry = registry();
    registry[0].outputs.push(IoEndpoint {
        name: "out1".into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels: vec![2, 3],
    });
    registry
}

/// Per pipeline: the instance serials of its live nodes, and of the previous
/// pipeline it is ringing out (`None` when it has none).
fn pipeline_serials(rt: &Arc<ChainRuntimeState>) -> Vec<(Vec<u64>, Option<Vec<u64>>)> {
    let p = rt.processing.lock().expect("processing lock");
    p.input_states
        .iter()
        .map(|is| {
            (
                is.blocks
                    .iter()
                    .map(|b| b.instance_serial)
                    .collect::<Vec<u64>>(),
                is.outgoing.as_ref().map(|tail| {
                    tail.blocks
                        .iter()
                        .map(|b| b.instance_serial)
                        .collect::<Vec<u64>>()
                }),
            )
        })
        .collect()
}

/// #454-T5: a spillover switch that GROWS the chain's pipelines (the E/S
/// gained a second output) gives the new pipeline a freshly built chain of
/// its own, with no previous pipeline to ring out, and it plays on its own
/// output. The old pipeline rings out exactly once, on the pipeline it came
/// from.
#[test]
fn spillover_that_adds_a_pipeline_builds_it_fresh_and_it_plays() {
    const FRAMES: usize = 128;
    const OUT_CHANNELS: usize = 4;
    const SECOND_OUT_LEFT: usize = 2;

    let c = chain(vec![core("d", "delay", delay_model())]);
    let rt = build(&c);
    let before = pipeline_serials(&rt);
    assert_eq!(before.len(), 1, "one input × one output = one pipeline");
    let old_delay = before[0].0.clone();
    assert_eq!(old_delay.len(), 1, "the pipeline runs the chain's delay");
    assert!(
        matches!(rt.output_routes.load().get(1), None | Some(None)),
        "before the switch there is no second output route"
    );

    update_chain_runtime_state_spillover(
        &rt,
        &c,
        SR,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &registry_two_outputs(),
    )
    .expect("spillover switch that grows the pipelines");

    let after = pipeline_serials(&rt);
    assert_eq!(after.len(), 2, "one input × two outputs = two pipelines");

    // The pipeline that existed: fresh nodes, its old delay ringing out.
    assert_eq!(
        after[0].1.as_deref(),
        Some(old_delay.as_slice()),
        "the old pipeline rings out on the pipeline it came from"
    );
    assert_eq!(outgoing_frames_remaining(&rt), Some(SPILLOVER_FRAMES));
    assert_ne!(
        after[0].0, old_delay,
        "spillover builds the surviving pipeline fresh"
    );

    // The pipeline that did not exist: its own fresh delay, nothing to ring out.
    assert_eq!(
        after[1].0.len(),
        1,
        "the new pipeline runs the chain's delay"
    );
    assert!(
        !after[1].0.contains(&old_delay[0]) && after[1].0 != after[0].0,
        "the new pipeline owns a freshly built node, not a reused one"
    );
    assert_eq!(
        after[1].1, None,
        "a pipeline that did not exist before has no previous pipeline to ring out"
    );
    {
        let p = rt.processing.lock().expect("processing lock");
        let grown = &p.input_states[1];
        assert_eq!(
            grown.output_route_indices,
            vec![1],
            "the new pipeline writes the new output"
        );
        assert_eq!(
            grown.fade_in_remaining, FADE_IN_FRAMES,
            "the new pipeline fades in, never starts hot"
        );
    }

    // And it plays: the guitar reaches the new output through it.
    let input = vec![0.5_f32; FRAMES]; // 1 input channel
    let mut out = vec![0.0_f32; FRAMES * OUT_CHANNELS];
    let mut peak = 0.0_f32;
    for callback in 0..32 {
        process_input_f32(&rt, 0, &input, 1);
        out.fill(0.0);
        process_output_f32(&rt, 1, &mut out, OUT_CHANNELS);
        assert!(
            out.iter().all(|s| s.is_finite()),
            "no NaN/inf on the new output"
        );
        if callback >= 4 {
            for frame in out.chunks_exact(OUT_CHANNELS) {
                peak = peak.max(frame[SECOND_OUT_LEFT].abs());
            }
        }
    }
    assert!(
        peak > 0.01,
        "the new pipeline must play on its own output — peak was {peak}"
    );
}
