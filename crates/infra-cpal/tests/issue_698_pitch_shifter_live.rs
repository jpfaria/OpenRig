//! Issue #698 — owner-reported: inserting the native Pitch Shifter
//! (`native_pitch_shifter`, NEUTRAL params: 0 semitones, 0 cents, mix 100%)
//! into a live chain turned the output into a continuous buzz. The worker
//! log shows sustained saturation, not a transient: every buffer 7-17 ms
//! against a 2.9 ms period, backlog pinned at 14 (ring full), and the #670
//! saturation recovery fires and immediately spirals again.
//!
//! Full-fidelity reproduction: REAL CoreAudio streams, the REAL Beat It
//! chain (NAM + IR), the REAL Green Day DI playing through the DI loop,
//! multi-chain shape (the owner's project runs 3+ chains). Insert the pitch
//! shifter mid-playback exactly like a GUI edit lands on the runtime
//! (upsert_chain), then ALSO toggle it off again — the owner disabled the
//! block trying to escape the buzz.
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use domain::ids::{BlockId, ChainId};
use hw_harness::{device_guard, hw_tests_enabled, init_registry, load_di, rig_project};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{AudioBlock, AudioBlockKind};

/// The owner's exact block, as the GUI seeded it (project.yaml as saved):
/// neutral shift, full wet.
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
fn inserting_the_pitch_shifter_live_does_not_saturate() {
    if !hw_tests_enabled("inserting_the_pitch_shifter_live_does_not_saturate") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(input), Some(output)) = (inputs.first(), outputs.first()) else {
        panic!("no audio devices available");
    };

    // The owner's project runs 3+ chains → 3+ DSP workers competing plus the
    // GUI. One isolated chain never reproduced the #670 spiral; keep the
    // multi-chain shape here too.
    let (mut project, chain_id, registry) =
        rig_project("beat_it_michael_jackson_rhythm.yaml", input, output);
    for i in 1..3 {
        let mut extra = project.chains[0].clone();
        extra.id = ChainId(format!("issue-698-extra-{i}"));
        project.chains.push(extra);
    }
    let mut controller = ProjectRuntimeController::start(&project).expect("start streams");
    controller.set_io_bindings(registry);
    controller
        .sync_project(&project)
        .expect("resync with bindings");
    let di = load_di("phil-STRATO-green_day.wav", controller.sample_rate());
    controller.set_chain_di_loop(&chain_id, Some(di.clone()));

    std::thread::sleep(std::time::Duration::from_secs(10));

    // Live INSERT of the pitch shifter right before the output block, the
    // same runtime path a GUI edit lands on.
    let mut with_pitch = project.clone();
    {
        let chain = &mut with_pitch.chains[0];
        let out_pos = chain.blocks.len() - 1;
        chain.blocks.insert(out_pos, owner_pitch_block(true));
    }
    // Same nudge as the #670 add-cab repro: the owner's machine always has
    // the GUI + desktop apps competing; saturate the cores during the insert
    // window, then REMOVE the load. The assertions measure the quiet period
    // AFTER the load is gone — red means the worker never recovers.
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
        "[#698 PITCH-IN] 30s after live-inserting native_pitch_shifter: \
         xruns={with_x} underruns={with_u}"
    );

    // The owner's escape attempt: toggle the block OFF while still playing.
    // The buzz must stop — a red here means the saturation outlives the
    // block that caused it.
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
        "[#698 PITCH-OFF] 20s after toggling the pitch shifter off: \
         xruns={off_x} underruns={off_u}"
    );

    assert_eq!(
        (with_x, with_u),
        (0, 0),
        "BUG #698: with native_pitch_shifter (neutral params) live-inserted \
         the chain recorded {with_x} xruns / {with_u} underruns in 30 s on an \
         unloaded machine — the owner's bee-box buzz (worker chronically over \
         budget, backlog pinned, saturation recovery spiraling)."
    );
    assert_eq!(
        (off_x, off_u),
        (0, 0),
        "BUG #698: the saturation OUTLIVES the pitch shifter — {off_x} xruns \
         / {off_u} underruns in 20 s after toggling the block off."
    );
}
