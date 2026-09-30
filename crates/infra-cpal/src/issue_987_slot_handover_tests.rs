//! #987 — the slot handover, on every platform.
//!
//! `issue_987_live_edit_click_tests` proves the handover end to end through
//! the controller, but that controller hosts JACK on Linux with `jack` on, so
//! those tests are compiled out there — and that is where CI runs. These drive
//! the slot the audio callbacks read directly, with runtimes built the way the
//! rebuild worker builds them, so the crossfade and the release of the replaced
//! runtime are guarded everywhere.
//!
//! THE RULE is the same: a live edit never adds energy the signal does not
//! have. A steady tone crossfaded from one level to another is a smooth curve;
//! a step or a gap is the click.

use std::collections::HashMap;
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use engine::runtime::{build_chain_runtime_state, ChainRuntimeState, RuntimeGraph};
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::slot_handover::{new_runtime_gain, HANDOVER_FADE_FRAMES, HANDOVER_WARMUP_FRAMES};
use crate::slot_processing::{
    process_input_buffer, process_input_buffer_patient, process_output_buffer,
};
use crate::{LiveRuntimeSlot, ProjectRuntimeController};

const SR: f32 = 48_000.0;
const BUF: usize = 64;
const TONE_HZ: f32 = 233.0;
const QUIET_TONE: f32 = 0.5;
/// Two of these summed on one output go past full scale.
const LOUD_TONE: f32 = 0.8;
const ELASTIC_TARGET: usize = 256;
const CHAIN: &str = "issue987-slot";
const LEVEL: &str = "issue987-slot:level";
/// Callbacks before anything is heard: the start-up fade is over.
const WARMUP: usize = 64;
/// Callbacks heard before the edit.
const BEFORE: usize = 16;
/// Callbacks heard after the edit: well past the warm-up and the fade.
const AFTER: usize = 128;
/// Frames at the end of a run that must already be the new runtime alone.
const TAIL_FRAMES: usize = 2048;
/// How far above the tone's own curvature a second difference may go (+20 dB)
/// before it is a step.
const STEP_MARGIN: f32 = 10.0;
/// Below this a frame is silent (−80 dBFS).
const SILENT: f32 = 1e-4;
/// Longest silent run the tone makes on its own.
const MAX_SILENT_RUN: usize = 2;
/// Well past the time the control side leaves a replaced runtime in the slot.
const PAST_THE_HOLD: Duration = Duration::from_secs(1);

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(block_gain::register_natives);
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

fn chain(volume_pct: f32) -> Chain {
    init_registry();
    let schema = schema_for_block_model("gain", "volume").expect("volume schema must exist");
    let mut params = ParameterSet::default();
    params.insert("volume", ParameterValue::Float(volume_pct));
    Chain {
        id: ChainId(CHAIN.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![AudioBlock {
            id: BlockId(LEVEL.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: params
                    .normalized_against(&schema)
                    .expect("volume param must normalize"),
            }),
        }],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

/// A runtime built the way the rebuild worker builds one.
fn runtime(volume_pct: f32) -> Arc<ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(&chain(volume_pct), SR, &[ELASTIC_TARGET], &registry())
            .expect("the runtime must build"),
    )
}

/// One HAL cycle after another: every input slot fed the tone, then the
/// output callback mixing its slots. It holds slot handles the way a cpal
/// stream does.
struct Player {
    inputs: Vec<LiveRuntimeSlot>,
    outputs: Vec<LiveRuntimeSlot>,
    /// `None` for the device callback, `Some(patience)` for the DSP worker.
    patience_ns: Option<u64>,
    peak: f32,
    phase: f32,
    input: Vec<f32>,
    output: Vec<f32>,
    scratch: Vec<f32>,
    loaded: Vec<Arc<ChainRuntimeState>>,
    heard: Vec<f32>,
}

impl Player {
    fn new(slots: &[&LiveRuntimeSlot], peak: f32, patience_ns: Option<u64>) -> Self {
        let mut player = Self {
            inputs: slots.iter().map(|slot| slot.handle()).collect(),
            outputs: slots.iter().map(|slot| slot.handle()).collect(),
            patience_ns,
            peak,
            phase: 0.0,
            input: vec![0.0; BUF],
            output: vec![0.0; BUF * 2],
            scratch: vec![0.0; BUF * 2],
            loaded: Vec::with_capacity(slots.len()),
            heard: Vec::new(),
        };
        player.play(WARMUP);
        player.heard.clear();
        player
    }

    fn play(&mut self, callbacks: usize) {
        let step = 2.0 * std::f32::consts::PI * TONE_HZ / SR;
        for _ in 0..callbacks {
            for s in self.input.iter_mut() {
                *s = self.peak * self.phase.sin();
                self.phase = (self.phase + step) % (2.0 * std::f32::consts::PI);
            }
            for slot in &self.inputs {
                match self.patience_ns {
                    None => process_input_buffer(slot, 0, &self.input, 1),
                    Some(patience) => {
                        process_input_buffer_patient(slot, 0, &self.input, 1, patience)
                    }
                }
            }
            process_output_buffer(
                &self.outputs,
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
}

fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
}

/// The steady peak a runtime at `volume_pct` plays the quiet tone at.
fn steady_peak(volume_pct: f32) -> f32 {
    let slot = LiveRuntimeSlot::new(runtime(volume_pct));
    let mut player = Player::new(&[&slot], QUIET_TONE, None);
    player.play(TAIL_FRAMES / BUF);
    peak(&player.heard)
}

/// Frames whose second difference steps above the tone's own curvature, and
/// the longest silent run.
fn clicks(heard: &[f32]) -> (usize, usize) {
    let w = 2.0 * std::f32::consts::PI * TONE_HZ / SR;
    let bound = STEP_MARGIN * peak(heard) * w * w;
    let steps = heard
        .windows(3)
        .filter(|f| (f[2] - 2.0 * f[1] + f[0]).abs() > bound)
        .count();
    let mut run = 0;
    let mut longest_gap = 0;
    for s in heard {
        run = if s.abs() < SILENT { run + 1 } else { 0 };
        longest_gap = longest_gap.max(run);
    }
    (steps, longest_gap)
}

/// A slot playing the quiet tone at 30 %, handed over to a runtime at 70 %.
fn hand_over_while_playing(patience_ns: Option<u64>) -> (LiveRuntimeSlot, Vec<f32>) {
    let slot = LiveRuntimeSlot::new(runtime(30.0));
    let mut player = Player::new(&[&slot], QUIET_TONE, patience_ns);
    player.play(BEFORE);
    slot.hand_over(runtime(70.0));
    player.play(AFTER);
    (slot, player.heard)
}

fn assert_handed_over(slot: &LiveRuntimeSlot, heard: &[f32], path: &str) {
    let (steps, longest_gap) = clicks(heard);
    assert!(
        steps == 0 && longest_gap <= MAX_SILENT_RUN,
        "#987 ({path}): the handover clicked — {steps} frames step above the tone's \
         curvature, longest silent run {longest_gap} frames"
    );
    let before = peak(&heard[..BEFORE * BUF]);
    let after = peak(&heard[heard.len() - TAIL_FRAMES..]);
    let (quiet, loud) = (steady_peak(30.0), steady_peak(70.0));
    assert!(
        (before - quiet).abs() <= 0.02 * quiet && (after - loud).abs() <= 0.02 * loud,
        "#987 ({path}): the output must go from the replaced runtime's level \
         ({quiet:.4}) to the new one's ({loud:.4}); heard {before:.4} then {after:.4}"
    );
    assert!(
        !slot.handover().plays_on(0),
        "#987 ({path}): once crossfaded, the output plays the new runtime alone"
    );
}

#[test]
fn a_handover_on_the_device_callback_crossfades_without_a_click() {
    let (slot, heard) = hand_over_while_playing(None);
    assert_handed_over(&slot, &heard, "device callback");
}

#[test]
fn a_handover_on_the_dsp_worker_crossfades_without_a_click() {
    let patience = BUF as u64 * 1_000_000_000 / SR as u64;
    let (slot, heard) = hand_over_while_playing(Some(patience));
    assert_handed_over(&slot, &heard, "DSP worker");
}

/// Two runtimes of one device sum on its output (#743). A handover on one of
/// them plays both of its versions for a moment; the mix still never goes
/// past full scale.
#[test]
fn a_handover_on_a_shared_output_never_sums_past_full_scale() {
    let handed = LiveRuntimeSlot::new(runtime(100.0));
    let sibling = LiveRuntimeSlot::new(runtime(100.0));
    let mut player = Player::new(&[&handed, &sibling], LOUD_TONE, None);
    handed.hand_over(runtime(100.0));
    player.play(AFTER);
    let loudest = peak(&player.heard);
    assert!(
        loudest <= 1.0 && loudest > 0.95,
        "#987: two {LOUD_TONE} tones summed through a handover must be held under \
         full scale by the mix limiter, not clipped or dropped; peak {loudest:.4}"
    );
    assert!(
        !handed.handover().plays_on(0),
        "the shared output must finish the handover"
    );
}

#[test]
fn the_new_runtime_is_silent_while_it_warms_up_then_fades_in_smoothly() {
    assert_eq!(new_runtime_gain(0), 0.0);
    assert_eq!(new_runtime_gain(HANDOVER_WARMUP_FRAMES - 1), 0.0);
    assert_eq!(new_runtime_gain(HANDOVER_WARMUP_FRAMES), 0.0);
    let middle = new_runtime_gain(HANDOVER_WARMUP_FRAMES + HANDOVER_FADE_FRAMES / 2);
    assert!(
        (middle - 0.5).abs() < 1e-6,
        "half way the fade is at 0.5, got {middle}"
    );
    assert_eq!(
        new_runtime_gain(HANDOVER_WARMUP_FRAMES + HANDOVER_FADE_FRAMES),
        1.0
    );
    assert_eq!(new_runtime_gain(usize::MAX / 2), 1.0);

    // A raised cosine: rising, and no frame-to-frame jump past its steepest slope.
    let steepest = std::f32::consts::PI / (2.0 * HANDOVER_FADE_FRAMES as f32);
    let fade = HANDOVER_WARMUP_FRAMES..=HANDOVER_WARMUP_FRAMES + HANDOVER_FADE_FRAMES;
    let gains: Vec<f32> = fade.map(new_runtime_gain).collect();
    for pair in gains.windows(2) {
        let rise = pair[1] - pair[0];
        assert!(
            rise >= 0.0 && rise <= steepest + 1e-6,
            "the fade must rise smoothly, stepped by {rise}"
        );
    }
}

#[test]
fn a_replaced_runtime_is_released_only_after_the_handover_hold() {
    let old = runtime(30.0);
    let replaced = Arc::downgrade(&old);
    let slot = LiveRuntimeSlot::new(old);
    slot.hand_over(runtime(70.0));

    let now = Instant::now();
    assert!(
        slot.reap(now).is_empty(),
        "the replaced runtime keeps playing its handover"
    );
    let released = slot.reap(now + PAST_THE_HOLD);
    assert_eq!(
        released.len(),
        1,
        "past the hold the replaced runtime is released"
    );
    assert!(
        Arc::ptr_eq(&released[0], &replaced.upgrade().expect("still alive")),
        "the runtime released is the one the edit replaced"
    );
    drop(released);
    assert!(
        replaced.upgrade().is_none(),
        "the caller holds the last reference, so it drops off the audio thread"
    );
}

/// An audio thread that loaded the old runtime for its callback holds it; the
/// control side must not take the last reference out from under it.
#[test]
fn a_replaced_runtime_an_audio_thread_still_holds_is_kept_until_it_lets_go() {
    let slot = LiveRuntimeSlot::new(runtime(30.0));
    slot.hand_over(runtime(70.0));
    let in_callback = slot
        .handover()
        .outgoing_for_input(BUF)
        .expect("the input side still feeds the replaced runtime");

    let past = Instant::now() + PAST_THE_HOLD;
    assert!(
        slot.reap(past).is_empty(),
        "a runtime an audio thread holds is never handed back to be dropped"
    );
    drop(in_callback);
    assert_eq!(
        slot.reap(past).len(),
        1,
        "once the audio thread lets go it is released"
    );
}

/// Two edits faster than a handover: the runtime the second one displaces is
/// already out of every callback, so it is released at once.
#[test]
fn a_second_edit_during_a_handover_releases_the_runtime_it_displaces() {
    let first = runtime(30.0);
    let displaced = Arc::downgrade(&first);
    let slot = LiveRuntimeSlot::new(first);
    slot.hand_over(runtime(50.0));
    slot.hand_over(runtime(70.0));

    let released = slot.reap(Instant::now());
    assert_eq!(
        released.len(),
        1,
        "the displaced runtime is released at once"
    );
    assert!(Arc::ptr_eq(
        &released[0],
        &displaced.upgrade().expect("still alive")
    ));
    assert_eq!(
        slot.reap(Instant::now() + PAST_THE_HOLD).len(),
        1,
        "the runtime the second edit replaced follows after its hold"
    );
}

/// The frontend tick applies a live rebuild as a handover, then drops the
/// runtime it replaced on the control worker once the handover is over.
#[test]
fn the_rebuild_tick_drops_the_replaced_runtime_once_its_handover_is_over() {
    let old = runtime(30.0);
    let replaced = Arc::downgrade(&old);
    let mut chains = HashMap::new();
    chains.insert((ChainId(CHAIN.into()), 0), old);
    let mut controller = ProjectRuntimeController::for_testing(RuntimeGraph { chains });
    controller.set_io_bindings(registry());

    controller.schedule_chain_rebuild(&chain(70.0), SR, HashMap::new(), vec![ELASTIC_TARGET]);
    let deadline = Instant::now() + Duration::from_secs(10);
    while !controller.pending_rebuilds.is_empty() {
        controller.poll_pending_rebuilds();
        assert!(Instant::now() < deadline, "the rebuild never landed");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        replaced.upgrade().is_some(),
        "the replaced runtime keeps playing its handover"
    );

    std::thread::sleep(PAST_THE_HOLD);
    controller.poll_pending_rebuilds();
    let deadline = Instant::now() + Duration::from_secs(10);
    while replaced.upgrade().is_some() {
        assert!(
            Instant::now() < deadline,
            "#987: the replaced runtime was never dropped after its handover"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
