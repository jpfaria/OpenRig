//! Issue #85 — an LV2 compressor with a SIDECHAIN input is refused and silently
//! bypassed: "LV2 plugin `lv2_zamcomp` has unsupported audio shape: 2 in / 1 out
//! — inserting faulted bypass". The owner's GUITARRA - TONES chain carries one,
//! so his compressor has not been running at all.
//!
//! 2 in / 1 out IS a supported shape: the second audio input is the sidechain
//! (ZaMcomp, ZamGate, every sidechain-capable plugin exposes it) and the single
//! output is mono. With no external sidechain routing in the model, the
//! detector reads the same signal the main input carries — an internal
//! sidechain, which is what the plugin does with `sidech` off anyway.
//!
//! Needs the real plugin tree, so it is gated on `OPENRIG_TEST_LV2_DIR`
//! (the `plugins/source` dir) and skips loudly without it.

use block_core::param::ParameterSet;
use domain::ids::{BlockId, ChainId};
use engine::offline::render_chain;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;

const MODEL: &str = "lv2_zamcomp";

fn plugins_root() -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("OPENRIG_TEST_LV2_DIR")?;
    Some(std::path::PathBuf::from(dir))
}

/// Squashing settings, so a working compressor cannot be mistaken for a
/// bypass: everything above -40 dB gets pulled down 20:1.
fn squashing_params() -> ParameterSet {
    let mut ps = ParameterSet::default();
    ps.insert("thr", domain::value_objects::ParameterValue::Float(-40.0));
    ps.insert("rat", domain::value_objects::ParameterValue::Float(20.0));
    ps.insert("att", domain::value_objects::ParameterValue::Float(1.0));
    ps.insert("rel", domain::value_objects::ParameterValue::Float(50.0));
    ps
}

fn chain_with_zamcomp() -> Chain {
    Chain {
        id: ChainId("issue-85-sidechain".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![AudioBlock {
            id: BlockId("zamcomp".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "dynamics".into(),
                model: MODEL.into(),
                params: squashing_params(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }
}

#[test]
fn a_sidechain_compressor_builds_instead_of_faulting() {
    let Some(root) = plugins_root() else {
        eprintln!(
            "[#85] SKIPPED — set OPENRIG_TEST_LV2_DIR=<plugins/source> to run the \
             sidechain-LV2 regression against the real plugin."
        );
        return;
    };
    lv2::register_builder();
    plugin_loader::registry::init(&root);

    let input = vec![[0.3_f32, 0.3_f32]; 4096];
    let outcome = render_chain(&chain_with_zamcomp(), 48_000.0, &input, 256, 0)
        .expect("render_chain must produce output");

    let faulted: Vec<String> = outcome
        .faulted_blocks
        .iter()
        .map(|f| format!("{}: {}", f.block_id, f.error))
        .collect();
    assert!(
        faulted.is_empty(),
        "#85: a sidechain LV2 (2 audio in / 1 out) must build — its second input \
         is the sidechain, not an unsupported shape. Got: {faulted:?}"
    );

    // …and it must actually PROCESS: a build that quietly passes the signal
    // through is the same silence-in-disguise the faulted bypass was.
    let mut bypassed = chain_with_zamcomp();
    bypassed.blocks[0].enabled = false;
    let dry = render_chain(&bypassed, 48_000.0, &input, 256, 0).expect("dry render");
    let moved = outcome
        .samples
        .iter()
        .zip(dry.samples.iter())
        .any(|(a, b)| (a[0] - b[0]).abs() > 1e-4);
    assert!(
        moved,
        "#85: the sidechain compressor built but changed nothing — a 20:1 ratio \
         over -40 dB on a -10 dBFS signal has to move the level"
    );
}
