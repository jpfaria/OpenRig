use super::*;
use application::command_schema::command_variant_names;

/// `Command` has exactly this many variants. If you add/remove one,
/// update this AND ensure its payload types derive `JsonSchema`
/// (otherwise the variant silently drops from the schema → no tool).
///
/// #513 / #493 bumped this from 44 → 49 with `SaveMidiDevices`,
/// `SaveMidiMapping`, `StartMidiLearn`, `StopMidiLearn`, and
/// `PublishMidiEvent`. Each adds one MCP tool automatically via
/// `command_schema` (single source of truth — the `Command` enum).
/// #513 (paths overrides) bumped to 51 with `SetPresetsPath` and
/// `SetPluginsPath`. #561 bumped to 52 with `ReloadPluginCatalog`,
/// then to 54 with `LoadPlugin` and `UnloadPlugin` (expanded scope:
/// per-plugin load / unload). #548 bumped to 58 with
/// `SelectActiveChainRelative`, `SelectActiveBlockRelative`,
/// `SetCompactViewEnabled`, `ToggleActiveBlockNeighborEnabled`.
/// #576 bumped to 59 with `RenderChain` — offline render via the
/// command bus so every transport adapter (MCP/gRPC/…) inherits it.
/// #582 bumped to 60 with `SetEvaluationsPath` — third
/// system-paths override alongside `SetPresetsPath`/`SetPluginsPath`.
/// #591 bumped to 61 with `SelectActiveChain` — chain-level selection
/// so a footswitch follows the on-screen active chain.
/// #614 bumped to 63 with `SetChainDiLoopSource` and
/// `SetChainDiLoopEnabled` — per-chain virtual DI loop (ephemeral,
/// never persisted; distinct from #324 project-level DI config).
/// #712 bumped to 65 with `SetMidiEnabled` and `SetMcpEnabled` —
/// per-machine master switches for the MIDI adapter / MCP server.
/// #716 bumped to 68 with `CreateIoBinding`, `UpdateIoBinding`, and
/// `DeleteIoBinding` — per-machine I/O binding registry (Task 3/4).
/// #716 bumped to 71 with the intent commands `RenameIoBinding`,
/// `AddIoEndpoint`, `RemoveIoEndpoint` — endpoint logic moved out of the
/// GUI into handlers (GUI is a pure dispatcher, LAW 1).
/// #716 bumped to 72 with `SetChainIoBindings` — a chain selects which I/O
/// bindings it uses; the tool auto-derives via `command_schema`.
/// #717 bumped to 73 with `SetChainDiLoopOutput` — persists the chain's
/// chosen DI output endpoint.
/// #323 bumped to 77 with the per-chain looper: `AddChainLooper`,
/// `RemoveChainLooper`, `SetChainLooperTransport`, `SetChainLooperParam` —
/// so an agent drives the looper through the same bus the GUI and the MIDI
/// footswitch use.
/// #323 bumped to 78 with `SetChainLooperAudioFile` — the pointer to the wav
/// a recorded loop was saved into.
const COMMAND_VARIANT_COUNT: usize = 78;

#[test]
fn parity_guard_every_command_variant_is_a_tool() {
    // Honest guard: the schema-derived tool set must cover ALL Command
    // variants, not just the schemars-describable subset. Catches the
    // #[schemars(skip)] regression (issue #489).
    assert_eq!(
        command_variant_names().len(),
        COMMAND_VARIANT_COUNT,
        "schema-derived variants != Command variants — a payload type \
         is missing JsonSchema (see #489)"
    );
    assert_eq!(tools().len(), COMMAND_VARIANT_COUNT);
    for t in tools() {
        assert!(variant_from_tool_name(&t.name).is_some(), "{}", t.name);
    }
    // Spot-check variants that were #[schemars(skip)]'d before #489.
    for v in [
        "AddChain",
        "ConfigureChain",
        "SaveChain",
        "LoadProject",
        "CreateProject",
        "SaveAudioSettings",
    ] {
        assert!(
            command_variant_names().contains(&v),
            "{v} missing from schema — JsonSchema not derived on its payload"
        );
    }
    // #716 spot-check: io-binding registry commands must appear as tools
    // (IoBinding / IoEndpoint derive JsonSchema in domain crate).
    for v in ["CreateIoBinding", "UpdateIoBinding", "DeleteIoBinding"] {
        assert!(
            command_variant_names().contains(&v),
            "{v} missing from schema — IoBinding payload type must derive JsonSchema"
        );
    }
}

#[test]
fn build_command_maps_unit_variant() {
    let cmd = build_command("save_project", Value::Null).unwrap();
    assert!(matches!(cmd, Command::SaveProject));
}

#[test]
fn build_command_unit_variant_with_empty_object_args() {
    // MCP clients send `arguments: {}` for a no-arg tool; serde's
    // externally-tagged unit variant rejects a map, so build_command
    // must emit the bare string for unit variants.
    let cmd = build_command("save_project", serde_json::json!({})).unwrap();
    assert!(matches!(cmd, Command::SaveProject));
}

#[test]
fn build_command_maps_struct_variant() {
    let cmd = build_command(
        "update_project_name",
        serde_json::json!({ "name": "Rig X" }),
    )
    .unwrap();
    match cmd {
        Command::UpdateProjectName { name } => assert_eq!(name, "Rig X"),
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn build_command_rejects_unknown_tool() {
    assert!(build_command("nope", Value::Null).is_err());
}

#[test]
fn build_command_is_command_from_variant_single_source() {
    // The MCP tool path and the shared `command_schema::command_from_variant`
    // (also used by `adapter-midi`) MUST produce the identical `Command`
    // for equivalent input — one source of truth, not two parallel
    // reconstructions. Locks the dedup against future drift.
    use application::command_schema::{command_from_variant, variant_from_tool_name};
    for (tool, args) in [
        ("save_project", serde_json::json!({})),
        ("update_project_name", serde_json::json!({ "name": "X" })),
        (
            "toggle_block_enabled",
            serde_json::json!({ "chain": "chain:a", "block": "block:b" }),
        ),
    ] {
        let via_tool = build_command(tool, args.clone()).unwrap();
        let variant = variant_from_tool_name(tool).unwrap();
        let via_variant = command_from_variant(variant, args).unwrap();
        assert_eq!(
            serde_json::to_value(&via_tool).unwrap(),
            serde_json::to_value(&via_variant).unwrap(),
            "tool {tool}: MCP build_command diverged from the shared builder"
        );
    }
}
