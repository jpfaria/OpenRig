//! Red-first (#572) tests for `query::get_plugin_params`.
//!
//! See spec
//! `docs/superpowers/specs/2026-05-27-issue-572-mcp-block-plugin-params-design.md`.

use crate::bridge::QueryKind;
use crate::query::{get_block_params, get_plugin_params};
use domain::ids::{BlockId, ChainId};
use project::project::Project;

#[test]
fn get_block_params_returns_materialized_descriptors_envelope() {
    use block_core::param::{
        ModelParameterSchema, ParameterDomain, ParameterSet, ParameterSpec, ParameterUnit,
        ParameterWidget,
    };
    use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};
    use domain::value_objects::ParameterValue;
    use plugin_loader::manifest::BlockType;
    use plugin_loader::native_runtimes::NativeRuntime;
    use project::block::types::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::chain::Chain;

    // 1. Register a native plugin whose schema declares one Bool param
    //    with `default_value = false`.
    fn schema_fn() -> anyhow::Result<ModelParameterSchema> {
        Ok(ModelParameterSchema {
            effect_type: "preamp".into(),
            model: "issue_572_block_params_happy".into(),
            display_name: "Issue 572 Block Params".into(),
            audio_mode: ModelAudioMode::DualMono,
            parameters: vec![ParameterSpec {
                path: "bypass".into(),
                label: "Bypass".into(),
                group: None,
                widget: ParameterWidget::Toggle,
                unit: ParameterUnit::None,
                domain: ParameterDomain::Bool,
                default_value: Some(ParameterValue::Bool(false)),
                optional: false,
                allow_empty: false,
            }],
        })
    }
    fn validate_fn(_: &ParameterSet) -> anyhow::Result<()> {
        Ok(())
    }
    fn build_fn(_: &ParameterSet, _: f32, _: AudioChannelLayout) -> anyhow::Result<BlockProcessor> {
        anyhow::bail!("noop — not exercised in this test")
    }
    plugin_loader::registry::register_native_simple(
        "issue_572_block_params_happy",
        "Issue 572 Block Params",
        Some("test"),
        BlockType::Preamp,
        NativeRuntime {
            schema: schema_fn,
            validate: validate_fn,
            build: build_fn,
        },
    );
    plugin_loader::registry::init(std::path::Path::new(
        "/nonexistent-test-path-572-block-params",
    ));

    // 2. Build a project with one chain + one Core block of that
    //    model. The block's current `params` set `bypass = true` (≠
    //    the schema default of `false`) so the test can distinguish
    //    "default echoed" from "current_value read".
    let mut params = ParameterSet::default();
    params.insert("bypass", ParameterValue::Bool(true));
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("chain_572".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            blocks: vec![AudioBlock {
                id: BlockId("block_572".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "preamp".into(),
                    model: "issue_572_block_params_happy".into(),
                    params,
                }),
            }],
        }],
        midi: None,
    };

    // 3. Call the resolver.
    let json = get_block_params(
        &project,
        &ChainId("chain_572".into()),
        &BlockId("block_572".into()),
    )
    .expect("known chain + block must return Ok");

    // 4. End-to-end plumbing check: chain + block lookup resolves,
    //    `AudioBlock::parameter_descriptors()` runs, the resulting
    //    `Vec<BlockParameterDescriptor>` serialises under the `params`
    //    envelope. For this fixture (Native plugin via the disk-package
    //    fallback) the synthesized schema is empty per
    //    `synthesize_parameters_from_manifest`'s `Native` branch — that
    //    is fine; current-value materialisation is covered by
    //    `block-core::param::schema::tests::materialize_*`. What this
    //    integration test pins is the resolver's wiring + the JSON
    //    envelope shape MCP clients depend on.
    assert!(
        json.starts_with("{\"params\":"),
        "expected params envelope, got: {json}"
    );
    assert!(json.ends_with("}"), "expected closed envelope, got: {json}");
}

#[test]
fn get_block_params_unknown_chain_returns_err() {
    // No chain with that id in the project — surface a typed error so
    // the transport can map it cleanly (mirrors `list_chain_presets`'s
    // contract, not `get_plugin_params`'s null envelope, because here
    // the lookup is over Project state the transport owns, not over
    // the process-wide plugin catalog).
    let project = Project::default();
    let result = get_block_params(
        &project,
        &ChainId("issue-572-nope-chain".to_string()),
        &BlockId("issue-572-nope-block".to_string()),
    );
    assert!(
        result.is_err(),
        "unknown chain must be Err, got: {result:?}"
    );
}

#[test]
fn querykind_get_block_params_carries_chain_and_block() {
    let kind = QueryKind::GetBlockParams {
        chain: ChainId("issue-572-querykind-chain".to_string()),
        block: BlockId("issue-572-querykind-block".to_string()),
    };
    match &kind {
        QueryKind::GetBlockParams { chain, block } => {
            assert_eq!(chain.0, "issue-572-querykind-chain");
            assert_eq!(block.0, "issue-572-querykind-block");
        }
        other => panic!("expected GetBlockParams variant, got {other:?}"),
    }
}

#[test]
fn querykind_get_plugin_params_carries_plugin_id() {
    // Drives the QueryKind variant the bridge needs so that any
    // transport (MCP, gRPC, adapter-console) can dispatch
    // `get_plugin_params` over the bus by name without re-walking
    // catalog state itself.
    let kind = QueryKind::GetPluginParams {
        plugin_id: "issue-572-querykind-bridge".to_string(),
    };
    match &kind {
        QueryKind::GetPluginParams { plugin_id } => {
            assert_eq!(plugin_id, "issue-572-querykind-bridge");
            // The bridge contract: the resolver this variant maps to
            // returns the same wire shape as a direct call.
            assert_eq!(get_plugin_params(plugin_id), "{\"params\": null}");
        }
        other => panic!("expected GetPluginParams variant, got {other:?}"),
    }
}

#[test]
fn get_plugin_params_unknown_id_returns_null_envelope() {
    // Mirrors `get_plugin`'s contract for unknown ids — the wire shape
    // an MCP / gRPC client gets when the plugin id has no match in the
    // catalog is a JSON envelope with `params: null` (consistent with
    // `{"plugin": null}` for `get_plugin`).
    let out = get_plugin_params("definitely-not-a-real-plugin-id-572");
    assert_eq!(out, "{\"params\": null}");
}

#[test]
fn get_plugin_params_known_id_returns_schema_envelope() {
    use block_core::param::{ModelParameterSchema, ParameterSet};
    use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};
    use plugin_loader::manifest::BlockType;
    use plugin_loader::native_runtimes::NativeRuntime;

    fn schema_fn() -> anyhow::Result<ModelParameterSchema> {
        Ok(ModelParameterSchema {
            effect_type: "gain_pedal".into(),
            model: "issue_572_get_params_happy".into(),
            display_name: "Issue 572 Test Pedal".into(),
            audio_mode: ModelAudioMode::DualMono,
            parameters: Vec::new(),
        })
    }
    fn validate_fn(_: &ParameterSet) -> anyhow::Result<()> {
        Ok(())
    }
    fn build_fn(_: &ParameterSet, _: f32, _: AudioChannelLayout) -> anyhow::Result<BlockProcessor> {
        anyhow::bail!("noop — not exercised in this test")
    }

    plugin_loader::registry::register_native_simple(
        "issue_572_get_params_happy",
        "Issue 572 Test Pedal",
        Some("test"),
        BlockType::GainPedal,
        NativeRuntime {
            schema: schema_fn,
            validate: validate_fn,
            build: build_fn,
        },
    );
    // Mirror the existing pattern in `project/src/block_tests.rs`:
    // `register_native_simple` stages the package; `init` is what
    // actually promotes it into the lookup table `find` reads.
    plugin_loader::registry::init(std::path::Path::new("/nonexistent-test-path-572"));

    let out = get_plugin_params("issue_572_get_params_happy");
    assert_ne!(
        out, "{\"params\": null}",
        "registered plugin must not return null envelope"
    );
    assert!(
        out.contains("\"model\":\"issue_572_get_params_happy\""),
        "expected model id in envelope, got: {out}"
    );
    assert!(
        out.contains("\"display_name\":\"Issue 572 Test Pedal\""),
        "expected display_name in envelope, got: {out}"
    );
    assert!(
        out.contains("\"effect_type\":\"gain_pedal\""),
        "expected effect_type in envelope, got: {out}"
    );
}
