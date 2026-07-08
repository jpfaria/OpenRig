//! Issue #698 — the owner's real complaint, at his real settings: buffer 64
//! @ 44.1 kHz (1451 us period). Two phases:
//!
//! 1. SOLO — the owner's chain 1 (NAM eternity + NAM dumble + 3 LV2s +
//!    limiter) playing alone must record zero damage.
//! 2. DUAL — chain 1 AND chain 2 (big_muff + JMP-1 + mesa IR) playing
//!    simultaneously, like two guitarists. The owner reports the sound
//!    "interferes" and crackles when both play. Each chain has its own
//!    isolated worker; damage counters are per chain.
//!
//! Needs the owner's capture library:
//!
//! ```sh
//! OPENRIG_HW_TESTS=1 \
//! OPENRIG_OWNER_PLUGINS=<OpenRig-plugins/plugins/source> \
//! cargo test -p infra-cpal --release --test issue_698_owner_64_dual_chain
//! ```
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use domain::ids::ChainId;
use hw_harness::{
    device_guard, hw_tests_enabled, init_registry_with_root, load_di, rig_project_with,
};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};

#[test]
fn owner_chains_at_64_frames_play_clean_solo_and_dual() {
    if !hw_tests_enabled("owner_chains_at_64_frames_play_clean_solo_and_dual") {
        return;
    }
    let Some(plugins_root) = std::env::var_os("OPENRIG_OWNER_PLUGINS") else {
        eprintln!(
            "[#698 HW] owner_chains_at_64_frames_play_clean_solo_and_dual: SKIPPED — \
             needs OPENRIG_OWNER_PLUGINS=<OpenRig-plugins/plugins/source>."
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

    // Owner settings: 44.1 kHz / 64 frames — the 1451 us period in his log.
    let (mut project, chain1_id, registry) =
        rig_project_with("issue_698_owner_chain1.yaml", input, output, 44_100, 64);
    let (chain2_project, _, _) =
        rig_project_with("issue_698_owner_chain2.yaml", input, output, 44_100, 64);
    let chain2_id = ChainId("issue-698-chain2".into());
    {
        let mut chain2 = chain2_project.chains[0].clone();
        chain2.id = chain2_id.clone();
        project.chains.push(chain2);
    }

    let mut controller = ProjectRuntimeController::start(&project).expect("start streams");
    controller.set_io_bindings(registry.clone());
    controller
        .sync_project(&project)
        .expect("resync with bindings");
    let di1 = load_di("phil-STRATO-green_day.wav", controller.sample_rate());

    // ── Phase 1: SOLO — only chain 1 plays.
    controller.set_chain_di_loop(&chain1_id, Some(di1.clone()));
    std::thread::sleep(std::time::Duration::from_secs(3));
    let x0 = controller.chain_xrun_count(&chain1_id);
    let u0 = controller.chain_underrun_count(&chain1_id);
    std::thread::sleep(std::time::Duration::from_secs(20));
    let solo_x = controller.chain_xrun_count(&chain1_id) - x0;
    let solo_u = controller.chain_underrun_count(&chain1_id) - u0;
    eprintln!("[#698 SOLO-64] chain1 20s alone: xruns={solo_x} underruns={solo_u}");

    // ── Phase 2: DUAL — both chains play simultaneously.
    let di2 = load_di("phil-STRATO-green_day.wav", controller.sample_rate());
    controller.set_chain_di_loop(&chain2_id, Some(di2));
    std::thread::sleep(std::time::Duration::from_secs(3));
    let x1a = controller.chain_xrun_count(&chain1_id);
    let u1a = controller.chain_underrun_count(&chain1_id);
    let x2a = controller.chain_xrun_count(&chain2_id);
    let u2a = controller.chain_underrun_count(&chain2_id);
    std::thread::sleep(std::time::Duration::from_secs(20));
    let dual1_x = controller.chain_xrun_count(&chain1_id) - x1a;
    let dual1_u = controller.chain_underrun_count(&chain1_id) - u1a;
    let dual2_x = controller.chain_xrun_count(&chain2_id) - x2a;
    let dual2_u = controller.chain_underrun_count(&chain2_id) - u2a;
    eprintln!(
        "[#698 DUAL-64] both playing 20s: chain1 xruns={dual1_x} underruns={dual1_u} | \
         chain2 xruns={dual2_x} underruns={dual2_u}"
    );

    // ── Phase 3: the owner's REAL project shape — FIVE chains, five RT
    // workers each declaring an 85%-of-period computation budget. If the
    // kernel's time-constraint admission demotes workers under this
    // overcommit, damage appears here that two chains never show (the
    // owner's "everything interferes" with his 5-chain project).
    drop(controller);
    let mut five = project.clone();
    for i in 3..6 {
        let mut extra = five.chains[0].clone();
        extra.id = ChainId(format!("issue-698-five-{i}"));
        five.chains.push(extra);
    }
    let mut controller = ProjectRuntimeController::start(&five).expect("start 5-chain streams");
    controller.set_io_bindings(registry);
    controller
        .sync_project(&five)
        .expect("resync with bindings");
    let all_ids: Vec<ChainId> = five.chains.iter().map(|c| c.id.clone()).collect();
    for id in &all_ids {
        let di = load_di("phil-STRATO-green_day.wav", controller.sample_rate());
        controller.set_chain_di_loop(id, Some(di));
    }
    std::thread::sleep(std::time::Duration::from_secs(5));
    let base: Vec<(u64, u64)> = all_ids
        .iter()
        .map(|id| {
            (
                controller.chain_xrun_count(id),
                controller.chain_underrun_count(id),
            )
        })
        .collect();
    std::thread::sleep(std::time::Duration::from_secs(20));
    let mut five_x = 0u64;
    let mut five_u = 0u64;
    for (id, (x0, u0)) in all_ids.iter().zip(base) {
        five_x += controller.chain_xrun_count(id) - x0;
        five_u += controller.chain_underrun_count(id) - u0;
    }
    eprintln!("[#698 FIVE-64] five chains playing 20s: total xruns={five_x} underruns={five_u}");

    assert_eq!(
        (solo_x, solo_u),
        (0, 0),
        "BUG #698: the owner's chain 1 (NAM x2 + 3 LV2s) does NOT fit the \
         64-frame budget even ALONE — {solo_x} xruns / {solo_u} underruns in \
         20 s on an idle machine (the owner's continuous crackle at 64)."
    );
    assert_eq!(
        (dual1_x + dual2_x, dual1_u + dual2_u),
        (0, 0),
        "BUG #698: with TWO chains playing (two guitarists) the workers \
         interfere — chain1 {dual1_x}/{dual1_u}, chain2 {dual2_x}/{dual2_u} \
         xruns/underruns in 20 s. Each stream is supposed to be an isolated \
         runtime; cross-stream timing pressure is damage."
    );
    assert_eq!(
        (five_x, five_u),
        (0, 0),
        "BUG #698: with FIVE chains playing (the owner's real project shape) \
         the chains recorded {five_x} xruns / {five_u} underruns in 20 s — \
         five RT workers each declaring 85% of the period overcommit the \
         time-constraint band and the kernel demotes them to E-cores, where \
         the chain no longer fits the budget."
    );
}
