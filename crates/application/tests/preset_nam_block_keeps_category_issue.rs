//! Bug repro (issue #593): a preset saved off chain "GUITARRA - DEFAULT"
//! shows GAIN / AMP / PREAMP blocks, but loading the SAME preset onto
//! another chain ("teyun") renders those blocks as generic purple NAM
//! blocks. The blocks' category (effect_type) is lost on load.
//!
//! Root cause: `infra_yaml`'s `into_audio_block` had a blanket
//! `if model.starts_with("nam_") => AudioBlockKind::Nam` branch (issue
//! #552). A NAM-backed stompbox is persisted under its NATURAL block type
//! (`type: gain`), and the live chain keeps it as `Core { effect_type:
//! "gain" }`. Forcing it to `Nam` on load drops the category, so the GUI
//! shows it as a generic NAM and the signal-chain role changes.
//!
//! This test registers a NAM-prefixed model under the `gain` block type,
//! writes a preset that references it as `type: gain`, loads the preset
//! file through the real parser, and asserts the block comes back as
//! `Core { effect_type: "gain" }` — NOT `Nam`.

use project::block::AudioBlockKind;

use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor};
use plugin_loader::{BlockType, NativeRuntime};

fn dummy_schema() -> anyhow::Result<ModelParameterSchema> {
    anyhow::bail!("test runtime: schema never invoked during parse")
}
fn dummy_validate(_: &ParameterSet) -> anyhow::Result<()> {
    Ok(())
}
fn dummy_build(_: &ParameterSet, _: f32, _: AudioChannelLayout) -> anyhow::Result<BlockProcessor> {
    anyhow::bail!("test runtime: build never invoked during parse")
}

#[test]
fn loading_preset_keeps_nam_gain_block_as_core_not_generic_nam() {
    // Register a NAM-prefixed model as a gain pedal so the disk-package
    // schema fallback resolves it (mirrors a real installed NAM stompbox).
    plugin_loader::registry::register_native_simple(
        "nam_fake_test_od",
        "Fake Test OD",
        None,
        BlockType::GainPedal,
        NativeRuntime {
            schema: dummy_schema,
            validate: dummy_validate,
            build: dummy_build,
        },
    );
    // Publish the registered native into the queried catalog (the live
    // app does this at boot, after every `block-*` crate registers).
    plugin_loader::registry::reload(&[]);
    assert!(
        plugin_loader::registry::find("nam_fake_test_od").is_some(),
        "precondition: the fake NAM model must be in the catalog"
    );

    let tmp = tempfile::tempdir().expect("tempdir");
    let preset_path = tmp.path().join("od.yaml");
    std::fs::write(
        &preset_path,
        "version: 1\nid: OD\nname: GUITARRA - DEFAULT\nblocks:\n  - type: gain\n    enabled: false\n    model: nam_fake_test_od\n",
    )
    .expect("write preset");

    let preset = infra_yaml::load_chain_preset_file(&preset_path).expect("preset loads");
    assert_eq!(preset.blocks.len(), 1, "the gain block must survive the load");

    match &preset.blocks[0].kind {
        AudioBlockKind::Core(core) => assert_eq!(
            core.effect_type, "gain",
            "a NAM-backed `type: gain` block must keep its gain category"
        ),
        AudioBlockKind::Nam(_) => panic!(
            "REGRESSION: NAM-backed `type: gain` block was coerced to a generic \
             Nam block, losing its gain category (the screenshot bug)"
        ),
        other => panic!("expected Core gain block, got {other:?}"),
    }
}
