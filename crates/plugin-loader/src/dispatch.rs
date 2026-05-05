//! Pure-metadata helpers used by `block-*` crates when instantiating
//! disk-backed plugins.
//!
//! These functions take only data shapes (`GridParameter`, `GridCapture`,
//! `ParameterValue`) plus the user's `ParameterSet`. They do no audio
//! work and pull in no nam/ir/lv2/vst3 dependency — that lives in each
//! `block-*` crate, which already has the right backend deps.
//!
//! Issue: #287

use std::fs;
use std::path::Path;

use anyhow::{anyhow, Result};
use block_core::param::ParameterSet;

use crate::manifest::{GridCapture, GridParameter, ParameterValue};

/// Pick the [`GridCapture`] whose declared values are closest to the
/// user's `params` for every declared `parameters` axis.
///
/// For numeric axes the user value is snapped to the nearest declared
/// value (linear distance). For text axes an exact match is required;
/// when the user's value is missing or doesn't match, the first capture
/// that matches the other axes wins.
///
/// Returns `None` if `captures` is empty or no capture matches all
/// declared text axes.
pub fn resolve_capture<'a>(
    parameters: &[GridParameter],
    captures: &'a [GridCapture],
    params: &ParameterSet,
) -> Option<&'a GridCapture> {
    if captures.is_empty() {
        return None;
    }
    if parameters.is_empty() {
        return captures.first();
    }
    let snapped: Vec<(String, ParameterValue)> = parameters
        .iter()
        .map(|parameter| {
            let value = snap_user_value(parameter, params);
            (parameter.name.clone(), value)
        })
        .collect();
    captures
        .iter()
        .min_by(|left, right| score(left, &snapped).cmp(&score(right, &snapped)))
}

/// Snap the user's value for `parameter` to the nearest declared value
/// (numeric) or pick the first declared (text fallback).
fn snap_user_value(parameter: &GridParameter, params: &ParameterSet) -> ParameterValue {
    let user_text = params.get_string(&parameter.name);
    if let Some(text) = user_text {
        for declared in &parameter.values {
            if let ParameterValue::Text(declared_text) = declared {
                if declared_text == text {
                    return ParameterValue::Text(text.to_string());
                }
            }
        }
    }
    if let Some(user_number) = params.get_f32(&parameter.name) {
        let mut best = parameter.values.first().cloned().unwrap_or(ParameterValue::Number(0.0));
        let mut best_dist = f64::INFINITY;
        for declared in &parameter.values {
            if let ParameterValue::Number(declared_value) = declared {
                let dist = ((*declared_value) - f64::from(user_number)).abs();
                if dist < best_dist {
                    best_dist = dist;
                    best = ParameterValue::Number(*declared_value);
                }
            }
        }
        return best;
    }
    parameter
        .values
        .first()
        .cloned()
        .unwrap_or(ParameterValue::Number(0.0))
}

/// Lower is better. Sums per-axis mismatches.
fn score(capture: &GridCapture, snapped: &[(String, ParameterValue)]) -> u64 {
    let mut total: u64 = 0;
    for (name, target) in snapped {
        match capture.values.get(name) {
            Some(actual) if values_equal(actual, target) => {}
            _ => total = total.saturating_add(1),
        }
    }
    total
}

fn values_equal(left: &ParameterValue, right: &ParameterValue) -> bool {
    match (left, right) {
        (ParameterValue::Number(a), ParameterValue::Number(b)) => a.to_bits() == b.to_bits(),
        (ParameterValue::Text(a), ParameterValue::Text(b)) => a == b,
        _ => false,
    }
}

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
    let mut combined = String::new();
    let mut found_files = false;
    if bundle_dir.is_dir() {
        for entry in fs::read_dir(bundle_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("ttl") {
                continue;
            }
            if let Ok(text) = fs::read_to_string(&path) {
                combined.push_str(&text);
                combined.push('\n');
                found_files = true;
            }
        }
    }
    if !found_files {
        return Err(anyhow!(
            "no .ttl files in LV2 bundle directory `{}`",
            bundle_dir.display()
        ));
    }
    let plugin_block = extract_plugin_block(&combined, plugin_uri).ok_or_else(|| {
        anyhow!(
            "plugin URI `{plugin_uri}` not found in TTL files under `{}`",
            bundle_dir.display()
        )
    })?;
    Ok(parse_ports(&plugin_block))
}

/// Substring of `combined` that starts at the plugin URI and runs until
/// its terminating `.` (or end of string) — every TTL plugin block ends
/// with a `.` separator.
fn extract_plugin_block(combined: &str, plugin_uri: &str) -> Option<String> {
    let needle = format!("<{plugin_uri}>");
    let start = combined.find(&needle)?;
    let after = &combined[start + needle.len()..];
    let mut depth: i32 = 0;
    let mut end = after.len();
    for (idx, ch) in after.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth = (depth - 1).max(0),
            '.' if depth == 0 => {
                end = idx;
                break;
            }
            _ => {}
        }
    }
    Some(after[..end].to_string())
}

fn parse_ports(plugin_block: &str) -> Vec<Lv2Port> {
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
    Some(Lv2Port {
        index,
        symbol,
        role,
        default_value,
        minimum,
        maximum,
        name,
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ParameterValue as ManifestParameterValue;
    use domain::value_objects::ParameterValue as DomainValue;

    fn cap(values: &[(&str, f64)], file: &str) -> GridCapture {
        GridCapture {
            values: values
                .iter()
                .map(|(k, v)| ((*k).to_string(), ManifestParameterValue::Number(*v)))
                .collect(),
            file: file.into(),
        }
    }

    fn pset(pairs: &[(&str, f32)]) -> ParameterSet {
        let mut p = ParameterSet::default();
        for (k, v) in pairs {
            p.insert(*k, DomainValue::Float(*v));
        }
        p
    }

    #[test]
    fn resolve_capture_picks_exact_numeric_match() {
        let parameters = vec![GridParameter {
            name: "gain".into(),
            display_name: None,
            values: vec![ManifestParameterValue::Number(10.0), ManifestParameterValue::Number(20.0)],
        }];
        let captures = vec![cap(&[("gain", 10.0)], "g10.nam"), cap(&[("gain", 20.0)], "g20.nam")];
        let chosen = resolve_capture(&parameters, &captures, &pset(&[("gain", 20.0)])).unwrap();
        assert_eq!(chosen.file.to_str(), Some("g20.nam"));
    }

    #[test]
    fn resolve_capture_snaps_to_nearest_numeric() {
        let parameters = vec![GridParameter {
            name: "gain".into(),
            display_name: None,
            values: vec![
                ManifestParameterValue::Number(10.0),
                ManifestParameterValue::Number(50.0),
                ManifestParameterValue::Number(90.0),
            ],
        }];
        let captures = vec![
            cap(&[("gain", 10.0)], "low.nam"),
            cap(&[("gain", 50.0)], "mid.nam"),
            cap(&[("gain", 90.0)], "high.nam"),
        ];
        let chosen = resolve_capture(&parameters, &captures, &pset(&[("gain", 47.0)])).unwrap();
        assert_eq!(chosen.file.to_str(), Some("mid.nam"));
    }

    #[test]
    fn resolve_capture_returns_first_when_no_parameters() {
        let captures = vec![cap(&[], "only.wav"), cap(&[], "other.wav")];
        let chosen = resolve_capture(&[], &captures, &ParameterSet::default()).unwrap();
        assert_eq!(chosen.file.to_str(), Some("only.wav"));
    }

    #[test]
    fn scan_lv2_ports_extracts_audio_and_control_indices() {
        use std::fs;
        let tmp = std::env::temp_dir().join(format!("openrig-ttl-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let ttl = "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
<urn:test:plug>\n\
    a lv2:Plugin ;\n\
    lv2:port [\n\
        a lv2:InputPort, lv2:AudioPort ;\n\
        lv2:index 0 ;\n\
        lv2:symbol \"in_l\" ;\n\
    ] ,\n\
    [\n\
        a lv2:OutputPort, lv2:AudioPort ;\n\
        lv2:index 1 ;\n\
        lv2:symbol \"out_l\" ;\n\
    ] ,\n\
    [\n\
        a lv2:InputPort, lv2:ControlPort ;\n\
        lv2:index 2 ;\n\
        lv2:symbol \"gain\" ;\n\
        lv2:default 0.5 ;\n\
    ] .\n";
        fs::write(tmp.join("plug.ttl"), ttl).unwrap();
        let ports = scan_lv2_ports(&tmp, "urn:test:plug").unwrap();
        assert_eq!(ports.len(), 3);
        let audio_in: Vec<_> = ports.iter().filter(|p| p.role == Lv2PortRole::AudioIn).collect();
        assert_eq!(audio_in.len(), 1);
        assert_eq!(audio_in[0].index, 0);
        assert_eq!(audio_in[0].symbol, "in_l");
        let control: Vec<_> = ports.iter().filter(|p| p.role == Lv2PortRole::ControlIn).collect();
        assert_eq!(control.len(), 1);
        assert_eq!(control[0].symbol, "gain");
        assert_eq!(control[0].default_value, Some(0.5));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn lv2_control_value_falls_back_through_chain() {
        let user = pset(&[("dry_level", 80.0)]);
        assert_eq!(lv2_control_value("dry_level", Some(50.0), &user), 80.0);
        assert_eq!(lv2_control_value("missing", Some(50.0), &user), 50.0);
        assert_eq!(lv2_control_value("missing", None, &user), 0.0);
    }
}
