//! Replicates the owner's report verbatim: "when I play a DI or my
//! instrument and change the chain config, NOTHING changes; I can disable a
//! block and the effect keeps going."
//!
//! The existing engine/controller unit tests exercise `set_block_enabled` and
//! the rebuild swap on a hand-built runtime in ISOLATION — they stay green
//! while the running app is broken because they never drive audio through the
//! SAME runtime the live audio callback reads (the `LiveRuntimeSlot` that the
//! controller publishes into). These tests close that gap: they build an
//! ACTIVE chain on a real `ProjectRuntimeController`, then edit it through the
//! exact surfaces the GUI uses —
//!   * `set_block_enabled` (issue #522 fast path, `sync_block_toggle`), and
//!   * `request_offthread_rebuild_if_live` + `poll_pending_rebuilds`
//!     (the live param/config edit path, `sync_live_chain_runtime`)
//!
//! — and MEASURE the audio the callback would actually emit by driving
//! `controller.chain_runtime(&id)` (the live slot). If the edit does not reach
//! that runtime, the sound never changes and the test fails, reproducing the
//! report.

#![cfg(not(all(target_os = "linux", feature = "jack")))]

use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use engine::runtime::ChainRuntimeState;
use engine::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use engine::runtime_graph::{build_chain_runtime_state, RuntimeGraph};
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use super::active_runtime::ActiveChainRuntime;
use super::resolved::{ChainStreamSignature, InputStreamSignature, OutputStreamSignature};
use super::{LiveRuntimeSlot, ProjectRuntimeController};

const SR: f32 = 48_000.0;
const BUF: usize = 64;
const BLOCK_ID: &str = "userreport:compressor";
const CHAIN_ID: &str = "userreport-chain";

fn init_registry() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        block_dyn::register_natives();
        block_gain::register_natives();
    });
}

/// A hard-compressing profile (4:1, low threshold) — enabling/disabling it is
/// plainly audible in the loud/quiet RMS ratio of a two-level tone.
fn compressor_params(ratio: f32) -> ParameterSet {
    let schema = schema_for_block_model("dynamics", "compressor_studio_clean")
        .expect("compressor schema must exist");
    let mut ps = ParameterSet::default();
    ps.insert("attack_ms", ParameterValue::Float(10.0));
    ps.insert("release_ms", ParameterValue::Float(80.0));
    ps.insert("ratio", ParameterValue::Float(ratio));
    ps.insert("threshold", ParameterValue::Float(70.0));
    ps.insert("mix", ParameterValue::Float(100.0));
    ps.insert("makeup_gain", ParameterValue::Float(50.0));
    ps.normalized_against(&schema)
        .expect("rig params must normalize")
}

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn chain(block_enabled: bool, ratio: f32) -> Chain {
    Chain {
        id: ChainId(CHAIN_ID.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![AudioBlock {
            id: BlockId(BLOCK_ID.into()),
            enabled: block_enabled,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "dynamics".into(),
                model: "compressor_studio_clean".into(),
                params: compressor_params(ratio),
            }),
        }],
        di_output: None,
    }
}

/// Seed a controller with the chain already ACTIVE — runtime present in the
/// graph AND in the live slot (what cold activation does), with an empty
/// stream bundle so no audio device is opened.
fn controller_with_active_chain(chain: &Chain) -> ProjectRuntimeController {
    let chain_id = chain.id.clone();
    let runtime = Arc::new(
        build_chain_runtime_state(chain, SR, &[DEFAULT_ELASTIC_TARGET], &registry())
            .expect("runtime should build"),
    );

    let mut graph = RuntimeGraph {
        chains: std::collections::HashMap::new(),
    };
    graph
        .chains
        .insert((chain_id.clone(), 0), Arc::clone(&runtime));

    let mut chain_slots = std::collections::HashMap::new();
    chain_slots.insert(
        (chain_id.clone(), 0),
        LiveRuntimeSlot::new(Arc::clone(&runtime)),
    );

    let mut active_chains = std::collections::HashMap::new();
    active_chains.insert(
        chain_id.clone(),
        ActiveChainRuntime {
            // A signature that MATCHES the binding registry, so the live edit
            // path sees "I/O unchanged" and takes the off-thread rebuild (what
            // happens in the running app for a plain param/block edit).
            stream_signature: ChainStreamSignature {
                inputs: vec![InputStreamSignature {
                    device_id: "dev".into(),
                    channels: vec![0],
                    stream_channels: 1,
                    sample_rate: 48_000,
                    buffer_size_frames: BUF as u32,
                }],
                outputs: vec![OutputStreamSignature {
                    device_id: "dev".into(),
                    channels: vec![0, 1],
                    stream_channels: 2,
                    sample_rate: 48_000,
                    buffer_size_frames: BUF as u32,
                }],
            },
            _input_streams: vec![],
            _output_streams: vec![],
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _jack_client: None,
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _dsp_worker: None,
        },
    );

    ProjectRuntimeController {
        runtime_graph: graph,
        active_chains,
        chain_slots,
        worker: crate::ControlWorker::new(),
        pending_rebuilds: Vec::new(),
        pending_activations: Vec::new(),
        sample_rate: 48_000,
        io_bindings: registry(),
        di_streams: std::cell::RefCell::new(std::collections::HashMap::new()),
        di_playback_cells: std::cell::RefCell::new(std::collections::HashMap::new()),
        di_retired: Default::default(),
        #[cfg(all(target_os = "linux", feature = "jack"))]
        supervisor: super::jack_supervisor::JackSupervisor::new(
            super::jack_supervisor::LiveJackBackend::new(),
        ),
    }
}

/// The chain's live slot handle — a clone that shares the same swappable
/// slot the cpal callback captures at stream-build time. Reading through it
/// after a rebuild is the exact wait-free seam the audio thread relies on.
fn live_slot(controller: &ProjectRuntimeController) -> LiveRuntimeSlot {
    controller
        .chain_slots
        .get(&(ChainId(CHAIN_ID.into()), 0))
        .expect("active chain must own a live slot")
        .handle()
}

/// Drive `seconds` of a 440 Hz tone at `amp` through a CAPTURED slot handle,
/// the way the real cpal input/output callbacks do: `slot.load()` every
/// buffer (so a worker-published rebuild is picked up), the mixed-output path
/// (`process_output_buffer`), and the #771 DI playback mix on top. Returns the
/// left-channel RMS of the last half (past the fade/attack transient).
fn drive_slot(slot: &LiveRuntimeSlot, amp: f32, seconds: f32) -> f32 {
    let callbacks = ((SR * seconds) as usize) / BUF;
    let mut phase = 0.0_f32;
    let step = 2.0 * std::f32::consts::PI * 440.0 / SR;
    let mut input = vec![0.0_f32; BUF];
    let mut output = vec![0.0_f32; BUF * 2];
    let out_slots = [slot.handle()];
    let mut loaded: Vec<Arc<ChainRuntimeState>> = Vec::with_capacity(1);
    let mut scratch = vec![0.0_f32; BUF * 2];
    let di_cell = crate::di_playback::DiPlaybackCell::default();
    let mut sum_sq = 0.0_f64;
    let mut count = 0_usize;
    for cb in 0..callbacks {
        for s in input.iter_mut() {
            *s = amp * phase.sin();
            phase += step;
        }
        crate::slot_processing::process_input_buffer(slot, 0, &input, 1);
        crate::slot_processing::process_output_buffer(
            &out_slots,
            &mut loaded,
            0,
            &mut output,
            2,
            &mut scratch,
        );
        crate::di_playback::mix_di_playback(&di_cell, &mut output, 2);
        if cb >= callbacks / 2 {
            for frame in output.chunks_exact(2) {
                sum_sq += (frame[0] as f64) * (frame[0] as f64);
                count += 1;
            }
        }
    }
    ((sum_sq / count.max(1) as f64) as f32).sqrt()
}

/// Drive the chain's live slot (re-reading the handle each call, so it always
/// reflects the latest published runtime).
fn drive_live(controller: &ProjectRuntimeController, amp: f32, seconds: f32) -> f32 {
    drive_slot(&live_slot(controller), amp, seconds)
}

/// loud/quiet RMS ratio: ~1 when compressing hard, much larger when the
/// compressor is bypassed. Makeup-gain-proof (both passages shift equally).
fn dynamics_ratio(controller: &ProjectRuntimeController) -> f32 {
    let quiet = drive_live(controller, 0.05, 0.5);
    let loud = drive_live(controller, 0.9, 0.5);
    loud / quiet
}

// ── DI monitoring: the owner led with "when I play a DI … NOTHING changes" ──
//
// The armed DI is a dedicated pre-rendered stream (issue #717/#771) built from
// the chain's DSP at arm time. Cold activation (controller.rs:370) and the
// synchronous `upsert_chain` (controller.rs:1169) both call
// `rearm_di_stream_after_rebuild` so a config edit re-renders the DI — but the
// OFF-THREAD live-rebuild fast path (`poll_pending_rebuilds`, added by #740/#762
// to kill the edit-time freeze) does NOT. So once edits started taking that
// fast path, changing the chain while monitoring the DI stopped being audible:
// the DI keeps playing the stale render. This reproduces exactly that.

const GAIN_BLOCK_ID: &str = "userreport:gain";

fn gain_params(volume_pct: f32) -> ParameterSet {
    let schema = schema_for_block_model("gain", "volume").expect("volume schema must exist");
    let mut ps = ParameterSet::default();
    ps.insert("volume", ParameterValue::Float(volume_pct));
    ps.normalized_against(&schema)
        .expect("volume param must normalize")
}

fn gain_chain(volume_pct: f32) -> Chain {
    Chain {
        id: ChainId(CHAIN_ID.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![AudioBlock {
            id: BlockId(GAIN_BLOCK_ID.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: gain_params(volume_pct),
            }),
        }],
        di_output: None,
    }
}

/// Pull frames out of the chain's DI playback cell the way the output callback
/// does (`mix_di_playback`), letting the render worker keep the ring topped up,
/// and return the peak of the rendered DI signal over ~0.4 s. Steady tone in →
/// steady peak out, so this tracks whatever gain the CURRENT render applied.
fn di_render_peak(controller: &ProjectRuntimeController) -> f32 {
    let cell = controller.di_playback_cell(&ChainId(CHAIN_ID.into()), 0);
    let mut buf = vec![0.0f32; BUF * 2];
    let mut peak = 0.0f32;
    // ~0.4 s of callbacks, pacing so the backpressured worker can refill.
    for i in 0..((SR * 0.4) as usize / BUF) {
        buf.iter_mut().for_each(|s| *s = 0.0);
        crate::di_playback::mix_di_playback(&cell, &mut buf, 2);
        // Ignore the first third (transient / ring warm-up after a re-arm).
        if i > ((SR * 0.4) as usize / BUF) / 3 {
            for s in &buf {
                peak = peak.max(s.abs());
            }
        }
        std::thread::sleep(Duration::from_micros(300));
    }
    peak
}

/// Consume `seconds` of DI playback through the cell the output callback holds
/// and return the LONGEST run of consecutive silent frames, in frames. The DI
/// source is DC, so a rendered DI is never silent: any silent run is playback
/// the listener does not hear.
fn di_max_silence_run(cell: &crate::di_playback::DiPlaybackCell, seconds: f32) -> usize {
    di_max_silence_run_while(cell, seconds, || {})
}

/// Same, running `tick` once per simulated callback — the frontend's rebuild
/// poll keeps turning while the output callback keeps consuming, as in the app.
fn di_max_silence_run_while(
    cell: &crate::di_playback::DiPlaybackCell,
    seconds: f32,
    mut tick: impl FnMut(),
) -> usize {
    let mut buf = vec![0.0f32; BUF * 2];
    let (mut worst, mut run) = (0usize, 0usize);
    for _ in 0..((SR * seconds) as usize / BUF) {
        tick();
        buf.iter_mut().for_each(|s| *s = 0.0);
        crate::di_playback::mix_di_playback(cell, &mut buf, 2);
        for frame in buf.chunks_exact(2) {
            if frame[0].abs() < 1e-6 && frame[1].abs() < 1e-6 {
                run += 1;
                worst = worst.max(run);
            } else {
                run = 0;
            }
        }
        std::thread::sleep(Duration::from_micros(300));
    }
    worst
}

fn wait_for_di_render(controller: &ProjectRuntimeController) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while controller
        .di_stream_loop_len(&ChainId(CHAIN_ID.into()))
        .is_none()
    {
        assert!(Instant::now() < deadline, "DI render never produced a loop");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Seed a controller whose chain has a live runtime + slot (so a DI can render
/// against it) but is NOT in `active_chains` — the owner's "only the DI is
/// running": the project is open and the DI monitors, but the guitar stream was
/// never enabled. A live param edit then takes the cold-activation path, which
/// (issue #808) forgot to re-render the monitored DI, so the timbre only
/// changed after a block toggle. The DI is an independent pipeline (invariant
/// #4) — editing the chain must re-render it regardless of the guitar's state.
fn controller_with_di_only_chain(chain: &Chain) -> ProjectRuntimeController {
    let chain_id = chain.id.clone();
    let runtime = Arc::new(
        build_chain_runtime_state(chain, SR, &[DEFAULT_ELASTIC_TARGET], &registry())
            .expect("runtime should build"),
    );
    let mut graph = RuntimeGraph {
        chains: std::collections::HashMap::new(),
    };
    graph
        .chains
        .insert((chain_id.clone(), 0), Arc::clone(&runtime));
    let mut chain_slots = std::collections::HashMap::new();
    chain_slots.insert(
        (chain_id.clone(), 0),
        LiveRuntimeSlot::new(Arc::clone(&runtime)),
    );
    ProjectRuntimeController {
        runtime_graph: graph,
        // The DI-only state: NO active guitar stream for this chain.
        active_chains: std::collections::HashMap::new(),
        chain_slots,
        worker: crate::ControlWorker::new(),
        pending_rebuilds: Vec::new(),
        pending_activations: Vec::new(),
        sample_rate: 48_000,
        io_bindings: registry(),
        di_streams: std::cell::RefCell::new(std::collections::HashMap::new()),
        di_playback_cells: std::cell::RefCell::new(std::collections::HashMap::new()),
        di_retired: Default::default(),
        #[cfg(all(target_os = "linux", feature = "jack"))]
        supervisor: super::jack_supervisor::JackSupervisor::new(
            super::jack_supervisor::LiveJackBackend::new(),
        ),
    }
}

/// #808 (owner's follow-up): "I changed the param with ONLY the DI running and
/// the timbre did NOT change — only when I toggled the block." With the guitar
/// stream inactive, a live edit takes the cold-activation path; that path must
/// STILL re-render the monitored DI (the toggle path already does). The DI keeps
/// its own runtime, so this holds with no active guitar stream.
#[test]
fn a_live_edit_re_renders_the_di_when_only_the_di_is_running() {
    init_registry();
    let mut controller = controller_with_di_only_chain(&gain_chain(15.0));

    // Arm the DI while the gain block attenuates the tone hard.
    let pcm = Arc::new(engine::DiPcm::new(vec![0.6; 48_000], 48_000, 1));
    controller
        .arm_di_stream(&gain_chain(15.0), Arc::clone(&pcm))
        .expect("arm DI");
    wait_for_di_render(&controller);
    let peak_quiet = di_render_peak(&controller);

    // Edit the config: open the gain to unity. This chain is NOT active, so the
    // GUI's `sync_live_chain_runtime` takes the cold-activation branch
    // (`schedule_chain_activation`) — exactly what happens on the owner's rig.
    let edited = gain_chain(100.0);
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![edited.clone()],
        midi: None,
    };
    controller
        .schedule_chain_activation(&project, &edited)
        .expect("activation scheduling must not error");
    // Give the (re-armed) DI render time to refill with the new config.
    std::thread::sleep(Duration::from_millis(500));
    let peak_open = di_render_peak(&controller);

    assert!(
        peak_open > peak_quiet * 2.0,
        "editing the chain with only the DI running did NOT reach the monitored \
         DI: rendered peak stayed quiet={peak_quiet:.4} open={peak_open:.4}. The DI \
         kept playing the stale render — the owner's 'I change the param with only \
         the DI running and NOTHING changes until I toggle the block'."
    );
}

/// #808 (owner, after the DI plays): "I change the param and it STILL doesn't
/// reach the DI — only toggling the block applies it." Monitoring a DI without
/// enabling the chain, a param edit routes through `upsert_chain` with the chain
/// DISABLED (the Pause path), which paused without re-rendering the monitored
/// DI. The DI is independent (invariant #4) — a config edit must re-render it
/// even when the chain is paused/disabled.
#[test]
fn a_param_edit_on_a_paused_di_only_chain_re_renders_the_di() {
    init_registry();
    let mut controller = controller_with_di_only_chain(&gain_chain(15.0));

    let pcm = Arc::new(engine::DiPcm::new(vec![0.6; 48_000], 48_000, 1));
    controller
        .arm_di_stream(&gain_chain(15.0), Arc::clone(&pcm))
        .expect("arm DI");
    wait_for_di_render(&controller);
    let peak_quiet = di_render_peak(&controller);

    // Edit a param with the chain DISABLED — the exact GUI path for a DI-only
    // chain is `upsert_chain` (Pause), never the Enable/activation path.
    let mut edited = gain_chain(100.0);
    edited.enabled = false;
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![edited.clone()],
        midi: None,
    };
    controller
        .upsert_chain(&project, &edited)
        .expect("upsert (pause) must not error");
    std::thread::sleep(Duration::from_millis(500));
    let peak_open = di_render_peak(&controller);

    assert!(
        peak_open > peak_quiet * 2.0,
        "a param edit on the paused (DI-only) chain did NOT reach the monitored \
         DI: quiet={peak_quiet:.4} open={peak_open:.4}. The DI kept the stale \
         render — the owner's 'change the param and nothing until I toggle the block'."
    );
}

/// The owner's PRIMARY report: monitoring a DI, change the chain config →
/// NOTHING changes. The DI must re-render through the edited chain.
#[test]
fn a_live_config_edit_re_renders_the_monitored_di() {
    init_registry();
    let mut controller = controller_with_active_chain(&gain_chain(15.0));

    // Arm the DI with a steady tone while the gain block attenuates hard.
    let pcm = Arc::new(engine::DiPcm::new(vec![0.6; 48_000], 48_000, 1));
    controller
        .arm_di_stream(&gain_chain(15.0), Arc::clone(&pcm))
        .expect("arm DI");
    wait_for_di_render(&controller);
    let peak_quiet = di_render_peak(&controller);

    // Edit the chain config: open the gain to unity. Push it through the live
    // off-thread rebuild path the GUI uses for a param edit.
    let edited = gain_chain(100.0);
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![edited.clone()],
        midi: None,
    };
    assert!(
        controller
            .request_offthread_rebuild_if_live(&project, &edited)
            .expect("live rebuild request"),
        "a live param edit must schedule an off-thread rebuild"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while controller.poll_pending_rebuilds() == 0 {
        assert!(Instant::now() < deadline, "rebuild never completed");
        std::thread::sleep(Duration::from_millis(10));
    }
    // Give the (re-armed) DI render time to refill with the new config.
    std::thread::sleep(Duration::from_millis(400));
    let peak_open = di_render_peak(&controller);

    assert!(
        peak_open > peak_quiet * 2.0,
        "changing the chain config did NOT reach the monitored DI: rendered DI \
         peak stayed quiet={peak_quiet:.4} open={peak_open:.4}. The DI keeps \
         playing the stale render — exactly the owner's 'I play a DI and change \
         the config and NOTHING changes'."
    );
}

/// #785: the DI re-render must be GAPLESS. The owner monitors a DI and every
/// live edit — a param change or a block toggle — interrupts the DI playback
/// for a moment, while the guitar chain keeps sounding. The re-arm tears the
/// live DI stream down and rebuilds it, so the playback drops a chunk. The DI
/// must keep playing across the edit: no silent run beyond the callback-sized
/// jitter the ring already absorbs.
#[test]
fn a_live_config_edit_must_not_interrupt_the_monitored_di() {
    init_registry();
    let mut controller = controller_with_active_chain(&gain_chain(100.0));

    // DC source at unity gain: the rendered DI is a constant, never silent.
    let pcm = Arc::new(engine::DiPcm::new(vec![0.6; 48_000 * 4], 48_000, 1));
    controller
        .arm_di_stream(&gain_chain(100.0), Arc::clone(&pcm))
        .expect("arm DI");
    wait_for_di_render(&controller);

    // The cell the output callback captured at stream-build time.
    let cell = controller.di_playback_cell(&ChainId(CHAIN_ID.into()), 0);
    let baseline = di_max_silence_run(&cell, 0.4);

    // Edit a param through the live off-thread rebuild path the GUI uses.
    let edited = gain_chain(60.0);
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![edited.clone()],
        midi: None,
    };
    assert!(
        controller
            .request_offthread_rebuild_if_live(&project, &edited)
            .expect("live rebuild request"),
        "a live param edit must schedule an off-thread rebuild"
    );

    // Keep the output callback consuming WHILE the frontend polls the rebuild
    // in — the DI must stay audible across the whole edit, not just after it.
    let during_edit = di_max_silence_run_while(&cell, 1.0, || {
        controller.poll_pending_rebuilds();
    });

    let tolerated = baseline.max(BUF * 2);
    assert!(
        during_edit <= tolerated,
        "#785: the live param edit interrupted the monitored DI — longest silent \
         run went from {baseline} frames (idle) to {during_edit} frames across the \
         edit (tolerated {tolerated}). The re-arm tears the DI stream down instead \
         of swapping the new render in gaplessly."
    );
}

/// #785, same gap through the block-toggle path: enabling/disabling a block
/// re-arms the DI too, so the monitored DI cuts out on every toggle.
#[test]
fn a_live_block_toggle_must_not_interrupt_the_monitored_di() {
    init_registry();
    let controller = controller_with_active_chain(&gain_chain(100.0));

    let pcm = Arc::new(engine::DiPcm::new(vec![0.6; 48_000 * 4], 48_000, 1));
    controller
        .arm_di_stream(&gain_chain(100.0), Arc::clone(&pcm))
        .expect("arm DI");
    wait_for_di_render(&controller);

    let cell = controller.di_playback_cell(&ChainId(CHAIN_ID.into()), 0);
    let baseline = di_max_silence_run(&cell, 0.4);

    let mut edited = gain_chain(100.0);
    edited.blocks[0].enabled = false;
    controller
        .toggle_block_enabled_live(&edited, &BlockId(GAIN_BLOCK_ID.into()), false)
        .expect("live block toggle must apply");

    let during_toggle = di_max_silence_run(&cell, 1.0);

    let tolerated = baseline.max(BUF * 2);
    assert!(
        during_toggle <= tolerated,
        "#785: the live block toggle interrupted the monitored DI — longest silent \
         run went from {baseline} frames (idle) to {during_toggle} frames across the \
         toggle (tolerated {tolerated}). The re-arm tears the DI stream down instead \
         of swapping the new render in gaplessly."
    );
}

/// #785: the OLD DI render must die. A hand-off leaves the outgoing worker
/// running on purpose (it feeds the playback the listener still hears) — the
/// incoming one stops it when it takes over. Edits arrive faster than a render
/// builds, so several workers can be in flight at once; when the dust settles
/// exactly ONE may be left. A worker left behind renders a whole chain into a
/// ring nobody plays, forever.
#[test]
fn rapid_live_edits_leave_exactly_one_di_worker_alive() {
    init_registry();
    let mut controller = controller_with_active_chain(&gain_chain(100.0));

    let pcm = Arc::new(engine::DiPcm::new(vec![0.6; 48_000 * 4], 48_000, 1));
    controller
        .arm_di_stream(&gain_chain(100.0), Arc::clone(&pcm))
        .expect("arm DI");
    wait_for_di_render(&controller);
    let cell = controller.di_playback_cell(&ChainId(CHAIN_ID.into()), 0);

    // Five edits back to back, as fast as the GUI can dispatch them — each one
    // re-arms while the previous render may still be building.
    for volume in [90.0, 80.0, 70.0, 60.0, 50.0] {
        let edited = gain_chain(volume);
        let project = Project {
            name: None,
            device_settings: vec![],
            chains: vec![edited.clone()],
            midi: None,
        };
        controller
            .request_offthread_rebuild_if_live(&project, &edited)
            .expect("live rebuild request");
        // Keep the output consuming so a hand-off can actually land.
        di_max_silence_run_while(&cell, 0.15, || {
            controller.poll_pending_rebuilds();
        });
    }

    // Let every hand-off settle (and every superseded worker notice).
    let deadline = Instant::now() + Duration::from_secs(10);
    while crate::di_stream_worker::DI_WORKERS_ALIVE.load(std::sync::atomic::Ordering::Relaxed) > 1
        && Instant::now() < deadline
    {
        di_max_silence_run_while(&cell, 0.1, || {
            controller.poll_pending_rebuilds();
        });
    }

    let alive =
        crate::di_stream_worker::DI_WORKERS_ALIVE.load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        alive, 1,
        "#785: {alive} di-stream workers still alive after the edits settled — a \
         superseded render was left behind, burning a core rendering the chain \
         into a ring nobody plays."
    );

    controller.disarm_di_stream(&ChainId(CHAIN_ID.into()));
    let deadline = Instant::now() + Duration::from_secs(5);
    while crate::di_stream_worker::DI_WORKERS_ALIVE.load(std::sync::atomic::Ordering::Relaxed) > 0
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        crate::di_stream_worker::DI_WORKERS_ALIVE.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "#785: disarm must stop every DI render thread"
    );
}

/// The owner's SECOND report, on the DI monitor: "I disable a block and the
/// effect keeps going." The #522 fast toggle flips only the guitar runtime, so
/// the dedicated DI pre-render must be re-rendered for the disable to be heard
/// on the DI.
#[test]
fn disabling_a_block_re_renders_the_monitored_di() {
    init_registry();
    let controller = controller_with_active_chain(&gain_chain(15.0));

    // Arm the DI while the gain block attenuates the tone hard.
    let pcm = Arc::new(engine::DiPcm::new(vec![0.6; 48_000], 48_000, 1));
    controller
        .arm_di_stream(&gain_chain(15.0), Arc::clone(&pcm))
        .expect("arm DI");
    wait_for_di_render(&controller);
    let peak_with_block = di_render_peak(&controller);

    // Disable the block through the same live toggle path the GUI uses. The
    // dispatcher has already flipped `enabled` in the project by the time the
    // sync runs, so the chain handed to the toggle carries the block OFF.
    let mut edited = gain_chain(15.0);
    edited.blocks[0].enabled = false;
    controller
        .toggle_block_enabled_live(&edited, &BlockId(GAIN_BLOCK_ID.into()), false)
        .expect("live block toggle must apply");
    std::thread::sleep(Duration::from_millis(400));
    let peak_without_block = di_render_peak(&controller);

    assert!(
        peak_without_block > peak_with_block * 2.0,
        "disabling the block did NOT reach the monitored DI: rendered DI peak \
         stayed attenuated with={peak_with_block:.4} without={peak_without_block:.4}. \
         The DI kept playing the block's effect — the owner's 'I disable a block \
         and the effect keeps going'."
    );
}

/// The owner's action: disable a block on a running chain. The effect MUST
/// stop — the sound has to return to the uncompressed dynamics.
#[test]
fn disabling_a_block_on_the_live_chain_stops_the_effect() {
    init_registry();
    let controller = controller_with_active_chain(&chain(true, 4.0));

    // Warm the compressor, then read the compressed profile.
    let _ = drive_live(&controller, 0.9, 0.3);
    let ratio_on = dynamics_ratio(&controller);

    // Disable the block through the SAME fast path `sync_block_toggle` uses.
    controller
        .set_block_enabled(&ChainId(CHAIN_ID.into()), &BlockId(BLOCK_ID.into()), false)
        .expect("fast-path disable must apply to the live runtime");
    // Let the click-safe fade settle on the live runtime.
    let _ = drive_live(&controller, 0.9, 0.3);
    let ratio_off = dynamics_ratio(&controller);

    assert!(
        ratio_off > ratio_on * 1.5,
        "disabling the block changed nothing the callback can hear: loud/quiet \
         RMS ratio on={ratio_on:.2} off={ratio_off:.2}. The effect kept going \
         even though the user turned the block off."
    );
}

/// The owner's action: change the chain config (a block parameter) while the
/// chain runs. The sound MUST change — the live edit path must rebuild the
/// runtime and publish it into the slot the callback reads.
#[test]
fn a_live_config_edit_changes_the_sound() {
    init_registry();
    let mut controller = controller_with_active_chain(&chain(true, 4.0));

    // Capture the slot handle ONCE, the way the cpal output callback does at
    // stream-build time. Everything below reads through THIS captured handle —
    // if the rebuild's publish is not observed here, the config edit is
    // inaudible even though the controller thinks it applied it.
    let captured = live_slot(&controller);
    let _ = drive_slot(&captured, 0.9, 0.3);
    let ratio_before = drive_slot(&captured, 0.9, 0.5) / drive_slot(&captured, 0.05, 0.5);
    let runtime_before = Arc::as_ptr(&controller.chain_runtime(&ChainId(CHAIN_ID.into())).unwrap());

    // Edit the config: drop the compressor to 1:1 (no compression). Push it
    // through the live off-thread rebuild path the GUI takes for a param edit.
    let edited = chain(true, 1.0);
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![edited.clone()],
        midi: None,
    };
    let scheduled = controller
        .request_offthread_rebuild_if_live(&project, &edited)
        .expect("live rebuild request must succeed");
    assert!(
        scheduled,
        "a live param edit on a running chain must schedule an off-thread \
         rebuild (the GUI relies on this for every config change)"
    );

    // Apply the finished rebuild the way the frontend tick does.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        controller.poll_pending_rebuilds();
        let now = Arc::as_ptr(&controller.chain_runtime(&ChainId(CHAIN_ID.into())).unwrap());
        if now != runtime_before {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the scheduled rebuild never reached the live slot — poll_pending_rebuilds \
             never swapped a new runtime in, so the config edit is inaudible"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let _ = drive_slot(&captured, 0.9, 0.3);
    let ratio_after = drive_slot(&captured, 0.9, 0.5) / drive_slot(&captured, 0.05, 0.5);

    assert!(
        ratio_after > ratio_before * 1.5,
        "changing the chain config changed nothing the captured callback slot \
         can hear: loud/quiet RMS ratio before={ratio_before:.2} after={ratio_after:.2}. \
         The edit never reached the live runtime."
    );
}
