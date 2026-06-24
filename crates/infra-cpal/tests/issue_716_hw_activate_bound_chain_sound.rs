//! #716 real-hardware battery — activating a binding-bound chain MUST produce
//! sound on the physical interface.
//!
//! User repro: open project TESTE, activate chain 1 → NO SOUND. The chain is
//! bound via the system E/S binding (io_binding_ids), carrying only effects —
//! its I/O comes from the binding, not from chain blocks. This opens the REAL
//! cpal streams on the REAL default devices, starts the bound chain, injects a
//! DI loop, and asserts the OUTPUT tap is non-silent. Red until the device/
//! stream layer resolves I/O from the binding.
//!
//! macOS + release only (real devices, real timing). Gated by OPENRIG_HW_TESTS.
//! Run on an idle machine:
//!   OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!       --test issue_716_hw_activate_bound_chain_sound
//! NOTE: this PLAYS ~3 s of a guitar DI through your output — keep volume sane.
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use std::time::{Duration, Instant};

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use hw_harness::{device_guard, hw_tests_enabled, init_registry, load_di};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

#[test]
fn activating_a_binding_bound_chain_produces_sound() {
    if !hw_tests_enabled("activating_a_binding_bound_chain_produces_sound") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(input), Some(output)) = (inputs.first(), outputs.first()) else {
        panic!("no audio devices available — this test needs a real interface");
    };
    eprintln!(
        "[#716 HW] input='{}' output='{}'",
        input.name, output.name
    );

    // The per-machine E/S binding (system config). The chain references it;
    // it never carries I/O blocks.
    let binding = IoBinding {
        id: "main".into(),
        name: "Default".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId(input.id.clone()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId(output.id.clone()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    };

    // Effect-only, binding-bound chain — exactly what the editor checklist saves.
    let chain_id = ChainId("rig:input-1".into());
    let project = Project {
        name: Some("issue-716-activate-sound".into()),
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
            blocks: vec![],
        }],
        midi: None,
    };

    let mut controller = ProjectRuntimeController::start_with_bindings(&project, vec![binding])
        .expect("start real streams for a binding-bound chain");

    // Cold activation builds off-thread and installs streams on the frontend
    // tick — simulate the GUI tick by draining pending rebuilds/activations.
    let deadline = Instant::now() + Duration::from_secs(8);
    while controller.chain_runtime(&chain_id).is_none() && Instant::now() < deadline {
        controller.poll_pending_rebuilds();
        std::thread::sleep(Duration::from_millis(20));
    }

    // The bound chain must have a LIVE runtime — the bug is it never activates.
    assert!(
        controller.chain_runtime(&chain_id).is_some() && controller.is_running(),
        "BUG #716: activating a binding-bound chain produced NO live runtime → no sound"
    );

    // Inject a DI loop and prove the OUTPUT actually carries signal.
    let di = load_di("phil-STRATO-green_day.wav", controller.sample_rate());
    controller.set_chain_di_loop(&chain_id, Some(di));

    let mut taps = Vec::new();
    let n = controller.stream_count(&chain_id);
    for i in 0..n.max(1) {
        if let Some([l, r]) = controller.subscribe_stream_tap(&chain_id, i, 8192) {
            taps.push(l);
            taps.push(r);
        }
    }
    assert!(
        !taps.is_empty(),
        "BUG #716: bound chain has no output stream taps — it is not streaming"
    );

    // Drain the output taps for ~3 s, tracking the peak magnitude.
    let mut peak = 0.0_f32;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        for ring in &taps {
            while let Some(s) = ring.pop() {
                peak = peak.max(s.abs());
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    eprintln!("[#716 HW] output peak over 3 s of DI = {peak:.5}");
    assert!(
        peak > 1e-3,
        "BUG #716: the activated binding-bound chain produced SILENT output \
         (peak={peak:.6}) — the DI did not reach the device. No sound on activate."
    );
}
