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
/// Unknown id (or schema resolution failure) → `{"params": null}`
/// (same null-wrap idiom as [`get_plugin`]). Pure read; the registry
/// itself is process-wide static state populated at startup.
pub fn get_plugin_params(plugin_id: &str) -> String {
    let Some(package) = plugin_loader::registry::find(plugin_id) else {
        return "{\"params\": null}".to_string();
    };
    let effect_type = block_type_str(&package.manifest.block_type);
    let Ok(schema) = project::block::schema_for_block_model(&effect_type, plugin_id) else {
        return "{\"params\": null}".to_string();
    };
    match serde_json::to_string(&schema) {
        Ok(json) => format!("{{\"params\": {json}}}"),
        Err(_) => "{\"params\": null}".to_string(),
    }
}

/// Snake_case string for a `BlockType` (matches its serde tag —
/// `Preamp` → `"preamp"`, `GainPedal` → `"gain_pedal"`). Used by
/// `get_plugin_params` to feed `schema_for_block_model`'s
/// effect-type argument; failure to render falls back to an empty
/// string so the caller surfaces a clean null envelope rather than
/// panicking on an unexpected new variant.
fn block_type_str(bt: &plugin_loader::manifest::BlockType) -> String {
    serde_json::to_value(bt)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

/// #572: list of materialised `BlockParameterDescriptor` for one placed
/// block (schema + `current_value` per parameter), serialised under a
/// `params` envelope. Mirrors what `list_chain_presets` does for the
/// preset bank — looks up the chain in the project, finds the block,
/// then delegates to `AudioBlock::parameter_descriptors()` (the same
/// helper the GUI uses) so the schema + current-value walk lives in
/// `project::block`, never re-derived per transport. Unknown chain /
/// block / schema mismatch → `Err`.
pub fn get_block_params(
    project: &project::project::Project,
    chain: &domain::ids::ChainId,
    block: &domain::ids::BlockId,
) -> Result<String, String> {
    let chain_ref = project
        .chains
        .iter()
        .find(|c| c.id == *chain)
        .ok_or_else(|| format!("chain not found: {}", chain.0))?;
    let block_ref = chain_ref
        .blocks
        .iter()
        .find(|b| b.id == *block)
        .ok_or_else(|| format!("block not found in chain {}: {}", chain.0, block.0))?;
    let descriptors = block_ref.parameter_descriptors()?;
    let payload = serde_json::to_string(&descriptors)
        .map_err(|e| format!("failed to serialize descriptors: {e}"))?;
    Ok(format!("{{\"params\": {payload}}}"))
}

/// #582: effective resolved system paths the user-facing tooling
/// reads at runtime. Serialised as JSON for the `openrig://paths`
/// MCP resource. The struct shape — not an ad-hoc `serde_json::json!`
/// — exists so adding a new path field to [`infra_filesystem::AssetPaths`]
/// is a hard compile error here too; the envelope cannot silently drift
/// from the source of truth.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ResolvedPaths {
    /// User data root for the current install (`~/Library/Application
    /// Support/OpenRig` on macOS, `%APPDATA%\OpenRig` on Windows,
    /// `~/.local/share/openrig` on Linux). All `None` overrides
    /// resolve to a subfolder of this root.
    pub data_root: String,
    /// Effective preset directory: the override when set in
    /// `config.yaml`, otherwise `<data_root>/presets`.
    pub presets_path: String,
    /// Effective plugin directory: the override when set in
    /// `config.yaml`, otherwise `<data_root>/plugins`.
    pub plugins_path: String,
    /// Effective evaluations directory (#582): the override when set in
    /// `config.yaml`, otherwise `<data_root>/evaluations` per
    /// [`infra_filesystem::default_evaluations_path`].
    pub evaluations_path: String,
}

impl ResolvedPaths {
    /// Build a [`ResolvedPaths`] from the user-side `config.yaml`
    /// (`AppConfig.paths`). Each `None` override falls back to the OS
    /// default — consumers (skills, MCP clients) read absolute paths
    /// without re-implementing the per-platform default themselves.
    pub fn from_app_config(paths: &infra_filesystem::AssetPaths) -> Self {
        let root = infra_filesystem::user_data_root();
        let default_presets = root.join("presets");
        let default_plugins = root.join("plugins");
        let evaluations = paths
            .evaluations_path
            .clone()
            .unwrap_or_else(infra_filesystem::default_evaluations_path);
        Self {
            data_root: root.to_string_lossy().into_owned(),
            presets_path: paths
                .presets_path
                .clone()
                .unwrap_or(default_presets)
                .to_string_lossy()
                .into_owned(),
            plugins_path: paths
                .plugins_path
                .clone()
                .unwrap_or(default_plugins)
                .to_string_lossy()
                .into_owned(),
            evaluations_path: evaluations.to_string_lossy().into_owned(),
        }
    }

    /// Stable JSON wire shape for the `openrig://paths` resource. The
    /// `to_string` is infallible for this concrete struct (all fields
    /// are `String`), so the helper exposes a `String` to keep callers
    /// out of `Result` plumbing.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("ResolvedPaths serializes")
    }
}

/// #582: load the user-side `config.yaml`, resolve every override
/// (falling back to OS defaults), and serialise the
/// [`ResolvedPaths`] envelope to JSON for the `openrig://paths`
/// resource resolver in `adapter-mcp` (via the bridge query channel).
pub fn resolved_paths_json() -> String {
    let config = infra_filesystem::FilesystemStorage::load_app_config().unwrap_or_default();
    ResolvedPaths::from_app_config(&config.paths).to_json()
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
                io: String::new(),
                endpoint: String::new(),
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
            io_binding_ids: vec![],
            blocks,
            di_output: None,
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
