//! Issue #779 (2nd root cause) — changing a VST3 parameter on a LIVE streaming
//! chain must NOT re-instantiate the VST3.
//!
//! The user changed a knob inside ChowCentaur on a streaming chain and the app
//! SIGSEGV'd. Root cause: a live param edit on macOS is routed through the
//! off-thread rebuild (`request_offthread_rebuild_if_live` → `schedule_chain_
//! rebuild`), which builds a FRESH runtime and calls `createInstance` on the
//! control worker WHILE the audio thread is inside `process()` on the old
//! instance. JUCE global state is not safe against a concurrent instantiate-vs-
//! process, so it crashes (the same concurrent-JUCE-op class as #778, for the
//! one pairing its lock cannot cover — `process()` runs on the RT thread and
//! must not lock). The design already says param edits retune in place and a
//! VST3 must NOT be re-instantiated on a param change (`runtime_block_builders`,
//! `controller_offthread_live_rebuild`); the off-thread path violated it.
//!
//! Deterministic guard (no crash, no timing): a live param edit must leave the
//! VST3 instantiation count UNCHANGED — the live instance is retuned in place,
//! not reloaded. Real-hardware + real-plugin battery (opens the physical
//! default devices so the chain actually streams, and loads the catalog
//! ChowCentaur), idle machine:
//! ```sh
//! OPENRIG_HW_TESTS=1 \
//! OPENRIG_TEST_VST3_DIR=<OpenRig-plugins>/plugins/source/vst3 \
//!   cargo test -p infra-cpal --release \
//!     --test issue_779_vst3_live_param_no_reinstantiate -- --nocapture --test-threads=1
//! ```
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use hw_harness::{device_guard, hw_tests_enabled, init_registry, BUFFER};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;

const SR: f64 = 48_000.0;

/// Init the VST3 catalog against `OPENRIG_TEST_VST3_DIR` and return ChowCentaur's
/// model id + the id of its first real parameter, or `None` to skip.
fn chow() -> Option<(String, u32)> {
    let dir = std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from)?;
    vst3_host::init_vst3_catalog(SR, &[dir]);
    let model = vst3_host::vst3_catalog()
        .iter()
        .find(|e| {
            e.info
                .bundle_path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("ChowCentaur.vst3"))
                .unwrap_or(false)
        })
        .map(|e| e.model_id.to_string())?;
    // Discover the first real param id (VST3 ids are arbitrary). This transient
    // load is BEFORE we snapshot the instantiation baseline, so it does not
    // affect the assertion.
    let entry = vst3_host::find_vst3_plugin(&model)?;
    let uid = vst3_host::resolve_uid_for_model(&model).ok()?;
    let plugin =
        vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR, 2, BUFFER as usize, &[])
            .ok()?;
    let param_id = plugin.param_info(0).ok()?.id;
    Some((model, param_id))
}

fn chow_project(
    input: &infra_cpal::AudioDeviceDescriptor,
    output: &infra_cpal::AudioDeviceDescriptor,
    model: &str,
    param_id: u32,
    value: f32,
) -> (Project, ChainId, Vec<IoBinding>) {
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
    let mut params = ParameterSet::default();
    params.insert(format!("p{param_id}"), ParameterValue::Float(value));
    let chain_id = ChainId("issue-779-vst3-live".into());
    let project = Project {
        name: Some("issue-779-vst3-live".into()),
        device_settings: vec![
            DeviceSettings {
                device_id: DeviceId(input.id.clone()),
                sample_rate: SR as u32,
                buffer_size_frames: BUFFER,
                bit_depth: 32,
            },
            DeviceSettings {
                device_id: DeviceId(output.id.clone()),
                sample_rate: SR as u32,
                buffer_size_frames: BUFFER,
                bit_depth: 32,
            },
        ],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 0.0, // silent monitors; identical DSP path
            io_binding_ids: vec!["io".into()],
            blocks: vec![AudioBlock {
                id: BlockId("chow".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: block_core::EFFECT_TYPE_VST3.into(),
                    model: model.into(),
                    params,
                }),
            }],
            di_output: None,
        }],
        midi: None,
    };
    (project, chain_id, registry)
}

#[test]
fn changing_a_vst3_param_on_a_live_chain_does_not_reinstantiate_it() {
    if !hw_tests_enabled("changing_a_vst3_param_on_a_live_chain_does_not_reinstantiate_it") {
        return;
    }
    let Some((model, param_id)) = chow() else {
        eprintln!("[#779 VST3] ChowCentaur / OPENRIG_TEST_VST3_DIR not available — skipping");
        return;
    };
    let _device = device_guard();
    init_registry();

    let inputs = infra_cpal::list_input_device_descriptors().expect("inputs");
    let outputs = infra_cpal::list_output_device_descriptors().expect("outputs");
    let (Some(input), Some(output)) = (inputs.first(), outputs.first()) else {
        eprintln!("[#779 VST3] no audio devices — skipping");
        return;
    };

    // Bring the streaming chain up (cold start is async, #693).
    let (project, chain_id, registry) = chow_project(input, output, &model, param_id, 0.2);
    let mut controller =
        infra_cpal::ProjectRuntimeController::start_with_io_bindings(&project, registry)
            .expect("start");
    let ready = Instant::now() + Duration::from_secs(15);
    while !(controller.chain_runtime(&chain_id).is_some() && controller.is_running()) {
        controller.poll_pending_rebuilds();
        assert!(Instant::now() < ready, "chain never became ready");
        std::thread::sleep(Duration::from_millis(16));
    }
    // Let the audio thread actually process a few VST3 buffers so it is inside
    // process() when the edit's rebuild fires.
    std::thread::sleep(Duration::from_millis(500));

    let before = vst3_host::instantiation_count();

    // The LIVE param edit: same chain + I/O, only the VST3 knob changed. This is
    // the exact path the GUI takes for a running chain (#740).
    let (edited, _, _) = chow_project(input, output, &model, param_id, 0.8);
    let scheduled = controller
        .request_offthread_rebuild_if_live(&edited, &edited.chains[0])
        .expect("live param edit");
    assert!(
        scheduled,
        "an I/O-unchanged live VST3 param edit must take the off-thread live path"
    );
    // Drain the off-thread rebuild.
    let deadline = Instant::now() + Duration::from_secs(10);
    while controller.poll_pending_rebuilds() == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(16));
    }
    std::thread::sleep(Duration::from_millis(200));

    let after = vst3_host::instantiation_count();
    assert_eq!(
        after,
        before,
        "a live VST3 param change must retune the instance IN PLACE, not re-instantiate it \
         (#779): a fresh createInstance on the worker thread runs concurrently with the audio \
         thread's process() on the old instance → SIGSEGV. Instantiations during the edit: {}",
        after - before
    );
}
