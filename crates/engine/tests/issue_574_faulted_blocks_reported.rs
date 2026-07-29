//! Regression test for issue #574: blocks whose runtime build fails are
//! silently replaced with pass-through bypass nodes, hiding the failure
//! from offline-render callers and producing misleading WAV output
//! (different presets, identical bytes).
//!
//! Contract under test (new in #574 fix):
//!   `engine::offline::render_chain` returns a `RenderOutcome` whose
//!   `faulted_blocks` field lists every block that could not be built.
//!   The render still produces samples (best-effort behavior preserved
//!   for the GUI) but the caller can no longer claim success when a
//!   block has been silently bypassed.

use block_core::param::ParameterSet;
use domain::ids::{BlockId, ChainId};
use engine::offline::render_chain;
use project::block::{AudioBlock, AudioBlockKind, NamBlock};
use project::chain::Chain;

fn chain_with_blocks(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("issue-574 regression".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

fn nam_block(block_id: &str, model: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(block_id.into()),
        enabled: true,
        kind: AudioBlockKind::Nam(NamBlock {
            model: model.into(),
            params: ParameterSet::default(),
        }),
    }
}

#[test]
fn nam_block_with_unbuildable_model_is_reported_via_faulted_blocks() {
    // A NAM model id that is guaranteed to fail registry lookup (no plugin
    // registered under this name; missing `model_path` param either way).
    // Before the fix, render_chain would silently replace this block with
    // a faulted bypass node and return Ok(samples) with no signal back to
    // the caller — see crates/engine/src/runtime_block_builders.rs:120-135.
    let chain = chain_with_blocks(
        "issue-574-faulted-nam",
        vec![nam_block(
            "unbuildable-amp",
            "nonexistent_nam_model_that_was_never_registered",
        )],
    );

    let input = vec![[0.3_f32, 0.3_f32]; 1024];
    let outcome = render_chain(&chain, 48_000.0, &input, 256, 0)
        .expect("render_chain must still produce best-effort output (samples Ok)");

    assert!(
        !outcome.faulted_blocks.is_empty(),
        "BUG #574: a NAM block whose runtime build fails must be reported \
         via RenderOutcome::faulted_blocks; instead got an empty list which \
         means the failure was silently hidden from the caller"
    );

    let faulted = outcome
        .faulted_blocks
        .iter()
        .find(|f| f.block_id == "unbuildable-amp")
        .expect("the failing block must be identified by its block id");

    assert!(
        !faulted.error.is_empty(),
        "faulted block must carry a non-empty error explaining why the \
         build failed; otherwise the caller cannot help the user diagnose \
         their preset"
    );
}

#[test]
fn chain_with_only_healthy_blocks_reports_no_faulted_blocks() {
    // Sanity baseline: an empty chain (or one whose blocks all build) must
    // not produce phantom faulted_blocks entries — the field is reserved
    // for genuine build failures.
    let chain = chain_with_blocks("issue-574-healthy-empty", vec![]);

    let input = vec![[0.3_f32, 0.3_f32]; 1024];
    let outcome = render_chain(&chain, 48_000.0, &input, 256, 0)
        .expect("empty chain must render successfully");

    assert!(
        outcome.faulted_blocks.is_empty(),
        "a chain with no failing blocks must report no faulted_blocks; \
         got {:?}",
        outcome.faulted_blocks
    );
    assert_eq!(
        outcome.samples.len(),
        input.len(),
        "the rendered output length must match input length when there is \
         no tail"
    );
}
