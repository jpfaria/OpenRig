//! #716 real-hardware battery — a binding-bound chain carrying a REAL effect
//! preset must run like any working chain: sound out, NO underruns, and live
//! block toggle finds the block (not "block not found in any input runtime").
//!
//! This is the end-to-end topology the narrow tests missed: the chain is bound
//! via io_binding_ids (no I/O blocks) AND carries a full effect chain — exactly
//! the user's project. It reproduces #2 (underruns / "rig is heavy") and #4
//! ("block not found in any input runtime") on the real interface.
//!
//! macOS + release only. Gated by OPENRIG_HW_TESTS. Run on an idle machine:
//!   OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!       --test issue_716_hw_bound_chain_with_effects
//! NOTE: PLAYS a guitar DI through your output.
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use hw_harness::{device_guard, hw_tests_enabled, init_registry, load_di};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::AudioBlockKind;
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

#[test]
fn bound_chain_with_effects_runs_clean_and_toggles() {
    if !hw_tests_enabled("bound_chain_with_effects_runs_clean_and_toggles") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("inputs");
    let outputs = list_output_device_descriptors().expect("outputs");
    let (Some(input), Some(output)) = (inputs.first(), outputs.first()) else {
        panic!("no audio devices available");
    };
    eprintln!("[#716 HW fx] input='{}' output='{}'", input.name, output.name);

    // Real effect chain from a preset (effects only — no I/O blocks).
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join("beat_it_michael_jackson_rhythm.yaml");
    let effect_blocks = infra_yaml::load_chain_preset_file(&preset)
        .expect("preset")
        .blocks;
    assert!(!effect_blocks.is_empty(), "preset must carry effect blocks");
    // An effect block id to toggle live (skip any I/O blocks the preset has).
    let fx_block_id = effect_blocks
        .iter()
        .find(|b| !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
        .map(|b| b.id.clone())
        .expect("preset has at least one effect block");

    // System E/S binding — the chain references it; it carries NO I/O blocks.
    // Two input endpoints on the same device — exactly like the user's binding
    // io-1-50c4 (SCARLET: In 1 ch0, In 2 ch1). This is what triggers #4: the
    // bound chain builds one per-input runtime PER endpoint, and the live block
    // toggle must still find the effect block in them.
    let binding = IoBinding {
        id: "main".into(),
        name: "Default".into(),
        inputs: vec![
            IoEndpoint {
                name: "in1".into(),
                device_id: DeviceId(input.id.clone()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            },
            IoEndpoint {
                name: "in2".into(),
                device_id: DeviceId(input.id.clone()),
                mode: ChannelMode::Mono,
                channels: vec![1],
            },
        ],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId(output.id.clone()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    };

    let chain_id = ChainId("rig:input-1".into());
    let project = Project {
        name: Some("issue-716-bound-fx".into()),
        device_settings: vec![
            DeviceSettings {
                device_id: DeviceId(input.id.clone()),
                sample_rate: 48_000,
                buffer_size_frames: 64,
                bit_depth: 32,
            },
            DeviceSettings {
                device_id: DeviceId(output.id.clone()),
                sample_rate: 48_000,
                buffer_size_frames: 64,
                bit_depth: 32,
            },
        ],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["main".into()],
            blocks: effect_blocks,
        }],
        midi: None,
    };

    let mut controller = ProjectRuntimeController::start_with_bindings(&project, vec![binding])
        .expect("start real streams");

    // Drain cold activation (GUI tick).
    let deadline = Instant::now() + Duration::from_secs(8);
    while controller.chain_runtime(&chain_id).is_none() && Instant::now() < deadline {
        controller.poll_pending_rebuilds();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        controller.chain_runtime(&chain_id).is_some(),
        "BUG #716: bound effect chain produced no live runtime"
    );

    let di = load_di("phil-STRATO-green_day.wav", controller.sample_rate());
    controller.set_chain_di_loop(&chain_id, Some(di));

    // Settle, then measure 10 s for stream stability (#2).
    std::thread::sleep(Duration::from_secs(2));
    let xrun0 = controller.chain_xrun_count(&chain_id);
    let under0 = controller.chain_underrun_count(&chain_id);
    std::thread::sleep(Duration::from_secs(10));
    let xruns = controller.chain_xrun_count(&chain_id) - xrun0;
    let underruns = controller.chain_underrun_count(&chain_id) - under0;
    eprintln!("[#716 HW fx] 10 s: xruns={xruns} underruns={underruns}");

    // Live block toggle must find the block (#4).
    let toggle = controller.set_block_enabled(&chain_id, &fx_block_id, false);
    eprintln!("[#716 HW fx] toggle '{}' -> {:?}", fx_block_id.0, toggle.is_ok());

    assert!(
        toggle.is_ok(),
        "BUG #716 (#4): live toggle of effect block '{}' failed: {:?} — \
         'block not found in any input runtime'",
        fx_block_id.0,
        toggle.err()
    );
    assert_eq!(
        (xruns, underruns),
        (0, 0),
        "BUG #716 (#2): bound effect chain starved the device — {xruns} xruns / \
         {underruns} underruns in 10 s ('audio overload, rig is heavy')"
    );
}
