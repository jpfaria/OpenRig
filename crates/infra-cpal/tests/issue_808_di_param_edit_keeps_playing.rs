//! #808 — the owner's report: "troquei o parâmetro e o DI PAROU de tocar".
//!
//! With a DI playing on a chain that was NEVER enabled, changing a block
//! parameter must (a) NOT stop the DI and (b) reach it — the rendered level
//! follows the new value.
//!
//! This defect only reproduces with a REAL output stream: the headless tests
//! drive a light block and drain the playback cell by hand, so they stayed
//! green while the rig went silent. The signal here is the playback's OUT peak,
//! which only the real output callback writes — if the DI stops, it goes silent.
//!
//! Real-hardware battery (`OPENRIG_HW_TESTS=1`, macOS release, idle machine).
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use hw_harness::{device_guard, hw_tests_enabled, init_registry, load_di_pcm, BUFFER};
use infra_cpal::{list_output_device_descriptors, ProjectRuntimeController};
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;

const CHAIN_ID: &str = "di808-param";
const RATE: u32 = 48_000;
/// The playback's OUT peak is a linear amplitude; below this the DI is silent.
const SILENT: f32 = 1e-4;

fn gain_params(volume_pct: f32) -> ParameterSet {
    let schema = schema_for_block_model("gain", "volume").expect("volume schema");
    let mut ps = ParameterSet::default();
    ps.insert("volume", ParameterValue::Float(volume_pct));
    ps.normalized_against(&schema).expect("normalize")
}

/// The chain stays DISABLED — the owner never enables it; only the DI plays.
fn chain(volume_pct: f32) -> Chain {
    Chain {
        id: ChainId(CHAIN_ID.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![AudioBlock {
            id: BlockId("di808:gain".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: gain_params(volume_pct),
            }),
        }],
        di_output: None,
    }
}

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

#[test]
fn a_param_edit_does_not_stop_a_playing_di() {
    if !hw_tests_enabled("a_param_edit_does_not_stop_a_playing_di") {
        return;
    }
    let _device = device_guard();
    init_registry();

    let outputs = list_output_device_descriptors().expect("outputs");
    let out = outputs.first().expect("at least one output device");
    let bindings = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId(out.id.clone()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    let project = |volume_pct: f32| Project {
        name: None,
        device_settings: vec![DeviceSettings {
            device_id: DeviceId(out.id.clone()),
            sample_rate: RATE,
            buffer_size_frames: BUFFER,
            bit_depth: 32,
        }],
        chains: vec![chain(volume_pct)],
        midi: None,
    };

    // Nothing enabled — exactly the owner's flow: open, hit play on the DI.
    let mut controller =
        ProjectRuntimeController::start_with_io_bindings(&project(15.0), bindings.clone())
            .expect("controller");
    let cid = ChainId(CHAIN_ID.into());

    controller
        .arm_di_stream(&chain(15.0), load_di_pcm("phil-STRATO-green_day.wav"))
        .expect("arm DI");

    assert!(
        wait_until(
            || out_peak(&controller, &cid) > SILENT,
            Duration::from_secs(10)
        ),
        "#808 precondition: the DI never reached the output with no chain enabled"
    );
    let quiet = out_peak(&controller, &cid);

    // The owner's action: change the block's parameter while the DI plays. A
    // DI-only (disabled) chain takes upsert_chain — the GUI's live-sync path.
    controller
        .upsert_chain(&project(100.0), &chain(100.0))
        .expect("param edit must not error");

    // (a) it must NEVER go silent from here on.
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut louder = false;
    while Instant::now() < deadline {
        let p = out_peak(&controller, &cid);
        assert!(
            p > SILENT,
            "#808: the DI STOPPED after the param edit (out peak {p:.5} \
             fell silent) — the owner's 'troquei o parâmetro e o DI parou'."
        );
        if p > quiet * 2.0 {
            louder = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // (b) and the edit must actually reach the DI's tone.
    assert!(
        louder,
        "#808: the param edit never reached the DI — out peak stayed at \
         {quiet:.5} (opened the gain 15% -> 100%), the owner's 'muda o \
         parâmetro e não muda o timbre'."
    );
}
