//! Issue #606 — a NAM-backed "gain" model (a gain-pedal capture, manifest
//! `type: gain_pedal`, `backend: nam`) must build via the NAM processor,
//! NOT the native block-gain registry.
//!
//! User reproduction (live log):
//!   [ERROR adapter_gui::helpers] block 'rig:input-1:block:…':
//!       unsupported gain model 'nam_lovepedal_eternity_burst'
//!
//! Root cause hypothesis: the catalog buckets NAM gain pedals under the
//! "gain" block family (docs/blocks-catalog.md: Gain = 204 models incl.
//! the NAM OD808/TS808/Klon/RAT… captures), so a project slot holding one
//! is a `Core { effect_type: "gain", model: "nam_…" }`. The runtime build
//! dispatch (`crates/engine/src/runtime_block_builders.rs`, shared by
//! `render_chain` and the live engine that `adapter_gui::helpers` drives)
//! routes every "gain" Core block to block-gain's NATIVE registry, which
//! only knows native gain models (e.g. `ts9`). The NAM-backed model is
//! rejected ("unsupported gain model") and the block is silently bypassed
//! (faulted) — the cab/IR path already delegates disk packages correctly,
//! gain does not.
//!
//! Contract: a Core block whose model is a NAM-backed disk package builds
//! successfully (routes to the NAM processor) regardless of the categorical
//! `effect_type` the catalog filed it under.
//!
//! Uses `nam_maxon_od808` from OpenRig-plugins (manifest `type: gain_pedal`,
//! `backend: nam`) — structurally identical to the user's
//! `nam_lovepedal_eternity_burst`. If the plugin tree is absent the test
//! fails loudly, like the sibling #537 disk-package repro.

use std::path::PathBuf;
use std::sync::Once;

use domain::ids::{BlockId, ChainId};
use engine::offline::render_chain;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

/// A NAM gain-pedal capture filed under the "gain" family in the catalog.
const NAM_GAIN_MODEL: &str = "nam_maxon_od808";

fn init_plugins() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        nam::register_builder();
        ir::register_builder();
        let candidates = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../../../OpenRig-plugins/plugins/source"),
            PathBuf::from(
                "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source",
            ),
        ];
        let roots: Vec<PathBuf> = candidates.into_iter().filter(|p| p.is_dir()).collect();
        assert!(
            !roots.is_empty(),
            "issue #606 repro requires OpenRig-plugins/plugins/source on disk — \
             none of the candidate roots resolved"
        );
        plugin_loader::registry::init_many(&roots);
    });
}

fn gain_core_block(block_id: &str, model: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(block_id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: model.into(),
            params: ParameterSet::default(),
        }),
    }
}

fn chain_with(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("issue-606 nam gain build".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks,
    }
}

#[test]
fn nam_backed_gain_model_builds_and_is_not_faulted() {
    init_plugins();

    // Sanity: the NAM gain-pedal package is discoverable in the registry.
    assert!(
        plugin_loader::registry::find(NAM_GAIN_MODEL).is_some(),
        "fixture package `{NAM_GAIN_MODEL}` must be discoverable in OpenRig-plugins"
    );

    let chain = chain_with("issue-606", vec![gain_core_block("od808", NAM_GAIN_MODEL)]);

    let input = vec![[0.3_f32, 0.3_f32]; 1024];
    let outcome = render_chain(&chain, 48_000.0, &input, 256, 0)
        .expect("render_chain must still produce best-effort output");

    let faulted: Vec<&str> = outcome
        .faulted_blocks
        .iter()
        .filter(|f| f.block_id == "od808")
        .map(|f| f.error.as_str())
        .collect();

    assert!(
        faulted.is_empty(),
        "BUG #606: NAM-backed gain model `{NAM_GAIN_MODEL}` was faulted instead of \
         building via the NAM processor — the build dispatch routed a NAM model to the \
         native block-gain registry. Errors: {faulted:?}"
    );
}

/// The user's actual symptom: a project references a NAM gain-pedal model
/// (`nam_lovepedal_eternity_burst`) that is NOT in the configured catalog
/// (it lives only as a `dist/` zip, never extracted into `plugins/source`).
/// At build time the model is not found, so the dispatch falls back to the
/// NATIVE block-gain registry keyed on `effect_type = "gain"` and reports
///   unsupported gain model 'nam_lovepedal_eternity_burst'
/// then silently bypasses the block.
///
/// Contract: the `nam_` prefix is a reserved disk-package namespace
/// (`nam_*`/`ir_*`/`lv2_*` are canonical catalog ids). A `nam_`-prefixed
/// model is NAM-backed and must route to the NAM loader — even when absent
/// from the catalog it must fault as a NAM/package problem, NEVER be
/// misrouted to the native block-gain registry with the misleading
/// "unsupported gain model" message.
#[test]
fn uncataloged_nam_model_is_not_misrouted_to_native_gain_registry() {
    init_plugins();

    // A `nam_`-namespaced id that no plugin pack will ever ship — stands in
    // for the user's `nam_lovepedal_eternity_burst` at the moment it was not
    // present in their configured `plugins/source` catalog.
    let model = "nam_uninstalled_pedal_for_issue_606";
    assert!(
        plugin_loader::registry::find(model).is_none(),
        "precondition: `{model}` must be ABSENT from the catalog for this repro"
    );

    let chain = chain_with("issue-606-uncataloged", vec![gain_core_block("od", model)]);
    let input = vec![[0.3_f32, 0.3_f32]; 1024];
    let outcome = render_chain(&chain, 48_000.0, &input, 256, 0)
        .expect("render_chain must still produce best-effort output");

    // The block legitimately faults (its .nam file isn't installed) — that
    // is fine. What must NOT happen is misrouting to the native gain
    // registry with the "unsupported gain model" message.
    let errs: Vec<&str> = outcome
        .faulted_blocks
        .iter()
        .filter(|f| f.block_id == "od")
        .map(|f| f.error.as_str())
        .collect();

    assert!(
        !errs
            .iter()
            .any(|e| e.to_lowercase().contains("unsupported gain model")),
        "BUG #606: a NAM-backed (`nam_…`) model absent from the catalog was misrouted to \
         the native block-gain registry, producing the misleading 'unsupported gain model' \
         error and silently bypassing the block. A `nam_`-prefixed model must route to the \
         NAM loader. Faults: {errs:?}"
    );
}
