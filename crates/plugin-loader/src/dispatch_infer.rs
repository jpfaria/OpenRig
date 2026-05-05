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
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn returns_absolute_uri_from_prefixed_ttl() {
        let tmp = std::env::temp_dir().join(format!("openrig-infer-uri-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("plug.ttl"),
            "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
             @prefix mda: <http://moddevices.com/plugins/mda/> .\n\
             mda:Leslie\n\
                 a lv2:Plugin ;\n\
                 lv2:port [\n\
                     a lv2:InputPort, lv2:AudioPort ;\n\
                     lv2:index 0 ;\n\
                     lv2:symbol \"in\" ;\n\
                 ] ,\n\
                 [\n\
                     a lv2:InputPort, lv2:ControlPort ;\n\
                     lv2:index 1 ;\n\
                     lv2:symbol \"speed\" ;\n\
                 ] .\n",
        )
        .unwrap();

        let inferred = infer_plugin_uri(&tmp).expect("expected to infer the absolute URI from TTL");
        assert_eq!(inferred, "http://moddevices.com/plugins/mda/Leslie");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn returns_absolute_uri_when_already_absolute() {
        let tmp =
            std::env::temp_dir().join(format!("openrig-infer-uri-abs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("plug.ttl"),
            "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
             <urn:test:plug>\n\
                 a lv2:Plugin ;\n\
                 lv2:port [\n\
                     a lv2:InputPort, lv2:ControlPort ;\n\
                     lv2:index 0 ;\n\
                     lv2:symbol \"gain\" ;\n\
                 ] .\n",
        )
        .unwrap();

        let inferred = infer_plugin_uri(&tmp).expect("expected to infer the absolute URI");
        assert_eq!(inferred, "urn:test:plug");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn returns_none_when_multiple_plugins_in_bundle() {
        let tmp =
            std::env::temp_dir().join(format!("openrig-infer-uri-multi-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(
            tmp.join("plug.ttl"),
            "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
             <urn:a> a lv2:Plugin ; lv2:port [ a lv2:ControlPort ; lv2:symbol \"x\" ] .\n\
             <urn:b> a lv2:Plugin ; lv2:port [ a lv2:ControlPort ; lv2:symbol \"y\" ] .\n",
        )
        .unwrap();

        assert!(
            infer_plugin_uri(&tmp).is_none(),
            "expected None when bundle has multiple plugin declarations"
        );
        let _ = fs::remove_dir_all(&tmp);
    }
}
