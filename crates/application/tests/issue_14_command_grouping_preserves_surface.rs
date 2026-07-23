//! Issue #14 — characterization pins for grouping `Command` into per-domain
//! sub-enums.
//!
//! The enum outgrew the repo's line cap, and the fix is to split it by domain.
//! But `Command` is not just a Rust type: `command_schema.rs` derives ONE MCP
//! tool per variant from its JsonSchema, and the serialized form is the wire
//! format MCP and gRPC clients already speak. A naive nesting would collapse 73
//! tools into a handful of group names and silently break every client.
//!
//! These tests are written BEFORE the split and must pass unchanged after it.
//! They pin the two things that must not move:
//!   1. the exact set of variant names the tool surface exposes, and
//!   2. the JSON a command serializes to.
//!
//! If a later change genuinely intends to alter the wire format, it has to edit
//! these lists deliberately — which is the point.

use application::command::Command;
use application::command_schema::command_variant_names;
use domain::ids::{BlockId, ChainId};

/// Every variant the MCP/gRPC surface exposed before the split.
const EXPECTED_VARIANTS: &[&str] = &[
    "AddBlock",
    "AddChain",
    "AddIoEndpoint",
    "ApplyRigNav",
    "CaptureRigEdits",
    "CloseProject",
    "ConfigureChain",
    "CreateIoBinding",
    "CreateProject",
    "DeleteChainPreset",
    "DeleteIoBinding",
    "InsertPrebuiltBlock",
    "LoadChainPreset",
    "LoadPlugin",
    "LoadProject",
    "MarkRecentProjectInvalid",
    "MoveBlock",
    "MoveChainDown",
    "MoveChainUp",
    "OverwriteBlock",
    "PickBlockParameterFile",
    "PublishMidiEvent",
    "RegisterRecentProject",
    "ReloadPluginCatalog",
    "RemoveBlock",
    "RemoveChain",
    "RemoveIoEndpoint",
    "RemoveRecentProject",
    "RenameIoBinding",
    "RenameRigPreset",
    "RenderChain",
    "ReplaceBlockModel",
    "SaveAudioSettings",
    "SaveChain",
    "SaveChainInputEndpoints",
    "SaveChainIo",
    "SaveChainOutputEndpoints",
    "SaveChainPreset",
    "SaveInsertBlock",
    "SaveMidiDevices",
    "SaveMidiMapping",
    "SaveProject",
    "SelectActiveBlockRelative",
    "SelectActiveChain",
    "SelectActiveChainRelative",
    "SelectBlockParameterOption",
    "SelectChainBlock",
    "SetBlockParameterBool",
    "SetBlockParameterNumber",
    "SetBlockParameterText",
    "SetChainDiLoopEnabled",
    "SetChainDiLoopOutput",
    "SetChainDiLoopSource",
    "SetChainIoBindings",
    "SetChainVolume",
    "SetCompactViewEnabled",
    "SetEvaluationsPath",
    "SetLanguage",
    "SetMcpEnabled",
    "SetMidiEnabled",
    "SetOutputMuted",
    "SetPluginsPath",
    "SetPresetsPath",
    "SetSpectrumEnabled",
    "SetTunerEnabled",
    "StartMidiLearn",
    "StopMidiLearn",
    "ToggleActiveBlockNeighborEnabled",
    "ToggleBlockEnabled",
    "ToggleChainEnabled",
    "UnloadPlugin",
    "UpdateIoBinding",
    "UpdateProjectName",
];

#[test]
fn every_command_variant_name_survives_the_grouping() {
    let mut actual: Vec<String> = command_variant_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    actual.sort();

    let mut expected: Vec<String> = EXPECTED_VARIANTS.iter().map(|s| s.to_string()).collect();
    expected.sort();

    let missing: Vec<&String> = expected.iter().filter(|n| !actual.contains(n)).collect();
    let added: Vec<&String> = actual.iter().filter(|n| !expected.contains(n)).collect();

    assert!(
        missing.is_empty(),
        "grouping Command dropped MCP tools that clients already call: {missing:?}"
    );
    assert!(
        added.is_empty(),
        "grouping Command exposed unexpected tool names — a group wrapper is \
         leaking into the tool surface instead of its variants: {added:?}"
    );
}

/// One representative variant per domain group, so a wrapper leaking into the
/// serialized form is caught wherever it happens.
#[test]
fn wire_format_is_unchanged_by_the_grouping() {
    let cases: Vec<(Command, &str)> = vec![
        (
            Command::ToggleBlockEnabled {
                chain: ChainId("c1".into()),
                block: BlockId("b1".into()),
            },
            r#"{"ToggleBlockEnabled":{"chain":"c1","block":"b1"}}"#,
        ),
        (
            Command::MoveChainUp {
                chain: ChainId("c1".into()),
            },
            r#"{"MoveChainUp":{"chain":"c1"}}"#,
        ),
        (Command::SaveProject, r#""SaveProject""#),
        (Command::CloseProject, r#""CloseProject""#),
        (Command::StopMidiLearn, r#""StopMidiLearn""#),
        (Command::ReloadPluginCatalog, r#""ReloadPluginCatalog""#),
        (
            Command::SetLanguage {
                language: Some("pt_BR".into()),
            },
            r#"{"SetLanguage":{"language":"pt_BR"}}"#,
        ),
        (
            Command::SetTunerEnabled { enabled: true },
            r#"{"SetTunerEnabled":{"enabled":true}}"#,
        ),
        (
            Command::SetOutputMuted { muted: false },
            r#"{"SetOutputMuted":{"muted":false}}"#,
        ),
        (
            Command::DeleteIoBinding { id: "io1".into() },
            r#"{"DeleteIoBinding":{"id":"io1"}}"#,
        ),
    ];

    for (command, expected) in cases {
        let actual = serde_json::to_string(&command).expect("Command serializes");
        assert_eq!(
            actual, expected,
            "the serialized form of {command:?} changed — MCP and gRPC clients \
             speak this exact JSON"
        );
    }
}

/// The round trip is what MCP actually does: a client sends the JSON above and
/// the server deserializes it back into a `Command`.
#[test]
fn commands_still_deserialize_from_the_original_wire_format() {
    let wire = r#"{"SetTunerEnabled":{"enabled":true}}"#;
    let parsed: Command = serde_json::from_str(wire).expect("wire format still parses");
    assert!(matches!(parsed, Command::SetTunerEnabled { enabled: true }));

    let unit: Command = serde_json::from_str(r#""SaveProject""#).expect("unit variant still parses");
    assert!(matches!(unit, Command::SaveProject));
}
