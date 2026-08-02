//! #127: the GUI's subscription seam, driven against a device-less controller.
//!
//! These prove the three things the seam exists to guarantee:
//!
//! * a consumer that asks BY IDENTITY gets that stream's signal and no other
//!   (`CLAUDE.md` LAW);
//! * the reduced reading (`poll_peak_dbfs`) is the same number the meter
//!   computed for itself before the seam existed;
//! * the raw window is still available in-process, from the same ring, for the
//!   analyzers that run next to the audio.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use application::audio_taps::TapPoint;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;

use super::gui_audio_taps;

const CHAIN: &str = "chain:127:taps";

fn mono_registry() -> Vec<IoBinding> {
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
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
    }]
}

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

/// A controller with one live runtime per chain and no audio devices.
fn controller(chains: &[Chain]) -> Rc<RefCell<Option<ProjectRuntimeController>>> {
    let mut graph = HashMap::new();
    for c in chains {
        graph.insert(
            (c.id.clone(), 0usize),
            Arc::new(build_chain_runtime_state(c, 48_000.0, &[1024], &mono_registry()).unwrap()),
        );
    }
    let mut controller = ProjectRuntimeController::for_testing(RuntimeGraph { chains: graph });
    controller.set_io_bindings(mono_registry());
    Rc::new(RefCell::new(Some(controller)))
}

/// Push one input buffer through the chain's audio callback — the only way a
/// tap ever receives anything.
fn feed_input(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    chain: &ChainId,
    samples: &[f32],
    channels: usize,
) {
    let borrow = runtime.borrow();
    let controller = borrow.as_ref().expect("controller");
    for rt in controller.runtimes_for_chain(chain) {
        engine::runtime::process_input_f32(&rt, 0, samples, channels);
    }
}

#[test]
fn a_stream_input_subscription_reports_the_peak_the_meter_would_have_computed() {
    // The meter's whole need in one call: a finished number over the window
    // that arrived since the last tick. Before the seam the timer popped the
    // ring itself; the reading must be identical, or the bars change.
    let c = chain(CHAIN);
    let runtime = controller(std::slice::from_ref(&c));
    let taps = gui_audio_taps(&runtime);

    let tap = taps
        .subscribe(
            &TapPoint::StreamInput {
                chain: c.id.clone(),
                stream: 0,
            },
            256,
        )
        .expect("a hosted chain's stream 0 can always be tapped");

    feed_input(&runtime, &c.id, &[0.25, -0.5, 0.1], 1);

    let peak = tap.poll_peak_dbfs();
    let expected = 20.0 * 0.5_f32.log10();
    assert!(
        (peak - expected).abs() < 0.01,
        "the reduced reading must be the peak of the window, got {peak} want {expected}"
    );
    assert_eq!(
        tap.poll_peak_dbfs(),
        engine::output_meter::SILENT_DBFS,
        "the poll consumed the window: a second read reports silence, not the \
         previous peak"
    );
}

#[test]
fn a_subscription_carries_the_raw_window_in_process() {
    // The tuner / spectrum / Tone Doctor leg: the same ring the consumer used
    // to hold directly, reached through the seam. `max` is honoured because
    // the spectrum caps its per-tick drain.
    let c = chain(CHAIN);
    let runtime = controller(std::slice::from_ref(&c));
    let taps = gui_audio_taps(&runtime);
    let tap = taps
        .subscribe(
            &TapPoint::StreamInput {
                chain: c.id.clone(),
                stream: 0,
            },
            256,
        )
        .expect("a hosted chain's stream 0 can always be tapped");

    feed_input(&runtime, &c.id, &[0.1, 0.2, 0.3, 0.4], 1);

    let mut out = Vec::new();
    assert_eq!(tap.drain_channel(0, 2, &mut out), 2, "capped at max");
    assert_eq!(out, vec![0.1, 0.2]);
    assert_eq!(
        tap.drain_channel(0, 64, &mut out),
        2,
        "the rest of the window"
    );
    assert_eq!(out, vec![0.1, 0.2, 0.3, 0.4]);
}

#[test]
fn a_subscription_never_hears_another_chain() {
    // `CLAUDE.md` LAW. The tap is opened by identity, so a sibling chain
    // playing loudly must leave it at silence — no pooling, no "all matching".
    let mine = chain(CHAIN);
    let sibling = chain("chain:127:sibling");
    let runtime = controller(&[mine.clone(), sibling.clone()]);
    let taps = gui_audio_taps(&runtime);

    let tap = taps
        .subscribe(
            &TapPoint::StreamInput {
                chain: mine.id.clone(),
                stream: 0,
            },
            256,
        )
        .expect("a hosted chain's stream 0 can always be tapped");

    feed_input(&runtime, &sibling.id, &[1.0, -1.0, 1.0], 1);

    assert_eq!(
        tap.poll_peak_dbfs(),
        engine::output_meter::SILENT_DBFS,
        "the sibling's full-scale signal must not reach this chain's tap"
    );
    let mut out = Vec::new();
    assert_eq!(tap.drain_channel(0, 64, &mut out), 0);
}

#[test]
fn an_input_subscription_keeps_its_channels_apart() {
    // The tuner's shape: ONE subscription over the endpoint's channels (one
    // tap on the audio thread, not N), with one row per channel. Channel 0 and
    // channel 1 are different strings on different pickups — they must never
    // be merged.
    let c = chain(CHAIN);
    let runtime = controller(std::slice::from_ref(&c));
    let taps = gui_audio_taps(&runtime);

    let tap = taps
        .subscribe(
            &TapPoint::InputChannels {
                chain: c.id.clone(),
                input: 0,
                total_channels: 2,
                channels: vec![0, 1],
            },
            256,
        )
        .expect("a hosted chain's input 0 can always be tapped");
    assert_eq!(tap.channels(), 2, "one ring per subscribed channel");

    // Interleaved L/R: channel 0 gets 0.1/0.3, channel 1 gets 0.2/0.4.
    feed_input(&runtime, &c.id, &[0.1, 0.2, 0.3, 0.4], 2);

    let mut left = Vec::new();
    tap.drain_channel(0, 64, &mut left);
    let mut right = Vec::new();
    tap.drain_channel(1, 64, &mut right);
    assert_eq!(
        left,
        vec![0.1, 0.3],
        "channel 0 is the first device channel"
    );
    assert_eq!(right, vec![0.2, 0.4], "channel 1 is the second");
}

#[test]
fn a_stopped_rig_hosts_no_taps() {
    // With the controller dropped there is nothing to subscribe against, and
    // the consumers must be told so rather than handed a handle that will
    // never fill — that is what invalidates the meter store.
    let runtime = Rc::new(RefCell::new(None));
    let taps = gui_audio_taps(&runtime);

    assert!(!taps.is_hosted());
    assert_eq!(taps.live_sample_rate(), 0, "never a fabricated rate (#723)");
    assert_eq!(taps.stream_count(&ChainId(CHAIN.into())), 0);
    assert!(taps
        .subscribe(
            &TapPoint::StreamInput {
                chain: ChainId(CHAIN.into()),
                stream: 0,
            },
            256,
        )
        .is_none());
}

#[test]
fn a_hosted_chain_reports_its_own_stream_count() {
    let c = chain(CHAIN);
    let runtime = controller(std::slice::from_ref(&c));
    let taps = gui_audio_taps(&runtime);

    assert!(taps.is_hosted());
    assert_eq!(taps.stream_count(&c.id), 1);
    assert_eq!(
        taps.stream_count(&ChainId("chain:127:absent".into())),
        0,
        "a chain with no runtime has no streams to tap"
    );
}
