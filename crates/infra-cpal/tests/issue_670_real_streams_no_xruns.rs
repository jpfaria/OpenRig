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

fn rig_project(
    preset_file: &str,
    input: &infra_cpal::AudioDeviceDescriptor,
    output: &infra_cpal::AudioDeviceDescriptor,
) -> (Project, ChainId) {
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join(preset_file);
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
    (project, chain_id)
}

fn load_di(name: &str, engine_sr: u32) -> Arc<engine::DiLoop> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/di-loops")
        .join(name);
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
    Arc::new(engine::DiLoop::from_samples(
        &samples,
        spec.sample_rate,
        spec.channels as usize,
        engine_sr,
        256,
    ))
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

    let (project, chain_id) = rig_project("beat_it_michael_jackson_rhythm.yaml", input, output);
    let controller = ProjectRuntimeController::start(&project).expect("start real streams");
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
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(input), Some(output)) = (inputs.first(), outputs.first()) else {
        panic!("no audio devices available");
    };

    // Owner-requested scenario: the Barao Vermelho - Bete Balanco preset with
    // the matching Phil DI take.
    let (project, chain_id) = rig_project("barao_vermelho_bete_balanco.yaml", input, output);
    let mut controller = ProjectRuntimeController::start(&project).expect("start streams");
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
