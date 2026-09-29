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

use std::sync::Arc;
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
    }
}

/// A running chain with a tone playing into it and every output sample kept.
struct Rig {
    controller: ProjectRuntimeController,
    slot: LiveRuntimeSlot,
    phase: f32,
    input: Vec<f32>,
    output: Vec<f32>,
    scratch: Vec<f32>,
    loaded: Vec<Arc<ChainRuntimeState>>,
    heard: Vec<f32>,
}

impl Rig {
    fn start(first: &Chain) -> Self {
        init_registry();
        let controller = controller_with_active_chain(first);
        let slot = controller
            .chain_slots
            .get(&(ChainId(CHAIN.into()), 0))
            .expect("an active chain owns a live slot")
            .handle();
        let mut rig = Self {
            controller,
            slot,
            phase: 0.0,
            input: vec![0.0; BUF],
            output: vec![0.0; BUF * 2],
            scratch: vec![0.0; BUF * 2],
            loaded: Vec::with_capacity(1),
            heard: Vec::new(),
        };
        for _ in 0..WARMUP {
            rig.callback();
        }
        rig.heard.clear();
        rig
    }

    /// One HAL cycle: the input callback, then the output callback.
    fn callback(&mut self) {
        let step = 2.0 * std::f32::consts::PI * TONE_HZ / SR;
        for s in self.input.iter_mut() {
            *s = TONE_PEAK * self.phase.sin();
            self.phase = (self.phase + step) % (2.0 * std::f32::consts::PI);
        }
        crate::slot_processing::process_input_buffer(&self.slot, 0, &self.input, 1);
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

    /// `callbacks` cycles with the frontend's rebuild tick between them, then
    /// on until no rebuild is left in flight.
    fn play(&mut self, callbacks: usize) {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut played = 0;
        while played < callbacks || !self.controller.pending_rebuilds.is_empty() {
            self.controller.poll_pending_rebuilds();
            self.callback();
            played += 1;
            assert!(Instant::now() < deadline, "a rebuild never landed");
            std::thread::sleep(Duration::from_micros(300));
        }
    }

    /// The block toggle the GUI, MIDI and MCP send: the dispatcher has flipped
    /// the block in the project, and the chain it hands over carries it. A
    /// block with no live processor answers "needs full rebuild", and the app
    /// then takes the live edit door.
    fn toggle(&mut self, next: &Chain, block: &str, enabled: bool) {
        if self
            .controller
            .toggle_block_enabled_live(next, &BlockId(block.into()), enabled)
            .is_err()
        {
            self.switch(next);
        }
    }

    /// A scene switch: the app's live edit door (`sync_live_chain_runtime`)
    /// for a chain whose streams stay the same.
    fn switch(&mut self, next: &Chain) {
        let project = Project {
            name: None,
            device_settings: vec![],
            chains: vec![next.clone()],
            midi: None,
        };
        assert!(
            self.controller
                .request_offthread_rebuild_if_live(&project, next)
                .expect("the live edit door must answer"),
            "a scene switch on a running chain must go through the live rebuild"
        );
    }

    fn assert_no_click(&self, what: &str) {
        let report = scan(&self.heard);
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
