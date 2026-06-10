//! Issue #670 — the owner's exact gesture and scenario: the Barao Vermelho
//! "Bete Balanco" preset playing Phil's matching DI, then ADD the Marshall
//! 4x12 V30 cab live (the preset ships without a cab) and SWAP the cab model
//! repeatedly ("acontece quando eu troco o CAB/IR"). Real streams; assert no
//! audible damage afterwards.
//!
//! Deliberately NOT gated on release: underruns are sound damage in ANY
//! profile. Run it twice to settle the debug-build question with a test:
//!   cargo test -p infra-cpal --release --test issue_670_cab_swap   (release)
//!   cargo test -p infra-cpal           --test issue_670_cab_swap   (debug,
//!     the profile RustRover's run button launches)
#![cfg(target_os = "macos")]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Once;

use domain::ids::{BlockId, ChainId, DeviceId};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::device::DeviceSettings;
use project::project::Project;

const BUFFER: u32 = 64;

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        nam::register_builder();
        ir::register_builder();
        lv2::register_builder();
        block_dyn::register_natives();
        block_filter::register_natives();
        block_reverb::register_natives();
        block_gain::register_natives();
        block_amp::register_natives();
        block_preamp::register_natives();
        block_cab::register_natives();
        block_delay::register_natives();
        block_mod::register_natives();
        block_pitch::register_natives();
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../engine/tests/fixtures/plugins");
        plugin_loader::registry::init(&root);
    });
}

#[test]
fn swapping_the_cab_while_playing_does_not_damage_sound() {
    let _ = env_logger::builder()
        .filter_module("infra_cpal", log::LevelFilter::Debug)
        .try_init();
    init_registry();

    // The owner's REAL interface: Scarlett 2i2, the device the failure
    // happens on — NOT a virtual loopback with perfect timing.
    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let input = inputs
        .iter()
        .find(|d| d.name.contains("Scarlett"))
        .expect("Scarlett input present");
    let output = outputs
        .iter()
        .find(|d| d.name.contains("Scarlett"))
        .expect("Scarlett output present");
    eprintln!(
        "[#670 SWAP] devices: in='{}' out='{}'",
        input.name, output.name
    );

    // The owner's Bete Balanco preset wired to the real devices.
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets/barao_vermelho_bete_balanco.yaml");
    let mut blocks = vec![AudioBlock {
        id: BlockId("in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId(input.id.clone()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }];
    blocks.extend(
        infra_yaml::load_chain_preset_file(&preset)
            .expect("preset")
            .blocks,
    );
    blocks.push(AudioBlock {
        id: BlockId("out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId(output.id.clone()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    });
    let chain_id = ChainId("issue-670-swap".into());
    let project = Project {
        name: Some("issue-670-cab-swap".into()),
        device_settings: vec![
            DeviceSettings {
                device_id: DeviceId(input.id.clone()),
                sample_rate: 44_100,
                buffer_size_frames: BUFFER,
                bit_depth: 32,
            },
            DeviceSettings {
                device_id: DeviceId(output.id.clone()),
                sample_rate: 44_100,
                buffer_size_frames: BUFFER,
                bit_depth: 32,
            },
        ],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            // Volume 0: the DSP path is identical (volume applies at the
            // output mixdown); the test just stops blasting the monitors.
            volume: 0.0,
            blocks,
        }],
        midi: None,
    };

    let mut controller = ProjectRuntimeController::start(&project).expect("start streams");

    // Real DI playback.
    let di = {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/di-loops/phil-STRATO-barao_vermelho-bete-balan\u{e7}o.wav");
        let mut reader = hound::WavReader::open(&path).expect("DI");
        let spec = reader.spec();
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
            hound::SampleFormat::Int => {
                let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
                reader
                    .samples::<i32>()
                    .map(|s| s.unwrap() as f32 / max)
                    .collect()
            }
        };
        Arc::new(engine::DiLoop::from_samples(
            &samples,
            spec.sample_rate,
            spec.channels as usize,
            controller.sample_rate(),
            256,
        ))
    };
    controller.set_chain_di_loop(&chain_id, Some(di.clone()));
    std::thread::sleep(std::time::Duration::from_secs(5));

    // The owner's gesture: the preset has NO cab — ADD the Marshall 4x12 V30
    // live, then SWAP the cab model repeatedly.
    let mut current = project.clone();
    {
        let chain = &mut current.chains[0];
        let out_pos = chain.blocks.len() - 1;
        chain.blocks.insert(
            out_pos,
            AudioBlock {
                id: BlockId("added-cab".into()),
                enabled: true,
                kind: AudioBlockKind::Core(project::block::CoreBlock {
                    effect_type: block_core::EFFECT_TYPE_CAB.into(),
                    model: "ir_marshall_1960bv_4x12".into(),
                    params: application::block_factory::default_params_for_model(
                        block_core::EFFECT_TYPE_CAB,
                        "ir_marshall_1960bv_4x12",
                    )
                    .expect("defaults"),
                }),
            },
        );
    }
    controller
        .upsert_chain(&current, &current.chains[0])
        .expect("live cab add");
    controller.set_chain_di_loop(&chain_id, Some(di.clone()));
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Human cadence (owner: swapping too fast isn't the gesture) — one swap,
    // then ~6 s of playing, watching the counters PER WINDOW.
    let swap_to = [
        "ir_fender_deluxe_reverb_oxford",
        "ir_v30_4x12",
        "ir_marshall_1960bv_4x12",
    ];
    let mut swaps = 0u32;
    let mut swap_damage: u64 = 0;
    for round in 0..2 {
        for model in &swap_to {
            let bx = controller.chain_xrun_count(&chain_id);
            let bu = controller.chain_underrun_count(&chain_id);
            {
                let chain = &mut current.chains[0];
                let cab = chain
                    .blocks
                    .iter_mut()
                    .find(|b| matches!(&b.kind, AudioBlockKind::Core(c) if c.model.starts_with("ir_")))
                    .expect("cab block");
                if let AudioBlockKind::Core(c) = &mut cab.kind {
                    c.model = (*model).to_string();
                    // Fresh defaults for the new model, like ReplaceBlockModel.
                    c.params = application::block_factory::default_params_for_model(
                        block_core::EFFECT_TYPE_CAB,
                        model,
                    )
                    .expect("defaults");
                }
            }
            controller
                .upsert_chain(&current, &current.chains[0])
                .expect("live cab swap");
            controller.set_chain_di_loop(&chain_id, Some(di.clone()));
            std::thread::sleep(std::time::Duration::from_secs(6));
            swaps += 1;
            let wx = controller.chain_xrun_count(&chain_id) - bx;
            let wu = controller.chain_underrun_count(&chain_id) - bu;
            eprintln!("[#670 SWAP] swap {swaps} -> {model}: window xruns={wx} underruns={wu}");
            swap_damage += wx + wu;
        }
        eprintln!("[#670 SWAP] round {} done", round + 1);
    }

    // Measure the minute AFTER the swaps.
    std::thread::sleep(std::time::Duration::from_secs(2));
    let x0 = controller.chain_xrun_count(&chain_id);
    let u0 = controller.chain_underrun_count(&chain_id);
    std::thread::sleep(std::time::Duration::from_secs(30));
    let xruns = controller.chain_xrun_count(&chain_id) - x0;
    let underruns = controller.chain_underrun_count(&chain_id) - u0;

    eprintln!(
        "[#670 SWAP] 30s after 3x live cab swaps (profile: {}): xruns={xruns} underruns={underruns}",
        if cfg!(debug_assertions) { "DEBUG" } else { "RELEASE" },
    );
    assert_eq!(
        (swap_damage, xruns, underruns),
        (0, 0, 0),
        "BUG #670: swapping the cab/IR live damaged the sound — {swap_damage} \
         xruns+underruns across the swap windows, then {xruns}/{underruns} in \
         the 30 s after — the owner's exact gesture."
    );
}

#[test]
fn zz_list_devices() {
    for d in list_input_device_descriptors().unwrap() {
        eprintln!("IN : {} | {} | ch={}", d.id, d.name, d.channels);
    }
    for d in list_output_device_descriptors().unwrap() {
        eprintln!("OUT: {} | {} | ch={}", d.id, d.name, d.channels);
    }
}
