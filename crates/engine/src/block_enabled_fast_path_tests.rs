//! Issue #522: toggling a single block's `enabled` flag must not rebuild
//! the chain runtime. Today the GUI dispatches `ToggleBlockEnabled` and
//! immediately calls `sync_live_chain_runtime` → `upsert_chain` →
//! `resolve_chain_audio_config` (CPAL hardware queries) →
//! `update_chain_runtime_state`. None of that work is necessary for a
//! per-block boolean flip — the engine already keeps disabled blocks
//! alive (see `runtime_block_builders.rs:60-71`) and supports
//! `FadeState::FadingOut` / `FadingIn` transitions on existing nodes.
//!
//! These tests pin a minimal direct API: `set_block_enabled(runtime,
//! block_id, enabled)` that finds the existing `BlockRuntimeNode` and
//! flips its fade state across every input runtime of the chain, with
//! ZERO chain re-resolve and ZERO processor rebuild.

use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry,
    OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;

use super::{
    build_chain_runtime_state, process_input_f32, process_output_f32, set_block_enabled,
    ChainRuntimeState, FadeState, DEFAULT_ELASTIC_TARGET,
};

const SR: f32 = 48_000.0;
const BLOCK_ID: &str = "test:reverb";

fn neutral_params(effect_type: &str, model: &str) -> ParameterSet {
    let schema =
        schema_for_block_model(effect_type, model).expect("schema must exist for test model");
    ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults must normalize")
}

fn core_block(id: &str, effect_type: &str, model: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.to_string(),
            model: model.to_string(),
            params: neutral_params(effect_type, model),
        }),
    }
}

fn test_chain() -> Chain {
    Chain {
        id: ChainId("issue-522-chain".into()),
        description: Some("issue #522 block-toggle fast path".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            AudioBlock {
                id: BlockId("test:in".into()),
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
            core_block(BLOCK_ID, "reverb", "room"),
            AudioBlock {
                id: BlockId("test:out".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    }
}

fn build_runtime() -> Arc<ChainRuntimeState> {
    let chain = test_chain();
    Arc::new(
        build_chain_runtime_state(&chain, SR, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime must build for test chain"),
    )
}

/// Drive one input + output callback. Issue #580 follow-up:
/// `set_block_enabled` now queues the toggle on a lock-free
/// `ArrayQueue` and the audio thread drains + applies it inside its
/// own `processing` `try_lock` guard. So these tests must run at
/// least one callback after the toggle to observe the resulting
/// `fade_state` mutation / `error_queue` entry.
fn drive_one_callback(runtime: &Arc<ChainRuntimeState>) {
    let input_total_channels = 1_usize; // mono input per `test_chain`
    let output_total_channels = 2_usize; // stereo output per `test_chain`
    let frames = 32_usize;
    let input_buf = vec![0.0_f32; frames * input_total_channels];
    let mut output_buf = vec![0.0_f32; frames * output_total_channels];
    process_input_f32(runtime, 0, &input_buf, input_total_channels);
    process_output_f32(runtime, 0, &mut output_buf, output_total_channels);
}

fn block_fade_state(runtime: &ChainRuntimeState, block: &BlockId) -> Option<FadeState> {
    let processing = runtime.processing.lock().expect("lock processing");
    for input_state in processing.input_states.iter() {
        for node in input_state.blocks.iter() {
            if &node.block_snapshot.id == block {
                return Some(node.fade_state);
            }
        }
    }
    None
}

#[test]
fn set_block_enabled_false_transitions_to_fading_out_without_rebuild() {
    let runtime = build_runtime();
    let block = BlockId(BLOCK_ID.into());

    let before = block_fade_state(&runtime, &block).expect("block exists in runtime");
    assert!(
        matches!(before, FadeState::FadingIn { .. } | FadeState::Active),
        "block must start active or fading in, got {before:?}"
    );

    set_block_enabled(&runtime, &block, false).expect("queueing must succeed");
    drive_one_callback(&runtime); // drain pending_block_toggles

    let after = block_fade_state(&runtime, &block).expect("block stays in runtime after disable");
    assert!(
        matches!(after, FadeState::FadingOut { .. } | FadeState::Bypassed),
        "block must transition to FadingOut/Bypassed, got {after:?}"
    );
}

#[test]
fn set_block_enabled_true_after_disable_transitions_back_to_fading_in() {
    let runtime = build_runtime();
    let block = BlockId(BLOCK_ID.into());

    set_block_enabled(&runtime, &block, false).expect("queue disable");
    set_block_enabled(&runtime, &block, true).expect("queue re-enable");
    drive_one_callback(&runtime); // drain both queued toggles

    let after = block_fade_state(&runtime, &block).expect("block stays in runtime");
    assert!(
        matches!(after, FadeState::FadingIn { .. } | FadeState::Active),
        "block must transition to FadingIn/Active on re-enable, got {after:?}"
    );
}

#[test]
fn set_block_enabled_unknown_block_posts_error_to_error_queue() {
    // Issue #580 follow-up: `set_block_enabled` now returns Ok if the
    // toggle was queued successfully — the per-block lookup (and its
    // "not found" diagnosis) happens on the audio thread, which posts
    // the failure to the runtime's `error_queue` for the GUI to drain
    // via `poll_errors`. Pin the new contract.
    let runtime = build_runtime();
    let missing = BlockId("does:not:exist".into());

    let result = set_block_enabled(&runtime, &missing, false);
    assert!(
        result.is_ok(),
        "queueing must succeed even for an unknown block id, got {result:?}"
    );

    drive_one_callback(&runtime); // audio thread drains + diagnoses

    let errors = runtime.poll_errors();
    let saw_not_found = errors
        .iter()
        .any(|e| e.block_id == missing && e.message.contains("not found"));
    assert!(
        saw_not_found,
        "audio thread must post a 'not found' BlockError for the missing \
         block id, got {errors:?}"
    );
}
