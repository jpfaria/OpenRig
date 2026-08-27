//! Responsibility: reads an LV2 bundle's ports out of its Turtle files.

use crate::dispatch_lv2_parse::parse_ports;
use anyhow::{anyhow, Result};
use std::fs;
use std::path::Path;

/// LV2 port classification — derived from the TTL `a lv2:...Port` lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lv2PortRole {
    AudioIn,
    AudioOut,
    ControlIn,
    ControlOut,
    AtomIn,
    AtomOut,
    Other,
}

/// One LV2 port discovered by [`scan_lv2_ports`].
#[derive(Debug, Clone)]
pub struct Lv2Port {
    pub index: usize,
    pub symbol: String,
    pub role: Lv2PortRole,
    pub default_value: Option<f32>,
    pub minimum: Option<f32>,
    pub maximum: Option<f32>,
    pub name: Option<String>,
    /// `lv2:portProperty lv2:toggled` — value is bool (0/1).
    pub is_toggle: bool,
    /// `lv2:portProperty lv2:integer` — value is integer (no decimals).
    pub is_integer: bool,
    /// `lv2:portProperty lv2:enumeration` — port has discrete `scale_points`.
    pub is_enumeration: bool,
    /// `lv2:scalePoint [ rdfs:label "X" ; rdf:value Y ; ]` collected in
    /// document order. Used together with `is_enumeration` to render a
    /// dropdown/select widget.
    pub scale_points: Vec<Lv2ScalePoint>,
    /// `pprop:rangeSteps N` — number of discrete positions across the
    /// range. Used for integer/quantised float steppers.
    pub range_steps: Option<u32>,
}

/// One labelled value in an enumeration port. Value is stored as `f32`
/// because LV2 enumerations always come from a numeric port; the label
/// is what the user sees in the dropdown.
#[derive(Debug, Clone, PartialEq)]
pub struct Lv2ScalePoint {
    pub value: f32,
    pub label: String,
}

/// Parse every `<plugin>.ttl` (and `manifest.ttl`) inside `bundle_dir`
/// and return the merged port list of the plugin matching `plugin_uri`.
///
/// This is a deliberately small TTL/turtle scanner — it understands the
/// shape OpenRig plugin packages use:
///
/// ```turtle
/// <urn:plugin>
///     a lv2:Plugin ;
///     lv2:port [
///         a lv2:InputPort, lv2:AudioPort ;
///         lv2:index 0 ;
///         lv2:symbol "in_l" ;
///         lv2:default 0.5 ;
///     ] ,
///     [ ... ] ;
/// ```
///
/// It does not implement RDF turtle in full — comments, blank-node
/// nesting beyond one level, or unusual whitespace will cause ports to
/// be skipped. For the curated bundles shipped by OpenRig this is
/// adequate.
pub fn scan_lv2_ports(bundle_dir: &Path, plugin_uri: &str) -> Result<Vec<Lv2Port>> {
    if !bundle_dir.is_dir() {
        return Err(anyhow!(
            "no .ttl files in LV2 bundle directory `{}`",
            bundle_dir.display()
        ));
    }
    // The same plugin URI typically appears in multiple .ttl files
    // inside a bundle: `manifest.ttl` declares the plugin and points at
    // the binary (no ports), `<plugin>_dsp.ttl` carries the actual port
    // declarations, preset .ttls re-reference the URI to attach values.
    // We need the block with the most ports — parse each file
    // separately and keep the longest list.
    let mut best: Vec<Lv2Port> = Vec::new();
    let mut any_ttl = false;
    let mut texts: Vec<String> = Vec::new();
    for entry in fs::read_dir(bundle_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("ttl") {
            continue;
        }
        any_ttl = true;
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(block) = extract_plugin_block(&text, plugin_uri) {
            let ports = parse_ports(&block);
            if ports.len() > best.len() {
                best = ports;
            }
        }
        texts.push(text);
    }
    if !any_ttl {
        return Err(anyhow!(
            "no .ttl files in LV2 bundle directory `{}`",
            bundle_dir.display()
        ));
    }

    // Fallback: many real bundles ship a manifest.yaml whose
    // `plugin_uri` does not match the URI declared inside the TTL
    // (e.g. MDA Leslie's manifest says drobilla.net but the TTL
    // declares moddevices.com). When the requested URI matches
    // nothing AND the bundle declares exactly one `a lv2:Plugin`,
    // use that single plugin's ports — surfacing knobs in the GUI is
    // worth more than enforcing a typo'd manifest URI.
    if best.is_empty() {
        if let Some(ports) = ports_for_only_plugin_in_bundle(&texts) {
            eprintln!(
                "LV2 bundle `{}`: requested URI `{plugin_uri}` not found; falling back to the only declared plugin (manifest URI is likely wrong)",
                bundle_dir.display()
            );
            return Ok(ports);
        }
        return Err(anyhow!(
            "plugin URI `{plugin_uri}` has no port declarations in any .ttl under `{}`",
            bundle_dir.display()
        ));
    }
    Ok(best)
}

/// If the bundle declares exactly one `a lv2:Plugin` subject across
/// all of its TTLs, return that plugin's ports. Otherwise return
/// `None` — multiple plugins make it ambiguous which one the user
/// meant to load, and we'd rather fail loudly than guess.
fn ports_for_only_plugin_in_bundle(texts: &[String]) -> Option<Vec<Lv2Port>> {
    let mut hits: Vec<&str> = Vec::new();
    for text in texts {
        for block in find_plugin_blocks_in_text(text) {
            // Skip blocks that declare zero ports — usually
            // `manifest.ttl` pointer entries that say "this plugin
            // exists, here's its binary" without enumerating ports.
            if !parse_ports(block).is_empty() {
                hits.push(block);
            }
        }
    }
    if hits.len() == 1 {
        return Some(parse_ports(hits[0]));
    }
    None
}

/// Iterator over substrings of a TTL document that correspond to a
/// `subject ... a lv2:Plugin ... .` statement. The returned slice
/// starts right after the subject token and runs to the terminating
/// `.`, which is exactly the shape `parse_ports` expects.
pub(crate) fn find_plugin_blocks_in_text(text: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = text[cursor..].find("lv2:Plugin") {
        let abs = cursor + rel;
        // Walk back from `lv2:Plugin` to confirm this is `a lv2:Plugin`
        // and not a different predicate that happens to share the
        // suffix (e.g. `lv2:PluginType`). Skip past the `a` keyword
        // and any whitespace separating it from the rest.
        let prefix = text[..abs].trim_end();
        if !prefix.ends_with(" a") && !prefix.ends_with("\ta") && !prefix.ends_with("\na") {
            cursor = abs + "lv2:Plugin".len();
            continue;
        }
        // Find the start of the statement: previous `.` at depth 0.
        let stmt_start = previous_statement_start(text, abs);
        // Skip over the subject: the block begins right at the start
        // of the statement (after any whitespace), which is exactly
        // what extract_plugin_block returns when given an absolute
        // URI. parse_ports walks the `[ ... ]` blocks anyway, so the
        // subject prefix doesn't interfere.
        let stmt_end = next_statement_terminator(text, abs);
        out.push(&text[stmt_start..stmt_end]);
        cursor = stmt_end;
    }
    out
}

fn previous_statement_start(text: &str, idx: usize) -> usize {
    let bytes = text.as_bytes();
    let mut i = idx;
    let mut in_uri = false;
    let mut in_string = false;
    while i > 0 {
        i -= 1;
        let ch = bytes[i] as char;
        match ch {
            '>' if !in_string => in_uri = true,
            '<' if !in_string => in_uri = false,
            '"' if !in_uri => in_string = !in_string,
            '.' if !in_uri && !in_string => return i + 1,
            _ => {}
        }
    }
    0
}

fn next_statement_terminator(text: &str, idx: usize) -> usize {
    let mut depth: i32 = 0;
    let mut in_uri = false;
    let mut in_string = false;
    for (offset, ch) in text[idx..].char_indices() {
        match ch {
            '"' if !in_uri => in_string = !in_string,
            '<' if !in_string && !in_uri => in_uri = true,
            '>' if !in_string && in_uri => in_uri = false,
            '[' if !in_uri && !in_string => depth += 1,
            ']' if !in_uri && !in_string => depth = (depth - 1).max(0),
            '.' if depth == 0 && !in_uri && !in_string => {
                return idx + offset;
            }
            _ => {}
        }
    }
    text.len()
}

/// Substring of `combined` that starts at the plugin URI and runs until
/// its terminating `.` separator. Tracks three kinds of nesting so
/// turtle quirks don't break the walker:
/// - `[ ... ]` blank-node depth (port descriptors).
/// - `< ... >` URI quoting — periods inside URLs like
///   `<http://lv2plug.in/...>` are NOT statement terminators.
/// - `" ... "` literal strings — same reason.
///
/// Also resolves turtle prefixed names: real bundles often declare
/// the plugin as `fomp:cs_phaser1` (after `@prefix fomp: <...> .`)
/// instead of the absolute `<URI>` form the manifest carries. We
/// expand `@prefix` declarations and look for both forms.
fn extract_plugin_block(combined: &str, plugin_uri: &str) -> Option<String> {
    let mut candidates: Vec<String> = vec![format!("<{plugin_uri}>")];
    for (prefix_name, base) in parse_turtle_prefixes(combined) {
        if let Some(local) = plugin_uri.strip_prefix(&base) {
            // Local-name characters per turtle spec are quite permissive;
            // we just guard against an empty local (would match the
            // bare prefix declaration itself).
            if !local.is_empty() {
                candidates.push(format!("{prefix_name}:{local}"));
            }
        }
    }

    let (start, needle_len) = candidates
        .iter()
        .filter_map(|n| combined.find(n.as_str()).map(|idx| (idx, n.len())))
        .min_by_key(|&(idx, _)| idx)?;
    let after = &combined[start + needle_len..];
    let mut depth: i32 = 0;
    let mut in_uri = false;
    let mut in_string = false;
    let mut end = after.len();
    for (idx, ch) in after.char_indices() {
        match ch {
            '"' if !in_uri => in_string = !in_string,
            '<' if !in_string && !in_uri => in_uri = true,
            '>' if !in_string && in_uri => in_uri = false,
            '[' if !in_uri && !in_string => depth += 1,
            ']' if !in_uri && !in_string => depth = (depth - 1).max(0),
            '.' if depth == 0 && !in_uri && !in_string => {
                end = idx;
                break;
            }
            _ => {}
        }
    }
    Some(after[..end].to_string())
}

/// Extract `@prefix <name>: <<base>> .` declarations from a turtle
/// document. Returns `(name, base)` pairs in document order. Only
/// well-formed lines are recognised — malformed prefixes are ignored
/// silently rather than failing the whole scan.
pub(crate) fn parse_turtle_prefixes(combined: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw_line in combined.lines() {
        let line = raw_line.trim_start();
        let Some(rest) = line.strip_prefix("@prefix") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(colon_idx) = rest.find(':') else {
            continue;
        };
        let name = rest[..colon_idx].trim().to_string();
        let after_colon = rest[colon_idx + 1..].trim_start();
        let Some(uri_start) = after_colon.find('<') else {
            continue;
        };
        let after_open = &after_colon[uri_start + 1..];
        let Some(uri_end) = after_open.find('>') else {
            continue;
        };
        let base = after_open[..uri_end].to_string();
        if !name.is_empty() {
            out.push((name, base));
        }
    }
    out
}
