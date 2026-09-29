//! #987 — the owner hears a small click whenever a running stream is
//! modified: a block turned on or off, a scene switched.
//!
//! THE RULE: a live edit never adds energy the signal does not have. A steady
//! tone through a chain of level-only blocks is a smooth curve. A change of
//! level may bend it, but only as a fade. A step (the output jumps from one
//! level to another between two samples) or a gap (the output drops to silence
//! while the tone plays) is broadband energy the tone never had: the click.
//! Both are read straight off the samples the callback emits.
//!
//! - Step: a sine of peak A at f has a second difference of at most
//!   A·(2πf/SR)². A fade bends the tone by a fraction of that. A step lands
//!   orders of magnitude above it.
//! - Gap: consecutive near-silent frames while the tone plays. A zero crossing
//!   of this tone lasts one sample.
//!
//! Each test drives the chain's live slot the way the cpal callbacks do: the
//! slot captured once, input then output on every callback, the frontend's
//! rebuild tick turning in between. It applies the edit through the surface
//! the app uses and scans every sample across it. The edits land on callbacks
//! that are not a whole number of tone periods apart, so each one meets the
//! tone at a different phase.

#![cfg(not(all(target_os = "linux", feature = "jack")))]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use engine::runtime::ChainRuntimeState;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use super::controller_live_edit_replicates_user_report_tests::{
    controller_with_active_chain, init_registry,
};
use super::{LiveRuntimeSlot, ProjectRuntimeController};

const SR: f32 = 48_000.0;
const BUF: usize = 64;
const TONE_HZ: f32 = 233.0;
const TONE_PEAK: f32 = 0.5;
const CHAIN: &str = "issue987-chain";
const LEVEL_A: &str = "issue987:level-a";
const LEVEL_B: &str = "issue987:level-b";
/// Callbacks before anything is heard: the start-up fade is over.
const WARMUP: usize = 64;
/// Callbacks between two edits (~95 ms), not a whole number of tone periods.
const BETWEEN: usize = 71;
/// Off/on (or scene A/B) rounds per test.
const ROUNDS: usize = 4;
/// How far above the tone's own curvature a second difference may go (+20 dB)
/// before it is a step.
const STEP_MARGIN: f32 = 10.0;
/// Below this a frame is silent (−80 dBFS).
const SILENT: f32 = 1e-4;
/// Longest silent run the tone makes on its own.
const MAX_SILENT_RUN: usize = 2;
/// Pause between two callbacks of the live audio thread: faster than the
/// device, so an edit that keeps the pipeline half built for a moment is
/// always caught inside it.
const LIVE_CALLBACK_PERIOD: Duration = Duration::from_micros(250);
/// Time between two edits on the live rig.
const LIVE_BETWEEN: Duration = Duration::from_millis(100);
const CAB: &str = "issue987:cab";
/// A repo IR fixture (git-lfs) with two captures, `b` and `d`.
const CAB_MODEL: &str = "ir_marshall_4x12_v30";
const CAB_CAPTURE: &str = "ir/marshall_4x12_v30/ir/ev_mix_b.wav";

fn level(id: &str, enabled: bool, volume_pct: f32) -> AudioBlock {
    let schema = schema_for_block_model("gain", "volume").expect("volume schema must exist");
    let mut params = ParameterSet::default();
    params.insert("volume", ParameterValue::Float(volume_pct));
    AudioBlock {
        id: BlockId(id.into()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: params
                .normalized_against(&schema)
                .expect("volume param must normalize"),
        }),
    }
}

fn chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(CHAIN.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
    }
}

/// The two callbacks of one HAL cycle, fed a tone, with every output sample
/// kept. It holds the slot handle the way a cpal stream does.
struct Player {
    slot: LiveRuntimeSlot,
    /// How long the input side may wait for an edit holding the processing
    /// lock: 0 for the device callback, one period for the DSP worker (#670),
    /// which is the path the app's F32 streams take.
    patience_ns: u64,
    phase: f32,
    input: Vec<f32>,
    output: Vec<f32>,
    scratch: Vec<f32>,
    loaded: Vec<Arc<ChainRuntimeState>>,
    heard: Vec<f32>,
}

impl Player {
    fn warmed_up(slot: LiveRuntimeSlot, patience_ns: u64) -> Self {
        let mut player = Self {
            slot,
            patience_ns,
            phase: 0.0,
            input: vec![0.0; BUF],
            output: vec![0.0; BUF * 2],
            scratch: vec![0.0; BUF * 2],
            loaded: Vec::with_capacity(1),
            heard: Vec::new(),
        };
        for _ in 0..WARMUP {
            player.callback();
        }
        player.heard.clear();
        player
    }

    /// One HAL cycle: the input callback, then the output callback.
    fn callback(&mut self) {
        let step = 2.0 * std::f32::consts::PI * TONE_HZ / SR;
        for s in self.input.iter_mut() {
            *s = TONE_PEAK * self.phase.sin();
            self.phase = (self.phase + step) % (2.0 * std::f32::consts::PI);
        }
        crate::slot_processing::process_input_buffer_patient(
            &self.slot,
            0,
            &self.input,
            1,
            self.patience_ns,
        );
        let out_slots = [self.slot.handle()];
        crate::slot_processing::process_output_buffer(
            &out_slots,
            &mut self.loaded,
            0,
            &mut self.output,
            2,
            &mut self.scratch,
        );
        self.heard
            .extend(self.output.chunks_exact(2).map(|frame| frame[0]));
    }
}

/// The chain as the app has it after activation. The seeded runtime is built
/// with the default elastic target, while the app activates and edits with
/// the targets of its live streams (`elastic::elastic_targets` on both paths),
/// so one settling edit before the audio starts gives it the app's routes.
/// Without it the first edit rebuilt the route (fill 256 -> 64) and played a
/// primed silent buffer the app never plays.
fn start_controller(first: &Chain) -> (ProjectRuntimeController, LiveRuntimeSlot) {
    init_registry();
    let mut controller = controller_with_active_chain(first);
    switch(&mut controller, first);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !controller.pending_rebuilds.is_empty() {
        controller.poll_pending_rebuilds();
        assert!(Instant::now() < deadline, "the settling edit never landed");
        std::thread::sleep(Duration::from_millis(1));
    }
    let slot = controller
        .chain_slots
        .get(&(ChainId(CHAIN.into()), 0))
        .expect("an active chain owns a live slot")
        .handle();
    (controller, slot)
}

/// The block toggle the GUI, MIDI and MCP send: the dispatcher has flipped
/// the block in the project, and the chain it hands over carries it. A block
/// with no live processor answers "needs full rebuild", and the app then takes
/// the live edit door.
fn toggle(controller: &mut ProjectRuntimeController, next: &Chain, block: &str, enabled: bool) {
    if controller
        .toggle_block_enabled_live(next, &BlockId(block.into()), enabled)
        .is_err()
    {
        switch(controller, next);
    }
}

/// A scene switch: the app's live edit door (`sync_live_chain_runtime`) for a
/// chain whose streams stay the same.
fn switch(controller: &mut ProjectRuntimeController, next: &Chain) {
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![next.clone()],
        midi: None,
    };
    assert!(
        controller
            .request_offthread_rebuild_if_live(&project, next)
            .expect("the live edit door must answer"),
        "a scene switch on a running chain must go through the live rebuild"
    );
}

fn assert_no_click(heard: &[f32], what: &str) {
    let report = scan(heard);
    assert!(
        report.steps == 0 && report.longest_gap <= MAX_SILENT_RUN,
        "#987 {what}: the output clicked. {} frames step above the tone's own \
         curvature (worst {:.1}x that curvature, first at {:.1} ms); longest silent \
         run {} frames (the tone makes at most {MAX_SILENT_RUN}). A live edit \
         must fade, never jump or drop out.",
        report.steps,
        report.worst_ratio,
        report.first_step_ms,
        report.longest_gap,
    );
}

/// A running chain whose callbacks run in lockstep with the control thread:
/// every sample is deterministic, and an edit lands between two callbacks.
struct Rig {
    controller: ProjectRuntimeController,
    player: Player,
}

impl Rig {
    fn start(first: &Chain) -> Self {
        let (controller, slot) = start_controller(first);
        Self {
            controller,
            player: Player::warmed_up(slot, 0),
        }
    }

    /// `callbacks` cycles with the frontend's rebuild tick between them, then
    /// on until no rebuild is left in flight.
    fn play(&mut self, callbacks: usize) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut played = 0;
        while played < callbacks || !self.controller.pending_rebuilds.is_empty() {
            self.controller.poll_pending_rebuilds();
            self.player.callback();
            played += 1;
            assert!(Instant::now() < deadline, "a rebuild never landed");
            std::thread::sleep(Duration::from_micros(300));
        }
    }

    fn toggle(&mut self, next: &Chain, block: &str, enabled: bool) {
        toggle(&mut self.controller, next, block, enabled);
    }

    fn switch(&mut self, next: &Chain) {
        switch(&mut self.controller, next);
    }

    fn assert_no_click(&self, what: &str) {
        assert_no_click(&self.player.heard, what);
    }
}

/// A running chain whose callbacks come from their own thread, the way
/// CoreAudio keeps calling in while the control thread edits: whatever an edit
/// leaves half done while it works is heard.
struct LiveRig {
    controller: ProjectRuntimeController,
    stop: Arc<AtomicBool>,
    audio: std::thread::JoinHandle<Vec<f32>>,
}

impl LiveRig {
    fn start(first: &Chain) -> Self {
        let (controller, slot) = start_controller(first);
        let stop = Arc::new(AtomicBool::new(false));
        let running = Arc::clone(&stop);
        let audio = std::thread::spawn(move || {
            let mut player = Player::warmed_up(slot, BUF as u64 * 1_000_000_000 / SR as u64);
            while !running.load(Ordering::Relaxed) {
                player.callback();
                std::thread::sleep(LIVE_CALLBACK_PERIOD);
            }
            player.heard
        });
        Self {
            controller,
            stop,
            audio,
        }
    }

    /// Let the audio run for `span` with the frontend's rebuild tick turning,
    /// then on until no rebuild is left in flight.
    fn play(&mut self, span: Duration) {
        let until = Instant::now() + span;
        let deadline = until + Duration::from_secs(10);
        while Instant::now() < until || !self.controller.pending_rebuilds.is_empty() {
            self.controller.poll_pending_rebuilds();
            assert!(Instant::now() < deadline, "a rebuild never landed");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn toggle(&mut self, next: &Chain, block: &str, enabled: bool) {
        toggle(&mut self.controller, next, block, enabled);
    }

    fn switch(&mut self, next: &Chain) {
        switch(&mut self.controller, next);
    }

    fn assert_no_click(self, what: &str) {
        self.stop.store(true, Ordering::Relaxed);
        let heard = self.audio.join().expect("the audio thread joins");
        assert_no_click(&heard, what);
    }
}

struct Scan {
    steps: usize,
    worst_ratio: f32,
    first_step_ms: f32,
    longest_gap: usize,
}

fn scan(heard: &[f32]) -> Scan {
    let peak = heard.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    let w = 2.0 * std::f32::consts::PI * TONE_HZ / SR;
    let bound = STEP_MARGIN * peak * w * w;
    let mut report = Scan {
        steps: 0,
        worst_ratio: 0.0,
        first_step_ms: f32::NAN,
        longest_gap: 0,
    };
    let mut run = 0;
    for (n, s) in heard.iter().enumerate() {
        run = if s.abs() < SILENT { run + 1 } else { 0 };
        report.longest_gap = report.longest_gap.max(run);
        if n < 2 {
            continue;
        }
        let curvature = (heard[n] - 2.0 * heard[n - 1] + heard[n - 2]).abs();
        if curvature > bound {
            if report.steps == 0 {
                report.first_step_ms = n as f32 * 1000.0 / SR;
            }
            report.steps += 1;
        }
        report.worst_ratio = report.worst_ratio.max(curvature / bound * STEP_MARGIN);
    }
    report
}

/// The scanner itself: a clean tone passes, a step and a gap are caught.
#[test]
fn the_scan_tells_a_click_from_a_tone() {
    let w = 2.0 * std::f32::consts::PI * TONE_HZ / SR;
    let tone: Vec<f32> = (0..4800)
        .map(|n| TONE_PEAK * (w * n as f32).sin())
        .collect();
    let clean = scan(&tone);
    assert_eq!(
        (clean.steps, clean.longest_gap <= MAX_SILENT_RUN),
        (0, true)
    );

    let mut faded = tone.clone();
    for (n, s) in faded.iter_mut().enumerate().skip(2400) {
        let t = ((n - 2400) as f32 / 480.0).min(1.0);
        *s *= 1.0 - 0.7 * (0.5 - 0.5 * (std::f32::consts::PI * t).cos());
    }
    assert_eq!(scan(&faded).steps, 0, "a 10 ms fade is not a click");

    let mut stepped = tone.clone();
    stepped.iter_mut().skip(2407).for_each(|s| *s *= 0.3);
    assert!(scan(&stepped).steps > 0, "a level step is a click");

    let mut gapped = tone;
    gapped[2400..2464].iter_mut().for_each(|s| *s = 0.0);
    assert!(
        scan(&gapped).longest_gap > MAX_SILENT_RUN,
        "a dropout is a click"
    );
}

#[test]
fn turning_a_block_off_and_on_while_the_tone_plays_does_not_click() {
    let on = chain(vec![level(LEVEL_A, true, 30.0)]);
    let off = chain(vec![level(LEVEL_A, false, 30.0)]);
    let mut rig = Rig::start(&on);
    rig.play(BETWEEN);
    for _ in 0..ROUNDS {
        rig.toggle(&off, LEVEL_A, false);
        rig.play(BETWEEN);
        rig.toggle(&on, LEVEL_A, true);
        rig.play(BETWEEN);
    }
    rig.assert_no_click("block off/on");
}

#[test]
fn turning_on_a_block_that_was_off_when_the_chain_started_does_not_click() {
    let on = chain(vec![level(LEVEL_A, true, 30.0)]);
    let off = chain(vec![level(LEVEL_A, false, 30.0)]);
    let mut rig = Rig::start(&off);
    rig.play(BETWEEN);
    for _ in 0..ROUNDS {
        rig.toggle(&on, LEVEL_A, true);
        rig.play(BETWEEN);
        rig.toggle(&off, LEVEL_A, false);
        rig.play(BETWEEN);
    }
    rig.assert_no_click("block that started off, turned on");
}

#[test]
fn switching_scene_while_the_tone_plays_does_not_click() {
    let scene_1 = chain(vec![
        level(LEVEL_A, true, 30.0),
        level(LEVEL_B, false, 70.0),
    ]);
    let scene_2 = chain(vec![
        level(LEVEL_A, false, 30.0),
        level(LEVEL_B, true, 70.0),
    ]);
    let mut rig = Rig::start(&scene_1);
    rig.play(BETWEEN);
    for _ in 0..ROUNDS {
        rig.switch(&scene_2);
        rig.play(BETWEEN);
        rig.switch(&scene_1);
        rig.play(BETWEEN);
    }
    rig.assert_no_click("scene switch");
}

/// The owner's chains hold VST3 reverbs, so every live edit on them takes the
/// in-place path (#779) instead of the fresh rebuild. A VST3 switched off
/// builds as a bypass node, so no plugin binary is needed to get there.
fn with_a_vst3(mut chain: Chain) -> Chain {
    chain.blocks.push(AudioBlock {
        id: BlockId("issue987:vst3-off".into()),
        enabled: false,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_VST3.into(),
            model: "vst3:Missing:Missing".into(),
            params: ParameterSet::default(),
        }),
    });
    chain
}

#[test]
fn switching_scene_on_a_chain_holding_a_vst3_does_not_click() {
    let scene_1 = with_a_vst3(chain(vec![
        level(LEVEL_A, true, 30.0),
        level(LEVEL_B, false, 70.0),
    ]));
    let scene_2 = with_a_vst3(chain(vec![
        level(LEVEL_A, false, 30.0),
        level(LEVEL_B, true, 70.0),
    ]));
    let mut rig = Rig::start(&scene_1);
    rig.play(BETWEEN);
    for _ in 0..ROUNDS {
        rig.switch(&scene_2);
        rig.play(BETWEEN);
        rig.switch(&scene_1);
        rig.play(BETWEEN);
    }
    rig.assert_no_click("scene switch on a chain holding a VST3");
}

#[test]
fn turning_on_a_block_that_was_off_on_a_chain_holding_a_vst3_does_not_click() {
    let on = with_a_vst3(chain(vec![level(LEVEL_A, true, 30.0)]));
    let off = with_a_vst3(chain(vec![level(LEVEL_A, false, 30.0)]));
    let mut rig = Rig::start(&off);
    rig.play(BETWEEN);
    for _ in 0..ROUNDS {
        rig.toggle(&on, LEVEL_A, true);
        rig.play(BETWEEN);
        rig.toggle(&off, LEVEL_A, false);
        rig.play(BETWEEN);
    }
    rig.assert_no_click("block that started off, turned on, on a chain holding a VST3");
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../engine/tests/fixtures/plugins")
}

/// True when the IR fixture is a real file, not a git-lfs pointer. Prints
/// BLOCKED otherwise, like the engine's real-block probes.
fn real_cab_ready(test: &str) -> bool {
    let path = fixtures_root().join(CAB_CAPTURE);
    let head: Vec<u8> = std::fs::read(&path)
        .map(|bytes| bytes.into_iter().take(64).collect())
        .unwrap_or_default();
    if head.is_empty() || head.starts_with(b"version https://git-lfs") {
        eprintln!(
            "BLOCKED {test}: the repo fixture {} is missing or still a git-lfs pointer \
             (run `git lfs pull`), so the real IR cannot be built",
            path.display()
        );
        return false;
    }
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        init_registry();
        ir::register_builder();
        plugin_loader::registry::init(&fixtures_root());
    });
    true
}

/// The repo's IR cab. A convolution is linear, so a tone through it stays a
/// tone: anything the scan finds is the edit, not the block.
fn cab(enabled: bool, capture: &str) -> AudioBlock {
    let schema = schema_for_block_model(block_core::EFFECT_TYPE_CAB, CAB_MODEL)
        .expect("the IR fixture's schema must exist");
    let mut params = ParameterSet::default();
    params.insert("preset", ParameterValue::String(capture.into()));
    AudioBlock {
        id: BlockId(CAB.into()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_CAB.into(),
            model: CAB_MODEL.into(),
            params: params
                .normalized_against(&schema)
                .expect("the cab params must normalize"),
        }),
    }
}

/// The owner's path: a chain holding a VST3 is edited in place (#779), with
/// the audio thread running while the edit builds the IR it switches to.
#[test]
fn switching_the_cab_by_scene_on_a_chain_holding_a_vst3_does_not_click() {
    if !real_cab_ready("switching_the_cab_by_scene_on_a_chain_holding_a_vst3_does_not_click") {
        return;
    }
    let scene_1 = with_a_vst3(chain(vec![level(LEVEL_A, true, 30.0), cab(true, "b")]));
    let scene_2 = with_a_vst3(chain(vec![level(LEVEL_A, true, 30.0), cab(true, "d")]));
    let mut rig = LiveRig::start(&scene_1);
    rig.play(LIVE_BETWEEN);
    for _ in 0..ROUNDS {
        rig.switch(&scene_2);
        rig.play(LIVE_BETWEEN);
        rig.switch(&scene_1);
        rig.play(LIVE_BETWEEN);
    }
    rig.assert_no_click("cab switched by scene, chain holding a VST3, audio running");
}

/// The owner's path for a pedal-style ON: a cab that was off when the chain
/// started has no live processor, so the toggle becomes an in-place edit that
/// builds it while the audio runs.
#[test]
fn turning_on_a_cab_that_was_off_on_a_chain_holding_a_vst3_does_not_click() {
    if !real_cab_ready("turning_on_a_cab_that_was_off_on_a_chain_holding_a_vst3_does_not_click") {
        return;
    }
    let on = with_a_vst3(chain(vec![level(LEVEL_A, true, 30.0), cab(true, "b")]));
    let off = with_a_vst3(chain(vec![level(LEVEL_A, true, 30.0), cab(false, "b")]));
    let mut rig = LiveRig::start(&off);
    rig.play(LIVE_BETWEEN);
    for _ in 0..ROUNDS {
        rig.toggle(&on, CAB, true);
        rig.play(LIVE_BETWEEN);
        rig.toggle(&off, CAB, false);
        rig.play(LIVE_BETWEEN);
    }
    rig.assert_no_click("cab that started off, turned on, chain holding a VST3, audio running");
}
