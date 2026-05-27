//! Read-only project introspection for adapters. The project uses opaque
//! string IDs (`chain:<uuid>`, `chain:<uuid>:block:<uuid>`), not ordinals,
//! so a `midi-map.yaml` author (or the MCP `openrig://ids` resource) needs a
//! way to discover them. This is the single place that formats that listing;
//! adapters never re-walk `Project` themselves.

use std::fmt::Write;

use domain::ids::ChainId;
use plugin_loader::manifest::Backend;
use project::project::Project;
use project::rig::RigProject;

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

/// #554: return the bank of chain-scoped presets for one chain as JSON.
///
/// The `chain_id` must be of the form `rig:<input-name>`. The input's
/// `bank: BTreeMap<usize, String>` (slot → preset name) is emitted as a
/// `slots` array sorted by slot index, plus the resolved `active_preset`
/// name (or `null` when the bank is empty / the active slot is unbound).
///
/// Reads from the in-memory `RigProject` only — never the filesystem.
/// The disk-side preset library (under `config.paths.presets_path`) is a
/// separate concept tracked by a different follow-up.
pub fn list_chain_presets(rig: &RigProject, chain_id: &ChainId) -> Result<String, String> {
    let input_name = chain_id.0.strip_prefix("rig:").ok_or_else(|| {
        format!(
            "chain id '{}' is not a rig: input (expected 'rig:<input-name>')",
            chain_id.0
        )
    })?;
    let input = rig.inputs.get(input_name).ok_or_else(|| {
        format!(
            "input '{input_name}' not found in project (chain id '{}' references no live input)",
            chain_id.0
        )
    })?;

    let mut slots = String::new();
    slots.push('[');
    let mut first = true;
    for (idx, preset_key) in input.bank.iter() {
        if !first {
            slots.push(',');
        }
        first = false;
        let label = rig
            .presets
            .get(preset_key)
            .and_then(|p| p.name.clone())
            .unwrap_or_else(|| preset_key.clone());
        let _ = write!(
            slots,
            "{{\"index\":{},\"name\":{},\"key\":{}}}",
            idx,
            json_string(&label),
            json_string(preset_key)
        );
    }
    slots.push(']');

    let active_preset = input
        .bank
        .get(&input.active_preset)
        .map(|key| {
            let label = rig
                .presets
                .get(key)
                .and_then(|p| p.name.clone())
                .unwrap_or_else(|| key.clone());
            json_string(&label)
        })
        .unwrap_or_else(|| "null".to_string());

    Ok(format!(
        "{{\"chain\":{},\"active_preset\":{},\"slots\":{}}}",
        json_string(&chain_id.0),
        active_preset,
        slots
    ))
}

/// #554 follow-up: list every named preset in `RigProject.presets` —
/// the in-memory pool that the rig's input banks reference by key.
/// A preset can sit in the pool without being bound to any input slot
/// yet (e.g. the user saved it via the rig screen but hasn't wired it
/// into a chain). The tone-builder skill's Step 0 reads this to make
/// sure it doesn't silently overwrite an existing preset on save.
///
/// Each entry returns the user-visible label (`RigPreset.name`,
/// falling back to the pool key when the field is absent) AND the
/// pool key — the key is what the bank slots reference and what
/// `Command::DeleteChainPreset` / load operations take, so consumers
/// need both. Sorted by display label so the GUI's combobox and the
/// MCP read produce the same order.
///
/// Pure: `&RigProject` in, `String` out. Reads the in-memory rig
/// only — the on-disk preset library (`config.paths.presets_path`)
/// is a separate concern.
pub fn list_project_presets(rig: &RigProject) -> String {
    let mut entries: Vec<(&String, String)> = rig
        .presets
        .iter()
        .map(|(key, preset)| {
            let label = preset.name.clone().unwrap_or_else(|| key.clone());
            (key, label)
        })
        .collect();
    entries.sort_by(|(_, a), (_, b)| a.cmp(b));
    let mut out = String::from("{\"presets\":[");
    let mut first = true;
    for (key, label) in entries {
        if !first {
            out.push(',');
        }
        first = false;
        let _ = write!(
            out,
            "{{\"name\":{},\"key\":{}}}",
            json_string(&label),
            json_string(key)
        );
    }
    out.push_str("]}");
    out
}

/// Minimal JSON-string escaper that wraps the result in double quotes.
/// Used by the #554 preset listings; avoids dragging `serde_json` into a
/// pure listing helper — preset names and chain ids never carry control
/// chars deeper than `"`, `\` or whitespace.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    out.push_str(&json_escape(s));
    out.push('"');
    out
}

/// JSON-escape a string for inclusion in a manually-built JSON literal.
/// Does NOT wrap the result in quotes — callers handle quoting (see
/// [`json_string`] when both escape + quote are wanted).
fn json_escape(s: &str) -> String {
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

/// #561 (expanded scope): text search across the catalog.
/// Case-insensitive substring match against `id`, `display_name`, and
/// `brand`. Empty query returns every entry (same as
/// [`list_plugin_catalog`]) — lets the agent treat search and listing
/// as one tool. Same JSON envelope as the listing.
pub fn find_plugins(query: &str) -> String {
    let needle = query.to_lowercase();
    let mut out = String::from("{\"plugins\": [");
    let mut first = true;
    for p in plugin_loader::registry::packages() {
        let matches = needle.is_empty()
            || p.manifest.id.to_lowercase().contains(&needle)
            || p.manifest.display_name.to_lowercase().contains(&needle)
            || p.manifest
                .brand
                .as_deref()
                .is_some_and(|b| b.to_lowercase().contains(&needle));
        if !matches {
            continue;
        }
        if !first {
            out.push_str(", ");
        }
        first = false;
        plugin_entry_json(p, &mut out);
    }
    out.push_str("]}");
    out
}

/// #572: parameter schema for one plugin (catalog-level). Looks the
/// plugin up in `plugin_loader::registry` by manifest id and returns
/// the `ModelParameterSchema` as JSON under a `params` envelope.
/// Unknown id → `{"params": null}` (same null-wrap idiom as
/// [`get_plugin`]). Pure read; the registry itself is process-wide
/// static state populated at startup.
pub fn get_plugin_params(plugin_id: &str) -> String {
    if plugin_loader::registry::find(plugin_id).is_none() {
        return "{\"params\": null}".to_string();
    }
    // Happy-path serialization lands in the next red-first cycle; the
    // current contract only covers the unknown-id null envelope.
    "{\"params\": null}".to_string()
}

#[cfg(test)]
#[path = "query_chain_presets_tests.rs"]
mod chain_presets_tests;

#[cfg(test)]
#[path = "query_project_presets_tests.rs"]
mod project_presets_tests;

#[cfg(test)]
#[path = "query_plugin_params_tests.rs"]
mod plugin_params_tests;

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
