//! Issue #947 — starting a chain must not light the overload LED. Starts the
//! REAL streams and samples the xrun / underrun counters from the very first
//! instant (the #670 tests skip the first 2 s, so they never see start-up),
//! both on the cold start and on the user's toggle off → on.
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use std::time::Duration;

use domain::ids::ChainId;
use hw_harness::{device_guard, hw_tests_enabled, init_registry, rig_project};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};

/// Samples `(streams, xruns, underruns)` every 33 ms (the meter tick) for 3 s,
/// draining the control worker like the GUI does.
fn sample_start(
    controller: &mut ProjectRuntimeController,
    chain_id: &ChainId,
) -> Vec<(usize, u64, u64)> {
    (0..90)
        .map(|_| {
            controller.poll_pending_rebuilds();
            let s = (
                controller.stream_count(chain_id),
                controller.chain_xrun_count(chain_id),
                controller.chain_underrun_count(chain_id),
            );
            std::thread::sleep(Duration::from_millis(33));
            s
        })
        .collect()
}

#[test]
fn starting_a_chain_counts_no_xrun_or_underrun() {
    if !hw_tests_enabled("starting_a_chain_counts_no_xrun_or_underrun") {
        return;
    }
    let _device = device_guard();
    init_registry();
    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(input), Some(output)) = (inputs.first(), outputs.first()) else {
        panic!("no audio devices available");
    };
    let (mut project, chain_id, registry) =
        rig_project("beat_it_michael_jackson_rhythm.yaml", input, output);
    let mut controller =
        ProjectRuntimeController::start_with_io_bindings(&project, registry).expect("start");

    let cold = sample_start(&mut controller, &chain_id);
    eprintln!("[#947] cold start (streams, xruns, underruns): {cold:?}");

    project.chains[0].enabled = false;
    controller.sync_project(&project).expect("sync disabled");
    std::thread::sleep(Duration::from_secs(1));
    controller.poll_pending_rebuilds();
    project.chains[0].enabled = true;
    controller.sync_project(&project).expect("sync enabled");
    let toggled = sample_start(&mut controller, &chain_id);
    eprintln!("[#947] toggle on (streams, xruns, underruns): {toggled:?}");

    assert!(cold.last().unwrap().0 > 0, "the chain never started");
    for (label, samples) in [("cold start", &cold), ("toggle on", &toggled)] {
        let (_, xruns, underruns) = *samples.last().unwrap();
        assert_eq!(
            (xruns, underruns),
            (0, 0),
            "{label} counted {xruns} xruns / {underruns} underruns — the overload LED lights on start"
        );
    }
}
