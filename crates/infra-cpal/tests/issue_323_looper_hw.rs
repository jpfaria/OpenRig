//! Issue #323 — the looper on the REAL audio stack.
//!
//! The offline tests prove the maths and the zero-allocation contract; this
//! proves the feature does not cost the rig its deadline. It opens the real
//! CoreAudio streams on the real devices, runs a real amp chain, then records,
//! overdubs up to the layer cap, undoes and plays back — while watching the
//! engine's own xrun / underrun counters, the same numbers the GUI turns into
//! "audio overload on chain".
//!
//! macOS + release only, gated by `OPENRIG_HW_TESTS=1` (docs/testing.md →
//! "Real-hardware battery").
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::{LooperOp, LooperState};
use hw_harness::{device_guard, hw_tests_enabled, init_registry, BUFFER};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;

const UID: u64 = 1;

/// Queue a record/overdub tap, allocating the layer buffer here — on the
/// control thread — exactly like the GUI wiring does.
fn tap_record(
    controller: &ProjectRuntimeController,
    chain: &domain::ids::ChainId,
    with_buffer: bool,
) {
    controller.push_chain_looper_op(chain, |runtime| {
        Some(LooperOp::TapRecord {
            uid: UID,
            buffer: with_buffer
                .then(|| vec![0.0f32; runtime.looper_max_frames() * 2].into_boxed_slice()),
        })
    });
}

#[test]
fn recording_and_overdubbing_on_real_streams_costs_no_xrun() {
    if !hw_tests_enabled("recording_and_overdubbing_on_real_streams_costs_no_xrun") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(input), Some(output)) = (inputs.first(), outputs.first()) else {
        panic!("no audio devices available — this fidelity test needs real devices");
    };
    eprintln!(
        "[#323 REAL] input='{}' output='{}' buffer={BUFFER}",
        input.name, output.name
    );

    // Build the rig HERE instead of loading a fixture preset: the shipped
    // presets reference the owner's NAM/LV2 capture library, so on a machine
    // without it every block is dropped and the chain never comes up — the
    // measurement would be vacuously green. A native gain block is enough to
    // put real DSP on the audio thread and is available everywhere.
    let chain_id = ChainId("issue-323-looper-hw".into());
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId(input.id.clone()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId(output.id.clone()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    let device = |id: &str| DeviceSettings {
        device_id: DeviceId(id.to_string()),
        sample_rate: 48_000,
        buffer_size_frames: BUFFER,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    };
    let project = Project {
        name: Some("issue-323-looper-hw".into()),
        device_settings: vec![device(&input.id), device(&output.id)],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            // Volume 0: identical DSP path, silent monitors.
            volume: 0.0,
            io_binding_ids: vec!["io".into()],
            blocks: vec![AudioBlock {
                id: BlockId("gain:0".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "ibanez_ts9".into(),
                    // Real DSP needs real parameters: an unparameterised
                    // block faults into a bypass and the chain measures
                    // nothing (seen while building this test).
                    params: ParameterSet::default()
                        .normalized_against(
                            &project::block::schema_for_block_model("gain", "ibanez_ts9")
                                .expect("the native TS9 schema is compiled in"),
                        )
                        .expect("defaults normalize"),
                }),
            }],
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };
    let mut controller = ProjectRuntimeController::start(&project).expect("start real streams");
    controller.set_io_bindings(registry);
    controller
        .sync_project(&project)
        .expect("resync with bindings");

    // The cold activation is asynchronous (#740): the runtime is built on the
    // control worker and installed by `poll_pending_rebuilds`, which the app
    // drives from a timer. Do the same here, or the chain is still not live
    // when the measurement starts.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        controller.poll_pending_rebuilds();
        if !controller.runtimes_for_chain(&chain_id).is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(
        !controller.runtimes_for_chain(&chain_id).is_empty(),
        "the chain never came up on this machine — the measurement below would \
         be vacuously green (zero counters for a runtime that does not exist)"
    );

    // Settle, then take the baseline the whole looper session is measured
    // against.
    std::thread::sleep(std::time::Duration::from_secs(2));
    let x0 = controller.chain_xrun_count(&chain_id);
    let u0 = controller.chain_underrun_count(&chain_id);

    // [#323-probe] temporary: how many runtimes answer for this chain, and
    // how many accepted the op.
    eprintln!(
        "[#323-probe] runtimes_for_chain={} chain_runtime={} xruns={} chain_id={}",
        controller.runtimes_for_chain(&chain_id).len(),
        controller.chain_runtime(&chain_id).is_some(),
        controller.chain_xrun_count(&chain_id),
        chain_id.0,
    );
    let queued =
        controller.push_chain_looper_op(&chain_id, |_| Some(LooperOp::Create { uid: UID }));
    eprintln!("[#323-probe] create queued on {queued} runtime(s)");
    std::thread::sleep(std::time::Duration::from_millis(500));
    eprintln!(
        "[#323-probe] statuses after create: {:?}",
        controller.chain_looper_statuses(&chain_id)
    );

    // Record an 8-second loop off the live input.
    tap_record(&controller, &chain_id, true);
    std::thread::sleep(std::time::Duration::from_secs(8));
    tap_record(&controller, &chain_id, false);
    std::thread::sleep(std::time::Duration::from_secs(2));

    let status = controller
        .chain_looper_status(&chain_id, UID)
        .expect("the looper exists on the live runtime");
    assert_eq!(
        status.state,
        LooperState::Playing,
        "closing the first recording must start playback"
    );
    assert!(
        status.len_frames > 0,
        "the recording captured no frames on the real stream"
    );

    // Overdub up to the layer cap, playing a couple of seconds per layer —
    // the worst case for the read-side layer sum on the audio thread.
    for _ in 1..engine::LOOPER_MAX_LAYERS {
        tap_record(&controller, &chain_id, true);
        std::thread::sleep(std::time::Duration::from_secs(2));
        tap_record(&controller, &chain_id, false);
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    let full = controller
        .chain_looper_status(&chain_id, UID)
        .expect("status");
    assert_eq!(
        full.layers,
        engine::LOOPER_MAX_LAYERS,
        "every overdub must have landed"
    );

    // Undo / redo / clear while the stream keeps running.
    for op in [
        LooperOp::Undo { uid: UID },
        LooperOp::Redo { uid: UID },
        LooperOp::Clear { uid: UID },
    ] {
        let queued = std::cell::Cell::new(Some(op));
        controller.push_chain_looper_op(&chain_id, |_| queued.take());
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    // The GUI tick collects the retired buffers; do the same so the drop
    // happens here and not on the audio thread.
    controller.drain_chain_looper_layers(&chain_id);

    let xruns = controller.chain_xrun_count(&chain_id) - x0;
    let underruns = controller.chain_underrun_count(&chain_id) - u0;
    eprintln!(
        "[#323 REAL] record + {} overdubs + undo/redo/clear: xruns={xruns} underruns={underruns}",
        engine::LOOPER_MAX_LAYERS - 1
    );
    assert_eq!(
        (xruns, underruns),
        (0, 0),
        "the looper cost the rig {xruns} xruns / {underruns} underruns on the \
         REAL audio stack at buffer {BUFFER} — recording, summing 8 layers and \
         handing buffers back must all stay inside the deadline (invariants \
         #3 / #7 / #8)",
    );
}
