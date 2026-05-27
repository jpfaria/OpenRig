//! Red-first (#572) tests for `query::get_plugin_params`.
//!
//! See spec
//! `docs/superpowers/specs/2026-05-27-issue-572-mcp-block-plugin-params-design.md`.

use crate::bridge::QueryKind;
use crate::query::get_plugin_params;

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
    fn build_fn(
        _: &ParameterSet,
        _: f32,
        _: AudioChannelLayout,
    ) -> anyhow::Result<BlockProcessor> {
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
