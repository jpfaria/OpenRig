//! Issue #776 — validation battery for a *catalog* VST3 in a rendered chain
//! (the user-facing level: "the block does nothing to the sound"). Mirrors the
//! #251 in-chain test but for a plugin discovered from the OpenRig plugins
//! folder. Env-gated on `OPENRIG_TEST_VST3_DIR`; skips cleanly when unset. Run
//! locally with `--test-threads=1` (ChowCentaur refuses concurrent instantiation).
//! Uses default block params — bypass defaults OFF, so the overdrive processes.

use block_core::param::ParameterSet;
use domain::ids::{BlockId, ChainId};
use engine::offline::render_chain;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use std::path::PathBuf;

const SR: f32 = 48_000.0;

fn chow_model_id() -> Option<&'static str> {
    let dir = std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from)?;
    vst3_host::init_vst3_catalog(SR as f64, &[dir]);
    vst3_host::vst3_catalog()
        .iter()
        .find(|e| {
            e.info
                .bundle_path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("ChowCentaur.vst3"))
                .unwrap_or(false)
        })
        .map(|e| e.model_id)
}

fn chain_with(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("issue-776 catalog vst3".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}

fn sine_input(frames: usize) -> Vec<[f32; 2]> {
    (0..frames)
        .map(|n| {
            let s = 0.3 * (2.0 * std::f32::consts::PI * 220.0 * (n as f32) / SR).sin();
            [s, s]
        })
        .collect()
}

fn vst3_block(model: &str, enabled: bool, params: ParameterSet) -> AudioBlock {
    AudioBlock {
        id: BlockId(format!("chow-{}", if enabled { "on" } else { "off" })),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_VST3.into(),
            model: model.into(),
            params,
        }),
    }
}

// ── Control ───────────────────────────────────────────────────────────────────

#[test]
fn t14_passthrough_chain_renders_the_input() {
    if chow_model_id().is_none() {
        return;
    }
    let input = sine_input(4096);
    let out = render_chain(&chain_with("pass", vec![]), SR, &input, 256, 0).expect("renders");
    assert_eq!(
        out.samples.len(),
        input.len(),
        "passthrough preserves length"
    );
}

// ── Build ─────────────────────────────────────────────────────────────────────

#[test]
fn t15_catalog_vst3_block_builds_without_faulting() {
    let Some(model) = chow_model_id() else { return };
    let input = sine_input(4096);
    let block = vst3_block(model, true, ParameterSet::default());
    let out = render_chain(&chain_with("chow", vec![block]), SR, &input, 256, 0).expect("renders");
    assert!(
        out.faulted_blocks.is_empty(),
        "catalog VST3 block faulted instead of building: {:?}",
        out.faulted_blocks
    );
}

#[test]
fn t16_two_catalog_vst3_blocks_build() {
    let Some(model) = chow_model_id() else { return };
    let input = sine_input(4096);
    let a = vst3_block(model, true, ParameterSet::default());
    let mut b = vst3_block(model, true, ParameterSet::default());
    b.id = BlockId("chow-2".into());
    let out = render_chain(&chain_with("two", vec![a, b]), SR, &input, 256, 0).expect("renders");
    assert!(
        out.faulted_blocks.is_empty(),
        "two catalog VST3 blocks must both build: {:?}",
        out.faulted_blocks
    );
}

// ── Processing (the user bug) ─────────────────────────────────────────────────

#[test]
fn t17_catalog_vst3_alters_the_rendered_output() {
    let Some(model) = chow_model_id() else { return };
    let input = sine_input(8192);
    let dry = render_chain(&chain_with("pass", vec![]), SR, &input, 256, 0).expect("dry");
    let block = vst3_block(model, true, ParameterSet::default());
    let wet = render_chain(&chain_with("chow", vec![block]), SR, &input, 256, 0).expect("wet");
    assert!(
        wet.faulted_blocks.is_empty(),
        "block faulted: {:?}",
        wet.faulted_blocks
    );
    assert_ne!(
        dry.samples, wet.samples,
        "BUG: catalog VST3 output is identical to passthrough — it never processed"
    );
}

#[test]
fn t18_catalog_vst3_rendered_output_is_not_silent() {
    let Some(model) = chow_model_id() else { return };
    let input = sine_input(8192);
    let block = vst3_block(model, true, ParameterSet::default());
    let wet = render_chain(&chain_with("chow", vec![block]), SR, &input, 256, 0).expect("wet");
    let energy: f32 = wet.samples.iter().map(|s| s[0] * s[0]).sum();
    assert!(energy > 1e-3, "rendered output must not be silent");
}

#[test]
fn t19_disabling_the_block_equals_passthrough() {
    let Some(model) = chow_model_id() else { return };
    let input = sine_input(4096);
    let dry = render_chain(&chain_with("pass", vec![]), SR, &input, 256, 0).expect("dry");
    let off = vst3_block(model, false, ParameterSet::default());
    let out = render_chain(&chain_with("off", vec![off]), SR, &input, 256, 0).expect("renders");
    assert_eq!(
        dry.samples, out.samples,
        "a disabled VST3 block must be a true bypass (== passthrough)"
    );
}

// ── The DI scenario: a 2nd instance while the 1st is alive ────────────────────

#[test]
fn t21_second_instance_processes_while_the_first_is_alive() {
    // Arming a DI loop (#771) pre-renders through a FRESH copy of the chain
    // graph — a 2nd ChowCentaur instance while the guitar chain's 1st is live.
    // The 2nd must still process, or the DI plays dry ("no effect on the DI").
    let Some(model) = chow_model_id() else { return };
    let entry = vst3_host::find_vst3_plugin(model).expect("entry");
    let uid = vst3_host::resolve_uid_for_model(model).expect("uid");
    // Instance #1: the "guitar" instance — kept ALIVE for the whole test.
    let _alive =
        vst3_host::Vst3Plugin::load(&entry.info.bundle_path, &uid, SR as f64, 2, 512, &[]).unwrap();

    let input = sine_input(8192);
    let dry = render_chain(&chain_with("pass", vec![]), SR, &input, 256, 0).expect("dry");
    // Instance #2: the "DI" copy, built while #1 is alive.
    let block = vst3_block(model, true, ParameterSet::default());
    let wet = render_chain(&chain_with("di", vec![block]), SR, &input, 256, 0).expect("wet");
    assert!(
        wet.faulted_blocks.is_empty(),
        "2nd VST3 instance faulted while the 1st was alive: {:?}",
        wet.faulted_blocks
    );
    assert_ne!(
        dry.samples, wet.samples,
        "BUG: a 2nd VST3 instance (the DI's copy) plays DRY while the 1st is alive"
    );
}

// ── Catalog surface ───────────────────────────────────────────────────────────

#[test]
fn t20_catalog_vst3_surfaces_in_the_vst3_model_list() {
    let Some(model) = chow_model_id() else { return };
    let models = project::catalog::supported_block_models(block_core::EFFECT_TYPE_VST3)
        .expect("vst3 model list");
    assert!(
        models.iter().any(|m| m.model_id == model),
        "the catalog ChowCentaur must appear in the VST3 block model list"
    );
}

#[test]
fn t22_catalog_vst3_appears_exactly_once_not_duplicated_by_the_manifest() {
    // #776 added `BlockType::Vst3` so the `type: vst3` manifest parses. The block
    // itself comes from DISCOVERY (vst3:<stem>:<class>), so the plugin-loader
    // package (id `vst3_chow_centaur`) must NOT also surface — the block must
    // appear exactly once. This loads BOTH the registry (parsing the manifest)
    // and the discovery catalog, like the app.
    if chow_model_id().is_none() {
        return; // discovery init'd inside chow_model_id()
    }
    let vst3_dir = std::env::var_os("OPENRIG_TEST_VST3_DIR")
        .map(PathBuf::from)
        .unwrap();
    // `<plugins_root>/vst3/<id>/manifest.yaml` — the root is the parent of vst3/.
    let plugins_root = vst3_dir.parent().unwrap().to_path_buf();
    plugin_loader::registry::reload(&[plugins_root]);

    let models = project::catalog::supported_block_models(block_core::EFFECT_TYPE_VST3)
        .expect("vst3 model list");
    let chow = models
        .iter()
        .filter(|m| m.display_name == "ChowCentaur")
        .count();
    assert_eq!(
        chow, 1,
        "ChowCentaur must appear exactly once in the vst3 block list, got {chow}"
    );
    assert!(
        !models.iter().any(|m| m.model_id == "vst3_chow_centaur"),
        "the manifest package id must not surface as a vst3 block (discovery owns it)"
    );
}
