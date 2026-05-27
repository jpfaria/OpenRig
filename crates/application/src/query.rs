//! Read-only project introspection for adapters. The project uses opaque
//! string IDs (`chain:<uuid>`, `chain:<uuid>:block:<uuid>`), not ordinals,
//! so a `midi-map.yaml` author (or the MCP `openrig://ids` resource) needs a
//! way to discover them. This is the single place that formats that listing;
//! adapters never re-walk `Project` themselves.

use std::fmt::Write;

use plugin_loader::manifest::Backend;
use project::project::Project;

/// Human-readable, copy-paste-ready listing of every chain and block with
/// its full ID, instrument/kind, and enabled state — the values that go
/// into `midi-map.yaml` `chain:` / `block:`.
pub fn list_ids(project: &Project) -> String {
    let mut out = String::new();
    let name = project.name.as_deref().unwrap_or("(unnamed)");
    let _ = writeln!(out, "project: {name}");
    if project.chains.is_empty() {
        out.push_str("(no chains)\n");
        return out;
    }
    for chain in &project.chains {
        let state = if chain.enabled { "enabled" } else { "disabled" };
        let _ = writeln!(
            out,
            "chain {}  instrument={}  {}",
            chain.id.0, chain.instrument, state
        );
        if chain.blocks.is_empty() {
            out.push_str("  (no blocks)\n");
        }
        for b in &chain.blocks {
            let bs = if b.enabled { "enabled" } else { "disabled" };
            let _ = writeln!(out, "  block {}  {}  {}", b.id.0, b.kind.label(), bs);
        }
    }
    let _ = writeln!(out, "(chains: {})", project.chains.len());
    out
}

/// JSON-escape a string for inclusion in a manually-built JSON literal.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
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

/// Block-type label used in the plugin catalog JSON. Stable strings —
/// adapters and clients pin against these.
fn block_type_label(bt: &plugin_loader::manifest::BlockType) -> &'static str {
    use plugin_loader::manifest::BlockType::*;
    match bt {
        GainPedal => "gain_pedal",
        Preamp => "preamp",
        Amp => "amp",
        Cab => "cab",
        Body => "body",
        Reverb => "reverb",
        Delay => "delay",
        Mod => "mod",
        Filter => "filter",
        Dyn => "dyn",
        Wah => "wah",
        Pitch => "pitch",
        Util => "util",
    }
}

/// Serialize one `LoadedPackage` entry as a JSON object for the plugin
/// catalog listing. Single source of truth for the shape (list and
/// get share it).
fn plugin_entry_json(p: &plugin_loader::LoadedPackage, out: &mut String) {
    let backend = match p.manifest.backend {
        Backend::Native { .. } => "native",
        _ => "disk",
    };
    let _ = write!(
        out,
        "{{\"id\": \"{}\", \"display_name\": \"{}\", \"brand\": {}, \"block_type\": \"{}\", \"backend\": \"{}\"}}",
        json_escape(&p.manifest.id),
        json_escape(&p.manifest.display_name),
        match p.manifest.brand.as_deref() {
            Some(b) => format!("\"{}\"", json_escape(b)),
            None => "null".to_string(),
        },
        block_type_label(&p.manifest.block_type),
        backend,
    );
}

/// #561 (expanded scope): JSON listing of every plugin currently in
/// the process-wide catalog (`plugin_loader::registry::packages()`).
/// Each entry carries id, display_name, brand (or null), block_type,
/// backend ("native" / "disk"). Pure read — no mutation.
pub fn list_plugin_catalog() -> String {
    let mut out = String::from("{\"plugins\": [");
    let mut first = true;
    for p in plugin_loader::registry::packages() {
        if !first {
            out.push_str(", ");
        }
        first = false;
        plugin_entry_json(p, &mut out);
    }
    out.push_str("]}");
    out
}

/// #561 (expanded scope): JSON entry for the plugin with manifest id
/// `id`, wrapped under a `plugin` key. Returns `{"plugin": null}` when
/// no plugin in the catalog matches. Pure read.
pub fn get_plugin(id: &str) -> String {
    match plugin_loader::registry::find(id) {
        Some(p) => {
            let mut out = String::from("{\"plugin\": ");
            plugin_entry_json(p, &mut out);
            out.push('}');
            out
        }
        None => "{\"plugin\": null}".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::ids::{BlockId, ChainId};
    use project::block::types::{AudioBlock, AudioBlockKind, InputBlock};
    use project::chain::Chain;

    fn input_block(id: &str, enabled: bool) -> AudioBlock {
        AudioBlock {
            id: BlockId(id.to_string()),
            enabled,
            kind: AudioBlockKind::Input(InputBlock {
                model: "default".to_string(),
                entries: vec![],
            }),
        }
    }

    fn chain(id: &str, blocks: Vec<AudioBlock>) -> Chain {
        Chain {
            id: ChainId(id.to_string()),
            description: None,
            instrument: "guitar".to_string(),
            enabled: true,
            volume: 100.0,
            blocks,
        }
    }

    fn project(chains: Vec<Chain>) -> Project {
        Project {
            name: Some("My Rig".to_string()),
            device_settings: vec![],
            chains,
            midi: None,
        }
    }

    #[test]
    fn empty_project_reports_no_chains() {
        let out = list_ids(&Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        });
        assert!(out.contains("project: (unnamed)"), "{out}");
        assert!(out.contains("(no chains)"), "{out}");
    }

    #[test]
    fn lists_full_ids_for_chains_and_blocks() {
        let p = project(vec![chain(
            "chain:abc",
            vec![input_block("chain:abc:block:def", true)],
        )]);
        let out = list_ids(&p);
        assert!(out.contains("project: My Rig"), "{out}");
        assert!(
            out.contains("chain chain:abc  instrument=guitar  enabled"),
            "{out}"
        );
        assert!(
            out.contains("  block chain:abc:block:def  input  enabled"),
            "{out}"
        );
        assert!(out.contains("(chains: 1)"), "{out}");
    }

    #[test]
    fn marks_disabled_block_and_empty_chain() {
        let p = project(vec![
            chain("chain:x", vec![input_block("chain:x:block:y", false)]),
            chain("chain:empty", vec![]),
        ]);
        let out = list_ids(&p);
        assert!(
            out.contains("block chain:x:block:y  input  disabled"),
            "{out}"
        );
        assert!(out.contains("  (no blocks)"), "{out}");
        assert!(out.contains("(chains: 2)"), "{out}");
    }
}
