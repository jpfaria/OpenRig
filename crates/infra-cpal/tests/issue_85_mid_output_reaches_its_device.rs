//! Issue #85 — THE acceptance evidence, on real CoreAudio streams: a chain
//! whose head/tail I/O is one E/S carries a mid `Output` pointing at ANOTHER
//! interface, and the signal must actually come out of that interface.
//!
//! The user's report: "coloquei um output depois do cab, para sair na TEYUN, e
//! não está saindo nada lá". Headless unit tests proved the tap emits inside
//! the engine and that the device map lists the tap's device; only a real run
//! proves the whole path — device resolution, stream opening, the output
//! callback picking this runtime up, and PCM arriving on the other interface.
//!
//! How it is measured without a guitar and without making noise:
//!   * the chain's source is a DI loop (a synthesized tone), so no player and
//!     no live input are needed;
//!   * the mid `Output` writes to **BlackHole 2ch**, a loopback device, and the
//!     test opens BlackHole's INPUT with a plain cpal stream to hear what
//!     arrived;
//!   * the chain's own tail output goes to a silent virtual device so nothing
//!     reaches the speakers.
//!
//! Owner action (cannot run in CI — needs the real devices):
//! ```sh
//! OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
//!     --test issue_85_mid_output_reaches_its_device -- --nocapture
//! ```
#![cfg(target_os = "macos")]

mod hw_harness;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use hw_harness::{device_guard, hw_tests_enabled, init_registry, BUFFER};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, AudioDeviceDescriptor,
    ProjectRuntimeController,
};
use project::block::{AudioBlock, AudioBlockKind, OutputBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

const RATE: u32 = 48_000;
const LOOPBACK: &str = "BlackHole";
/// A virtual device that goes nowhere audible — keeps the battery silent.
const SILENT: &str = "MJAudioRecorder";

fn descriptor<'a>(
    devices: &'a [AudioDeviceDescriptor],
    needle: &str,
) -> Option<&'a AudioDeviceDescriptor> {
    devices.iter().find(|d| d.name.contains(needle))
}

fn settings(device_id: &str) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(device_id.into()),
        sample_rate: RATE,
        buffer_size_frames: BUFFER,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

/// One second of a 440 Hz tone at -6 dBFS, mono — the chain's DI source.
fn di_tone() -> Arc<engine::DiPcm> {
    let frames = RATE as usize;
    let step = 2.0 * std::f32::consts::PI * 440.0 / RATE as f32;
    let samples: Vec<f32> = (0..frames).map(|i| 0.5 * (i as f32 * step).sin()).collect();
    Arc::new(engine::DiPcm::new(samples, RATE, 1))
}

/// A unity-gain block so the chain is a wire: the tap must carry the DI tone,
/// not a modelled tone whose level depends on a preset.
fn wire_block() -> AudioBlock {
    let preset = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join("beat_it_michael_jackson_rhythm.yaml");
    infra_yaml::load_chain_preset_file(&preset)
        .expect("preset")
        .blocks
        .into_iter()
        .next()
        .expect("preset has at least one block")
}

/// Runs the rig and returns the peak observed on the loopback interface.
/// `mid` decides whether the loopback is reached through a mid `Output` block
/// (the #85 case) or is simply the chain's own tail output (the control).
fn peak_on_loopback(mid: bool) -> f32 {
    // Quiet by default; `RUST_LOG=info` surfaces the device/stream trace.
    let _ = env_logger::try_init();
    let _device = device_guard();
    init_registry();

    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let Some(loop_out) = descriptor(&outputs, LOOPBACK) else {
        panic!("#85 needs the BlackHole 2ch loopback OUTPUT to measure the tap");
    };
    let Some(loop_in) = descriptor(&inputs, LOOPBACK) else {
        panic!("#85 needs the BlackHole 2ch loopback INPUT to measure the tap");
    };
    // The chain's own I/O: any input (the DI loop replaces it) and an output
    // device that is NOT the loopback, so the tap's audio can only have come
    // from the tap itself.
    // Prefer a silent virtual device for the chain's own E/S so the run makes
    // no sound; fall back to any non-loopback device.
    let quiet = |devices: &[AudioDeviceDescriptor]| -> Option<AudioDeviceDescriptor> {
        devices
            .iter()
            .find(|d| d.name.contains(SILENT))
            .or_else(|| devices.iter().find(|d| !d.name.contains(LOOPBACK)))
            .cloned()
    };
    // The INPUT stream is what clocks the chain (the DI loop only replaces the
    // samples), so it must be a device that really delivers callbacks — a
    // virtual recorder sits idle and nothing would ever be processed.
    let Some(chain_in) = inputs
        .iter()
        .find(|d| d.name.contains(LOOPBACK))
        .or_else(|| {
            inputs
                .iter()
                .find(|d| !d.name.contains(LOOPBACK) && !d.name.contains(SILENT))
        })
        .cloned()
    else {
        panic!("#85 needs one non-loopback input device for the chain's own E/S");
    };
    let Some(chain_out) = quiet(&outputs) else {
        panic!("#85 needs one non-loopback output device for the chain's own E/S");
    };
    eprintln!(
        "[#85 REAL] chain E/S in='{}' out='{}' | mid Output → '{}' (measured on '{}')",
        chain_in.name, chain_out.name, loop_out.name, loop_in.name
    );

    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId(chain_in.id.clone()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![IoEndpoint {
                name: "out0".into(),
                // Control: the chain's OWN tail output is the loopback, so the
                // measurement does not depend on the mid port at all.
                device_id: DeviceId(if mid {
                    chain_out.id.clone()
                } else {
                    loop_out.id.clone()
                }),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: Vec::new(),
            outputs: vec![IoEndpoint {
                name: "aux-out".into(),
                device_id: DeviceId(loop_out.id.clone()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        },
    ];

    let chain_id = ChainId("issue-85-mid-output-real".into());
    let project = Project {
        name: Some("issue-85-mid-output-real".into()),
        device_settings: vec![
            settings(&chain_in.id),
            settings(&chain_out.id),
            settings(&loop_out.id),
        ],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["main".into()],
            blocks: if mid {
                vec![
                    wire_block(),
                    AudioBlock {
                        id: BlockId("issue85:mid-output".into()),
                        enabled: true,
                        kind: AudioBlockKind::Output(OutputBlock {
                            model: "standard".into(),
                            io: "aux".into(),
                            endpoint: "aux-out".into(),
                        }),
                    },
                ]
            } else {
                vec![wire_block()]
            },
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };

    // Listen on the loopback INPUT before the chain starts writing to it.
    let host = cpal::default_host();
    let device = host
        .input_devices()
        .expect("enumerate inputs")
        .find(|d| d.name().map(|n| n.contains(LOOPBACK)).unwrap_or(false))
        .expect("loopback input device");
    let config = device.default_input_config().expect("loopback config");
    let peak_milli = Arc::new(AtomicU32::new(0));
    let observed = Arc::clone(&peak_milli);
    let callbacks = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&callbacks);
    let stream = device
        .build_input_stream(
            &config.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                counted.fetch_add(1, Ordering::Relaxed);
                let mut peak = 0.0_f32;
                for s in data {
                    peak = peak.max(s.abs());
                }
                let milli = (peak * 1000.0) as u32;
                observed.fetch_max(milli, Ordering::Relaxed);
            },
            |e| eprintln!("[#85 REAL] loopback input error: {e}"),
            None,
        )
        .expect("open loopback input stream");
    stream.play().expect("start loopback input");

    // The registry must be installed BEFORE the cold activation, or the bound
    // chain resolves zero inputs and no stream ever opens (#716).
    let mut controller = ProjectRuntimeController::start_with_io_bindings(&project, registry)
        .expect("start real streams");
    // The DI loop is the source, so the tone flows without anyone playing.
    // Cold activation is built off-thread and the cpal streams are created on
    // THIS thread when the result is polled — the GUI does it on a timer.
    for _ in 0..100 {
        controller.poll_pending_rebuilds();
        if controller.stream_count(&chain_id) > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!(
        "[#85 REAL] before arming: streams={}",
        controller.stream_count(&chain_id)
    );
    controller.set_chain_di_loop(&chain_id, Some(di_tone()));
    std::thread::sleep(Duration::from_secs(4));
    eprintln!(
        "[#85 REAL] streams={} di_armed={} di_len={:?} loopback_callbacks={}",
        controller.stream_count(&chain_id),
        controller.chain_has_di_loop(&chain_id),
        controller.chain_di_loop_len(&chain_id),
        callbacks.load(Ordering::Relaxed)
    );

    let peak = peak_milli.load(Ordering::Relaxed) as f32 / 1000.0;
    controller.set_chain_di_loop(&chain_id, None);
    drop(stream);
    eprintln!(
        "[#85 REAL] peak on the loopback ({}): {peak:.3}",
        if mid {
            "through a mid Output"
        } else {
            "as the chain's tail output"
        }
    );
    peak
}

/// Control: the harness itself hears the rig. If THIS is silent the measuring
/// path is broken, not the mid port.
#[test]
fn the_rig_reaches_the_loopback_as_a_tail_output() {
    if !hw_tests_enabled("the_rig_reaches_the_loopback_as_a_tail_output") {
        return;
    }
    let peak = peak_on_loopback(false);
    assert!(
        peak > 0.01,
        "the DI tone never reached the loopback at all (peak {peak:.3})"
    );
}

#[test]
fn a_mid_output_writes_to_its_own_interface() {
    if !hw_tests_enabled("a_mid_output_writes_to_its_own_interface") {
        return;
    }
    let peak = peak_on_loopback(true);
    assert!(
        peak > 0.01,
        "#85: nothing came out of the mid Output's interface (peak {peak:.3}). \
         The port is bound to another E/S and the chain's runtime must write it."
    );
}
