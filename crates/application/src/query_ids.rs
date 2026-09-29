//! Responsibility: lists the ids a transport needs to address the project.

use project::block::AudioBlockKind;
use project::project::Project;
use std::fmt::Write;

/// Human-readable, copy-paste-ready listing of every chain and block with
/// its full ID, instrument/kind, and enabled state — the values that go
/// into `midi-map.yaml` `chain:` / `block:`. #328: the blocks inside a
/// split's paths are listed under the split, one `path a` / `path b` row
/// each, so they can be addressed too.
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
            let _ = writeln!(
                out,
                "  block {}  {}  {}",
                b.id.0,
                b.kind.label(),
                block_state(b.enabled)
            );
            if let AudioBlockKind::Split(split) = &b.kind {
                for (side, lane) in [("a", &split.a), ("b", &split.b)] {
                    for p in lane {
                        let _ = writeln!(
                            out,
                            "    path {side}  block {}  {}  {}",
                            p.id.0,
                            p.kind.label(),
                            block_state(p.enabled)
                        );
                    }
                }
            }
        }
    }
    let _ = writeln!(out, "(chains: {})", project.chains.len());
    out
}

fn block_state(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}
