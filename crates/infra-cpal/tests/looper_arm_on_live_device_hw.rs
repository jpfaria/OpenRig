//! Arming a loop's playback stream on the interface a chain is already
//! streaming on must never hang the caller.
//!
//! Closing a recording arms the loop's isolated playback stream from the GUI
//! thread. On the owner's rig the GUI froze for good on that call: the GUI
//! thread sat in `AudioOutputUnitStart` waiting on the HAL lock while the
//! chain's input IOThread, on the same device, waited inside its callback.
//! This drives the same arm/disarm on the REAL device with a running chain and
//! aborts (dumping a `sample` of the process) when one call stalls.
//!
//! macOS only, gated by `OPENRIG_HW_TESTS=1` (docs/testing.md →
//! "Real-hardware battery").
#![cfg(target_os = "macos")]

mod hw_harness;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use hw_harness::{device_guard, hw_tests_enabled, init_registry, BUFFER};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::{Chain, LooperConfig};
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;

const UID: u64 = 1;
const CYCLES: usize = 40;
const STALL: Duration = Duration::from_secs(5);

/// Aborts the process when the call in flight runs past `STALL`. A hung
/// CoreAudio start never returns, so a timeout assertion on the caller's
/// thread could never fire; the watchdog samples the process first so the
/// failure carries the stacks of the deadlock.
/// `began` holds the milliseconds since `origin` (+1, so 0 means idle) at which
/// the call in flight started.
fn start_watchdog(origin: Instant) -> Arc<AtomicU64> {
    let began_ms = Arc::new(AtomicU64::new(0));
    let watched = Arc::clone(&began_ms);
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(100));
        let began = watched.load(Ordering::Acquire);
        if began == 0 {
            continue;
        }
        let now = origin.elapsed().as_millis() as u64;
        if now.saturating_sub(began) > STALL.as_millis() as u64 {
            let out = std::env::temp_dir().join("openrig-looper-arm-hang.sample.txt");
            let _ = std::process::Command::new("sample")
                .arg(std::process::id().to_string())
                .arg("2")
                .arg("-file")
                .arg(&out)
                .status();
            eprintln!(
                "[looper-arm HW] a looper stream arm/disarm stalled for more than {}s — \
                 the caller is hung (stacks: {})",
                STALL.as_secs(),
                out.display()
            );
            let _ = std::fs::remove_file(std::env::temp_dir().join("openrig-issue670-device.lock"));
            std::process::abort();
        }
    });
    began_ms
}

fn timed<R>(began: &AtomicU64, origin: Instant, f: impl FnOnce() -> R) -> R {
    began.store(origin.elapsed().as_millis() as u64 + 1, Ordering::Release);
    let r = f();
    began.store(0, Ordering::Release);
    r
}

fn gain_block() -> AudioBlock {
    AudioBlock {
        id: BlockId("gain:0".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "ibanez_ts9".into(),
            params: ParameterSet::default()
                .normalized_against(
                    &project::block::schema_for_block_model("gain", "ibanez_ts9")
                        .expect("the native TS9 schema is compiled in"),
                )
                .expect("defaults normalize"),
        }),
    }
}

#[test]
fn a_loop_on_the_chains_own_interface_never_hangs_or_starves_the_chain() {
    if !hw_tests_enabled("a_loop_on_the_chains_own_interface_never_hangs_or_starves_the_chain") {
        return;
    }
    let _device = device_guard();
    init_registry();

    // The hang needs the chain's input and the loop's output on ONE device:
    // the first full-duplex interface, or the one whose name contains
    // `OPENRIG_HW_DEVICE` (a virtual loopback driver is not the hardware HAL
    // path the hang was seen on).
    let wanted = std::env::var("OPENRIG_HW_DEVICE").ok();
    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let device = inputs
        .iter()
        .filter(|i| outputs.iter().any(|o| o.id == i.id))
        .find(|i| wanted.as_deref().is_none_or(|w| i.name.contains(w)))
        .expect(
            "no matching full-duplex audio device — this test needs one interface with in and out",
        );
    eprintln!("[looper-arm HW] device='{}' buffer={BUFFER}", device.name);

    let chain_id = ChainId("looper-arm-hw".into());
    // The rig the hang was seen on: one input feeding two bindings, three
    // stereo outputs on the same interface (main, monitor, an external send),
    // and the loop on the chain's own outputs (no output picked).
    let out_count = outputs
        .iter()
        .find(|o| o.id == device.id)
        .map_or(2, |o| o.channels as usize);
    let pair = |first: usize| {
        if first + 1 < out_count {
            vec![first, first + 1]
        } else {
            vec![0, 1]
        }
    };
    let endpoint = |name: &str, mode: ChannelMode, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.id.clone()),
        mode,
        channels,
    };
    let registry = vec![
        IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![endpoint("in0", ChannelMode::Mono, vec![0])],
            outputs: vec![
                endpoint("main", ChannelMode::Stereo, pair(0)),
                endpoint("monitor", ChannelMode::Stereo, pair(24)),
            ],
        },
        IoBinding {
            id: "send".into(),
            name: "SEND".into(),
            inputs: vec![endpoint("in0", ChannelMode::Mono, vec![0])],
            outputs: vec![endpoint("send", ChannelMode::Stereo, pair(4))],
        },
    ];
    let chain = Chain {
        id: chain_id.clone(),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        // Volume 0 and a silent take: identical stream work, silent monitors.
        volume: 0.0,
        io_binding_ids: vec!["io".into(), "send".into()],
        blocks: vec![gain_block()],
        di_output: None,
        loopers: vec![LooperConfig::new(UID)],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    };
    let project = Project {
        name: Some("looper-arm-hw".into()),
        device_settings: vec![DeviceSettings {
            device_id: DeviceId(device.id.clone()),
            // Run at the rate the interface is already clocked at
            // (`OPENRIG_HW_RATE`), or the test re-clocks the owner's rig.
            sample_rate: std::env::var("OPENRIG_HW_RATE")
                .ok()
                .and_then(|r| r.parse().ok())
                .unwrap_or(48_000),
            buffer_size_frames: BUFFER,
            bit_depth: 32,
        }],
        chains: vec![chain.clone()],
        midi: None,
    };

    let mut controller = ProjectRuntimeController::start(&project).expect("start real streams");
    controller.set_io_bindings(registry);
    controller
        .sync_project(&project)
        .expect("resync with bindings");
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        controller.poll_pending_rebuilds();
        if !controller.runtimes_for_chain(&chain_id).is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !controller.runtimes_for_chain(&chain_id).is_empty(),
        "the chain never came up — the arms below would not share a device with a live stream"
    );
    std::thread::sleep(Duration::from_secs(1));

    // Baseline: the chain alone, no loop stream.
    let (x0, u0) = (
        controller.chain_xrun_count(&chain_id),
        controller.chain_underrun_count(&chain_id),
    );
    std::thread::sleep(Duration::from_secs(3));
    let baseline = (
        controller.chain_xrun_count(&chain_id) - x0,
        controller.chain_underrun_count(&chain_id) - u0,
    );

    let origin = Instant::now();
    let began = start_watchdog(origin);
    controller.looper_create(&chain_id, UID);
    controller.looper_load(&chain_id, UID, &vec![0.0f32; 48_000 * 2]);

    let mut slowest = Duration::ZERO;
    let (x1, u1) = (
        controller.chain_xrun_count(&chain_id),
        controller.chain_underrun_count(&chain_id),
    );
    for cycle in 0..CYCLES {
        controller.looper_play(&chain_id, UID);
        let t = Instant::now();
        timed(&began, origin, || controller.sync_looper_streams(&chain));
        slowest = slowest.max(t.elapsed());
        assert!(
            controller.looper_stream_active(&chain_id, UID),
            "cycle {cycle}: a playing loop must arm its stream — otherwise nothing \
             was started on the device and the test proves nothing"
        );
        std::thread::sleep(Duration::from_millis(250));

        controller.looper_stop(&chain_id, UID);
        let t = Instant::now();
        timed(&began, origin, || controller.sync_looper_streams(&chain));
        slowest = slowest.max(t.elapsed());
        std::thread::sleep(Duration::from_millis(150));
    }
    let armed = (
        controller.chain_xrun_count(&chain_id) - x1,
        controller.chain_underrun_count(&chain_id) - u1,
    );
    eprintln!(
        "[looper-arm HW] {CYCLES} arm/disarm cycles, slowest call {slowest:?}; \
         chain (xruns, underruns): baseline 3 s {baseline:?}, with the loop {armed:?}"
    );
    assert_eq!(
        armed,
        (0, 0),
        "a loop playing on the chain's own interface cost the live chain \
         {armed:?} (xruns, underruns) — its stream must be invisible to the \
         chain (baseline without the loop: {baseline:?})"
    );
}
