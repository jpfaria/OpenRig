//! #127 — the click is a runtime, and a failed output change may not silence
//! one that is already sounding.
//!
//! Two doors of the metronome's own stream (#14), both invariant #4 applied
//! literally: the click owns its stream, so it stands with no chain enabled
//! (#808) and a failure on the way to a NEW output is no reason to cut the
//! one that is playing.
//!
//! Both need a REAL output device — `start_metronome` resolves the id through
//! `host.output_devices()` and opens a `cpal::Stream` — so they belong to the
//! real-hardware battery (`OPENRIG_HW_TESTS=1`, docs/testing.md) and return
//! immediately with a loud notice without it. Nothing is ever audible: the
//! generator's `enabled` flag is never set and the volume is zeroed, so every
//! buffer the stream renders is silence.
//!
//! The JACK build opens no cpal stream for the click (`start_metronome` is a
//! stub there), so neither door exists on that target.
#![cfg(not(all(target_os = "linux", feature = "jack")))]

mod hw_harness;

use std::collections::HashMap;

use engine::metronome_state::MetronomeSettings;
use engine::runtime::RuntimeGraph;
use hw_harness::{device_guard, hw_tests_enabled};
use infra_cpal::{list_output_device_descriptors, ProjectRuntimeController};

/// An output id no host can resolve — the device the user picked being gone,
/// renamed or taken by another app.
const MISSING_OUTPUT: &str = "openrig-127-no-such-output-device";

/// A controller with NO chain: the click has to stand on its own (#808).
/// Muted before anything opens — a test has no business making a sound on the
/// owner's monitors.
fn chainless_controller() -> ProjectRuntimeController {
    let controller = ProjectRuntimeController::for_testing(RuntimeGraph {
        chains: HashMap::new(),
    });
    controller.set_metronome_settings(MetronomeSettings {
        volume: 0.0,
        ..Default::default()
    });
    controller
}

/// The machine's first usable output and the channels to route the click to.
/// `None` on a machine with no output device at all.
fn first_real_output() -> Option<(String, Vec<usize>)> {
    let device = list_output_device_descriptors()
        .ok()?
        .into_iter()
        .find(|device| device.channels > 0)?;
    let channels = if device.channels >= 2 {
        vec![0, 1]
    } else {
        vec![0]
    };
    Some((device.id, channels))
}

/// The owner's report: start the click with no chain enabled, then enable a
/// chain and disable it. The teardown doors (`sync_live_chain_runtime` /
/// `remove_live_chain_runtime`) drop the whole controller once nothing is
/// running — and with the click's stream uncounted, "nothing" included it. The
/// click died and, with the runtime gone, the lamp timer left POWER lit over
/// silence.
#[test]
fn an_open_metronome_stream_keeps_the_runtime_running() {
    if !hw_tests_enabled("an_open_metronome_stream_keeps_the_runtime_running") {
        return;
    }
    let Some((device_id, channels)) = first_real_output() else {
        eprintln!("[#127 metronome] SKIPPED — this machine reports no output device.");
        return;
    };
    let _device = device_guard();
    let controller = chainless_controller();

    controller
        .start_metronome(&device_id, &channels)
        .expect("the click must open on the machine's own output device");

    assert!(
        controller.metronome_active(),
        "precondition: the click's stream is open"
    );
    assert!(
        controller.is_running(),
        "#127: an open metronome stream IS the runtime sounding — reporting it stopped makes \
         the chain teardown doors drop the controller, killing the click and freezing POWER \
         lit over silence"
    );
}

/// A click that is playing and an output pick that cannot be honoured (the
/// device is gone or busy). The change fails; the state has to stay truthful —
/// the previous stream keeps playing rather than the flags claiming a click
/// that no longer has a stream.
#[test]
fn a_failed_output_change_leaves_the_click_playing() {
    if !hw_tests_enabled("a_failed_output_change_leaves_the_click_playing") {
        return;
    }
    let Some((device_id, channels)) = first_real_output() else {
        eprintln!("[#127 metronome] SKIPPED — this machine reports no output device.");
        return;
    };
    let _device = device_guard();
    let controller = chainless_controller();
    controller
        .start_metronome(&device_id, &channels)
        .expect("the click must open on the machine's own output device");
    assert!(
        controller.metronome_active(),
        "precondition: there is a playing click for the failed change to threaten"
    );

    let refused = controller.start_metronome(MISSING_OUTPUT, &channels);

    assert!(
        refused.is_err(),
        "an output that cannot be opened must reach the caller as a reason: {refused:?}"
    );
    assert!(
        controller.metronome_active(),
        "#127: a failed output change must not silence a click that was already sounding — \
         `refresh_metronome_output` leaves `enabled` set, so dropping the stream here reads \
         as playing forever with nothing to hear"
    );
}
