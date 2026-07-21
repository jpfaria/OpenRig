//! LV2 TTL port + scale-point parsing (issue #792 split from dispatch.rs).
//!
//! Turns a single plugin's Turtle block into typed [`Lv2Port`]s. The scan
//! entry point and the port types stay in `dispatch.rs`, which re-exports
//! `parse_ports` / `lv2_control_value` so existing paths keep resolving.

use block_core::param::ParameterSet;

use crate::dispatch::{Lv2Port, Lv2PortRole, Lv2ScalePoint};

pub(crate) fn parse_ports(plugin_block: &str) -> Vec<Lv2Port> {
    let mut ports = Vec::new();
    let bytes = plugin_block.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        let mut depth: i32 = 1;
        let start = i + 1;
        let mut j = start;
        while j < bytes.len() && depth > 0 {
            match bytes[j] {
                b'[' => depth += 1,
                b']' => depth -= 1,
                _ => {}
            }
            j += 1;
        }
        let inner = &plugin_block[start..j.saturating_sub(1)];
        if let Some(port) = parse_port_block(inner) {
            ports.push(port);
        }
        i = j;
    }
    ports
}

fn parse_port_block(block: &str) -> Option<Lv2Port> {
    let index = capture_after(block, "lv2:index").and_then(|raw| raw.parse::<usize>().ok())?;
    let symbol = capture_quoted(block, "lv2:symbol")?;
    let role = classify(block);
    if matches!(role, Lv2PortRole::Other) {
        return None;
    }
    let default_value = capture_after(block, "lv2:default").and_then(|raw| raw.parse::<f32>().ok());
    let minimum = capture_after(block, "lv2:minimum").and_then(|raw| raw.parse::<f32>().ok());
    let maximum = capture_after(block, "lv2:maximum").and_then(|raw| raw.parse::<f32>().ok());
    let name = capture_quoted(block, "lv2:name");
    let properties = parse_port_properties(block);
    Some(Lv2Port {
        index,
        symbol,
        role,
        default_value,
        minimum,
        maximum,
        name,
        is_toggle: properties.contains("lv2:toggled"),
        is_integer: properties.contains("lv2:integer"),
        is_enumeration: properties.contains("lv2:enumeration"),
        scale_points: parse_scale_points(block),
        range_steps: capture_after(block, "pprop:rangeSteps")
            .and_then(|raw| raw.parse::<u32>().ok()),
    })
}

/// Read every `lv2:portProperty` directive in the port block and return
/// the unique set of property tokens. Multiple portProperty directives
/// can coexist on one port and each can carry a comma-separated list:
/// `lv2:portProperty lv2:integer, lv2:enumeration ;`.
fn parse_port_properties(block: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let mut search = block;
    while let Some(start) = search.find("lv2:portProperty") {
        let after = &search[start + "lv2:portProperty".len()..];
        // Read until the terminating `;` or `]`.
        let end = after.find([';', ']']).unwrap_or(after.len());
        let list = &after[..end];
        for token in list.split(',') {
            let trimmed = token.trim();
            if !trimmed.is_empty() {
                out.insert(trimmed.to_string());
            }
        }
        search = &after[end..];
    }
    out
}

/// Read every `lv2:scalePoint [ rdfs:label "X" ; rdf:value Y ; ]` block
/// and return the parsed `(value, label)` pairs in document order.
/// Order matches what enumeration UIs typically render, so callers can
/// pass it straight to `enum_parameter`.
fn parse_scale_points(block: &str) -> Vec<Lv2ScalePoint> {
    let mut out = Vec::new();
    let bytes = block.as_bytes();
    let needle = "lv2:scalePoint";
    let mut cursor = 0;
    while let Some(rel) = block[cursor..].find(needle) {
        let after_keyword = cursor + rel + needle.len();
        // Scan forward to the opening `[` that follows the directive,
        // skipping whitespace.
        let mut i = after_keyword;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'[' {
            cursor = after_keyword;
            continue;
        }
        let inner_start = i + 1;
        let mut depth: i32 = 1;
        let mut j = inner_start;
        while j < bytes.len() && depth > 0 {
            match bytes[j] {
                b'[' => depth += 1,
                b']' => depth -= 1,
                _ => {}
            }
            j += 1;
        }
        let inner = &block[inner_start..j.saturating_sub(1)];
        let value = capture_after(inner, "rdf:value").and_then(|raw| raw.parse::<f32>().ok());
        let label = capture_quoted(inner, "rdfs:label");
        if let (Some(value), Some(label)) = (value, label) {
            out.push(Lv2ScalePoint { value, label });
        }
        cursor = j;
    }
    out
}

fn classify(block: &str) -> Lv2PortRole {
    let is_input = block.contains("lv2:InputPort");
    let is_output = block.contains("lv2:OutputPort");
    if block.contains("lv2:AudioPort") {
        if is_input {
            return Lv2PortRole::AudioIn;
        }
        if is_output {
            return Lv2PortRole::AudioOut;
        }
    }
    if block.contains("lv2:ControlPort") {
        if is_input {
            return Lv2PortRole::ControlIn;
        }
        if is_output {
            return Lv2PortRole::ControlOut;
        }
    }
    if block.contains("atom:AtomPort") {
        if is_input {
            return Lv2PortRole::AtomIn;
        }
        if is_output {
            return Lv2PortRole::AtomOut;
        }
    }
    Lv2PortRole::Other
}

/// Find a `key` directive in `block` and return the literal that follows
/// (whitespace-delimited, semicolon-terminated). Returns the raw token
/// before the trailing `;` so callers can parse to int/float themselves.
fn capture_after(block: &str, key: &str) -> Option<String> {
    let mut search = block;
    while let Some(start) = search.find(key) {
        let rest = &search[start + key.len()..];
        let token: String = rest
            .chars()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| !c.is_whitespace() && *c != ';')
            .collect();
        if !token.is_empty() {
            return Some(token);
        }
        search = &rest[1..];
    }
    None
}

/// Find a `key "..."` directive in `block` and return the contents of
/// the double-quoted string.
fn capture_quoted(block: &str, key: &str) -> Option<String> {
    let start = block.find(key)?;
    let rest = &block[start + key.len()..];
    let after_open = rest.find('"')?;
    let after = &rest[after_open + 1..];
    let close = after.find('"')?;
    Some(after[..close].to_string())
}

/// Resolve a control parameter's value: prefer the user's `ParameterSet`
/// keyed by the LV2 symbol, fall back to the port's `lv2:default`, then
/// `0.0` as last resort.
pub fn lv2_control_value(symbol: &str, default: Option<f32>, params: &ParameterSet) -> f32 {
    if let Some(value) = params.get_f32(symbol) {
        return value;
    }
    if let Some(value) = params.get_i64(symbol) {
        return value as f32;
    }
    default.unwrap_or(0.0)
}

