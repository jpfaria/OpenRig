use super::*;
use application::command::ProjectCommand;
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
/// #14 bumped to 82 with the nine `MetronomeCommand` leaves
/// (`SetMetronomeEnabled`/`Bpm`/`TimeSignature`/`Subdivision`/`Volume`/
/// `Timbre`/`CountIn`/`Output` and `MetronomeTap`).
/// #829 bumped to 83 with `RefreshAudioDevices` — device re-enumeration
/// was reachable only by clicking the GUI's refresh button.
/// #791 bumped to 85 with `DiagnoseChainTone` and `ApplyToneDoctorFix` —
/// the Tone Doctor moved onto the bus, so MCP/gRPC reach the same
/// diagnosis and the same measured fix the GUI panel shows.
/// #323 bumped to 93 with the eight `LooperCommand` leaves
/// (`AddChainLooper`/`RemoveChainLooper`/`SetChainLooperTransport`/`Param`/
/// `Input`/`Output`/`AudioFile`/`Preset`) — the per-chain looper, incl.
/// phase-2 tone independence (`SetChainLooperPreset`).
/// #127 bumped to 94 with `SetIoBindings` — installing the effective I/O
/// binding registry into the LIVE runtime was reachable only from the GUI
/// (it called `ProjectRuntimeController::set_io_bindings` directly).
/// #127 bumped to 95 with `SyncChainRuntime` — bringing ONE chain's live
/// runtime back in step with the project was a UI-only call
/// (`sync_live_chain_runtime` inside a Slint closure).
/// #127 bumped to 96 with `StopProjectRuntime` — STOPPING the rig was
/// reachable only from the GUI's back-to-launcher button, so a client that
/// started the audio (a chain enable, a DI play) could not silence it again.
/// #826 bumped to 99 with the three loop-editing leaves
/// (`EditChainLooperAudio`/`UndoChainLooperEdit`/`RedoChainLooperEdit`) —
/// trimming, cropping and cutting a recorded loop, so a headless client
/// reshapes a take exactly as the waveform editor does.
/// #827 bumped to 100 with `SaveChainLooperTake` — keeping a recorded loop as
/// a named take in the app-wide library, so a headless client can save one
/// and hand it to the DI exactly as the editor's Save button does.
/// #328 bumped to 103 with `AddSplit`, `SetSplitEnd` and `RemoveSplit` —
/// the chain split (Split → Mix, Y → A/B) created, switched and removed
/// from any transport.
/// #328 bumped to 104 with `SetChainEndpointEnabled` — the endpoint
/// checklist of the chain graph's input/output nodes.
/// #1007 bumped to 103 with `SetMixerFader`/`SetMixerMute`/`ToggleMixerMute`
/// — the global mixer's per-endpoint fader and mute — then to 105 with
/// `SetMixerSolo`/`ToggleMixerSolo`, the strip SOLO, then to 109 with a
/// chain's own faders (`SetChainMixerFader`/`SetChainMixerMute`/
/// `ToggleChainMixerMute`/`SetChainDiFader`).
/// Both merged (#328 + #1007): 100 + 4 + 9.
/// #328 §11 bumped to 115 with `AddSplitPath`/`RemoveSplitPath` — a split
/// with any number of paths.
/// #1021 bumped to 116 with `DeleteLooperTake` — removing a saved take from
/// the library, so a headless client can prune it as the DI panel's trash does.
/// The backing-track player bumped to 127 with the ten `PlayerCommand` leaves
/// (`LoadPlayerTrack`/`SetPlayerPlaying`/`StopPlayer`/`SeekPlayer`/
/// `SetPlayerVolume`/`Speed`/`Semitones`/`Loop`/`ClearPlayerLoop`/
/// `SetPlayerOutput`) and `SetBackingTracksPath`, then to 128 with
/// `DeletePlayerTrack` — the trash on the user's own tracks. The drum machine
/// adds its ten `DrumsCommand` leaves (play, stop, toggle, enable, kit, groove,
/// fill, volume, output) — the drums follow the project tempo (#1050).
const COMMAND_VARIANT_COUNT: usize = 137;

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
    assert!(matches!(cmd, Command::Project(ProjectCommand::SaveProject)));
}

#[test]
fn build_command_unit_variant_with_empty_object_args() {
    // MCP clients send `arguments: {}` for a no-arg tool; serde's
    // externally-tagged unit variant rejects a map, so build_command
    // must emit the bare string for unit variants.
    let cmd = build_command("save_project", serde_json::json!({})).unwrap();
    assert!(matches!(cmd, Command::Project(ProjectCommand::SaveProject)));
}

#[test]
fn build_command_maps_struct_variant() {
    let cmd = build_command(
        "update_project_name",
        serde_json::json!({ "name": "Rig X" }),
    )
    .unwrap();
    match cmd {
        Command::Project(ProjectCommand::UpdateProjectName { name }) => assert_eq!(name, "Rig X"),
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

#[test]
fn add_block_tool_keeps_the_split_path_and_defaults_to_the_top_level() {
    let with_path = build_command(
        "add_block",
        serde_json::json!({
            "chain": "rig:in", "kind": "gain", "model_id": "fuzz_ge", "position": 0,
            "path": { "split": "s1", "path": 1 }
        }),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&with_path).unwrap()["AddBlock"]["path"],
        serde_json::json!({ "split": "s1", "path": 1 }),
        "#328: the split path an MCP client sends must reach the command"
    );

    let without = build_command(
        "add_block",
        serde_json::json!({ "chain": "rig:in", "kind": "gain", "model_id": "fuzz_ge", "position": 0 }),
    )
    .unwrap();
    assert!(
        serde_json::to_value(&without).unwrap()["AddBlock"]
            .get("path")
            .is_none(),
        "a path-less add_block stays a top-level add"
    );
}

#[test]
fn split_tools_build_their_commands() {
    for (tool, args, wire) in [
        (
            "add_split",
            serde_json::json!({ "chain": "rig:in", "position": 1, "end": "mix" }),
            serde_json::json!({ "AddSplit": { "chain": "rig:in", "position": 1, "end": "mix" } }),
        ),
        (
            "set_split_end",
            serde_json::json!({ "chain": "rig:in", "split_id": "s1", "end": "y" }),
            serde_json::json!({ "SetSplitEnd": { "chain": "rig:in", "split_id": "s1", "end": "y" } }),
        ),
        (
            "remove_split",
            serde_json::json!({ "chain": "rig:in", "split_id": "s1" }),
            serde_json::json!({ "RemoveSplit": { "chain": "rig:in", "split_id": "s1" } }),
        ),
    ] {
        let cmd = build_command(tool, args).unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert_eq!(serde_json::to_value(&cmd).unwrap(), wire, "{tool}");
    }
}

#[test]
fn set_chain_endpoint_enabled_tool_builds_its_command() {
    let args = serde_json::json!({
        "chain": "rig:in", "node": { "path_output": { "split": "s1", "path": 0 } }, "io": "io-main",
        "endpoint": "Out 1", "enabled": false
    });
    let cmd = build_command("set_chain_endpoint_enabled", args.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&cmd).unwrap(),
        serde_json::json!({ "SetChainEndpointEnabled": args })
    );
}
