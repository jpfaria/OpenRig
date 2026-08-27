//! Responsibility: converts a command variant to the tool name a transport advertises.

use crate::command_schema::command_variant_names;

/// `SetBlockParameterNumber` -> `set_block_parameter_number`.
pub fn tool_name(variant: &str) -> String {
    let mut s = String::with_capacity(variant.len() + 8);
    for (i, ch) in variant.char_indices() {
        if ch.is_uppercase() && i != 0 {
            s.push('_');
        }
        s.push(ch.to_ascii_lowercase());
    }
    s
}

/// Reverse of [`tool_name`]; `None` if it matches no `Command` variant.
pub fn variant_from_tool_name(tool: &str) -> Option<&'static str> {
    command_variant_names()
        .iter()
        .copied()
        .find(|v| tool_name(v) == tool)
}
