//! Infer the canonical plugin URI from a bundle's TTL files.
//!
//! Used by the `fix_lv2_manifest_uris` tool to repair `plugin_uri:`
//! entries in manifest.yaml when they don't match what the TTL
//! actually declares (e.g. mda_leslie's manifest pointed at
//! `drobilla.net` while the TTL said `moddevices.com`).
//!
//! Lives in its own module so that `dispatch.rs` stays under the
//! 600-line cap. Issue #287.

use std::fs;
use std::path::Path;

use super::dispatch::{find_plugin_blocks_in_text, parse_ports, parse_turtle_prefixes};

/// Return the canonical absolute URI declared inside the bundle's
/// TTLs, expanding turtle prefixes if needed.
///
/// Returns `None` when the bundle declares zero or more than one
/// plugin (with ports) — ambiguous bundles must be resolved by hand.
pub fn infer_plugin_uri(bundle_dir: &Path) -> Option<String> {
    if !bundle_dir.is_dir() {
        return None;
    }
    let mut texts: Vec<String> = Vec::new();
    for entry in fs::read_dir(bundle_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("ttl") {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            texts.push(text);
        }
    }

    let mut subjects: Vec<String> = Vec::new();
    for text in &texts {
        let prefixes = parse_turtle_prefixes(text);
        for block in find_plugin_blocks_in_text(text) {
            if parse_ports(block).is_empty() {
                continue;
            }
            if let Some(uri) = subject_of_plugin_block(text, block, &prefixes) {
                if !subjects.contains(&uri) {
                    subjects.push(uri);
                }
            }
        }
    }

    if subjects.len() == 1 {
        subjects.into_iter().next()
    } else {
        None
    }
}

/// Walk back from the start of a plugin block (a substring that
/// covers `subject ... a lv2:Plugin ... .`) to the first non-whitespace
/// token preceding it — that's the subject. Resolves prefixed names
/// against the document's `@prefix` table.
fn subject_of_plugin_block(
    full_text: &str,
    block: &str,
    prefixes: &[(String, String)],
) -> Option<String> {
    let block_start = block.as_ptr() as usize - full_text.as_ptr() as usize;
    let candidate = full_text[block_start..].split_whitespace().next()?;

    if let Some(stripped) = candidate.strip_prefix('<') {
        return stripped.strip_suffix('>').map(|s| s.to_string());
    }

    if let Some(colon) = candidate.find(':') {
        let prefix = &candidate[..colon];
        let local = &candidate[colon + 1..];
        for (name, base) in prefixes {
            if name == prefix {
                return Some(format!("{base}{local}"));
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "dispatch_infer_tests.rs"]
mod tests;
