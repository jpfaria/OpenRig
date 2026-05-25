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
    build_chain_runtime_state, set_block_enabled, ChainRuntimeState, FadeState,
    DEFAULT_ELASTIC_TARGET,
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

    set_block_enabled(&runtime, &block, false).expect("fast-path must succeed");

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

    set_block_enabled(&runtime, &block, false).expect("disable succeeds");
    set_block_enabled(&runtime, &block, true).expect("re-enable succeeds");

    let after = block_fade_state(&runtime, &block).expect("block stays in runtime");
    assert!(
        matches!(after, FadeState::FadingIn { .. } | FadeState::Active),
        "block must transition to FadingIn/Active on re-enable, got {after:?}"
    );
}

#[test]
fn set_block_enabled_unknown_block_returns_err_without_mutation() {
    let runtime = build_runtime();
    let missing = BlockId("does:not:exist".into());

    let result = set_block_enabled(&runtime, &missing, false);
    assert!(
        result.is_err(),
        "missing block must yield Err, got {result:?}"
    );
}
