//! Issue #574 — root cause: NAM amps don't actually run through
//! `engine::offline::render_chain` because `build_nam_audio_processor`
//! takes the legacy "model_path in stage.params" path instead of
//! routing through `LoadedPackage::build_processor` (which would
//! inject the path from the discovered manifest). Every NAM block
//! therefore fails to build, gets replaced with a pass-through
//! bypass node, and the WAV is byte-identical to a chain that
//! never had the amp.
//!
//! This test asserts the symptom the user reported: a NAM amp that
//! exists in the plugin registry must process audio when its chain
//! is rendered via `engine::offline::render_chain`. The previous fix
//! (commit `bab0e8d1`) made the failure visible — this test will
//! pass only once the actual dispatch is wired through
//! `LoadedPackage::build_processor`.

use std::path::PathBuf;
use std::sync::Once;

use block_core::param::ParameterSet;
use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use engine::offline::render_chain;
use project::block::{AudioBlock, AudioBlockKind, NamBlock};
use project::chain::Chain;

const SR: f32 = 48_000.0;

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

fn chain_with_blocks(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("issue-574 nam dispatch regression".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

fn nam_amp_block(block_id: &str, model: &str, preset: &str) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert("preset", ParameterValue::String(preset.into()));
    AudioBlock {
        id: BlockId(block_id.into()),
        enabled: true,
        kind: AudioBlockKind::Nam(NamBlock {
            model: model.into(),
            params,
        }),
    }
}

/// Steady-state guitar-ish sine that the amp's neural model can actually
/// react to. Pure silence would also "differ from passthrough" if the
/// amp adds DC, which is not a meaningful test of "the amp processes
/// audio".
fn sine_input(frames: usize) -> Vec<[f32; 2]> {
    (0..frames)
        .map(|n| {
            let t = n as f32 / SR;
            let s = 0.2 * (2.0 * std::f32::consts::PI * 220.0 * t).sin();
            [s, s]
        })
        .collect()
}

#[test]
fn nam_amp_from_loaded_package_actually_processes_audio_in_render_chain() {
    init_test_registry();

    // Sanity: the fixture package is in the registry. If this fails the
    // test setup is broken, not the bug under test.
    assert!(
        plugin_loader::registry::find("nam_marshall_plexi").is_some(),
        "fixture plugin nam_marshall_plexi must be discoverable in \
         crates/engine/tests/fixtures/plugins/nam/marshall_plexi/"
    );

    let input = sine_input(1024);

    // Chain A — empty (passthrough). Output equals input.
    let chain_a = chain_with_blocks("issue-574-passthrough", vec![]);
    let outcome_a =
        render_chain(&chain_a, SR, &input, 256, 0).expect("passthrough chain must render");

    // Chain B — same input, but with the NAM amp from the registry.
    // The amp MUST build via the LoadedPackage dispatch path; the chain
    // must contain a real processor, not a faulted bypass.
    let chain_b = chain_with_blocks(
        "issue-574-with-nam-amp",
        vec![nam_amp_block("amp", "nam_marshall_plexi", "angus")],
    );
    let outcome_b = render_chain(&chain_b, SR, &input, 256, 0)
        .expect("NAM-amp chain must render (best-effort) even before the dispatch fix");

    assert!(
        outcome_b.faulted_blocks.is_empty(),
        "BUG #574 root cause: the NAM amp `nam_marshall_plexi` exists in \
         the plugin registry but `engine::offline::render_chain` still \
         reports it as faulted, which means `build_nam_audio_processor` \
         is NOT routing through `LoadedPackage::build_processor`. \
         Faulted: {:?}",
        outcome_b.faulted_blocks
    );

    assert_ne!(
        outcome_a.samples, outcome_b.samples,
        "BUG #574: a NAM amp resolved through the plugin registry MUST \
         contribute audio when rendered via `engine::offline::render_chain`. \
         Same input + with-amp == without-amp means the amp is a \
         pass-through, which is the byte-identical-WAV symptom the user \
         reported."
    );
}
