//! Issue #965 — an insert chain on the owner's interface (Quantum HD 8,
//! 44.1 kHz / 64 frames) through the REAL cpal / CoreAudio streams: the lean
//! cushions hold with zero xruns and zero underruns on a cold start, across a
//! minute of playing and after live edits.
//!
//! Silent by construction: the insert loop never leaves the interface — its
//! send plays on `Out 3` and its return is `In 11`, the HD 8's loopback of
//! `Out 3` — the tail plays on the free `Out 5/6`, and the chain volume is 0.
//! The chain is the heavy Beat It preset (two NAM captures and an IR cab)
//! with the insert in front, the owner's ANAL+DIG shape.
//!
//! macOS + release + `OPENRIG_HW_TESTS=1`, the Quantum connected and the
//! machine idle; otherwise it skips with a notice.
#![cfg(all(target_os = "macos", not(debug_assertions)))]

mod hw_harness;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use hw_harness::{device_guard, hw_tests_enabled, init_registry};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;

const RATE: u32 = 44_100;
const BUFFER: u32 = 64;
const GUITAR_IN: usize = 0; // In 1
const TAIL_OUT: [usize; 2] = [4, 5]; // Out 5/6, free on the owner's patchbay
const SEND_OUT: usize = 2; // Out 3
const RETURN_IN: usize = 10; // In 11 = loopback of Out 3

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

/// The owner's ANAL+DIG shape on the Quantum: insert first, then the heavy
/// preset, tail to a free output.
fn owners_insert_chain(input_id: &str, output_id: &str) -> (Project, ChainId, Vec<IoBinding>) {
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets/beat_it_michael_jackson_rhythm.yaml");
    let mut blocks: Vec<AudioBlock> = infra_yaml::load_chain_preset_file(&preset)
        .expect("preset")
        .blocks;
    blocks.insert(
        0,
        AudioBlock {
            id: BlockId("insert:965".into()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "loop965".into(),
            }),
        },
    );
    let registry = vec![
        IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![endpoint("in", input_id, ChannelMode::Mono, &[GUITAR_IN])],
            outputs: vec![endpoint("out", output_id, ChannelMode::Stereo, &TAIL_OUT)],
        },
        IoBinding {
            id: "loop965".into(),
            name: "LOOP".into(),
            inputs: vec![endpoint("ret", input_id, ChannelMode::Mono, &[RETURN_IN])],
            outputs: vec![endpoint("snd", output_id, ChannelMode::Mono, &[SEND_OUT])],
        },
    ];
    let chain_id = ChainId("issue-965-insert".into());
    let project = Project {
        name: Some("issue-965-insert".into()),
        device_settings: vec![settings(input_id), settings(output_id)],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 0.0,
            io_binding_ids: vec!["io".into()],
            blocks,
            di_output: None,
            loopers: vec![],
        }],
        midi: None,
    };
    (project, chain_id, registry)
}

/// Drives the frontend tick the app runs (off-thread builds land through
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

/// Samples every route's cushion for `secs` while ticking like the app;
/// returns (min, max) fill per route and the xruns / underruns / peak load
/// over the stretch.
fn watch(
    controller: &mut ProjectRuntimeController,
    chain_id: &ChainId,
    secs: u64,
) -> (Vec<(usize, usize)>, u64, u64, f32) {
    let x0 = controller.chain_xrun_count(chain_id);
    let u0 = controller.chain_underrun_count(chain_id);
    let _ = controller.chain_peak_load(chain_id);
    let mut fills: Vec<(usize, usize)> = Vec::new();
    let end = Instant::now() + Duration::from_secs(secs);
    let mut peak = 0.0_f32;
    while Instant::now() < end {
        controller.poll_pending_rebuilds();
        for (_, routes) in controller.chain_output_route_stats(chain_id) {
            for r in routes {
                if fills.len() <= r.route {
                    fills.resize(r.route + 1, (usize::MAX, 0));
                }
                let f = &mut fills[r.route];
                f.0 = f.0.min(r.fill_frames);
                f.1 = f.1.max(r.fill_frames);
            }
        }
        peak = peak.max(controller.chain_peak_load(chain_id));
        std::thread::sleep(Duration::from_millis(20));
    }
    (
        fills,
        controller.chain_xrun_count(chain_id) - x0,
        controller.chain_underrun_count(chain_id) - u0,
        peak,
    )
}

#[test]
fn an_insert_chain_on_the_quantum_holds_its_lean_cushions() {
    if !hw_tests_enabled("an_insert_chain_on_the_quantum_holds_its_lean_cushions") {
        return;
    }
    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(input), Some(output)) = (
        inputs.iter().find(|d| d.name.contains("Quantum")),
        outputs.iter().find(|d| d.name.contains("Quantum")),
    ) else {
        eprintln!("[#965 HW] SKIPPED — no Quantum interface connected");
        return;
    };
    let _device = device_guard();
    init_registry();

    let (project, chain_id, registry) = owners_insert_chain(&input.id, &output.id);
    let mut controller = ProjectRuntimeController::start(&project).expect("start real streams");
    controller.set_io_bindings(registry);
    controller
        .sync_project(&project)
        .expect("resync with bindings");
    wait_until_streaming(&mut controller, &chain_id);

    let (cold_fill, cold_x, cold_u, cold_peak) = watch(&mut controller, &chain_id, 3);
    eprintln!(
        "[#965 HW] cold start 3 s: xruns={cold_x} underruns={cold_u} peak_load={cold_peak:.2} fills={cold_fill:?}"
    );
    let (fill, xruns, underruns, peak) = watch(&mut controller, &chain_id, 60);
    eprintln!(
        "[#965 HW] steady 60 s: xruns={xruns} underruns={underruns} peak_load={peak:.2} fills(min,max)={fill:?}"
    );

    // Live edits, like the GUI: a NAM param through the off-thread live
    // rebuild (#740 — a fresh runtime swapped in while playing, the path
    // that used to re-prime and then cut IR routes), then the cab off and
    // back on through the synchronous upsert.
    let mut edited = project.clone();
    {
        use domain::value_objects::ParameterValue;
        let nam = edited.chains[0]
            .blocks
            .iter_mut()
            .find(|b| matches!(&b.kind, AudioBlockKind::Core(c) if c.model.starts_with("nam_")))
            .expect("a NAM block");
        if let AudioBlockKind::Core(c) = &mut nam.kind {
            c.params.insert("output_db", ParameterValue::Float(-1.0));
        }
    }
    assert!(
        controller
            .request_offthread_rebuild_if_live(&edited, &edited.chains[0])
            .expect("off-thread live rebuild"),
        "a param edit on a streaming chain takes the off-thread path"
    );
    let _ = watch(&mut controller, &chain_id, 2);
    let mut toggled = edited.clone();
    for b in toggled.chains[0].blocks.iter_mut() {
        if matches!(&b.kind, AudioBlockKind::Core(c) if c.model.starts_with("ir_")) {
            b.enabled = false;
        }
    }
    controller
        .upsert_chain(&toggled, &toggled.chains[0])
        .expect("cab-off rebuild");
    let _ = watch(&mut controller, &chain_id, 2);
    controller
        .upsert_chain(&edited, &edited.chains[0])
        .expect("cab-on rebuild");
    let _ = watch(&mut controller, &chain_id, 1);
    let (edit_fill, edit_x, edit_u, edit_peak) = watch(&mut controller, &chain_id, 30);
    eprintln!(
        "[#965 HW] 30 s after live edits: xruns={edit_x} underruns={edit_u} peak_load={edit_peak:.2} fills(min,max)={edit_fill:?}"
    );

    controller.stop();

    assert_eq!(
        (cold_x, cold_u, xruns, underruns, edit_x, edit_u),
        (0, 0, 0, 0, 0, 0),
        "the lean insert cushions must hold on the owner's interface: \
         (cold xruns, cold underruns, steady xruns, steady underruns, \
         post-edit xruns, post-edit underruns)"
    );
}

/// The #670 add-cab-live scenario (12 threads saturating every core during
/// the add, then a quiet 30 s) on the owner's interface, where the chain's
/// input and outputs share ONE clock. The #670 battery runs it on the first
/// enumerated devices (an input and an output on two different clocks at the
/// same nominal rate, where drift alone drains any cushion); this one isolates
/// the worker: the lean cushions must ride the spike and recover.
#[test]
fn adding_a_cab_live_under_load_on_the_quantum_does_not_spiral() {
    if !hw_tests_enabled("adding_a_cab_live_under_load_on_the_quantum_does_not_spiral") {
        return;
    }
    let inputs = list_input_device_descriptors().expect("list inputs");
    let outputs = list_output_device_descriptors().expect("list outputs");
    let (Some(input), Some(output)) = (
        inputs.iter().find(|d| d.name.contains("Quantum")),
        outputs.iter().find(|d| d.name.contains("Quantum")),
    ) else {
        eprintln!("[#965 HW] SKIPPED — no Quantum interface connected");
        return;
    };
    let _device = device_guard();
    init_registry();

    let (project, chain_id, registry) = owners_insert_chain(&input.id, &output.id);
    // No cab at the start: it is added live below.
    let mut start = project.clone();
    start.chains[0]
        .blocks
        .retain(|b| !matches!(&b.kind, AudioBlockKind::Core(c) if c.model.starts_with("ir_")));
    let mut controller = ProjectRuntimeController::start(&start).expect("start real streams");
    controller.set_io_bindings(registry);
    controller
        .sync_project(&start)
        .expect("resync with bindings");
    wait_until_streaming(&mut controller, &chain_id);
    let _ = watch(&mut controller, &chain_id, 5);

    let stop_load = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let loaders: Vec<_> = (0..12)
        .map(|_| {
            let stop = std::sync::Arc::clone(&stop_load);
            std::thread::spawn(move || {
                let mut x = 0.001f64;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    x = (x.sin().cos() + 1.0001).sqrt();
                    std::hint::black_box(x);
                }
            })
        })
        .collect();
    controller
        .upsert_chain(&project, &project.chains[0])
        .expect("live add-cab rebuild");
    let (load_fill, load_x, load_u, load_peak) = watch(&mut controller, &chain_id, 8);
    stop_load.store(true, std::sync::atomic::Ordering::Relaxed);
    for l in loaders {
        let _ = l.join();
    }
    eprintln!(
        "[#965 HW] add cab under load 8 s: xruns={load_x} underruns={load_u} peak_load={load_peak:.2} fills={load_fill:?}"
    );
    let _ = watch(&mut controller, &chain_id, 4);
    let (fill, xruns, underruns, peak) = watch(&mut controller, &chain_id, 30);
    eprintln!(
        "[#965 HW] quiet 30 s after the add: xruns={xruns} underruns={underruns} peak_load={peak:.2} fills(min,max)={fill:?}"
    );
    controller.stop();
    assert_eq!(
        (xruns, underruns),
        (0, 0),
        "after live-adding the cab under load, the chain must recover on the \
         owner's interface"
    );
}
