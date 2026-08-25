//! Responsibility: escapes a value so it can go into a JSON string.

use std::fmt::Write;

/// Minimal JSON-string escaper that wraps the result in double quotes.
/// Used by the #554 preset listings; avoids dragging `serde_json` into a
/// pure listing helper — preset names and chain ids never carry control
/// chars deeper than `"`, `\` or whitespace.
pub(crate) fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    out.push_str(&json_escape(s));
    out.push('"');
    out
}

/// JSON-escape a string for inclusion in a manually-built JSON literal.
/// Does NOT wrap the result in quotes — callers handle quoting (see
/// [`json_string`] when both escape + quote are wanted).
pub(crate) fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}
