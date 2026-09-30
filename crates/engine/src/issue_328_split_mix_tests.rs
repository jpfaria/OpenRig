//! #328 end to end: a Split → Mix chain built by the engine and driven
//! through the real input/output callbacks (spec §4.1, §4.3, §6, §7).

#![allow(unused_imports)]
use super::volume_invariants::*;
use super::{update_chain_runtime_state, ChainRuntimeState};

use domain::io_binding::ChannelMode;
use project::block::split_params::{self, default_split_params};
use project::block::{SplitBlock, SplitEnd};

use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_state::{FadeState, RuntimeProcessor};

pub(super) fn volume_block(id: &str, pct: f32) -> AudioBlock {
    let mut params = neutral_params("gain", "volume");
    params.insert("volume", ParameterValue::Float(pct));
    core_block(id, "gain", "volume", params)
}

pub(super) fn split_block(
    id: &str,
    end: SplitEnd,
    knobs: &[(&str, ParameterValue)],
    a: Vec<AudioBlock>,
    b: Vec<AudioBlock>,
) -> AudioBlock {
    let mut params = default_split_params();
    for (key, value) in knobs {
        params.insert(*key, value.clone());
    }
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock { end, params, a, b }),
    }
}

/// A `generic_ir` block whose response is a unit impulse: a pure delay of
/// one convolution partition (64 samples) with no other change.
pub(super) fn unit_impulse_ir_block(id: &str) -> AudioBlock {
    static PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    let path = PATH.get_or_init(|| {
        let path = std::env::temp_dir().join(format!(
            "openrig_issue_328_unit_impulse_{}.wav",
            std::process::id()
        ));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(&path, spec).expect("create unit-impulse IR");
        for sample in [1.0_f32, 0.0, 0.0, 0.0] {
            writer.write_sample(sample).expect("write IR sample");
        }
        writer.finalize().expect("finalize IR");
        path
    });
    let mut params = ParameterSet::default();
    params.insert(
        "file",
        ParameterValue::String(path.to_string_lossy().into_owned()),
    );
    core_block(id, "ir", "generic_ir", params)
}

/// Read the first live split node of the runtime's first segment.
pub(super) fn with_split<R>(
    runtime: &ChainRuntimeState,
    read: impl FnOnce(&SplitRuntimeState) -> R,
) -> R {
    let guard = runtime.processing.lock().expect("processing lock");
    let split = guard.input_states[0]
        .blocks
        .iter()
        .find_map(|node| match &node.processor {
            RuntimeProcessor::Split(split) => Some(split),
            _ => None,
        })
        .expect("the chain must run a live split node");
    read(split)
}

pub(super) fn sine_block(frames: usize, start: usize) -> Vec<f32> {
    (start..start + frames)
        .map(|n| 0.5 * (std::f32::consts::TAU * 1_000.0 * n as f32 / SR).sin())
        .collect()
}

fn mono_chain(id: &str, blocks: Vec<AudioBlock>) -> (Chain, Vec<IoBinding>) {
    chain_with_blocks(
        id,
        input_mono(vec![0]),
        blocks,
        output(ChannelMode::Stereo, vec![0, 1]),
    )
}

fn steady_sine_peak(runtime: &Arc<ChainRuntimeState>, callbacks: usize, skip: usize) -> f32 {
    let mut peak = 0.0_f32;
    for callback in 0..callbacks {
        let out = drive_and_capture(runtime, 1, &sine_block(256, callback * 256), 2);
        if callback >= skip {
            peak = peak.max(peak_abs(&out));
        }
    }
    peak
}

fn invert() -> (&'static str, ParameterValue) {
    (
        split_params::MIX_B_POLARITY,
        ParameterValue::String(split_params::POLARITY_INVERT.into()),
    )
}

#[test]
fn a_new_split_with_empty_paths_passes_the_signal_at_unity() {
    let (chain, registry) = mono_chain(
        "empty",
        vec![split_block("split", SplitEnd::Mix, &[], vec![], vec![])],
    );
    with_split(&build_runtime(&chain, &registry), |_| ());
    let peaks = measure_steady_per_channel_peak(&chain, &registry, 1, &[0.5], 2, 4);
    assert!(
        (peaks[0] - 0.5).abs() < TOLERANCE && (peaks[1] - 0.5).abs() < TOLERANCE,
        "an empty split must pass the bus at unity, got {peaks:?}"
    );
}

#[test]
fn identical_paths_at_default_knobs_keep_the_level_of_one_path() {
    let (chain, registry) = mono_chain(
        "unity",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![volume_block("a_vol", 50.0)],
            vec![volume_block("b_vol", 50.0)],
        )],
    );
    let (reference, ref_registry) = mono_chain("unity_ref", vec![volume_block("ref_vol", 50.0)]);
    with_split(&build_runtime(&chain, &registry), |_| ());
    let got = measure_steady_per_channel_peak(&chain, &registry, 1, &[0.5], 2, 4);
    let want = measure_steady_per_channel_peak(&reference, &ref_registry, 1, &[0.5], 2, 4);
    assert!(
        (got[0] - want[0]).abs() < TOLERANCE && (got[1] - want[1]).abs() < TOLERANCE,
        "two identical paths at the default mixer must sound like one path: {got:?} vs {want:?}"
    );
}

#[test]
fn dual_amp_hard_left_right_keeps_each_amp_on_its_side() {
    let (chain, registry) = mono_chain(
        "dual_amp",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[
                (split_params::MIX_PAN_A, ParameterValue::Float(-50.0)),
                (split_params::MIX_PAN_B, ParameterValue::Float(50.0)),
                (split_params::MIX_MASTER, ParameterValue::Float(100.0)),
            ],
            vec![volume_block("amp_a", 100.0)],
            vec![volume_block("amp_b", 50.0)],
        )],
    );
    let (reference, ref_registry) = mono_chain("dual_amp_ref", vec![volume_block("ref_b", 50.0)]);
    with_split(&build_runtime(&chain, &registry), |_| ());
    let peaks = measure_steady_per_channel_peak(&chain, &registry, 1, &[0.5], 2, 4);
    let amp_b = measure_steady_per_channel_peak(&reference, &ref_registry, 1, &[0.5], 2, 4)[1];
    assert!(
        (peaks[0] - 0.5).abs() < TOLERANCE,
        "L carries amp A only, got {}",
        peaks[0]
    );
    assert!(
        (peaks[1] - amp_b).abs() < TOLERANCE,
        "R carries amp B only, got {} want {amp_b}",
        peaks[1]
    );
}

#[test]
fn inverted_path_b_cancels_an_identical_path_a() {
    let (chain, registry) = mono_chain(
        "polarity",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[invert()],
            vec![volume_block("a_vol", 100.0)],
            vec![volume_block("b_vol", 100.0)],
        )],
    );
    with_split(&build_runtime(&chain, &registry), |_| ());
    let peak = measure_steady_peak(&chain, &registry, 1, &[0.5], 2, 4);
    assert!(
        peak < 1e-6,
        "identical paths with B inverted cancel, peak {peak}"
    );
}

#[test]
fn mode_ii_feeds_each_path_the_channel_its_balance_picks() {
    let (chain, registry) = chain_with_blocks(
        "mode_ii",
        input_stereo(vec![0, 1]),
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[
                (
                    split_params::SPLIT_MODE,
                    ParameterValue::String(split_params::SPLIT_MODE_DUAL_MONO.into()),
                ),
                (split_params::BALANCE_A, ParameterValue::Float(50.0)),
                (split_params::BALANCE_B, ParameterValue::Float(-50.0)),
                (split_params::MIX_PAN_A, ParameterValue::Float(-50.0)),
                (split_params::MIX_PAN_B, ParameterValue::Float(50.0)),
                (split_params::MIX_MASTER, ParameterValue::Float(100.0)),
            ],
            vec![],
            vec![],
        )],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    with_split(&build_runtime(&chain, &registry), |_| ());
    let peaks = measure_steady_per_channel_peak(&chain, &registry, 2, &[0.9, 0.1], 2, 4);
    assert!(
        (peaks[0] - 0.1).abs() < TOLERANCE && (peaks[1] - 0.9).abs() < TOLERANCE,
        "path A takes R onto L, path B takes L onto R: {peaks:?}"
    );
}

#[test]
fn ir_in_path_a_is_aligned_against_a_dry_path_b() {
    let (cancel, registry) = mono_chain(
        "align",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[invert()],
            vec![unit_impulse_ir_block("a_ir")],
            vec![],
        )],
    );
    let runtime = build_runtime(&cancel, &registry);
    with_split(&runtime, |split| {
        assert_eq!(split.align_b.delay(), 64, "the dry path waits for the cab")
    });
    let peak = steady_sine_peak(&runtime, 24, 8);
    assert!(
        peak < 1e-4,
        "an aligned dry path cancels the cab path, peak {peak}"
    );

    let (sum, sum_registry) = mono_chain(
        "align_sum",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![unit_impulse_ir_block("a_ir")],
            vec![],
        )],
    );
    let peak = steady_sine_peak(&build_runtime(&sum, &sum_registry), 24, 8);
    assert!(
        peak > 0.4,
        "without the inversion the signal flows, peak {peak}"
    );
}

#[test]
fn a_block_after_the_split_is_built_for_stereo_content() {
    let (chain, registry) = mono_chain(
        "post",
        vec![
            split_block(
                "split",
                SplitEnd::Mix,
                &[],
                vec![volume_block("a_vol", 100.0)],
                vec![],
            ),
            volume_block("post_vol", 100.0),
        ],
    );
    let runtime = build_runtime(&chain, &registry);
    let guard = runtime.processing.lock().expect("processing lock");
    let post = guard.input_states[0]
        .blocks
        .iter()
        .find(|node| node.block_id.0 == "post_vol")
        .expect("post-split block");
    assert!(
        !post.content_mono,
        "a split may pan its paths apart; what follows is stereo"
    );
}

#[test]
fn each_segment_builds_its_own_split() {
    let (chain, _) = mono_chain(
        "isolation",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![volume_block("a_vol", 100.0)],
            vec![],
        )],
    );
    let registry = vec![IoBinding {
        id: IO_BINDING_ID.into(),
        name: "IO".into(),
        inputs: vec![
            input_mono(vec![0]),
            IoEndpoint {
                name: "in1".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Mono,
                channels: vec![1],
            },
        ],
        outputs: vec![output(ChannelMode::Stereo, vec![0, 1])],
    }];
    let runtime = build_runtime(&chain, &registry);
    let guard = runtime.processing.lock().expect("processing lock");
    let buffers: Vec<*const crate::runtime_audio_frame::AudioFrame> = guard
        .input_states
        .iter()
        .filter_map(|state| {
            state.blocks.iter().find_map(|node| match &node.processor {
                RuntimeProcessor::Split(split) => Some(split.b_buf.as_ptr()),
                _ => None,
            })
        })
        .collect();
    assert_eq!(buffers.len(), 2, "every segment runs its own split");
    assert_ne!(
        buffers[0], buffers[1],
        "no path buffer is shared between segments"
    );
}

#[test]
fn disabling_the_split_fades_it_out_through_its_paths() {
    let (chain, registry) = mono_chain(
        "disable",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![volume_block("a_vol", 100.0)],
            vec![],
        )],
    );
    let runtime = build_runtime(&chain, &registry);
    let mut off = chain.clone();
    off.blocks[0].enabled = false;
    update_chain_runtime_state(
        &runtime,
        &off,
        SR,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &registry,
    )
    .expect("in-place update");
    let guard = runtime.processing.lock().expect("processing lock");
    let node = &guard.input_states[0].blocks[0];
    assert!(
        matches!(node.fade_state, FadeState::FadingOut { .. }),
        "got {:?}",
        node.fade_state
    );
    assert!(
        matches!(&node.processor, RuntimeProcessor::Split(split) if split.a.len() == 1),
        "the fade-out runs through the real paths"
    );
}

#[test]
fn the_offline_render_plays_the_split() {
    let (chain, _) = mono_chain(
        "offline",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[invert()],
            vec![volume_block("a_vol", 100.0)],
            vec![volume_block("b_vol", 100.0)],
        )],
    );
    let input: Vec<[f32; 2]> = sine_block(1024, 0).into_iter().map(|s| [s, s]).collect();
    let outcome = crate::offline::render_chain(&chain, SR, &input, 64, 0).expect("offline render");
    let loudest = outcome
        .samples
        .iter()
        .skip(256)
        .map(|[l, r]| l.abs().max(r.abs()))
        .fold(0.0_f32, f32::max);
    assert!(
        loudest < 1e-6,
        "the offline render runs both paths and the mixer, peak {loudest}"
    );
}

#[test]
fn the_tone_doctor_hears_the_split() {
    let (chain, _) = mono_chain(
        "doctor",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[invert()],
            vec![volume_block("a_vol", 100.0)],
            vec![volume_block("b_vol", 100.0)],
        )],
    );
    let input: Vec<[f32; 2]> = sine_block(48_000, 0).into_iter().map(|s| [s, s]).collect();
    let diagnosis = crate::tone_doctor::diagnose(&chain, SR, &input, 256).expect("diagnosis");
    assert!(
        diagnosis.full_descriptors.rms_dbfs < -90.0,
        "the tone doctor renders the split (here: cancelled), rms {} dBFS",
        diagnosis.full_descriptors.rms_dbfs
    );
}

pub(super) fn serial_of(runtime: &ChainRuntimeState, id: &str) -> u64 {
    let guard = runtime.processing.lock().expect("processing lock");
    for node in &guard.input_states[0].blocks {
        if node.block_id.0 == id {
            return node.instance_serial;
        }
        if let RuntimeProcessor::Split(split) = &node.processor {
            for path_node in split.a.iter().chain(split.b.iter()) {
                if path_node.block_id.0 == id {
                    return path_node.instance_serial;
                }
            }
        }
    }
    panic!("block {id} is not in the runtime")
}

pub(super) fn with_split_knob(chain: &Chain, key: &str, value: ParameterValue) -> Chain {
    let mut edited = chain.clone();
    for block in edited.blocks.iter_mut() {
        if let AudioBlockKind::Split(split) = &mut block.kind {
            split.params.insert(key, value.clone());
        }
    }
    edited
}

fn update(runtime: &Arc<ChainRuntimeState>, chain: &Chain, registry: &[IoBinding]) {
    update_chain_runtime_state(
        runtime,
        chain,
        SR,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        registry,
    )
    .expect("in-place update");
}

#[test]
fn a_mixer_knob_edit_keeps_the_path_processors() {
    let (chain, registry) = mono_chain(
        "knob_edit",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![volume_block("amp_a", 100.0)],
            vec![volume_block("amp_b", 100.0)],
        )],
    );
    let runtime = build_runtime(&chain, &registry);
    let before = (serial_of(&runtime, "amp_a"), serial_of(&runtime, "amp_b"));
    update(
        &runtime,
        &with_split_knob(
            &chain,
            split_params::MIX_PAN_A,
            ParameterValue::Float(-50.0),
        ),
        &registry,
    );
    assert_eq!(
        (serial_of(&runtime, "amp_a"), serial_of(&runtime, "amp_b")),
        before,
        "a knob move must not rebuild the amps in the paths"
    );
    with_split(&runtime, |split| {
        assert_eq!(
            split.knobs.load(true).mix_pan_a,
            -50.0,
            "the new knob value applies"
        )
    });
}

#[test]
fn moving_a_block_from_path_a_to_path_b_keeps_its_processor() {
    let (chain, registry) = mono_chain(
        "lane_drag",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![volume_block("a1", 100.0), volume_block("a2", 80.0)],
            vec![volume_block("b1", 100.0)],
        )],
    );
    let runtime = build_runtime(&chain, &registry);
    let before = serial_of(&runtime, "a2");
    let mut moved = chain.clone();
    if let AudioBlockKind::Split(split) = &mut moved.blocks[0].kind {
        let dragged = split.a.remove(1);
        split.b.push(dragged);
    }
    update(&runtime, &moved, &registry);
    assert_eq!(
        serial_of(&runtime, "a2"),
        before,
        "dragging across lanes keeps the processor"
    );
}

#[test]
fn moving_a_block_out_of_a_path_keeps_its_processor() {
    let (chain, registry) = mono_chain(
        "to_shared",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[],
            vec![volume_block("a1", 100.0)],
            vec![volume_block("b1", 100.0)],
        )],
    );
    let runtime = build_runtime(&chain, &registry);
    let before = serial_of(&runtime, "a1");
    let mut moved = chain.clone();
    let dragged = match &mut moved.blocks[0].kind {
        AudioBlockKind::Split(split) => split.a.remove(0),
        _ => unreachable!("block 0 is the split"),
    };
    moved.blocks.insert(0, dragged);
    update(&runtime, &moved, &registry);
    assert_eq!(
        serial_of(&runtime, "a1"),
        before,
        "dragging to the shared blocks keeps the processor"
    );
}

#[test]
fn a_knob_edit_keeps_the_alignment_history() {
    let (chain, registry) = mono_chain(
        "history",
        vec![split_block(
            "split",
            SplitEnd::Mix,
            &[invert()],
            vec![unit_impulse_ir_block("a_ir")],
            vec![],
        )],
    );
    let runtime = build_runtime(&chain, &registry);
    let mut callback = 0;
    let mut before = 0.0_f32;
    for _ in 0..12 {
        let out = drive_and_capture(&runtime, 1, &sine_block(256, callback * 256), 2);
        if callback >= 8 {
            before = before.max(peak_abs(&out));
        }
        callback += 1;
    }
    assert!(
        before < 1e-4,
        "precondition: the aligned paths cancel, peak {before}"
    );
    update(
        &runtime,
        &with_split_knob(&chain, split_params::MIX_MASTER, ParameterValue::Float(40.0)),
        &registry,
    );
    let mut after = 0.0_f32;
    for _ in 0..8 {
        let out = drive_and_capture(&runtime, 1, &sine_block(256, callback * 256), 2);
        after = after.max(peak_abs(&out));
        callback += 1;
    }
    assert!(
        after < 1e-4,
        "a knob edit must not restart the delayed path from silence, peak {after}"
    );
}
