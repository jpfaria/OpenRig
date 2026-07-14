//! Shared harness for the real-hardware battery (issues #670, #698).
//!
//! Opens the REAL cpal / CoreAudio streams on the REAL default devices (the
//! actual HAL I/O threads, workgroup joins, elastic buffers — everything the
//! live app runs except the GUI). Gated by `OPENRIG_HW_TESTS=1` and only
//! meaningful on an otherwise idle machine (docs/testing.md →
//! "Real-hardware battery").

// Each `--test` binary compiles this module independently and uses a subset
// of the helpers; the unused ones are not dead code, they belong to the
// sibling binaries.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Once;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::AudioBlock;
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

pub const BUFFER: u32 = 64;

pub fn init_registry() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../engine/tests/fixtures/plugins");
    init_registry_with_root(&root);
}

/// Register all block builders and point the plugin registry at `root`.
/// The registry initializes once per process: the FIRST caller's root wins,
/// so a test needing a non-fixture root must run in its own process (cargo
/// gives every `--test` target one).
pub fn init_registry_with_root(root: &std::path::Path) {
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
        plugin_loader::registry::init(root);
    });
}

pub fn rig_project(
    preset_file: &str,
    input: &infra_cpal::AudioDeviceDescriptor,
    output: &infra_cpal::AudioDeviceDescriptor,
) -> (Project, ChainId, Vec<IoBinding>) {
    rig_project_with(preset_file, input, output, 48_000, BUFFER)
}

/// `rig_project` with explicit device rate/buffer — the owner runs the
/// Scarlett at 44.1 kHz / 128 frames (2.9 ms period), the battery default
/// is 48 kHz / 64.
///
/// Model A (#716): the chain selects the "io" binding for its head input /
/// tail output; the device endpoints live in the returned registry, not in
/// block `entries`. Callers must `controller.set_io_bindings(registry)` (and
/// re-sync) so the streams resolve to the real devices.
pub fn rig_project_with(
    preset_file: &str,
    input: &infra_cpal::AudioDeviceDescriptor,
    output: &infra_cpal::AudioDeviceDescriptor,
    sample_rate: u32,
    buffer_frames: u32,
) -> (Project, ChainId, Vec<IoBinding>) {
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join(preset_file);
    let blocks: Vec<AudioBlock> = infra_yaml::load_chain_preset_file(&preset)
        .expect("preset")
        .blocks;
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId(input.id.clone()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId(output.id.clone()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    let chain_id = ChainId("issue-670-real".into());
    let project = Project {
        name: Some("issue-670-real-streams".into()),
        device_settings: vec![
            DeviceSettings {
                device_id: DeviceId(input.id.clone()),
                sample_rate,
                buffer_size_frames: buffer_frames,
                bit_depth: 32,
            },
            DeviceSettings {
                device_id: DeviceId(output.id.clone()),
                sample_rate,
                buffer_size_frames: buffer_frames,
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
            io_binding_ids: vec!["io".into()],
            blocks,
            di_output: None,
        }],
        midi: None,
    };
    (project, chain_id, registry)
}

pub fn load_di(name: &str, engine_sr: u32) -> Arc<engine::DiLoop> {
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

/// Like [`load_di`] but returns the source PCM (`DiPcm`) the current
/// `set_chain_di_loop` API takes; the controller resamples to the engine rate
/// on arm. (The old `DiLoop`-returning path pre-dates the DiPcm split.)
pub fn load_di_pcm(name: &str) -> Arc<engine::DiPcm> {
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
    Arc::new(engine::DiPcm::new(
        samples,
        spec.sample_rate,
        spec.channels as usize,
    ))
}

/// Real-hardware battery gate (issue #670). These tests open the PHYSICAL
/// audio interface and assert real-time deadlines — they are only meaningful
/// on an otherwise IDLE machine, run on demand:
///
/// ```sh
/// OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
///     --test issue_670_cab_swap --test issue_670_real_streams_no_xruns \
///     --test issue_698_pitch_shifter_live
/// ```
///
/// Under the full workspace suite / quality gate the machine is saturated by
/// parallel builds and tests, and the timing assertions fail for reasons
/// unrelated to the app. Without the variable each test returns immediately
/// with a loud notice (NOT silently green). See docs/testing.md
/// ("Real-hardware battery").
pub fn hw_tests_enabled(test_name: &str) -> bool {
    if std::env::var_os("OPENRIG_HW_TESTS").is_some() {
        return true;
    }
    eprintln!(
        "[HW] {test_name}: SKIPPED — real-hardware timing test. \
         Run with OPENRIG_HW_TESTS=1 on an idle machine (docs/testing.md)."
    );
    false
}

/// Cross-PROCESS hardware lock: cargo runs separate test binaries
/// concurrently, so an in-process mutex cannot serialize the one physical
/// interface between them. A create-new lock file does (stale locks older
/// than 10 min are reclaimed; the guard removes the file on drop, including
/// on panic unwind).
pub struct DeviceFileLock;

impl Drop for DeviceFileLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(device_lock_path());
    }
}

fn device_lock_path() -> std::path::PathBuf {
    std::env::temp_dir().join("openrig-issue670-device.lock")
}

pub fn device_guard() -> DeviceFileLock {
    let path = device_lock_path();
    loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return DeviceFileLock,
            Err(_) => {
                if let Ok(meta) = std::fs::metadata(&path) {
                    if let Ok(modified) = meta.modified() {
                        if modified.elapsed().unwrap_or_default()
                            > std::time::Duration::from_secs(600)
                        {
                            let _ = std::fs::remove_file(&path);
                            continue;
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }
}
