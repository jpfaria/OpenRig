//! #987 — the in-place edit's node paths, on every platform.
//!
//! A live edit walks the chain twice: once ahead of the swap to prebuild every
//! node it needs fresh (`runtime_graph_prebuild`), then under the processing
//! lock to move nodes (`build_runtime_block_nodes_with`). These tests drive
//! `update_chain_runtime_state` on a runtime built like the app builds one and
//! read the nodes it leaves in the pipeline: which processor was kept, which
//! was rebuilt, which one hands over from the node it replaced. The cpal-level
//! click tests cover the same edits end to end but are compiled out where the
//! controller hosts JACK (Linux with `jack`, the CI); these run everywhere.

use std::cell::RefCell;
use std::sync::{Arc, Once};

use block_core::AudioChannelLayout;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, SelectBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::runtime::{build_chain_runtime_state, update_chain_runtime_state};
use crate::runtime_node_handover::WARMED_FADE_IN_FRAMES;
use crate::runtime_state::{ChainRuntimeState, FadeState, RuntimeProcessor};

const SR: f32 = 48_000.0;
const TARGET: usize = 256;
const CHAIN: &str = "issue987-edit";
const LEVEL: &str = "issue987-edit:level";
const EQ: &str = "issue987-edit:eq";
const CHORUS: &str = "issue987-edit:chorus";
const SELECT: &str = "issue987-edit:select";
const OPTION: &str = "issue987-edit:option";
const VST3: &str = "issue987-edit:vst3";

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        block_gain::register_natives();
        block_filter::register_natives();
        block_mod::register_natives();
    });
}

/// One mono guitar input to a stereo or a mono output.
fn registry(output: ChannelMode) -> Vec<IoBinding> {
    let channels = match output {
        ChannelMode::Mono => vec![0],
        _ => vec![0, 1],
    };
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
            mode: output,
            channels,
        }],
    }]
}

fn core(
    id: &str,
    enabled: bool,
    effect_type: &str,
    model: &str,
    params: &[(&str, f32)],
) -> AudioBlock {
    init_registry();
    let schema = schema_for_block_model(effect_type, model).expect("the model's schema must exist");
    let mut values = ParameterSet::default();
    for (path, value) in params {
        values.insert(*path, ParameterValue::Float(*value));
    }
    AudioBlock {
        id: BlockId(id.into()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.into(),
            model: model.into(),
            params: values
                .normalized_against(&schema)
                .expect("the params must normalize"),
        }),
    }
}

fn level(enabled: bool, volume_pct: f32) -> AudioBlock {
    core(LEVEL, enabled, "gain", "volume", &[("volume", volume_pct)])
}

/// A dual-mono EQ that retunes its filters in place.
fn eq(gain_db: f32) -> AudioBlock {
    core(
        EQ,
        true,
        "filter",
        "eq_eight_band_parametric",
        &[("band1_gain", gain_db)],
    )
}

/// A mono-to-stereo block: on a mono bus it turns the bus stereo, on a stereo
/// bus it decorrelates the two channels.
fn chorus(enabled: bool) -> AudioBlock {
    core(CHORUS, enabled, "modulation", "stereo_chorus", &[])
}

/// A VST3 whose bundle is not installed: off it builds as a bypass node.
fn vst3(enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId(VST3.into()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_VST3.into(),
            model: "vst3:Missing:Missing".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn select(selected: &str) -> AudioBlock {
    let mut option = level(true, 50.0);
    option.id = BlockId(OPTION.into());
    AudioBlock {
        id: BlockId(SELECT.into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId(selected.into()),
            options: vec![option],
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
        mix: Default::default(),
    }
}

fn build(first: &Chain, output: ChannelMode) -> Arc<ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(first, SR, &[TARGET], &registry(output))
            .expect("the runtime must build"),
    )
}

fn edit(runtime: &Arc<ChainRuntimeState>, next: &Chain, output: ChannelMode) {
    update_chain_runtime_state(runtime, next, SR, false, &[TARGET], &registry(output))
        .expect("the live edit must apply");
}

/// What a test reads off one node of the pipeline.
#[derive(Debug)]
struct Node {
    serial: u64,
    input_layout: AudioChannelLayout,
    output_layout: AudioChannelLayout,
    content_mono: bool,
    fade: FadeState,
    real_processor: bool,
    hands_over: bool,
}

fn node(runtime: &ChainRuntimeState, id: &str) -> Node {
    let processing = runtime.processing.lock().expect("the lock is free");
    let node = processing.input_states[0]
        .blocks
        .iter()
        .find(|node| node.block_id.0 == id)
        .unwrap_or_else(|| panic!("the pipeline must hold '{id}'"));
    Node {
        serial: node.instance_serial,
        input_layout: node.input_layout,
        output_layout: node.output_layout,
        content_mono: node.content_mono,
        fade: node.fade_state,
        real_processor: matches!(node.processor, RuntimeProcessor::Audio(_)),
        hands_over: node.handover.is_some(),
    }
}

/// A block switched off keeps its processor frozen; switched back on it is
/// the same processor, warmed up before it fades back in.
#[test]
fn turning_a_block_back_on_reuses_its_processor_warmed_before_the_fade_in() {
    let on = chain(vec![level(true, 30.0)]);
    let off = chain(vec![level(false, 30.0)]);
    let runtime = build(&on, ChannelMode::Stereo);
    let before = node(&runtime, LEVEL);

    edit(&runtime, &off, ChannelMode::Stereo);
    edit(&runtime, &on, ChannelMode::Stereo);

    let after = node(&runtime, LEVEL);
    assert_eq!(
        after.serial, before.serial,
        "the processor must be reused, not rebuilt"
    );
    assert_eq!(
        after.fade,
        FadeState::FadingIn {
            frames_remaining: WARMED_FADE_IN_FRAMES
        },
        "a block back on warms up before its fade-in"
    );
}

/// A parameter the processor can take in place keeps the processor and its
/// filter state.
#[test]
fn a_param_change_retunes_the_live_processor_in_place() {
    let runtime = build(&chain(vec![eq(0.0)]), ChannelMode::Stereo);
    let before = node(&runtime, EQ);

    edit(&runtime, &chain(vec![eq(6.0)]), ChannelMode::Stereo);

    let after = node(&runtime, EQ);
    assert_eq!(
        after.serial, before.serial,
        "the EQ must retune, not rebuild"
    );
    assert!(
        !after.hands_over,
        "a retuned processor has nothing to hand over from"
    );
}

/// On a mono bus a mono-to-stereo block turned on makes the bus stereo from
/// there on: the block after it is rebuilt for the stereo bus and hands over
/// from the dry input, while the block that changes the bus keeps its own
/// fade-in (the dry input cannot stand in for it).
#[test]
fn a_stereo_block_turned_on_rebuilds_the_blocks_after_it_for_the_new_bus() {
    let off = chain(vec![chorus(false), level(true, 30.0)]);
    let on = chain(vec![chorus(true), level(true, 30.0)]);
    let runtime = build(&off, ChannelMode::Mono);
    let level_before = node(&runtime, LEVEL);
    assert_eq!(level_before.input_layout, AudioChannelLayout::Mono);

    edit(&runtime, &on, ChannelMode::Mono);

    let chorus_now = node(&runtime, CHORUS);
    assert!(
        chorus_now.real_processor,
        "the chorus turned on has a processor"
    );
    assert_eq!(chorus_now.output_layout, AudioChannelLayout::Stereo);
    assert!(
        !chorus_now.hands_over,
        "a block that changes the bus keeps the fade it was built with"
    );
    let level_now = node(&runtime, LEVEL);
    assert_ne!(
        level_now.serial, level_before.serial,
        "the level is rebuilt for the new bus"
    );
    assert_eq!(level_now.input_layout, AudioChannelLayout::Stereo);
    assert!(
        level_now.hands_over,
        "the rebuilt level takes over from the dry input"
    );
}

/// On a stereo bus fed a mono guitar both channels carry the same signal
/// until a stereo block decorrelates them; the block after it is then rebuilt
/// as two independent channels.
#[test]
fn a_stereo_block_turned_on_rebuilds_the_blocks_after_it_for_decorrelated_content() {
    let off = chain(vec![chorus(false), level(true, 30.0)]);
    let on = chain(vec![chorus(true), level(true, 30.0)]);
    let runtime = build(&off, ChannelMode::Stereo);
    let before = node(&runtime, LEVEL);
    assert!(
        before.content_mono,
        "a mono guitar on a stereo bus is mono content"
    );

    edit(&runtime, &on, ChannelMode::Stereo);

    let after = node(&runtime, LEVEL);
    assert_eq!(
        after.input_layout, before.input_layout,
        "the bus stays stereo"
    );
    assert!(!after.content_mono, "the chorus decorrelated the channels");
    assert_ne!(
        after.serial, before.serial,
        "mono and dual-mono are different processors"
    );
}

/// A block that keeps its id but becomes a different kind of block (a
/// `Select` replaced by the single block it held) has no processor to retune:
/// it is built fresh, ahead of the swap.
#[test]
fn a_block_that_changes_kind_under_the_same_id_is_rebuilt() {
    let mut single = level(true, 50.0);
    single.id = BlockId(SELECT.into());
    let runtime = build(&chain(vec![select(OPTION)]), ChannelMode::Stereo);
    let before = node(&runtime, SELECT);
    assert!(
        !before.real_processor,
        "the select runs its options, not a processor of its own"
    );

    edit(&runtime, &chain(vec![single]), ChannelMode::Stereo);

    let after = node(&runtime, SELECT);
    assert!(
        after.real_processor,
        "the block is now a level with its own processor"
    );
    assert_ne!(
        after.serial, before.serial,
        "a new kind of block is a new node"
    );
}

/// A `Select` keeps the quiesced path, and it can fail to build (an option
/// the preset no longer has). The edit must report that, not claim it applied.
#[test]
fn a_live_edit_whose_select_cannot_build_reports_the_failure() {
    let runtime = build(
        &chain(vec![level(true, 30.0), select(OPTION)]),
        ChannelMode::Stereo,
    );
    let next = chain(vec![level(true, 30.0), select("issue987-edit:gone")]);

    let result = update_chain_runtime_state(
        &runtime,
        &next,
        SR,
        false,
        &[TARGET],
        &registry(ChannelMode::Stereo),
    );

    let error = result.expect_err("a select without its selected option cannot build");
    assert!(
        error.to_string().contains("unknown option"),
        "the failure must name its cause, got: {error}"
    );
    assert!(
        runtime.processing.try_lock().is_ok(),
        "a failed edit must not leave the processing lock held"
    );
}

fn serials(runtime: &ChainRuntimeState) -> Vec<u64> {
    let processing = runtime.processing.lock().expect("the lock is free");
    processing.input_states[0]
        .blocks
        .iter()
        .map(|node| node.instance_serial)
        .collect()
}

/// #998: a live edit that fails to build leaves the pipeline playing exactly
/// the nodes it had, not an empty chain that plays the dry input.
#[test]
fn a_live_edit_that_fails_to_build_keeps_the_nodes_it_had() {
    let runtime = build(
        &chain(vec![level(true, 30.0), select(OPTION)]),
        ChannelMode::Stereo,
    );
    let before = serials(&runtime);
    let next = chain(vec![level(true, 30.0), select("issue987-edit:gone")]);

    let result = update_chain_runtime_state(
        &runtime,
        &next,
        SR,
        false,
        &[TARGET],
        &registry(ChannelMode::Stereo),
    );

    assert!(result.is_err(), "the edit cannot build");
    assert_eq!(
        serials(&runtime),
        before,
        "a failed edit must leave the nodes it found in place"
    );
}

/// A live edit swaps its nodes under the processing lock (#987) whenever every
/// fresh node could be built ahead; the swap must never log there — a log
/// takes a lock and does I/O while the audio thread cannot run (invariant #8).
/// An edit that needs a fresh VST3 or holds a `Select` keeps the quiesced
/// path, which logs outside the lock.
#[test]
fn a_live_edit_never_logs_while_it_holds_the_processing_lock() {
    // Prebuilt: the chorus and the level after it are built ahead.
    let ready_off = chain(vec![eq(0.0), chorus(false), level(true, 30.0)]);
    let ready_on = chain(vec![eq(3.0), chorus(true), level(true, 30.0)]);
    let ready = build(&ready_off, ChannelMode::Mono);
    let logged = record_logs(&ready, || edit(&ready, &ready_on, ChannelMode::Mono));
    assert_eq!(
        logged.under_lock,
        Vec::<String>::new(),
        "the prebuilt swap logged under the lock"
    );

    // Quiesced by a VST3 that has to be built fresh.
    let vst3_off = chain(vec![level(true, 30.0), vst3(false)]);
    let vst3_on = chain(vec![level(true, 30.0), vst3(true)]);
    let quiesced = build(&vst3_off, ChannelMode::Stereo);
    let logged = record_logs(&quiesced, || edit(&quiesced, &vst3_on, ChannelMode::Stereo));
    assert_eq!(
        logged.under_lock,
        Vec::<String>::new(),
        "the VST3 edit logged under the lock"
    );
    assert!(
        logged.free.iter().any(|m| m.contains("rebuild block")),
        "a fresh VST3 is built on the quiesced path, outside the lock: {:?}",
        logged.free
    );

    // Quiesced by a Select: every other path of the swap runs, logging.
    let select_off = chain(vec![
        eq(0.0),
        chorus(false),
        level(true, 30.0),
        select(OPTION),
    ]);
    let select_on = chain(vec![
        eq(3.0),
        chorus(true),
        level(true, 30.0),
        select(OPTION),
    ]);
    let quiesced = build(&select_off, ChannelMode::Mono);
    let logged = record_logs(&quiesced, || edit(&quiesced, &select_on, ChannelMode::Mono));
    assert_eq!(
        logged.under_lock,
        Vec::<String>::new(),
        "the Select edit logged under the lock"
    );
    for expected in [
        "reuse block",
        "in-place param update",
        "rebuild block",
        "layout changed",
    ] {
        assert!(
            logged.free.iter().any(|m| m.contains(expected)),
            "the quiesced edit must log '{expected}' outside the lock: {:?}",
            logged.free
        );
    }
}

/// Records sorted by whether the watched runtime's processing lock was held
/// when they were logged.
#[derive(Default)]
struct Logged {
    under_lock: Vec<String>,
    free: Vec<String>,
}

thread_local! {
    static WATCHED: RefCell<Option<(Arc<ChainRuntimeState>, Logged)>> = const { RefCell::new(None) };
}

struct LockAwareLogger;

impl log::Log for LockAwareLogger {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        WATCHED.with(|watched| {
            if let Some((runtime, logged)) = watched.borrow_mut().as_mut() {
                let message = record.args().to_string();
                if runtime.processing.try_lock().is_err() {
                    logged.under_lock.push(message);
                } else {
                    logged.free.push(message);
                }
            }
        });
    }

    fn flush(&self) {}
}

/// Run `edit` on this thread with every log record sorted against `runtime`'s
/// processing lock. Other threads' records are not watched.
fn record_logs(runtime: &Arc<ChainRuntimeState>, edit: impl FnOnce()) -> Logged {
    static LOGGER: LockAwareLogger = LockAwareLogger;
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        log::set_logger(&LOGGER).expect("no other logger is installed in the engine tests");
        log::set_max_level(log::LevelFilter::Trace);
    });
    WATCHED.with(|watched| *watched.borrow_mut() = Some((Arc::clone(runtime), Logged::default())));
    edit();
    WATCHED.with(|watched| {
        watched
            .borrow_mut()
            .take()
            .map(|(_, logged)| logged)
            .unwrap_or_default()
    })
}
