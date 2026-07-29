//! Issue #588 — memory: editing a LIVE chain must NOT reload the NAM
//! model of a block that is being reused.
//!
//! Root cause under test: `RuntimeGraph::upsert_chain` takes an in-place
//! fast path for an already-running chain (param / volume / toggle edit
//! that keeps the input topology). Before deciding to take that path it
//! calls `build_per_input_runtimes(...)` to read the new group ids for a
//! topology comparison — and that build instantiates EVERY block in the
//! chain, including loading each NAM/IR model from disk via the native
//! library, only to throw the freshly-built runtime away and update the
//! existing one in place.
//!
//! Consequence: every volume drag / knob turn / preset-equivalent edit on
//! a chain that contains heavy NAM amps transiently doubles the model
//! footprint (two full copies of every model live at once) and re-runs
//! `CreateModelFromFile` for models that did not change. Repeated edits
//! churn the native allocator and, if the native layer memoizes by path,
//! grow unbounded — the observed multi-GB RSS.
//!
//! The contract: a chain edit that reuses a block must not reload that
//! block's model. The model creation count must not increase when the
//! block (and its model) is unchanged across the edit.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Once;

use block_core::param::ParameterSet;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::value_objects::ParameterValue;
use project::block::{AudioBlock, AudioBlockKind, NamBlock};
use project::chain::Chain;
use project::project::Project;

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

fn registry() -> Vec<domain::io_binding::IoBinding> {
    vec![domain::io_binding::IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![domain::io_binding::IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

/// Input → NAM amp → Output. A buildable single-input chain whose only
/// heavy resource is the NAM model of `nam_marshall_plexi`.
fn nam_chain(id: &str, volume: f32) -> Chain {
    let mut amp_params = ParameterSet::default();
    amp_params.insert("preset", ParameterValue::String("angus".into()));
    Chain {
        id: ChainId(id.into()),
        description: Some("issue-588 model reload on edit".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume,
        io_binding_ids: vec!["io".into()],
        blocks: vec![AudioBlock {
            id: BlockId("amp".into()),
            enabled: true,
            kind: AudioBlockKind::Nam(NamBlock {
                model: "nam_marshall_plexi".into(),
                params: amp_params,
            }),
        }],
        di_output: None,
        loopers: vec![],
    }
}

#[test]
fn editing_a_live_chain_does_not_reload_the_reused_nam_model() {
    init_test_registry();

    assert!(
        plugin_loader::registry::find("nam_marshall_plexi").is_some(),
        "fixture plugin nam_marshall_plexi must be discoverable in \
         crates/engine/tests/fixtures/plugins/"
    );

    let chain = nam_chain("issue-588", 100.0);
    let mut rates = HashMap::new();
    rates.insert(chain.id.clone(), SR);
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain.clone()],
        midi: None,
    };

    // Build the live graph: the NAM model is loaded exactly once here.
    let mut graph =
        engine::runtime_graph::build_runtime_graph(&project, &rates, &HashMap::new(), &registry())
            .expect("graph with a NAM amp must build");

    // Baseline after build. The absolute value depends on the stereo
    // channel fan-out of a mono amp and is not the subject of this test;
    // what matters is that an EDIT does not grow it.
    let created_after_build = nam::models_created();
    let live_after_build = nam::live_models();
    assert!(
        live_after_build >= 1,
        "the chain's NAM amp must be resident after building the chain"
    );

    // User drags the volume slider: 100 → 150. Same call the controller
    // makes for a live chain (topology unchanged → in-place fast path).
    // The amp block is byte-identical across the edit, so its model MUST
    // be reused, never reloaded.
    let edited = nam_chain("issue-588", 150.0);
    graph
        .upsert_chain(&edited, SR, &HashMap::new(), false, &[2048], &registry())
        .expect("in-place volume edit must succeed");

    let created_after_edit = nam::models_created();
    let live_after_edit = nam::live_models();

    assert_eq!(
        created_after_edit - created_after_build,
        0,
        "issue #588: a volume edit on a live chain RELOADED the NAM model \
         (CreateModelFromFile ran {} extra time(s)). `upsert_chain` is \
         building a full throwaway runtime just to compare input topology, \
         reloading every model in the chain. Derive the topology without \
         instantiating processors so reused blocks keep their existing \
         model.",
        created_after_edit - created_after_build
    );

    assert_eq!(
        live_after_edit, live_after_build,
        "issue #588: a chain edit leaked a model — {live_after_build} resident \
         before the edit, {live_after_edit} after. The old runtime's models \
         must be freed once the edit swaps in the new one."
    );
}
