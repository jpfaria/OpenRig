//! Issue #328 — the dual-amp rig on the REAL interface: a Split → Mix with a
//! native amp on each path, amp A hard left and amp B hard right, playing the
//! Green Day DI for 60 s through the real CoreAudio streams. The engine's own
//! xrun / underrun counters must stay at ZERO. The chain's peak load is
//! printed next to the single-amp chain's (spec §6: the split costs path A +
//! path B + one mix pass, measured here before and after).
//!
//! macOS + release only; gated by OPENRIG_HW_TESTS=1 (docs/testing.md).
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use std::time::Duration;

use domain::ids::{BlockId, ChainId};
use domain::io_binding::IoBinding;
use domain::value_objects::ParameterValue;
use hw_harness::{device_guard, hw_tests_enabled, init_registry, load_di_pcm, rig_project, BUFFER};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{
    schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd,
};
use project::block::{split_param_keys, split_params};
use project::param::ParameterSet;
use project::project::Project;

fn native_amp(id: &str, model: &str) -> AudioBlock {
    let schema = schema_for_block_model("amp", model).expect("native amp schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("amp defaults normalize");
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "amp".into(),
            model: model.into(),
            params,
        }),
    }
}

fn dual_amp_split() -> AudioBlock {
    let mut split = SplitBlock::with_paths(
        SplitEnd::Mix,
        vec![
            vec![native_amp("amp_a", "tweed_breakup")],
            vec![native_amp("amp_b", "blackface_clean")],
        ],
    );
    split
        .params
        .insert(&split_param_keys::mix_pan(0), ParameterValue::Float(-50.0));
    split
        .params
        .insert(&split_param_keys::mix_pan(1), ParameterValue::Float(50.0));
    split
        .params
        .insert(split_params::MIX_MASTER, ParameterValue::Float(100.0));
    AudioBlock {
        id: BlockId("dual_amp".into()),
        enabled: true,
        kind: AudioBlockKind::Split(split),
    }
}

/// Play the Green Day DI through `project` for `seconds`; returns
/// (xruns, underruns, peak load) measured after a 2 s settle.
fn play(
    project: &Project,
    chain_id: &ChainId,
    registry: Vec<IoBinding>,
    seconds: u64,
) -> (u64, u64, f32) {
    let mut controller = ProjectRuntimeController::start(project).expect("start real streams");
    controller.set_io_bindings(registry);
    controller
        .sync_project(project)
        .expect("resync with bindings");
    controller.set_chain_di_loop(chain_id, Some(load_di_pcm("phil-STRATO-green_day.wav")));
    std::thread::sleep(Duration::from_secs(2));
    let x0 = controller.chain_xrun_count(chain_id);
    let u0 = controller.chain_underrun_count(chain_id);
    std::thread::sleep(Duration::from_secs(seconds));
    (
        controller.chain_xrun_count(chain_id) - x0,
        controller.chain_underrun_count(chain_id) - u0,
        controller.chain_peak_load(chain_id),
    )
}

#[test]
fn dual_amp_split_left_right_no_xruns() {
    if !hw_tests_enabled("dual_amp_split_left_right_no_xruns") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(input), Some(output)) = (inputs.first(), outputs.first()) else {
        panic!("no audio devices available — this test needs real devices");
    };
    let (mut project, chain_id, registry) = rig_project("clean.yaml", input, output);

    project.chains[0].blocks = vec![native_amp("amp_a", "tweed_breakup")];
    let (_, _, single_amp_load) = play(&project, &chain_id, registry.clone(), 20);

    project.chains[0].blocks = vec![dual_amp_split()];
    let (xruns, underruns, split_load) = play(&project, &chain_id, registry, 60);

    eprintln!(
        "[#328 REAL] buffer={BUFFER}: single amp peak load {single_amp_load:.3}, \
         dual-amp split peak load {split_load:.3}; 60 s split: xruns={xruns} underruns={underruns}"
    );
    assert_eq!(
        (xruns, underruns),
        (0, 0),
        "BUG #328: the dual-amp split recorded {xruns} xruns / {underruns} underruns in \
         60 s on the real interface at buffer {BUFFER}"
    );
}
