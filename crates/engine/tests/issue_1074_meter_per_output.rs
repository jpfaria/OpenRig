//! #1074 — one jack is one pipeline fanning out to every output, so the meter
//! row can no longer read one level per output off a per-output pipeline. Each
//! output route reports the level it actually played — after the chain volume
//! and its own faders — and the labels list every output of a stream, so the
//! GUI shows the input once and one meter per output.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::mixer_strip::MixerDirection;
use engine::mixer_gains::set_endpoint_gain;
use engine::output_meter::SILENT_DBFS;
use engine::runtime::{build_chain_runtime_state, process_input_f32, process_output_f32};
use engine::stream_io_labels::{chain_stream_io_labels, OutputIoLabel};
use project::chain::Chain;

const SR: f32 = 48_000.0;
const FRAMES: usize = 64;
const DEVICE: &str = "dev:1074-route-meter";

fn ep(name: &str, device: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn chain(id: &str, bindings: &[&str]) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: bindings.iter().map(|b| b.to_string()).collect(),
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

fn db(linear: f32) -> f32 {
    20.0 * linear.log10()
}

#[test]
fn each_output_route_meters_the_level_its_own_fader_plays() {
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![ep("in", DEVICE, ChannelMode::Mono, &[0])],
        outputs: vec![
            ep("main", DEVICE, ChannelMode::Mono, &[0]),
            ep("frfr", DEVICE, ChannelMode::Mono, &[1]),
        ],
    }];
    set_endpoint_gain(MixerDirection::Output, DEVICE, &[1], 0.25);
    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(
            &chain("chain:1074:route-meter", &["io"]),
            SR,
            &[FRAMES, FRAMES],
            &registry,
        )
        .expect("a pass-through chain builds"),
    );
    let input = vec![0.5_f32; FRAMES];
    let mut out = vec![0.0_f32; FRAMES];
    for _ in 0..32 {
        process_input_f32(&runtime, 0, &input, 1);
        process_output_f32(&runtime, 0, &mut out, 1);
        process_output_f32(&runtime, 1, &mut out, 1);
    }

    // The route accounting (`openrig://routes`) is its own reader: draining
    // it must not empty the meter.
    let _ = runtime.take_output_route_stats();
    let main = runtime.take_route_meter_dbfs(0).expect("route 0 is owned");
    let frfr = runtime.take_route_meter_dbfs(1).expect("route 1 is owned");

    assert!(
        (main - db(0.5)).abs() < 0.1,
        "MAIN plays at unity: {main} dBFS"
    );
    assert!(
        (frfr - db(0.125)).abs() < 0.1,
        "FRFR plays 12 dB down through its own fader: {frfr} dBFS"
    );
    assert_eq!(
        runtime.take_route_meter_dbfs(0),
        Some(SILENT_DBFS),
        "a read consumes the window"
    );
    assert_eq!(
        runtime.take_route_meter_dbfs(9),
        None,
        "a route the runtime does not own has no meter"
    );
}

#[test]
fn a_stream_lists_every_output_it_feeds() {
    let device = "coreaudio:quantum";
    let binding = |id: &str, name: &str, out: &[usize]| IoBinding {
        id: id.into(),
        name: name.into(),
        inputs: vec![ep("in", device, ChannelMode::Mono, &[0])],
        outputs: vec![ep("out", device, ChannelMode::Stereo, out)],
    };
    let registry = vec![
        binding("guitarra-1", "GUITARRA 1 - MAIN", &[0, 1]),
        binding("guitarra-1-5050", "GUITARRA 1 - SYN5050", &[16, 17]),
    ];

    let labels = chain_stream_io_labels(
        &chain("rig:input-2", &["guitarra-1", "guitarra-1-5050"]),
        &registry,
    );

    assert_eq!(labels.len(), 1, "one jack, one stream");
    assert_eq!(
        labels[0].outputs,
        vec![
            OutputIoLabel {
                route: 0,
                name: "GUITARRA 1 - MAIN".into(),
                channels: "1,2".into(),
                device: device.into(),
            },
            OutputIoLabel {
                route: 1,
                name: "GUITARRA 1 - SYN5050".into(),
                channels: "17,18".into(),
                device: device.into(),
            },
        ]
    );
}
