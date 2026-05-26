//! RED-first test forcing the `SeparateStems` command variant + match arm
//! to exist. Routed handler can grow incrementally; this only asserts
//! the variant is reachable through `Command` and surfaces as an MCP
//! tool (auto-derived from the schema).

use std::path::PathBuf;

use application::command::Command;
use application::command_schema::command_variant_names;

#[test]
fn separate_stems_variant_exists_and_is_serializable_as_a_command() {
    let cmd = Command::SeparateStems {
        source_path: PathBuf::from("/tmp/source.wav"),
    };
    let json = serde_json::to_value(&cmd).expect("serialize SeparateStems");
    assert!(
        json.get("SeparateStems").is_some(),
        "serialized form must carry the SeparateStems tag, got {json}"
    );
}

#[test]
fn separate_stems_appears_in_command_variant_names_for_mcp_parity() {
    let names = command_variant_names();
    assert!(
        names.contains(&"SeparateStems"),
        "MCP parity: SeparateStems must be in command_variant_names(), got {names:?}"
    );
}
