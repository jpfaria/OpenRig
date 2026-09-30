//! #328 — what a Y → A/B chain sounds like on each output. Input 0.5 on a mono
//! channel; path A is a volume at 50 % (×0.5), path B a volume at 25 % (×0.25).
//! Path A feeds out-a and out-ab, path B feeds out-b and out-ab, so
//! out-a = 0.25, out-b = 0.125 and out-ab = 0.25 + 0.125 = 0.375 — each output
//! hears exactly its paths, and both paths on one output sum at unity.

use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use project::block::split_params::{
    default_split_params, LEVEL_TO_A, MIX_B_POLARITY, MIX_LEVEL_A, MIX_LEVEL_B, MIX_MASTER,
    MIX_MASTER_SUM, MIX_PAN_A,
};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd};
use project::chain::Chain;
use project::endpoint_disables::{EndpointDisables, EndpointRef};
use project::param::ParameterSet;

use crate::runtime::{
    build_chain_runtime_state, process_input_f32, process_output_f32, ChainRuntimeState,
    DEFAULT_ELASTIC_TARGET,
};
use crate::runtime_graph::update_chain_runtime_state;

const CHANNELS: usize = 6;
const FRAMES: usize = 64;
const INPUT: f32 = 0.5;
const TOLERANCE: f32 = 1e-3;

fn stereo_out(name: &str, channels: [usize; 2]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels: channels.to_vec(),
    }
}

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "main".into(),
        name: "MAIN".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![
            stereo_out("out-a", [0, 1]),
            stereo_out("out-b", [2, 3]),
            stereo_out("out-ab", [4, 5]),
        ],
    }]
}

fn volume(id: &str, pct: f32) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert("volume", ParameterValue::Float(pct));
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params,
        }),
    }
}

fn y_split(params: ParameterSet, enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Y,
            params,
            a: vec![volume("amp-a", 50.0)],
            b: vec![volume("amp-b", 25.0)],
        }),
    }
}

fn endpoint(name: &str) -> EndpointRef {
    EndpointRef {
        io: "main".into(),
        endpoint: name.into(),
    }
}

/// Path A → out-a + out-ab; path B → out-b + out-ab.
fn y_chain(split: AudioBlock) -> Chain {
    Chain {
        mix: Default::default(),
        id: ChainId("issue-328-y".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![split],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: EndpointDisables {
            inputs: vec![],
            outputs: vec![],
            path_a_outputs: vec![endpoint("out-b")],
            path_b_outputs: vec![endpoint("out-a")],
        },
    }
}

fn runtime(chain: &Chain) -> Arc<ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET; 3], &registry())
            .expect("the Y chain must build"),
    )
}

/// Left-channel peak of out-a, out-b and out-ab once the fades settled.
fn route_peaks(runtime: &Arc<ChainRuntimeState>) -> [f32; 3] {
    let mut input = vec![0.0_f32; FRAMES * CHANNELS];
    for frame in input.chunks_exact_mut(CHANNELS) {
        frame[0] = INPUT;
    }
    let mut out = vec![0.0_f32; FRAMES * CHANNELS];
    let mut peaks = [0.0_f32; 3];
    for callback in 0..256 {
        process_input_f32(runtime, 0, &input, CHANNELS);
        for (route, peak) in peaks.iter_mut().enumerate() {
            out.fill(0.0);
            process_output_f32(runtime, route, &mut out, CHANNELS);
            if callback >= 128 {
                for frame in out.chunks_exact(CHANNELS) {
                    *peak = peak.max(frame[route * 2].abs());
                }
            }
        }
    }
    peaks
}

fn assert_peaks(peaks: [f32; 3], expected: [f32; 3], why: &str) {
    for (route, (got, want)) in peaks.iter().zip(expected).enumerate() {
        assert!(
            (got - want).abs() < TOLERANCE,
            "{why} — route {route} (out-a, out-b, out-ab) got {got}, expected {want}; all: {peaks:?}"
        );
    }
}

#[test]
fn each_output_carries_only_the_paths_that_feed_it() {
    let runtime = runtime(&y_chain(y_split(default_split_params(), true)));
    assert_peaks(
        route_peaks(&runtime),
        [0.25, 0.125, 0.375],
        "#328: out-a hears path A only, out-b path B only, out-ab both at unity",
    );
}

#[test]
fn mixer_knobs_on_a_y_split_change_no_output() {
    let mut params = default_split_params();
    params.insert(MIX_LEVEL_A, ParameterValue::Float(0.0));
    params.insert(MIX_LEVEL_B, ParameterValue::Float(0.0));
    params.insert(MIX_PAN_A, ParameterValue::Float(50.0));
    params.insert(MIX_B_POLARITY, ParameterValue::String("invert".into()));
    params.insert(MIX_MASTER, ParameterValue::Float(10.0));
    params.insert(MIX_MASTER_SUM, ParameterValue::Bool(true));
    let runtime = runtime(&y_chain(y_split(params, true)));
    assert_peaks(
        route_peaks(&runtime),
        [0.25, 0.125, 0.375],
        "#328: Y has no mixer — mixer knobs set over MCP or a scene must not reach any output",
    );
}

#[test]
fn a_knob_edit_rebuilds_each_output_on_its_own_paths() {
    let chain = y_chain(y_split(default_split_params(), true));
    let runtime = runtime(&chain);
    let _ = route_peaks(&runtime);
    let mut params = default_split_params();
    params.insert(LEVEL_TO_A, ParameterValue::Float(50.0));
    let edited = y_chain(y_split(params, true));
    update_chain_runtime_state(
        &runtime,
        &edited,
        48_000.0,
        false,
        &[DEFAULT_ELASTIC_TARGET; 3],
        &registry(),
    )
    .expect("the in-place rebuild succeeds");
    assert_peaks(
        route_peaks(&runtime),
        [0.125, 0.125, 0.25],
        "#328: after a live edit each output still runs its own paths; level-to-A halves path A only",
    );
}

#[test]
fn a_bypassed_y_split_passes_the_shared_signal_once_to_every_output() {
    let runtime = runtime(&y_chain(y_split(default_split_params(), false)));
    assert_peaks(
        route_peaks(&runtime),
        [0.5, 0.5, 0.5],
        "#328: a bypassed split is a passthrough — out-ab must not get the signal twice",
    );
}

#[test]
fn an_offline_render_of_a_y_chain_plays_both_paths() {
    let chain = y_chain(y_split(default_split_params(), true));
    let input = vec![[INPUT, INPUT]; 4096];
    let outcome = crate::offline::render_chain(&chain, 48_000.0, &input, 64, 0)
        .expect("the offline render succeeds");
    let last = outcome.samples.last().copied().expect("rendered samples");
    assert!(
        (last[0] - 0.375).abs() < TOLERANCE,
        "#328: an offline render hears a Y chain as both paths summed at unity, got {last:?}"
    );
}

/// The owner's rig: Split → Mix of two amps, a shared block, then a Y whose
/// path A is a cab and whose path B is empty. Input 0.5; the Mix sums path A
/// at 50 % and path B at 100 % under its default master (×0.5):
/// (0.25 + 0.5) × 0.5 = 0.375; the shared volume at 80 % gives 0.3. Path A
/// (the cab at 50 %) feeds out-a = 0.15, path B feeds out-b = 0.3 (out-a is
/// 6 dB below), and out-ab hears both at unity = 0.45.
fn mix_then_y_chain(mix_params: ParameterSet) -> Chain {
    let y = AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Y,
            params: default_split_params(),
            a: vec![volume("cab", 50.0)],
            b: vec![],
        }),
    };
    let mix = AudioBlock {
        id: BlockId("mix".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            end: SplitEnd::Mix,
            params: mix_params,
            a: vec![volume("amp-1", 50.0)],
            b: vec![volume("amp-2", 100.0)],
        }),
    };
    let mut chain = y_chain(y);
    chain.blocks.insert(0, volume("shared", 80.0));
    chain.blocks.insert(0, mix);
    chain
}

#[test]
fn behind_a_mix_each_y_output_hears_the_mix_then_only_its_own_path() {
    let runtime = runtime(&mix_then_y_chain(default_split_params()));
    assert_peaks(
        route_peaks(&runtime),
        [0.15, 0.3, 0.45],
        "#328: every Y output hears the whole Mix, then only the Y paths it checks",
    );
}

#[test]
fn a_mix_knob_edit_reaches_every_y_output() {
    let runtime = runtime(&mix_then_y_chain(default_split_params()));
    let _ = route_peaks(&runtime);
    let mut params = default_split_params();
    params.insert(MIX_MASTER, ParameterValue::Float(100.0));
    update_chain_runtime_state(
        &runtime,
        &mix_then_y_chain(params),
        48_000.0,
        false,
        &[DEFAULT_ELASTIC_TARGET; 3],
        &registry(),
    )
    .expect("the in-place rebuild succeeds");
    assert_peaks(
        route_peaks(&runtime),
        [0.3, 0.6, 0.9],
        "#328: the Mix master doubled — each Y output, rebuilt live, doubles with it",
    );
}

/// A Mix is not routing, so switching it off takes the in-place toggle; every
/// Y output runs its own copy of the Mix, and every copy must go off.
#[test]
fn switching_the_mix_off_in_place_reaches_every_y_output() {
    let runtime = runtime(&mix_then_y_chain(default_split_params()));
    let _ = route_peaks(&runtime);
    crate::runtime::set_block_enabled(&runtime, &BlockId("mix".into()), false)
        .expect("the toggle is queued");
    assert_peaks(
        route_peaks(&runtime),
        [0.2, 0.4, 0.6],
        "#328: with the Mix off each output hears the input through the shared block and its own Y paths",
    );
}
