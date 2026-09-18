//! #923 — per-output-route accounting, read off the audio thread.
//!
//! The chain's meters say what each SEGMENT produced, not what each output
//! DEVICE stream actually pulled. When one tail output plays and its sibling
//! stays silent, the missing number is "did that route's stream ever pop, and
//! what did it carry?". These pin the counters the output callback feeds.

use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;

use crate::output_meter::SILENT_DBFS;
use crate::runtime::{
    build_chain_runtime_state, process_input_f32, process_output_f32, DEFAULT_ELASTIC_TARGET,
};
use crate::runtime_state::ChainRuntimeState;

const DEVICE_CHANNELS: usize = 4;
const FRAMES: usize = 64;

/// One input feeding TWO tail outputs on the same device: routes 0 and 1.
fn two_route_runtime() -> Arc<ChainRuntimeState> {
    let dev = || DeviceId("dev".into());
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: dev(),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![
            IoEndpoint {
                name: "main".into(),
                device_id: dev(),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            },
            IoEndpoint {
                name: "aux".into(),
                device_id: dev(),
                mode: ChannelMode::Stereo,
                channels: vec![2, 3],
            },
        ],
    }];
    let chain = Chain {
        id: ChainId("t".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    };
    Arc::new(
        build_chain_runtime_state(
            &chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET, DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("the chain must build"),
    )
}

fn feed(rt: &Arc<ChainRuntimeState>, callbacks: usize) {
    let mut input = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
    for frame in input.chunks_exact_mut(DEVICE_CHANNELS) {
        frame[0] = 0.5;
    }
    for _ in 0..callbacks {
        process_input_f32(rt, 0, &input, DEVICE_CHANNELS);
    }
}

#[test]
fn a_fresh_runtime_reports_one_silent_row_per_route() {
    let rt = two_route_runtime();
    let stats = rt.take_output_route_stats();
    assert_eq!(stats.len(), 2, "one row per output route");
    assert_eq!(stats[0].channels, vec![0, 1]);
    assert_eq!(stats[1].channels, vec![2, 3]);
    for s in &stats {
        assert_eq!(s.callbacks, 0);
        assert_eq!(s.peak_dbfs, SILENT_DBFS);
    }
}

#[test]
fn only_the_route_whose_stream_popped_counts_a_callback() {
    let rt = two_route_runtime();
    // Past the fade-in so the popped frames carry level.
    feed(&rt, 64);
    let mut out = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
    for _ in 0..40 {
        process_output_f32(&rt, 1, &mut out, DEVICE_CHANNELS);
    }
    let stats = rt.take_output_route_stats();
    assert_eq!(stats[0].callbacks, 0, "route 0's stream never ran");
    assert_eq!(stats[0].peak_dbfs, SILENT_DBFS);
    assert_eq!(stats[1].callbacks, 40, "route 1's stream ran 40 times");
    assert!(
        stats[1].peak_dbfs > -20.0,
        "route 1 carried the guitar — peak was {} dBFS",
        stats[1].peak_dbfs
    );
}

#[test]
fn the_peak_is_since_the_last_read_and_the_callback_count_is_cumulative() {
    let rt = two_route_runtime();
    feed(&rt, 64);
    let mut out = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
    process_output_f32(&rt, 0, &mut out, DEVICE_CHANNELS);
    let first = rt.take_output_route_stats();
    assert!(first[0].peak_dbfs > -20.0);

    let second = rt.take_output_route_stats();
    assert_eq!(second[0].callbacks, 1, "callbacks accumulate");
    assert_eq!(
        second[0].peak_dbfs, SILENT_DBFS,
        "nothing popped since the last read"
    );
}

#[test]
fn a_starved_route_reports_its_underruns() {
    let rt = two_route_runtime();
    let mut out = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
    // No input fed: every pop past the primed cushion underruns.
    for _ in 0..64 {
        process_output_f32(&rt, 0, &mut out, DEVICE_CHANNELS);
    }
    let stats = rt.take_output_route_stats();
    assert!(stats[0].underruns > 0, "route 0 drained empty");
    assert_eq!(stats[1].underruns, 0, "route 1 was never popped");
}

#[test]
fn each_route_reports_the_frames_queued_in_its_own_cushion() {
    let rt = two_route_runtime();
    feed(&rt, 2);
    let mut out = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
    process_output_f32(&rt, 1, &mut out, DEVICE_CHANNELS);
    let stats = rt.take_output_route_stats();
    assert_eq!(stats[0].fill_frames, 2 * FRAMES, "route 0 was never popped");
    assert_eq!(stats[1].fill_frames, FRAMES, "route 1 popped one period");
    assert_eq!(stats[0].latency_trims, 0);
}
