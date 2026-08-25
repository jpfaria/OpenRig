//! Responsibility: reports what the plugin catalog holds.

use plugin_loader::manifest::Backend;
use std::fmt::Write;

use crate::json_fmt::json_escape;

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
        Vst3 => "vst3",
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
