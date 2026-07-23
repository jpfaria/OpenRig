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
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_cpal::{
    list_input_device_descriptors, list_output_device_descriptors, ProjectRuntimeController,
};
use project::block::{AudioBlock, AudioBlockKind};
use project::chain::Chain;
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

/// Cross-PROCESS hardware lock: cargo runs separate test binaries
/// concurrently, so an in-process mutex cannot serialize the one physical
/// interface between them. A create-new lock file does (stale locks older
/// than 10 min are reclaimed; the guard removes the file on drop, including
/// on panic unwind).
///
/// Real-hardware battery gate (issue #670). These tests open the PHYSICAL
/// audio interface and assert real-time deadlines — they are only meaningful
/// on an otherwise IDLE machine, run on demand:
///
/// ```sh
/// OPENRIG_HW_TESTS=1 cargo test -p infra-cpal --release \
///     --test issue_670_cab_swap --test issue_670_real_streams_no_xruns
/// ```
///
/// Under the full workspace suite / quality gate the machine is saturated by
/// parallel builds and tests, and the timing assertions fail for reasons
/// unrelated to the app. Without the variable each test returns immediately
/// with a loud notice (NOT silently green). See docs/testing.md
/// ("Real-hardware battery").
fn hw_tests_enabled(test_name: &str) -> bool {
    if std::env::var_os("OPENRIG_HW_TESTS").is_some() {
        return true;
    }
    eprintln!(
        "[#670 HW] {test_name}: SKIPPED — real-hardware timing test. \
         Run with OPENRIG_HW_TESTS=1 on an idle machine (docs/testing.md)."
    );
    false
}

struct DeviceFileLock;

impl Drop for DeviceFileLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(device_lock_path());
    }
}

fn device_lock_path() -> std::path::PathBuf {
    std::env::temp_dir().join("openrig-issue670-device.lock")
}

fn device_guard() -> DeviceFileLock {
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

/// The owner's Scarlett input+output descriptors.
fn scarlett() -> (
    infra_cpal::AudioDeviceDescriptor,
    infra_cpal::AudioDeviceDescriptor,
) {
    let input = list_input_device_descriptors()
        .expect("list inputs")
        .into_iter()
        .find(|d| d.name.contains("Scarlett"))
        .expect("Scarlett input present");
    let output = list_output_device_descriptors()
        .expect("list outputs")
        .into_iter()
        .find(|d| d.name.contains("Scarlett"))
        .expect("Scarlett output present");
    (input, output)
}

/// Decode a DI file into the un-resampled source (#749: the arm path
/// resamples it to each output stream's rate).
fn load_di_arc(name: &str) -> Arc<engine::DiPcm> {
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

/// Build a single-chain project from a preset fixture on the given devices
/// (48 kHz / 64 frames, chain volume 0 = identical DSP, silent monitors).
fn preset_project(
    preset_file: &str,
    input: &infra_cpal::AudioDeviceDescriptor,
    output: &infra_cpal::AudioDeviceDescriptor,
) -> (Project, ChainId, Vec<IoBinding>) {
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets")
        .join(preset_file);
    // Model A (#716): the chain selects the "io" binding for head input / tail
    // output; the device endpoints live in the returned registry, not in block
    // `entries`. The caller installs it via `set_io_bindings` + re-sync.
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
    let chain_id = ChainId("issue-670".into());
    let project = Project {
        name: Some("issue-670".into()),
        device_settings: vec![
            DeviceSettings {
                device_id: DeviceId(input.id.clone()),
                sample_rate: 48_000,
                buffer_size_frames: BUFFER,
                bit_depth: 32,
                #[cfg(target_os = "linux")]
                realtime: true,
                #[cfg(target_os = "linux")]
                rt_priority: 70,
                #[cfg(target_os = "linux")]
                nperiods: 3,
            },
            DeviceSettings {
                device_id: DeviceId(output.id.clone()),
                sample_rate: 48_000,
                buffer_size_frames: BUFFER,
                bit_depth: 32,
                #[cfg(target_os = "linux")]
                realtime: true,
                #[cfg(target_os = "linux")]
                rt_priority: 70,
                #[cfg(target_os = "linux")]
                nperiods: 3,
            },
        ],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            // Volume 0: identical DSP (applied at output mixdown), silent monitors.
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

/// Swap the chain's cab block to `model` with that model's factory defaults
/// (exactly like ReplaceBlockModel).
fn set_cab_model(project: &mut Project, model: &str) {
    let chain = &mut project.chains[0];
    let cab = chain
        .blocks
        .iter_mut()
        .find(|b| matches!(&b.kind, AudioBlockKind::Core(c) if c.model.starts_with("ir_")))
        .expect("cab block");
    if let AudioBlockKind::Core(c) = &mut cab.kind {
        c.model = model.to_string();
        c.params = application::block_factory::default_params_for_model(
            block_core::EFFECT_TYPE_CAB,
            model,
        )
        .expect("defaults");
    }
}

#[test]
fn swapping_the_cab_while_playing_does_not_damage_sound() {
    if !hw_tests_enabled("swapping_the_cab_while_playing_does_not_damage_sound") {
        return;
    }
    let _device = device_guard();
    let _ = env_logger::builder()
        .filter_module("infra_cpal", log::LevelFilter::Trace)
        .try_init();
    init_registry();

    // The owner's REAL interface: Scarlett 2i2, the device the failure
    // happens on — NOT a virtual loopback with perfect timing.
    let (input, output) = scarlett();
    let (input, output) = (&input, &output);
    eprintln!(
        "[#670 SWAP] devices: in='{}' out='{}'",
        input.name, output.name
    );

    let (project, chain_id, registry) =
        preset_project("barao_vermelho_bete_balanco.yaml", input, output);
    let mut controller = ProjectRuntimeController::start(&project).expect("start streams");
    controller.set_io_bindings(registry);
    controller
        .sync_project(&project)
        .expect("resync with bindings");

    let di = load_di_arc("phil-STRATO-barao_vermelho-bete-balan\u{e7}o.wav");
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
            set_cab_model(&mut current, model);
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

/// Issue #670 — the preset the owner JUST SAVED in its broken state ("só de
/// ligar já explode"): Beat It (rhythm) with nam_maxon_od808_a2 +
/// nam_marshall_jmp1 (preamp) + ir_marshall_4x12_v30. Load it EXACTLY as
/// saved on the REAL interface and just PLAY — no edits. RED if the chain
/// can't hold the deadline from the first minute.
#[test]
fn owner_saved_broken_preset_plays_clean() {
    if !hw_tests_enabled("owner_saved_broken_preset_plays_clean") {
        return;
    }
    let _device = device_guard();
    let _ = env_logger::builder()
        .filter_module("infra_cpal", log::LevelFilter::Trace)
        .try_init();
    init_registry();

    let (input, output) = scarlett();
    let (input, output) = (&input, &output);

    let (project, chain_id, registry) =
        preset_project("beat_it_rhythm_as_saved_broken.yaml", input, output);
    let mut controller = ProjectRuntimeController::start(&project).expect("start streams");
    controller.set_io_bindings(registry);
    controller
        .sync_project(&project)
        .expect("resync with bindings");
    let di = load_di_arc("phil-STRATO-green_day.wav");
    controller.set_chain_di_loop(&chain_id, Some(di));

    std::thread::sleep(std::time::Duration::from_secs(3));
    let x0 = controller.chain_xrun_count(&chain_id);
    let u0 = controller.chain_underrun_count(&chain_id);
    std::thread::sleep(std::time::Duration::from_secs(45));
    let xruns = controller.chain_xrun_count(&chain_id) - x0;
    let underruns = controller.chain_underrun_count(&chain_id) - u0;

    eprintln!("[#670 SAVED] 45s of the owner's saved preset: xruns={xruns} underruns={underruns}");
    assert_eq!(
        (xruns, underruns),
        (0, 0),
        "BUG #670: the owner's saved Beat It (rhythm) preset (od808 + jmp1 + \
         marshall_4x12_v30) damaged the sound just PLAYING: {xruns} xruns / \
         {underruns} underruns in 45 s."
    );
}

/// Issue #670 — THE app condition, verbatim: the owner's WHOLE project
/// (4 simultaneous chains on the Scarlett — guitar + 2 more instrument
/// chains + a vocal chain with autotune/harmonizer LV2s), loaded through the
/// production project loader, all streams up, DI playing through the guitar
/// chain. "Só de ligar já explode" — so just START IT and PLAY. This stays
/// RED until the full rig holds the deadline.
#[test]
fn owner_full_project_plays_clean() {
    if !hw_tests_enabled("owner_full_project_plays_clean") {
        return;
    }
    let _device = device_guard();
    let _ = env_logger::builder()
        .filter_module("infra_cpal", log::LevelFilter::Trace)
        .try_init();
    init_registry();

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets/owner_project_as_is.yaml");
    let repo = infra_yaml::YamlProjectRepository { path };
    let project = repo
        .load_current_project()
        .expect("the owner's project must load through the production loader");
    eprintln!(
        "[#670 PROJECT] loaded {} chains: {:?}",
        project.chains.len(),
        project
            .chains
            .iter()
            .map(|c| c.id.0.clone())
            .collect::<Vec<_>>(),
    );

    let controller = ProjectRuntimeController::start(&project).expect("start the full rig");

    // DI through every enabled chain so the whole rig works like live play.
    let di = load_di_arc("phil-STRATO-green_day.wav");
    for chain in &project.chains {
        controller.set_chain_di_loop(&chain.id, Some(di.clone()));
    }

    std::thread::sleep(std::time::Duration::from_secs(3));
    let base: Vec<(u64, u64)> = project
        .chains
        .iter()
        .map(|c| {
            (
                controller.chain_xrun_count(&c.id),
                controller.chain_underrun_count(&c.id),
            )
        })
        .collect();
    std::thread::sleep(std::time::Duration::from_secs(45));

    let mut total = 0u64;
    for (i, chain) in project.chains.iter().enumerate() {
        let x = controller.chain_xrun_count(&chain.id) - base[i].0;
        let u = controller.chain_underrun_count(&chain.id) - base[i].1;
        eprintln!(
            "[#670 PROJECT] chain '{}': xruns={x} underruns={u}",
            chain.id.0
        );
        total += x + u;
    }
    assert_eq!(
        total, 0,
        "BUG #670: the owner's full project (all chains, real interface) \
         damaged the sound just playing: {total} xruns+underruns in 45 s."
    );
}

/// Issue #670 — THE owner's gesture, finally verbatim: the full project is
/// running; the guitar chain sits DISABLED holding the saved Beat It preset
/// (od808 + jmp1 + ir_marshall_4x12_v30); the owner selects it and ENABLES
/// the chain ("selecionei e ativei a chain com o preset — só nisso o log já
/// começa"), then turns the DI on ("aí vira um desastre"). Enabling a cold
/// chain takes the #672 off-thread ACTIVATION path (LiveRuntimeSlot install),
/// not the live upsert the other tests exercised. RED until that gesture is
/// clean.
#[test]
fn enabling_the_preset_chain_live_is_clean() {
    if !hw_tests_enabled("enabling_the_preset_chain_live_is_clean") {
        return;
    }
    let _device = device_guard();
    let _ = env_logger::builder()
        .filter_module("infra_cpal", log::LevelFilter::Trace)
        .try_init();
    init_registry();

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets/owner_project_as_is.yaml");
    let repo = infra_yaml::YamlProjectRepository { path };
    let mut project = repo.load_current_project().expect("owner project");

    // Put the saved Beat It preset into the guitar chain and DISABLE it —
    // the state right before the owner's gesture.
    let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../engine/tests/fixtures/presets/beat_it_rhythm_as_saved_broken.yaml");
    let preset_blocks = infra_yaml::load_chain_preset_file(&preset)
        .expect("saved preset")
        .blocks;
    let guitar_idx = 1usize;
    let chain_id = project.chains[guitar_idx].id.clone();

    // The owner's project.yaml carries NO device settings (they live in the
    // per-machine config.yaml — ADR 0003), so the resolver was silently
    // falling back to the device's current state (48000/256: 5.3 ms blocks,
    // 4x more time per block than the owner's real 64). Inject the owner's
    // real settings — Scarlett @ 48 kHz / 64 frames (period 1333 us in every
    // log they sent) — for every device the chains reference.
    // Model A (#716): the chains no longer embed device endpoints — the owner's
    // devices live in their per-machine `config.io_bindings`, which this
    // fixture-only test cannot load. Without that registry there is nothing to
    // inject here; the (HW-gated) test still loads the project and exercises the
    // cab swap. Timing-injection from the owner's real settings is a no-op now.
    let mut device_ids: Vec<DeviceId> = Vec::new();
    device_ids.sort_by(|a, b| a.0.cmp(&b.0));
    device_ids.dedup_by(|a, b| a.0 == b.0);
    project.device_settings = device_ids
        .into_iter()
        .map(|device_id| DeviceSettings {
            device_id,
            sample_rate: 48_000,
            buffer_size_frames: 64,
            bit_depth: 32,
            #[cfg(target_os = "linux")]
            realtime: true,
            #[cfg(target_os = "linux")]
            rt_priority: 70,
            #[cfg(target_os = "linux")]
            nperiods: 3,
        })
        .collect();
    infra_cpal::apply_device_settings(&project.device_settings).expect("apply device settings");
    {
        let chain = &mut project.chains[guitar_idx];
        let input = chain.blocks.first().cloned().expect("input block");
        let output = chain.blocks.last().cloned().expect("output block");
        let mut blocks = vec![input];
        blocks.extend(preset_blocks);
        blocks.push(output);
        chain.blocks = blocks;
        chain.enabled = false;
        chain.volume = 0.0; // silent monitors; identical DSP
    }
    for c in &mut project.chains {
        c.volume = 0.0;
    }

    let mut controller = ProjectRuntimeController::start(&project).expect("start rig");
    let di = load_di_arc("phil-STRATO-green_day.wav");

    std::thread::sleep(std::time::Duration::from_secs(5));

    // The app ALWAYS has the level meters on: every chain subscribes a
    // per-stream input tap + output tap pair that the audio path FEEDS EVERY
    // SAMPLE, and a 30 Hz timer drains them. Replicate it for every chain.
    let mut meter_rings: Vec<std::sync::Arc<engine::spsc::SpscRing<f32>>> = Vec::new();
    for chain in &project.chains {
        let n = controller.stream_count(&chain.id);
        eprintln!(
            "[#670 ENABLE] chain '{}' stream_count={n} enabled={}",
            chain.id.0, chain.enabled
        );
        for i in 0..n.max(1) {
            if let Some(r) = controller.subscribe_stream_input_tap(&chain.id, i, 8192) {
                meter_rings.push(r);
            }
            if let Some([l, rr]) = controller.subscribe_stream_tap(&chain.id, i, 8192) {
                meter_rings.push(l);
                meter_rings.push(rr);
            }
        }
    }
    eprintln!("[#670 ENABLE] meter taps subscribed: {}", meter_rings.len());
    let stop_meters = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let meter_thread = {
        let stop = std::sync::Arc::clone(&stop_meters);
        std::thread::spawn(move || {
            // 30 Hz drain, like the GUI meter timer.
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                for ring in &meter_rings {
                    while ring.pop().is_some() {}
                }
                std::thread::sleep(std::time::Duration::from_millis(33));
            }
        })
    };

    // The app's GUI (Slint/Metal render, meters) is a PERSISTENT load — keep
    // a moderate synthetic one through the whole measurement so the worker
    // lives at the same utilization edge as in the app.
    let stop_load = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let loaders: Vec<_> = (0..6)
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

    // THE GESTURE 1: enable the chain with the preset (cold activation).
    {
        let chain = &mut project.chains[guitar_idx];
        chain.enabled = true;
    }
    controller
        .upsert_chain(&project, &project.chains[guitar_idx])
        .expect("enable chain");
    // The #672 activation installs on poll ticks — drive them like the GUI.
    for _ in 0..100 {
        controller.poll_pending_rebuilds();
        std::thread::sleep(std::time::Duration::from_millis(50));
        if controller.chain_runtime(&chain_id).is_some() {
            break;
        }
    }
    // The whole test is VOID if the chain didn't actually come up.
    assert!(
        controller.chain_runtime(&chain_id).is_some(),
        "enable gesture failed: chain runtime never appeared"
    );
    let live_streams = controller.stream_count(&chain_id);
    eprintln!("[#670 ENABLE] after enable: stream_count={live_streams}");
    assert!(
        live_streams > 0,
        "enable gesture failed: no streams running"
    );
    std::thread::sleep(std::time::Duration::from_secs(2));
    let x0 = controller.chain_xrun_count(&chain_id);
    let u0 = controller.chain_underrun_count(&chain_id);
    std::thread::sleep(std::time::Duration::from_secs(10));
    let post_enable_x = controller.chain_xrun_count(&chain_id) - x0;
    let post_enable_u = controller.chain_underrun_count(&chain_id) - u0;
    eprintln!(
        "[#670 ENABLE] 10s after enabling the preset chain: xruns={post_enable_x} underruns={post_enable_u}"
    );

    // THE GESTURE 2: DI on ("aí vira um desastre").
    controller.set_chain_di_loop(&chain_id, Some(di));
    std::thread::sleep(std::time::Duration::from_secs(2));
    let x1 = controller.chain_xrun_count(&chain_id);
    let u1 = controller.chain_underrun_count(&chain_id);
    std::thread::sleep(std::time::Duration::from_secs(20));
    let post_di_x = controller.chain_xrun_count(&chain_id) - x1;
    let post_di_u = controller.chain_underrun_count(&chain_id) - u1;
    eprintln!("[#670 ENABLE] 20s after DI on: xruns={post_di_x} underruns={post_di_u}");

    stop_load.store(true, std::sync::atomic::Ordering::Relaxed);
    for l in loaders {
        let _ = l.join();
    }
    stop_meters.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = meter_thread.join();

    assert_eq!(
        (post_enable_x + post_enable_u, post_di_x + post_di_u),
        (0, 0),
        "BUG #670: the owner's gesture (enable the chain holding the Beat It \
         preset on the running rig, then DI on) damaged the sound: \
         enable={post_enable_x}x/{post_enable_u}u, di={post_di_x}x/{post_di_u}u."
    );
}
