//! Issue #980 — the owner's ANAL+DIG on his Quantum HD 8 (44.1 kHz / 64
//! frames) through the REAL cpal / CoreAudio streams must play without
//! underruns.
//!
//! Measured on the owner's rig (24-25/09): two guitars (E/S `guitarra-1` on
//! In 1, `guitarra-2` on In 2), each E/S with two stereo outputs (Main `[0,1]`
//! and Out 2 `[10,11]`), the SYN-2 insert bound but off, and the chain NAM
//! Dumble + IR Cream 4x12 + ValhallaSupermassive + CloudReverb. Every route
//! kept losing audio in bursts of 128..1536 frames; over 20 s each route's
//! `underruns` equalled its `dropped_frames` exactly (3904 = 3904 on one
//! guitar, 2944 = 2944 on the other) with `input_busy_skips` 0 — the
//! dsp-worker delivered its buffers late. The machine was running other
//! builds at the time (load 10-14).
//!
//! This test rebuilds that rig — same interface, rate, buffer, E/S, outputs,
//! insert and blocks with the owner's parameters — and plays it idle for 60 s,
//! then 30 s with every core saturated by ordinary threads (the load he had).
//! Silent by construction: the chain volume is 0 (the DSP path is identical,
//! the volume applies at the output mixdown) and the insert is off.
//!
//! macOS + release + `OPENRIG_HW_TESTS=1` + `OPENRIG_OWNER_PLUGINS=<OpenRig-
//! plugins/plugins/source>`, the Quantum connected and the machine otherwise
//! idle; otherwise it skips with a notice.
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;
mod worker_thread_watch;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use hw_harness::{device_guard, hw_tests_enabled, init_registry_with_root};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

const RATE: u32 = 44_100;
const BUFFER: u32 = 64;
const MAIN: [usize; 2] = [0, 1];
const OUT_2: [usize; 2] = [10, 11];

fn endpoint(name: &str, dev: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn settings(dev: &str) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(dev.into()),
        sample_rate: RATE,
        buffer_size_frames: BUFFER,
        bit_depth: 32,
    }
}

/// The owner's E/S registry: two guitars, each out to Main and Out 2, plus
/// the SYN-2 loop (send `[3]`, return `[2,3]`).
fn owners_registry(input_id: &str, output_id: &str) -> Vec<IoBinding> {
    let guitar = |id: &str, channel: usize| IoBinding {
        id: id.into(),
        name: id.into(),
        inputs: vec![endpoint("in", input_id, ChannelMode::Mono, &[channel])],
        outputs: vec![
            endpoint("main", output_id, ChannelMode::Stereo, &MAIN),
            endpoint("out 2", output_id, ChannelMode::Stereo, &OUT_2),
        ],
    };
    vec![
        guitar("guitarra-1", 0),
        guitar("guitarra-2", 1),
        IoBinding {
            id: "syn2-main".into(),
            name: "SYN-2".into(),
            inputs: vec![endpoint("ret", input_id, ChannelMode::Stereo, &[2, 3])],
            outputs: vec![endpoint("snd", output_id, ChannelMode::Mono, &[3])],
        },
    ]
}

fn owners_anal_dig(input_id: &str, output_id: &str) -> (Project, ChainId) {
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets/issue_980_owner_anal_dig.yaml");
    let mut blocks: Vec<AudioBlock> = infra_yaml::load_chain_preset_file(&preset)
        .expect("preset")
        .blocks;
    blocks.insert(
        0,
        AudioBlock {
            id: BlockId("insert:980".into()),
            enabled: false,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "syn2-main".into(),
            }),
        },
    );
    let chain_id = ChainId("issue-980-anal-dig".into());
    let project = Project {
        name: Some("issue-980-anal-dig".into()),
        device_settings: vec![settings(input_id), settings(output_id)],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 0.0,
            io_binding_ids: vec!["guitarra-1".into(), "guitarra-2".into()],
            blocks,
            di_output: None,
            loopers: vec![],
            mix: Default::default(),
        }],
        midi: None,
    };
    (project, chain_id)
}

/// Drives the frontend tick (off-thread builds land through
/// `poll_pending_rebuilds`) until the chain's routes exist.
fn wait_until_streaming(controller: &mut ProjectRuntimeController, chain_id: &ChainId) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        controller.poll_pending_rebuilds();
        if controller
            .chain_output_route_stats(chain_id)
            .iter()
            .any(|(_, routes)| !routes.is_empty())
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the chain never started streaming");
}

/// Per route: (group, channels) → (underruns, dropped_frames, input_busy_skips).
type Counters = Vec<((usize, Vec<usize>), (u64, u64, u64))>;

fn counters(controller: &ProjectRuntimeController, chain_id: &ChainId) -> Counters {
    controller
        .chain_output_route_stats(chain_id)
        .into_iter()
        .flat_map(|(group, routes)| {
            routes.into_iter().map(move |r| {
                (
                    (group, r.channels.clone()),
                    (r.underruns, r.dropped_frames, r.input_busy_skips),
                )
            })
        })
        .collect()
}

/// Plays for `secs` while ticking like the app; returns each route's
/// underruns / dropped frames / busy skips over the stretch.
fn play(controller: &mut ProjectRuntimeController, chain_id: &ChainId, secs: u64) -> Counters {
    let before = counters(controller, chain_id);
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        controller.poll_pending_rebuilds();
        std::thread::sleep(Duration::from_millis(20));
    }
    counters(controller, chain_id)
        .into_iter()
        .map(|(key, (u, d, b))| {
            let (u0, d0, b0) = before
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, c)| *c)
                .unwrap_or((0, 0, 0));
            (key, (u - u0, d - d0, b - b0))
        })
        .collect()
}

fn underruns(counters: &Counters) -> u64 {
    counters.iter().map(|(_, (u, _, _))| u).sum()
}

#[test]
fn the_owners_two_guitars_into_two_outputs_play_without_underruns() {
    if !hw_tests_enabled("the_owners_two_guitars_into_two_outputs_play_without_underruns") {
        return;
    }
    let Some(plugins_root) = std::env::var_os("OPENRIG_OWNER_PLUGINS") else {
        eprintln!(
            "[#980 HW] SKIPPED — needs OPENRIG_OWNER_PLUGINS=<OpenRig-plugins/plugins/source>."
        );
        return;
    };
    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(input), Some(output)) = (
        inputs.iter().find(|d| d.name.contains("Quantum")),
        outputs.iter().find(|d| d.name.contains("Quantum")),
    ) else {
        eprintln!("[#980 HW] SKIPPED — no Quantum interface connected");
        return;
    };
    let _device = device_guard();
    // `OPENRIG_980_TRACE=1` shows WHY a run failed: every late dsp-worker
    // buffer (wall time, thread CPU time, ring backlog), every RT policy
    // re-declare, and — from the outside, through the Mach API — every change
    // of a worker's priority, policy or declared computation, all with
    // millisecond timestamps.
    let trace = std::env::var_os("OPENRIG_980_TRACE").is_some();
    if trace {
        let _ = env_logger::builder()
            .parse_filters(
                "infra_cpal::dsp_worker=trace,infra_cpal::rt_thread_policy=info,\
                 infra_cpal::memory_wiring=info,probe980=info",
            )
            .format_timestamp_millis()
            .is_test(true)
            .try_init();
    }
    let plugins_root = PathBuf::from(plugins_root);
    init_registry_with_root(&plugins_root);
    project::vst3_editor::init_vst3_catalog(RATE as f64, &[plugins_root]);

    let (project, chain_id) = owners_anal_dig(&input.id, &output.id);
    let mut controller = ProjectRuntimeController::start(&project).expect("start real streams");
    controller.set_io_bindings(owners_registry(&input.id, &output.id));
    controller
        .sync_project(&project)
        .expect("resync with bindings");
    wait_until_streaming(&mut controller, &chain_id);
    let watch = trace.then(|| worker_thread_watch::start(Duration::from_millis(1)));

    let _cold = play(&mut controller, &chain_id, 3);
    let idle = play(&mut controller, &chain_id, 60);
    eprintln!(
        "[#980 HW] idle 60 s, per route (group, channels) -> (underruns, dropped, busy): {idle:?}"
    );

    let stop = Arc::new(AtomicBool::new(false));
    let loaders: Vec<_> = (0..12)
        .map(|_| {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut x = 0.001f64;
                while !stop.load(Ordering::Relaxed) {
                    x = (x.sin().cos() + 1.0001).sqrt();
                    std::hint::black_box(x);
                }
            })
        })
        .collect();
    let loaded = play(&mut controller, &chain_id, 30);
    stop.store(true, Ordering::Relaxed);
    for l in loaders {
        let _ = l.join();
    }
    eprintln!("[#980 HW] loaded 30 s, per route (group, channels) -> (underruns, dropped, busy): {loaded:?}");
    if let Some(watch) = watch {
        eprintln!(
            "[#980 HW] worker current-priority samples (every 1 ms): {:?}",
            watch.finish()
        );
    }

    controller.stop();

    assert_eq!(
        (underruns(&idle), underruns(&loaded)),
        (0, 0),
        "the owner's ANAL+DIG must play without underruns on his interface \
         (idle 60 s, loaded 30 s); per route (group, channels) -> \
         (underruns, dropped, busy): idle {idle:?} loaded {loaded:?}"
    );
}
