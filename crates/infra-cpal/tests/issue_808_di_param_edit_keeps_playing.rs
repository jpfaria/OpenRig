//! #808 — the owner's report: "troquei o parâmetro e o DI PAROU de tocar".
//!
//! With a DI playing on a chain that was NEVER enabled, changing a block
//! parameter must not stop it. The rig that reproduces it runs a NAM amp: the
//! render is heavy and an edit rebuilds it, which a light block never exercises
//! (the headless tests drive a gain block and drain the playback cell by hand,
//! so they stayed green while the rig went silent).
//!
//! The signal is the playback's OUT peak — written only by the real output
//! callback, so it drops to zero the instant the DI stops reaching the device.
//!
//! Real-hardware battery (`OPENRIG_HW_TESTS=1`, macOS release, idle machine).
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use std::time::{Duration, Instant};

use domain::ids::ChainId;
use hw_harness::{device_guard, hw_tests_enabled, init_registry, load_di_pcm, rig_project};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::AudioBlock;
use project::chain::Chain;

/// The playback's OUT peak is a linear amplitude; below this the DI is silent.
const SILENT: f32 = 1e-4;

/// The playback's OUT peak — written only by the real output callback, so it is
/// zero the moment the DI stops reaching the device.
fn out_peak(controller: &ProjectRuntimeController, cid: &ChainId) -> f32 {
    controller
        .di_playback_peaks(cid)
        .map(|(_, o)| o)
        .unwrap_or(0.0)
}

/// Poll `cond` until it holds or the deadline passes (LAW 17 — never a sleep).
fn wait_until(mut cond: impl FnMut() -> bool, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    cond()
}

/// Change the NAM's OUTPUT level — the owner's action, on a parameter whose
/// effect is measurable in the rendered peak.
fn edit_nam_output_db(chain: &Chain, value: f64) -> Chain {
    let mut edited = chain.clone();
    let nam: &mut AudioBlock = edited
        .blocks
        .iter_mut()
        .find(|b| {
            b.model_ref()
                .map(|m| m.model.starts_with("nam_"))
                .unwrap_or(false)
        })
        .expect("the preset must carry a NAM block");
    project::block::param_writer::set_parameter_number(nam, "output_db", value)
        .expect("NAM output_db is a number parameter");
    edited
}

/// Loudest OUT peak the real output callback reported over `window`.
fn max_out_peak(
    controller: &ProjectRuntimeController,
    cid: &ChainId,
    window: Duration,
) -> f32 {
    let deadline = Instant::now() + window;
    let mut max = 0.0f32;
    while Instant::now() < deadline {
        max = max.max(out_peak(controller, cid));
        std::thread::sleep(Duration::from_millis(20));
    }
    max
}

#[test]
fn a_nam_param_edit_does_not_stop_a_playing_di() {
    if !hw_tests_enabled("a_nam_param_edit_does_not_stop_a_playing_di") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("inputs");
    let outputs = list_output_device_descriptors().expect("outputs");
    let (mut project, chain_id, bindings) = rig_project(
        "barao_vermelho_bete_balanco.yaml",
        inputs.first().expect("an input device"),
        outputs.first().expect("an output device"),
    );
    // The owner never enables the chain — only the DI plays.
    project.chains[0].enabled = false;
    project.chains[0].volume = 100.0;
    let chain = project.chains[0].clone();

    let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, bindings)
        .expect("controller");

    controller
        .arm_di_stream(&chain, load_di_pcm("phil-STRATO-green_day.wav"))
        .expect("arm DI");

    assert!(
        wait_until(
            || out_peak(&controller, &chain_id) > SILENT,
            Duration::from_secs(20)
        ),
        "#808 precondition: the DI never reached the output with no chain enabled"
    );

    let before = max_out_peak(&controller, &chain_id, Duration::from_secs(3));

    // The owner's action: change a NAM parameter while the DI plays. A DI-only
    // (disabled) chain takes upsert_chain — the GUI's live-sync path. -24 dB on
    // the amp's output is unmistakable in the rendered peak.
    let edited = edit_nam_output_db(&chain, -24.0);
    project.chains[0] = edited.clone();
    controller
        .upsert_chain(&project, &edited)
        .expect("param edit must not error");

    // (a) it must NEVER fall silent from here on.
    let deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline {
        let p = out_peak(&controller, &chain_id);
        assert!(
            p > SILENT,
            "#808: the DI STOPPED after the NAM param edit (out peak {p:.5} \
             fell silent) — the owner's 'troquei o parâmetro e o DI parou'."
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    // (b) and the edit must reach the DI's tone.
    let after = max_out_peak(&controller, &chain_id, Duration::from_secs(3));
    assert!(
        after < before * 0.5,
        "#808: the NAM param edit never reached the DI — peak stayed at \
         {after:.5} (was {before:.5}) after dropping the amp output 24 dB. \
         The owner's 'mudo o parâmetro e o timbre não muda'."
    );
}
