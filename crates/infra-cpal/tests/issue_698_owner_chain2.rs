//! Issue #698 — full-fidelity owner recipe: the EXACT chain the buzz
//! happened on (NAM big_muff A2 + NAM JMP-1 + IR mesa_os_4x12_v30 +
//! compressor + guitar EQ, disabled blocks included), the owner's device
//! shape (44.1 kHz / 128 frames = the 2.9 ms period in the captured log),
//! and the owner's real capture library.
//!
//! Needs the owner's plugins root on top of the hardware gate, so it skips
//! loudly unless BOTH are set:
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 \
//! OPENRIG_OWNER_PLUGINS=/path/to/OpenRig-plugins/plugins/source \
//! cargo test -p infra-cpal --release --test issue_698_owner_chain2
//! ```
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use domain::ids::{BlockId, ChainId};
use hw_harness::{device_guard, hw_tests_enabled, init_registry_with_root, load_di, rig_project_with};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{AudioBlock, AudioBlockKind};

/// The owner's exact block, as the GUI seeded it: neutral shift, full wet.
fn owner_pitch_block(enabled: bool) -> AudioBlock {
    use domain::value_objects::ParameterValue;
    let mut params = block_core::param::ParameterSet::default();
    params.insert("shift_semitones", ParameterValue::Float(0.0));
    params.insert("shift_cents", ParameterValue::Float(0.0));
    params.insert("mix", ParameterValue::Float(100.0));
    AudioBlock {
        id: BlockId("added-pitch-698".into()),
        enabled,
        kind: AudioBlockKind::Core(project::block::CoreBlock {
            effect_type: block_core::EFFECT_TYPE_PITCH.into(),
            model: "native_pitch_shifter".into(),
            params,
        }),
    }
}

#[test]
fn owner_chain2_live_pitch_insert_does_not_saturate() {
    if !hw_tests_enabled("owner_chain2_live_pitch_insert_does_not_saturate") {
        return;
    }
    let Some(plugins_root) = std::env::var_os("OPENRIG_OWNER_PLUGINS") else {
        eprintln!(
            "[#698 HW] owner_chain2_live_pitch_insert_does_not_saturate: SKIPPED — \
             needs OPENRIG_OWNER_PLUGINS=<OpenRig-plugins/plugins/source> (real \
             capture library; the fixture set does not bundle big_muff/mesa)."
        );
        return;
    };
    let _device = device_guard();
    init_registry_with_root(std::path::Path::new(&plugins_root));

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(input), Some(output)) = (inputs.first(), outputs.first()) else {
        panic!("no audio devices available");
    };

    // Owner shape: 44.1 kHz / 128 frames (the captured 2902 us period) and
    // the multi-chain project (3 chains → 3 workers + GUI competing).
    let (mut project, chain_id) =
        rig_project_with("issue_698_owner_chain2.yaml", input, output, 44_100, 128);
    for i in 1..3 {
        let mut extra = project.chains[0].clone();
        extra.id = ChainId(format!("issue-698-extra-{i}"));
        project.chains.push(extra);
    }
    let mut controller = ProjectRuntimeController::start(&project).expect("start streams");
    let di = load_di("phil-STRATO-green_day.wav", controller.sample_rate());
    controller.set_chain_di_loop(&chain_id, Some(di.clone()));

    std::thread::sleep(std::time::Duration::from_secs(10));

    // Live INSERT right before the output block, like the GUI edit.
    let mut with_pitch = project.clone();
    {
        let chain = &mut with_pitch.chains[0];
        let out_pos = chain.blocks.len() - 1;
        chain.blocks.insert(out_pos, owner_pitch_block(true));
    }
    let stop_load = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let loaders: Vec<_> = (0..12)
        .map(|_| {
            let stop = std::sync::Arc::clone(&stop_load);
            std::thread::spawn(move || {
                let mut x = 0.001f64;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    x = (x.sin().cos() + 1.0001).sqrt();
                    std::hint::black_box(x);
                }
            })
        })
        .collect();

    controller
        .upsert_chain(&with_pitch, &with_pitch.chains[0])
        .expect("live insert-pitch rebuild");
    controller.set_chain_di_loop(&chain_id, Some(di.clone()));

    std::thread::sleep(std::time::Duration::from_secs(8));
    stop_load.store(true, std::sync::atomic::Ordering::Relaxed);
    for l in loaders {
        let _ = l.join();
    }
    std::thread::sleep(std::time::Duration::from_secs(4));
    let x0 = controller.chain_xrun_count(&chain_id);
    let u0 = controller.chain_underrun_count(&chain_id);
    std::thread::sleep(std::time::Duration::from_secs(30));
    let with_x = controller.chain_xrun_count(&chain_id) - x0;
    let with_u = controller.chain_underrun_count(&chain_id) - u0;

    eprintln!(
        "[#698 OWNER-IN] 30s after live-inserting native_pitch_shifter into the \
         owner chain: xruns={with_x} underruns={with_u}"
    );

    // The owner's escape attempt: toggle the block OFF while still playing.
    let mut pitch_off = project.clone();
    {
        let chain = &mut pitch_off.chains[0];
        let out_pos = chain.blocks.len() - 1;
        chain.blocks.insert(out_pos, owner_pitch_block(false));
    }
    controller
        .upsert_chain(&pitch_off, &pitch_off.chains[0])
        .expect("toggle-off rebuild");
    controller.set_chain_di_loop(&chain_id, Some(di));

    std::thread::sleep(std::time::Duration::from_secs(4));
    let x1 = controller.chain_xrun_count(&chain_id);
    let u1 = controller.chain_underrun_count(&chain_id);
    std::thread::sleep(std::time::Duration::from_secs(20));
    let off_x = controller.chain_xrun_count(&chain_id) - x1;
    let off_u = controller.chain_underrun_count(&chain_id) - u1;

    eprintln!(
        "[#698 OWNER-OFF] 20s after toggling the pitch shifter off: \
         xruns={off_x} underruns={off_u}"
    );

    assert_eq!(
        (with_x, with_u),
        (0, 0),
        "BUG #698: the owner chain (big_muff + JMP-1 + mesa IR, 44.1 kHz/128) \
         recorded {with_x} xruns / {with_u} underruns in 30 s with the pitch \
         shifter live-inserted — the bee-box buzz."
    );
    assert_eq!(
        (off_x, off_u),
        (0, 0),
        "BUG #698: the saturation OUTLIVES the pitch shifter — {off_x} xruns \
         / {off_u} underruns in 20 s after toggling the block off."
    );
}
