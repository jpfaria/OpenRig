//! #992 — an LV2 block (x42 fil4, a 2-in/2-out plugin) must act on the live
//! guitar input exactly as it acts on any other source.
//!
//! Reported 2026-09-29: the fil4 "has no effect on the live guitar". The
//! whole history is in `docs/audio-incidents/992-filter-silent-on-live-guitar.md`.
//!
//! THE RULE: a fil4 whose master gain is −18 dB takes 18 dB off a steady tone
//! played into the chain's mono guitar input — whether the chain was started
//! with that gain or the gain was turned down while it plays, whether the
//! guitar's Main is stereo or a single mono channel (invariant #5: the stream
//! is stereo inside, a mono output is a mixdown at the end).
//!
//! Each test drives the chain's live slot the way the cpal callbacks do (the
//! slot captured once, input then output on every callback, the frontend's
//! rebuild tick in between), with the mono input binding the app resolves for
//! a guitar, and reads the level straight off the samples the output emits.

#![cfg(not(all(target_os = "linux", feature = "jack")))]

use std::path::PathBuf;
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
const CHAIN: &str = "issue992-chain";
const FIL4: &str = "issue992:fil4";
const FIL4_MODEL: &str = "lv2_x42_fil4";
const FIL4_EFFECT_TYPE: &str = "filter";
const AMP: &str = "issue992:amp";
const AMP_MODEL: &str = "nam_marshall_jcm_800_2203_a2";
const AMP_EFFECT_TYPE: &str = "preamp";
/// Callbacks before anything is measured: start-up fade and any rebuild
/// handover (warm-up + crossfade) are long over.
const SETTLE: usize = 200;
/// Callbacks the level is read over (~0.27 s, a whole number of nothing).
const MEASURE: usize = 200;
/// The fil4 master gain the owner set.
const CUT_DB: f32 = -18.0;
/// How far the measured drop may stray from the gain the block was given.
const TOLERANCE_DB: f32 = 1.5;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../engine/tests/fixtures/plugins")
}

/// The fil4 binary is a git-lfs fixture; prints BLOCKED when it is still a
/// pointer, like the other real-block probes.
fn fil4_ready(test: &str) -> bool {
    let path = fixtures_root().join(if cfg!(target_os = "macos") {
        "lv2/x42_fil4/platform/macos-universal/fil4.dylib"
    } else if cfg!(target_os = "windows") {
        "lv2/x42_fil4/platform/windows-x86_64/fil4.dll"
    } else if cfg!(target_arch = "aarch64") {
        "lv2/x42_fil4/platform/linux-aarch64/fil4.so"
    } else {
        "lv2/x42_fil4/platform/linux-x86_64/fil4.so"
    });
    let head: Vec<u8> = std::fs::read(&path)
        .map(|bytes| bytes.into_iter().take(64).collect())
        .unwrap_or_default();
    if head.is_empty() || head.starts_with(b"version https://git-lfs") {
        eprintln!(
            "BLOCKED {test}: the repo fixture {} is missing or still a git-lfs pointer \
             (run `git lfs pull`), so the real fil4 cannot be built",
            path.display()
        );
        return false;
    }
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        init_registry();
        lv2::register_builder();
        nam::register_builder();
        plugin_loader::registry::init(&fixtures_root());
    });
    true
}

/// A repo NAM capture (git-lfs) in front of the fil4, as on the owner's chain:
/// the amp is a dual-mono block, the fil4 a true-stereo one.
fn amp() -> AudioBlock {
    let schema = schema_for_block_model(AMP_EFFECT_TYPE, AMP_MODEL)
        .expect("the NAM fixture's schema must exist");
    AudioBlock {
        id: BlockId(AMP.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: AMP_EFFECT_TYPE.into(),
            model: AMP_MODEL.into(),
            params: ParameterSet::default()
                .normalized_against(&schema)
                .expect("the amp params must normalize"),
        }),
    }
}

fn fil4(gain_db: f32) -> AudioBlock {
    let schema = schema_for_block_model(FIL4_EFFECT_TYPE, FIL4_MODEL)
        .expect("the fil4 fixture's schema must exist");
    let mut params = ParameterSet::default();
    params.insert("gain", ParameterValue::Float(gain_db));
    AudioBlock {
        id: BlockId(FIL4.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: FIL4_EFFECT_TYPE.into(),
            model: FIL4_MODEL.into(),
            params: params
                .normalized_against(&schema)
                .expect("the fil4 params must normalize"),
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

/// The app's live edit door (`sync_live_chain_runtime`) for a chain whose
/// streams stay the same: a parameter edit, a scene switch.
fn edit(controller: &mut ProjectRuntimeController, next: &Chain) {
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
        "an edit on a running chain must go through the live rebuild"
    );
}

/// A running chain on its mono guitar input, callbacks in lockstep with the
/// control thread.
struct Rig {
    controller: ProjectRuntimeController,
    slot: LiveRuntimeSlot,
    phase: f32,
    input: Vec<f32>,
    output: Vec<f32>,
    scratch: Vec<f32>,
    loaded: Vec<Arc<ChainRuntimeState>>,
}

impl Rig {
    /// The chain as the app has it after activation: one settling edit gives
    /// the seeded runtime the app's elastic targets (see #987).
    fn start(first: &Chain) -> Self {
        let mut controller = controller_with_active_chain(first);
        edit(&mut controller, first);
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
        };
        rig.play(SETTLE);
        rig
    }

    /// One HAL cycle: the input callback, then the output callback. Returns
    /// the left channel it emitted.
    fn callback(&mut self) -> Vec<f32> {
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
            BUF as u64 * 1_000_000_000 / SR as u64,
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
        self.output.chunks_exact(2).map(|frame| frame[0]).collect()
    }

    /// `callbacks` cycles with the frontend's rebuild tick between them, then
    /// on until no rebuild is left in flight. Returns everything heard.
    fn play(&mut self, callbacks: usize) -> Vec<f32> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut heard = Vec::with_capacity(callbacks * BUF);
        let mut played = 0;
        while played < callbacks || !self.controller.pending_rebuilds.is_empty() {
            self.controller.poll_pending_rebuilds();
            heard.extend(self.callback());
            played += 1;
            assert!(Instant::now() < deadline, "a rebuild never landed");
            std::thread::sleep(Duration::from_micros(300));
        }
        heard
    }

    /// The RMS level, in dBFS, of the next `MEASURE` callbacks.
    fn level_db(&mut self) -> f32 {
        rms_db(&self.play(MEASURE))
    }
}

fn rms_db(samples: &[f32]) -> f32 {
    let mean_sq = samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32;
    10.0 * mean_sq.max(1e-20).log10()
}

fn assert_cut(drop_db: f32, what: &str) {
    assert!(
        (drop_db - (-CUT_DB)).abs() <= TOLERANCE_DB,
        "#992 {what}: the fil4 master gain was {CUT_DB} dB but the live input's \
         output dropped by {drop_db:.1} dB. A block must act on the live input as \
         it does on any other source."
    );
}

/// The block as the chain was started: a fil4 at −18 dB against one at 0 dB.
#[test]
fn a_fil4_started_at_minus_18_db_cuts_the_live_mono_input_by_18_db() {
    if !fil4_ready("a_fil4_started_at_minus_18_db_cuts_the_live_mono_input_by_18_db") {
        return;
    }
    let unity = Rig::start(&chain(vec![fil4(0.0)])).level_db();
    let cut = Rig::start(&chain(vec![fil4(CUT_DB)])).level_db();
    assert!(
        unity > -20.0,
        "#992 the tone must come through a fil4 at 0 dB (read {unity:.1} dBFS)"
    );
    assert_cut(unity - cut, "fil4 started at -18 dB");
}

/// The owner's gesture: the gain turned down while the guitar plays.
#[test]
fn turning_the_fil4_gain_down_while_the_live_input_plays_cuts_it_by_18_db() {
    if !fil4_ready("turning_the_fil4_gain_down_while_the_live_input_plays_cuts_it_by_18_db") {
        return;
    }
    let mut rig = Rig::start(&chain(vec![fil4(0.0)]));
    let before = rig.level_db();
    edit(&mut rig.controller, &chain(vec![fil4(CUT_DB)]));
    rig.play(SETTLE);
    let after = rig.level_db();
    assert!(
        before > -20.0,
        "#992 the tone must come through a fil4 at 0 dB (read {before:.1} dBFS)"
    );
    assert_cut(before - after, "fil4 gain turned down while playing");
}

/// The owner's chain shape: a dual-mono NAM amp, then the fil4. After the amp
/// the fil4 is linear, so its −18 dB must come out as 18 dB.
#[test]
fn turning_the_fil4_gain_down_after_a_nam_amp_on_the_live_input_cuts_it_by_18_db() {
    if !fil4_ready("turning_the_fil4_gain_down_after_a_nam_amp_on_the_live_input_cuts_it_by_18_db")
    {
        return;
    }
    let mut rig = Rig::start(&chain(vec![amp(), fil4(0.0)]));
    let before = rig.level_db();
    edit(&mut rig.controller, &chain(vec![amp(), fil4(CUT_DB)]));
    rig.play(SETTLE);
    let after = rig.level_db();
    assert!(
        before > -40.0,
        "#992 the tone must come through the amp and a fil4 at 0 dB (read {before:.1} dBFS)"
    );
    assert_cut(
        before - after,
        "fil4 after a NAM amp, gain turned down while playing",
    );
}

/// The owner's chains hold VST3 reverbs, so every live edit on them takes the
/// in-place path (#779) instead of the fresh rebuild. A VST3 switched off
/// builds as a bypass node, so no plugin binary is needed to get there.
fn with_a_vst3(mut chain: Chain) -> Chain {
    chain.blocks.push(AudioBlock {
        id: BlockId("issue992:vst3-off".into()),
        enabled: false,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_VST3.into(),
            model: "vst3:Missing:Missing".into(),
            params: ParameterSet::default(),
        }),
    });
    chain
}

/// The owner's path: the gain turned down on a chain holding a VST3.
#[test]
fn turning_the_fil4_gain_down_on_a_chain_holding_a_vst3_cuts_the_live_input_by_18_db() {
    if !fil4_ready(
        "turning_the_fil4_gain_down_on_a_chain_holding_a_vst3_cuts_the_live_input_by_18_db",
    ) {
        return;
    }
    let mut rig = Rig::start(&with_a_vst3(chain(vec![amp(), fil4(0.0)])));
    let before = rig.level_db();
    edit(
        &mut rig.controller,
        &with_a_vst3(chain(vec![amp(), fil4(CUT_DB)])),
    );
    rig.play(SETTLE);
    let after = rig.level_db();
    assert!(
        before > -40.0,
        "#992 the tone must come through the amp and a fil4 at 0 dB (read {before:.1} dBFS)"
    );
    assert_cut(
        before - after,
        "fil4 gain turned down while playing, chain holding a VST3",
    );
}

/// The owner's first gesture: the fil4 added to a chain that is playing.
#[test]
fn adding_a_fil4_at_minus_18_db_to_a_chain_holding_a_vst3_cuts_the_live_input_by_18_db() {
    if !fil4_ready(
        "adding_a_fil4_at_minus_18_db_to_a_chain_holding_a_vst3_cuts_the_live_input_by_18_db",
    ) {
        return;
    }
    let mut rig = Rig::start(&with_a_vst3(chain(vec![amp()])));
    let before = rig.level_db();
    edit(
        &mut rig.controller,
        &with_a_vst3(chain(vec![amp(), fil4(CUT_DB)])),
    );
    rig.play(SETTLE);
    let after = rig.level_db();
    assert_cut(
        before - after,
        "fil4 at -18 dB added while playing, chain holding a VST3",
    );
}

/// The same gesture on a chain with no VST3 (the fresh off-thread rebuild).
#[test]
fn adding_a_fil4_at_minus_18_db_to_a_playing_chain_cuts_the_live_input_by_18_db() {
    if !fil4_ready("adding_a_fil4_at_minus_18_db_to_a_playing_chain_cuts_the_live_input_by_18_db") {
        return;
    }
    let mut rig = Rig::start(&chain(vec![amp()]));
    let before = rig.level_db();
    edit(&mut rig.controller, &chain(vec![amp(), fil4(CUT_DB)]));
    rig.play(SETTLE);
    let after = rig.level_db();
    assert_cut(before - after, "fil4 at -18 dB added while playing");
}

/// The owner's E/S registry as `config.yaml` holds it: `guitarra-1` is a mono
/// input into a stereo Main, `syn2-main` the SYN-2 loop (stereo return, mono
/// send).
fn owners_registry() -> Vec<domain::io_binding::IoBinding> {
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    let ep = |name: &str, mode: ChannelMode, channels: &[usize]| IoEndpoint {
        name: name.into(),
        device_id: domain::ids::DeviceId("dev".into()),
        mode,
        channels: channels.to_vec(),
    };
    vec![
        IoBinding {
            id: "io".into(),
            name: "GUITARRA 1".into(),
            inputs: vec![ep("in", ChannelMode::Mono, &[0])],
            outputs: vec![ep("main", ChannelMode::Stereo, &[0, 1])],
        },
        IoBinding {
            id: "syn2-main".into(),
            name: "SYN-2".into(),
            inputs: vec![ep("ret", ChannelMode::Stereo, &[2, 3])],
            outputs: vec![ep("snd", ChannelMode::Mono, &[3])],
        },
        // Two more E/S read the same guitar channel, each into one side of
        // Main as a mono output.
        IoBinding {
            id: "guitarra-1-main-l".into(),
            name: "GUITARRA 1 - MAIN L".into(),
            inputs: vec![ep("in", ChannelMode::Mono, &[0])],
            outputs: vec![ep("main-l", ChannelMode::Mono, &[0])],
        },
        IoBinding {
            id: "guitarra-1-main-r".into(),
            name: "GUITARRA 1 - MAIN R".into(),
            inputs: vec![ep("in", ChannelMode::Mono, &[0])],
            outputs: vec![ep("main-r", ChannelMode::Mono, &[1])],
        },
    ]
}

fn insert_off() -> AudioBlock {
    AudioBlock {
        id: BlockId("issue992:insert".into()),
        enabled: false,
        kind: AudioBlockKind::Insert(project::block::InsertBlock {
            model: "standard".into(),
            io: "syn2-main".into(),
        }),
    }
}

/// The level of the tone through a runtime built straight from `chain` and
/// the owner's registry, read off its main route.
fn built_level_db(chain: &Chain) -> f32 {
    let runtime = Arc::new(
        engine::runtime_graph::build_chain_runtime_state(
            chain,
            SR,
            &[engine::runtime_audio_frame::DEFAULT_ELASTIC_TARGET],
            &owners_registry(),
        )
        .expect("the owner's chain must build"),
    );
    let slot = LiveRuntimeSlot::new(runtime);
    let (mut phase, mut heard) = (0.0_f32, Vec::new());
    let (mut input, mut output, mut scratch) =
        (vec![0.0; BUF], vec![0.0; BUF * 2], vec![0.0; BUF * 2]);
    let mut loaded = Vec::with_capacity(1);
    let step = 2.0 * std::f32::consts::PI * TONE_HZ / SR;
    for n in 0..(SETTLE + MEASURE) {
        for s in input.iter_mut() {
            *s = TONE_PEAK * phase.sin();
            phase = (phase + step) % (2.0 * std::f32::consts::PI);
        }
        crate::slot_processing::process_input_buffer_patient(&slot, 0, &input, 1, 0);
        crate::slot_processing::process_output_buffer(
            &[slot.handle()],
            &mut loaded,
            0,
            &mut output,
            2,
            &mut scratch,
        );
        if n >= SETTLE {
            heard.extend(output.chunks_exact(2).map(|frame| frame[0]));
        }
    }
    rms_db(&heard)
}

/// The owner's chain as it stood: the SYN-2 insert switched off at the head,
/// then the fil4, on `guitarra-1` (mono in, stereo Main).
#[test]
fn a_fil4_after_an_insert_that_is_off_cuts_the_owners_guitar_by_18_db() {
    if !fil4_ready("a_fil4_after_an_insert_that_is_off_cuts_the_owners_guitar_by_18_db") {
        return;
    }
    let unity = built_level_db(&chain(vec![insert_off(), fil4(0.0)]));
    let cut = built_level_db(&chain(vec![insert_off(), fil4(CUT_DB)]));
    assert!(
        unity > -20.0,
        "#992 the tone must come through a fil4 at 0 dB (read {unity:.1} dBFS)"
    );
    assert_cut(unity - cut, "fil4 after the SYN-2 insert switched off");
}

/// The same E/S with no insert: the control.
#[test]
fn a_fil4_cuts_the_owners_guitar_by_18_db() {
    if !fil4_ready("a_fil4_cuts_the_owners_guitar_by_18_db") {
        return;
    }
    let unity = built_level_db(&chain(vec![fil4(0.0)]));
    let cut = built_level_db(&chain(vec![fil4(CUT_DB)]));
    assert_cut(unity - cut, "fil4 on the owner's guitarra-1, no insert");
}

/// A fil4 as the owner's project stores it: only the knobs that were written
/// (here the master gain), no `enable` and no section switches — the way an
/// MCP/tone-builder insert leaves it. The live runtime builds the block from
/// exactly these params.
fn fil4_as_stored(gain_db: f32) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert("gain", ParameterValue::Float(gain_db));
    AudioBlock {
        id: BlockId(FIL4.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: FIL4_EFFECT_TYPE.into(),
            model: FIL4_MODEL.into(),
            params,
        }),
    }
}

/// Measured on the owner's rig (2026-09-29): the live fil4 was built with the
/// 19 numeric params the project holds and nothing else; toggling it or
/// moving its gain changed nothing on the live guitar. A param the project
/// does not hold must play at its declared default, never at zero.
#[test]
fn a_fil4_stored_without_its_switches_still_cuts_the_live_input_by_18_db() {
    if !fil4_ready("a_fil4_stored_without_its_switches_still_cuts_the_live_input_by_18_db") {
        return;
    }
    let unity = Rig::start(&chain(vec![fil4_as_stored(0.0)])).level_db();
    let cut = Rig::start(&chain(vec![fil4_as_stored(CUT_DB)])).level_db();
    assert_cut(unity - cut, "fil4 stored with only its gain");
}

/// A guitar E/S whose Main is a single mono channel (the owner's
/// `guitarra-1-main-l` shape: In 1 → Out 1).
fn mono_main_registry() -> Vec<domain::io_binding::IoBinding> {
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    let ep = |name: &str, channels: &[usize]| IoEndpoint {
        name: name.into(),
        device_id: domain::ids::DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: channels.to_vec(),
    };
    vec![IoBinding {
        id: "io".into(),
        name: "GUITARRA 1 - MAIN L".into(),
        inputs: vec![ep("in", &[0])],
        outputs: vec![ep("main-l", &[0])],
    }]
}

/// The level of the tone through a runtime built from `chain` and `registry`,
/// read off its first route (`channels` wide).
fn built_level_db_with(
    chain: &Chain,
    registry: &[domain::io_binding::IoBinding],
    channels: usize,
) -> f32 {
    let runtime = Arc::new(
        engine::runtime_graph::build_chain_runtime_state(
            chain,
            SR,
            &[engine::runtime_audio_frame::DEFAULT_ELASTIC_TARGET],
            registry,
        )
        .expect("the chain must build"),
    );
    let slot = LiveRuntimeSlot::new(runtime);
    let (mut phase, mut heard) = (0.0_f32, Vec::new());
    let (mut input, mut output, mut scratch) = (
        vec![0.0; BUF],
        vec![0.0; BUF * channels],
        vec![0.0; BUF * channels],
    );
    let mut loaded = Vec::with_capacity(1);
    let step = 2.0 * std::f32::consts::PI * TONE_HZ / SR;
    for n in 0..(SETTLE + MEASURE) {
        for s in input.iter_mut() {
            *s = TONE_PEAK * phase.sin();
            phase = (phase + step) % (2.0 * std::f32::consts::PI);
        }
        crate::slot_processing::process_input_buffer_patient(&slot, 0, &input, 1, 0);
        crate::slot_processing::process_output_buffer(
            &[slot.handle()],
            &mut loaded,
            0,
            &mut output,
            channels,
            &mut scratch,
        );
        if n >= SETTLE {
            heard.extend(output.chunks_exact(channels).map(|frame| frame[0]));
        }
    }
    rms_db(&heard)
}

/// Invariant #5: the stream is stereo inside whatever its outputs are; a mono
/// output is a mixdown at the end. A true-stereo block on a guitar whose Main
/// is one mono channel must act, not be swapped for a bypass (measured on real
/// streams: "does not accept mono input — inserting faulted bypass", drop 0.0 dB).
#[test]
fn a_fil4_on_a_guitar_into_a_mono_main_cuts_it_by_18_db() {
    if !fil4_ready("a_fil4_on_a_guitar_into_a_mono_main_cuts_it_by_18_db") {
        return;
    }
    let registry = mono_main_registry();
    let unity = built_level_db_with(&chain(vec![fil4(0.0)]), &registry, 1);
    let cut = built_level_db_with(&chain(vec![fil4(CUT_DB)]), &registry, 1);
    assert!(
        unity > -20.0,
        "#992 the tone must reach the mono Main (read {unity:.1} dBFS)"
    );
    assert_cut(unity - cut, "fil4 on a guitar into a mono Main");
}
