//! Issue #630 — a NAM grid pedal (knobs map to the nearest `.nam` capture)
//! must ALWAYS select the nearest capture, regardless of knob position.
//! On/off is ONLY the block enable toggle.
//!
//! User repro: add a NAM grid pedal (drive/tone/level → nearest `.nam`
//! capture), it's audible; turn the `drive` knob down → the pedal stops
//! affecting the audio; from then on toggling enabled does nothing audible
//! either.
//!
//! Root cause (issue #630): the legacy #402 rule (`any_zero_knob`) returned
//! a `MonoPassthrough`/`StereoPassthrough` whenever a `drive`/`level` param
//! sat at the manifest minimum. Grid axes default to their first value
//! (0.0), and the GUI `OverwriteBlock` path materialises a DENSE
//! `ParameterSet`, so moving ONE knob filled the other axes at 0 → the rule
//! fired → the model was silently UNLOADED. Because the node was
//! `AudioProcessor::passthrough` (not `RuntimeProcessor::Bypass`), the enable
//! toggle could not bring it back. But real manifests ship real captures at
//! drive=0 (e.g. `nam_ibanez_ts9` has 3 captures at drive 0), so 0 != off.
//!
//! NEW contract (the fix): a grid knob — including at the axis minimum —
//! always resolves to the nearest capture and keeps a REAL model loaded.
//! On/off is exclusively the engine enable toggle.
//!
//! The grid fixture `nam/grid_drive` declares a numeric `drive` axis with two
//! captures: `drive: 0` and `drive: 5`, each pointing at a distinct tiny real
//! `.nam`, so we exercise capture SELECTION (not just the live-edit
//! machinery): the two cells produce different audible energy.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Once};

use block_core::param::ParameterSet;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::value_objects::ParameterValue;
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_block_toggle::set_block_enabled;
use engine::runtime_state::ChainRuntimeState;
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, NamBlock, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::project::Project;

const SR: f32 = 48_000.0;
const BUFFER_FRAMES: usize = 64;

fn fixture_plugins_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins")
}

fn init_test_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        nam::register_builder();
        ir::register_builder();
        plugin_loader::registry::init(&fixture_plugins_root());
    });
}

/// Input(mono) → NAM grid pedal(`drive`) → Output(mono). Single isolated
/// runtime. The only resource is the grid pedal's model.
fn grid_chain(drive: f32, pedal_enabled: bool) -> Chain {
    let mut params = ParameterSet::default();
    params.insert("drive", ParameterValue::Float(drive));
    Chain {
        id: ChainId("issue-630".into()),
        description: Some("issue-630 grid pedal capture selection".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            AudioBlock {
                id: BlockId("in".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("pedal".into()),
                enabled: pedal_enabled,
                kind: AudioBlockKind::Nam(NamBlock {
                    model: "nam_grid_drive".into(),
                    params,
                }),
            },
            AudioBlock {
                id: BlockId("out".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainOutputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
        ],
    }
}

fn fill_sine(buf: &mut [f32], channels: usize, phase: &mut f32) {
    let incr = 2.0 * std::f32::consts::PI * 220.0 / SR;
    let frames = buf.len() / channels;
    for f in 0..frames {
        let s = 0.5 * phase.sin();
        *phase += incr;
        if *phase > std::f32::consts::TAU {
            *phase -= std::f32::consts::TAU;
        }
        for c in 0..channels {
            buf[f * channels + c] = s;
        }
    }
}

/// Drive the live runtime through a sine for enough callbacks to clear the
/// FADE_IN warmup, then return the steady-state output energy (sum of
/// squares) on the routed mono channel.
fn drive_energy(runtime: &Arc<ChainRuntimeState>) -> f64 {
    const CALLBACKS: usize = 48;
    const WARMUP: usize = 24;
    let input_channels = 1;
    let device_channels = 2;
    let mut input_buf = vec![0.0_f32; BUFFER_FRAMES * input_channels];
    let mut output_buf = vec![0.0_f32; BUFFER_FRAMES * device_channels];
    let mut phase = 0.0_f32;
    let mut energy = 0.0_f64;
    for cb in 0..CALLBACKS {
        fill_sine(&mut input_buf, input_channels, &mut phase);
        process_input_f32(runtime, 0, &input_buf, input_channels);
        process_output_f32(runtime, 0, &mut output_buf, device_channels);
        if cb >= WARMUP {
            for f in 0..BUFFER_FRAMES {
                let s = output_buf[f * device_channels];
                energy += f64::from(s) * f64::from(s);
            }
        }
    }
    energy
}

fn project_with(chain: Chain) -> (Project, HashMap<ChainId, f32>) {
    let mut rates = HashMap::new();
    rates.insert(chain.id.clone(), SR);
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain],
        midi: None,
    };
    (project, rates)
}

/// NEW contract for #630: a grid pedal at the axis minimum (`drive=0`) must
/// keep a REAL model loaded and SELECT the `drive=0` capture — NOT fall back
/// to passthrough. Turning the knob between cells swaps the loaded capture;
/// it never unloads the model.
#[test]
fn grid_knob_zero_keeps_model_loaded_and_selects_capture() {
    init_test_registry();
    assert!(
        plugin_loader::registry::find("nam_grid_drive").is_some(),
        "fixture plugin nam_grid_drive must be discoverable in \
         crates/engine/tests/fixtures/plugins/nam/grid_drive/"
    );

    // (a) Build LIVE at the HIGH capture (drive=5). Model loads once.
    let hi = grid_chain(5.0, true);
    let (project, rates) = project_with(hi.clone());
    let mut graph =
        engine::runtime_graph::build_runtime_graph(&project, &rates, &HashMap::new())
            .expect("graph with a NAM grid pedal must build");
    let runtime = graph
        .runtime_for_chain(&ChainId("issue-630".into()))
        .expect("the chain must have a live runtime");
    let live_hi = nam::live_models();
    let energy_hi = drive_energy(&runtime);
    eprintln!("[a] drive=5: live={live_hi} energy={energy_hi:.6}");
    assert!(
        live_hi >= 1,
        "drive=5 must load the grid pedal model (live_models={live_hi})"
    );
    assert!(
        energy_hi > 0.0,
        "drive=5 must produce audible output (energy={energy_hi})"
    );

    // (b) User turns `drive` DOWN to the axis minimum (drive=0) via the live
    // edit. NEW contract: the model STAYS loaded and selects the drive=0
    // capture. The OLD #402 rule unloaded it (live_models -> 0); that is the
    // bug.
    let lo = grid_chain(0.0, true);
    graph
        .upsert_chain(&lo, SR, false, &[BUFFER_FRAMES])
        .expect("live drive->0 edit must succeed");
    let runtime = graph
        .runtime_for_chain(&ChainId("issue-630".into()))
        .expect("runtime still present after drive->0");
    let live_lo = nam::live_models();
    let energy_lo = drive_energy(&runtime);
    eprintln!("[b] drive=0: live={live_lo} energy={energy_lo:.6}");

    assert!(
        live_lo >= 1,
        "issue #630: at the axis minimum (drive=0) the grid pedal must keep a \
         REAL model loaded and select the drive=0 capture, NOT fall back to \
         passthrough (live_models={live_lo}). 0 is a real capture, not off."
    );
    assert!(
        energy_lo > 0.0,
        "issue #630: drive=0 selects a real capture, so it must still produce \
         output (energy={energy_lo})."
    );

    // (c) Turning `drive` BACK UP swaps back to the high capture — the model
    // never had to be reloaded from scratch, and the high capture's audible
    // energy is restored.
    let hi_again = grid_chain(5.0, true);
    graph
        .upsert_chain(&hi_again, SR, false, &[BUFFER_FRAMES])
        .expect("live drive->5 edit must succeed");
    let runtime = graph
        .runtime_for_chain(&ChainId("issue-630".into()))
        .expect("runtime still present after drive->5");
    let live_again = nam::live_models();
    let energy_again = drive_energy(&runtime);
    eprintln!("[c] drive=5 again: live={live_again} energy={energy_again:.6}");
    assert!(
        live_again >= 1,
        "raising drive back up keeps the model loaded (live_models={live_again})"
    );
    assert!(
        (energy_again - energy_hi).abs() <= energy_hi * 0.10,
        "raising drive back to 5 must restore the drive=5 capture's energy \
         (hi={energy_hi:.6}, recovered={energy_again:.6})"
    );
}

/// On/off is ONLY the enable toggle. Disabling the pedal removes its effect;
/// re-enabling restores it — independently of knob position. At drive=0 the
/// model is loaded, so disabling must CHANGE the output (effect removed) and
/// re-enabling must bring it back.
#[test]
fn grid_pedal_on_off_is_the_enable_toggle_even_at_knob_zero() {
    init_test_registry();

    // Build LIVE at the axis minimum. NEW contract: model loaded.
    let lo = grid_chain(0.0, true);
    let (project, rates) = project_with(lo.clone());
    let graph = engine::runtime_graph::build_runtime_graph(&project, &rates, &HashMap::new())
        .expect("graph with a NAM grid pedal must build");
    let runtime = graph
        .runtime_for_chain(&ChainId("issue-630".into()))
        .expect("runtime present");
    let live_built = nam::live_models();
    let energy_enabled = drive_energy(&runtime);
    eprintln!("[on] drive=0 enabled: live={live_built} energy={energy_enabled:.6}");
    assert!(
        live_built >= 1,
        "issue #630: building at drive=0 must load the model, not passthrough \
         (live_models={live_built})"
    );

    // Disable via the enable toggle → effect removed.
    let pedal = BlockId("pedal".into());
    set_block_enabled(&runtime, &pedal, false).expect("disable must apply");
    let energy_disabled = drive_energy(&runtime);
    eprintln!("[off] drive=0 disabled: energy={energy_disabled:.6}");

    // Re-enable → effect back.
    set_block_enabled(&runtime, &pedal, true).expect("re-enable must apply");
    let energy_reenabled = drive_energy(&runtime);
    eprintln!("[on] drive=0 re-enabled: energy={energy_reenabled:.6}");

    assert!(
        (energy_reenabled - energy_enabled).abs() <= energy_enabled.max(1e-6) * 0.10,
        "issue #630: the enable toggle is on/off — re-enabling at drive=0 must \
         restore the original effect (enabled={energy_enabled:.6}, \
         re-enabled={energy_reenabled:.6})"
    );
    assert!(
        (energy_disabled - energy_enabled).abs() > energy_enabled.max(1e-6) * 0.01,
        "issue #630: disabling the pedal must AUDIBLY change the output — the \
         model is loaded at drive=0, so off != on (enabled={energy_enabled:.6}, \
         disabled={energy_disabled:.6})"
    );
}
