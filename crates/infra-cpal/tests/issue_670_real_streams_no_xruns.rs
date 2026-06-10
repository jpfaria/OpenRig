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
fn real_streams_beat_it_di_loop_no_xruns() {
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

    // The real Beat It chain wired to the REAL default devices.
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets/beat_it_michael_jackson_rhythm.yaml");
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
    let chain_id = ChainId("issue-670-real".into());
    let project = Project {
        name: Some("issue-670-real-streams".into()),
        device_settings: vec![
            DeviceSettings {
                device_id: DeviceId(input.id.clone()),
                sample_rate: 48_000,
                buffer_size_frames: BUFFER,
                bit_depth: 32,
            },
            DeviceSettings {
                device_id: DeviceId(output.id.clone()),
                sample_rate: 48_000,
                buffer_size_frames: BUFFER,
                bit_depth: 32,
            },
        ],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 139.0,
            blocks,
        }],
        midi: None,
    };

    // REAL streams: real cpal devices, real HAL threads, real workgroup joins.
    let controller = ProjectRuntimeController::start(&project).expect("start real streams");

    // Inject the REAL Green Day DI through the chain's DI loop — the engine
    // plays it instead of the live input, no human needed.
    let (samples, src_sr, channels) = {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/di-loops/phil-STRATO-green_day.wav");
        let mut reader = hound::WavReader::open(&path).expect("DI loop");
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
        (samples, spec.sample_rate, spec.channels as usize)
    };
    let di = Arc::new(engine::DiLoop::from_samples(
        &samples,
        src_sr,
        channels,
        controller.sample_rate(),
        256,
    ));
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
