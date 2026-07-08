//! Issue #670 — THE user's test, at FULL fidelity: start the REAL cpal /
//! CoreAudio streams on the REAL default devices (the actual HAL I/O threads,
//! workgroup joins, elastic buffers — everything the live app runs except the
//! GUI), load the REAL Beat It chain, inject the REAL Green Day DI through the
//! chain's DI loop, play for 60 seconds, and assert the engine's own xrun /
//! underrun counters — the exact numbers meter_wiring turns into
//! "audio overload on chain" — stay at ZERO.
//!
//! macOS + release only: needs real audio devices and meaningful timing.
//! No GUI, no human: if this is red, the audio stack fails on its own.
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use domain::ids::{BlockId, ChainId};
use hw_harness::{device_guard, hw_tests_enabled, init_registry, load_di, rig_project, BUFFER};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{AudioBlock, AudioBlockKind};

#[test]
fn real_streams_beat_it_di_loop_no_xruns() {
    if !hw_tests_enabled("real_streams_beat_it_di_loop_no_xruns") {
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
        "[#670 REAL] input='{}' output='{}' buffer={BUFFER}",
        input.name, output.name
    );

    let (project, chain_id, registry) =
        rig_project("beat_it_michael_jackson_rhythm.yaml", input, output);
    let mut controller = ProjectRuntimeController::start(&project).expect("start real streams");
    // Model A (#716): install the per-machine binding registry, then re-sync so
    // the streams resolve to the real devices.
    controller.set_io_bindings(registry);
    controller
        .sync_project(&project)
        .expect("resync with bindings");
    let di = load_di("phil-STRATO-green_day.wav", controller.sample_rate());
    controller.set_chain_di_loop(&chain_id, Some(di));

    // Let it settle, then measure a full minute of playback.
    std::thread::sleep(std::time::Duration::from_secs(2));
    let xrun0 = controller.chain_xrun_count(&chain_id);
    let under0 = controller.chain_underrun_count(&chain_id);
    std::thread::sleep(std::time::Duration::from_secs(60));
    let xruns = controller.chain_xrun_count(&chain_id) - xrun0;
    let underruns = controller.chain_underrun_count(&chain_id) - under0;

    eprintln!(
        "[#670 REAL] 60s of Green Day through real streams: xruns={xruns} underruns={underruns}"
    );
    assert_eq!(
        (xruns, underruns),
        (0, 0),
        "BUG #670: the REAL audio stack (real CoreAudio streams, no GUI) \
         recorded {xruns} xruns / {underruns} underruns in 60 s of the Beat It \
         chain playing the Green Day DI at buffer {BUFFER} — the user's live \
         'audio overload', reproduced with no human and no GUI.",
    );
}

/// Issue #670 follow-up, owner-reported: after toggling a block / changing a
/// param the rig clicks WHILE PLAYING (underruns: 64, 192 — output starved).
/// Suspected mechanism: the IR chain's output elastic gets its 512-frame
/// cushion ONLY on the initial build ("a rebuild runs warm and is not
/// primed"), so after any live rebuild the standing slack is ~zero and every
/// worker delay >1 period starves the output. This plays 20 s, performs a
/// live rebuild exactly like the GUI toggle path (upsert_chain), plays 60 s
/// more, and asserts the post-rebuild stretch records no damage.
#[test]
fn rebuild_while_playing_keeps_the_cushion() {
    if !hw_tests_enabled("rebuild_while_playing_keeps_the_cushion") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(input), Some(output)) = (inputs.first(), outputs.first()) else {
        panic!("no audio devices available");
    };

    // Owner-requested scenario: the Barao Vermelho - Bete Balanco preset with
    // the matching Phil DI take.
    let (project, chain_id, registry) =
        rig_project("barao_vermelho_bete_balanco.yaml", input, output);
    let mut controller = ProjectRuntimeController::start(&project).expect("start streams");
    controller.set_io_bindings(registry);
    controller
        .sync_project(&project)
        .expect("resync with bindings");
    let di = load_di(
        "phil-STRATO-barao_vermelho-bete-balan\u{e7}o.wav",
        controller.sample_rate(),
    );
    controller.set_chain_di_loop(&chain_id, Some(di.clone()));

    std::thread::sleep(std::time::Duration::from_secs(2));
    let x0 = controller.chain_xrun_count(&chain_id);
    let u0 = controller.chain_underrun_count(&chain_id);
    std::thread::sleep(std::time::Duration::from_secs(20));
    let pre_x = controller.chain_xrun_count(&chain_id) - x0;
    let pre_u = controller.chain_underrun_count(&chain_id) - u0;

    // Live rebuilds while playing, exactly like the GUI: (1) change a NAM
    // param (forces a real rebuild incl. model reload — an identical upsert
    // takes the reuse path and proves nothing), (2) toggle a block off and
    // (3) back on.
    let mut edited = project.clone();
    {
        use domain::value_objects::ParameterValue;
        let chain = &mut edited.chains[0];
        let nam = chain
            .blocks
            .iter_mut()
            .find(|b| matches!(&b.kind, AudioBlockKind::Core(c) if c.model.starts_with("nam_")))
            .expect("a NAM block");
        if let AudioBlockKind::Core(c) = &mut nam.kind {
            c.params.insert("output_db", ParameterValue::Float(-1.0));
        }
    }
    controller
        .upsert_chain(&edited, &edited.chains[0])
        .expect("param-change rebuild");
    controller.set_chain_di_loop(&chain_id, Some(di.clone()));
    std::thread::sleep(std::time::Duration::from_secs(3));
    let mut toggled = edited.clone();
    {
        let chain = &mut toggled.chains[0];
        let nam = chain
            .blocks
            .iter_mut()
            .find(|b| matches!(&b.kind, AudioBlockKind::Core(c) if c.model.starts_with("nam_")))
            .expect("a NAM block");
        nam.enabled = false;
    }
    controller
        .upsert_chain(&toggled, &toggled.chains[0])
        .expect("toggle-off rebuild");
    controller.set_chain_di_loop(&chain_id, Some(di.clone()));
    std::thread::sleep(std::time::Duration::from_secs(3));
    controller
        .upsert_chain(&edited, &edited.chains[0])
        .expect("toggle-on rebuild");
    controller.set_chain_di_loop(&chain_id, Some(di));

    std::thread::sleep(std::time::Duration::from_secs(2));
    let x1 = controller.chain_xrun_count(&chain_id);
    let u1 = controller.chain_underrun_count(&chain_id);
    std::thread::sleep(std::time::Duration::from_secs(60));
    let post_x = controller.chain_xrun_count(&chain_id) - x1;
    let post_u = controller.chain_underrun_count(&chain_id) - u1;

    eprintln!(
        "[#670 REBUILD] pre-rebuild 20s: xruns={pre_x} underruns={pre_u}  |  post-rebuild 60s: xruns={post_x} underruns={post_u}"
    );
    assert_eq!(
        (post_x, post_u),
        (0, 0),
        "BUG #670: after a live rebuild (block toggle / param change) the chain \
         recorded {post_x} xruns / {post_u} underruns in 60 s while playing — \
         the post-rebuild output cushion is gone and every worker delay starves \
         the output (the owner's clicks)."
    );
}

/// Issue #670 — owner-reported: ADDING the ir_marshall_1960bv_4x12 cab (the owner's exact pick) to Beat It while
/// playing floods the worker (every buffer 3-7 ms, backlog pinned at 14 =
/// ring saturated) and never recovers. Suspected death spiral: the added IR's
/// lazy first-call init (FFT planning + allocations) runs ON the worker → a
/// multi-ms spike → backlog builds → the worker runs chronically over its
/// declared RT budget → the kernel demotes it to an efficiency core → every
/// buffer is now multi-ms forever. Reproduce: play, add the cab live, play on.
#[test]
fn adding_a_cab_live_does_not_spiral() {
    if !hw_tests_enabled("adding_a_cab_live_does_not_spiral") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(input), Some(output)) = (inputs.first(), outputs.first()) else {
        panic!("no audio devices available");
    };

    // The owner's app runs the WHOLE project — their rig has 3+ chains, so
    // 3+ DSP workers (each with its idle spin) plus the GUI all compete on
    // the same machine. One isolated chain never reproduced the spiral; the
    // multi-chain shape is the live condition.
    let (mut project, chain_id, registry) =
        rig_project("beat_it_michael_jackson_rhythm.yaml", input, output);
    for i in 1..3 {
        let mut extra = project.chains[0].clone();
        extra.id = ChainId(format!("issue-670-extra-{i}"));
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

    // Live ADD of the v30 cab, exactly like the GUI picker (block factory
    // shape: Core(cab, ir_v30_4x12) with the first capture's axis seed).
    let mut edited = project.clone();
    {
        use domain::value_objects::ParameterValue;
        let chain = &mut edited.chains[0];
        let mut params = block_core::param::ParameterSet::default();
        params.insert("output_db", ParameterValue::Float(-20.0));
        let out_pos = chain.blocks.len() - 1;
        chain.blocks.insert(
            out_pos,
            AudioBlock {
                id: BlockId("added-v30".into()),
                enabled: true,
                kind: AudioBlockKind::Core(project::block::CoreBlock {
                    effect_type: block_core::EFFECT_TYPE_CAB.into(),
                    model: "ir_marshall_1960bv_4x12".into(),
                    params,
                }),
            },
        );
    }
    // Owner's machine has the GUI + browser competing; on an idle test box
    // the spiral needs that nudge. Saturate the cores DURING the add and for
    // a few seconds after (the spiral window), then REMOVE the load — the
    // assertion below measures the quiet period AFTER the load is gone, so
    // a red here means the worker never recovers (the actual defect).
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
        .upsert_chain(&edited, &edited.chains[0])
        .expect("live add-cab rebuild");
    controller.set_chain_di_loop(&chain_id, Some(di));

    std::thread::sleep(std::time::Duration::from_secs(8));
    stop_load.store(true, std::sync::atomic::Ordering::Relaxed);
    for l in loaders {
        let _ = l.join();
    }
    // Quiet settling, then measure WITHOUT any load: only a non-recovering
    // (kernel-demoted) worker still fails here.
    std::thread::sleep(std::time::Duration::from_secs(4));
    let x0 = controller.chain_xrun_count(&chain_id);
    let u0 = controller.chain_underrun_count(&chain_id);
    std::thread::sleep(std::time::Duration::from_secs(30));
    let xruns = controller.chain_xrun_count(&chain_id) - x0;
    let underruns = controller.chain_underrun_count(&chain_id) - u0;

    eprintln!(
        "[#670 ADDCAB] 30s after live-adding the v30 cab: xruns={xruns} underruns={underruns}"
    );
    assert_eq!(
        (xruns, underruns),
        (0, 0),
        "BUG #670: after live-adding the ir_v30_4x12 cab the chain recorded \
         {xruns} xruns / {underruns} underruns in 30 s — the owner's worker \
         death spiral (lazy IR init on the worker → backlog → kernel demotion)."
    );
}
