//! #873 guard — a bridge property the Rust publishes MUST be read by a surface.
//!
//! The split moved shared UI state onto Slint globals. The failure mode it
//! introduced is silent: Rust keeps calling
//! `SomeBridge::get(&window).set_foo(..)`, the value lands in the global, and
//! nothing renders it because the surface still reads a local property that no
//! longer gets written — or was deleted along with the move. Nothing fails to
//! compile. Nothing fails a unit test. The feature is just dead on screen.
//!
//! That is how the tuner and spectrum windows shipped blank (the owner caught
//! them by eye), and the same slip hit the block editor's drawer title, its
//! status message, the model picker and the chain editor's device selectors.
//!
//! So this test cross-checks the two halves mechanically: every
//! `Bridge.property` written from Rust has to be consumed — read by a `.slint`
//! surface, or read back by Rust itself as state. A property nobody consumes is
//! either a dead publish or a disconnected surface; both are bugs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn files_with_ext(root: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        // Vendored UI is not ours to police.
        if path.to_string_lossy().contains("surrealism-ui") {
            continue;
        }
        if path.is_dir() {
            files_with_ext(&path, ext, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            out.push(path);
        }
    }
}

/// `set_foo_bar` in Rust addresses `foo-bar` in Slint.
fn to_slint_name(rust_name: &str) -> String {
    rust_name.replace('_', "-")
}

/// Every `(Bridge, property)` the Rust publishes, with the files that publish it.
fn published() -> BTreeMap<(String, String), BTreeSet<String>> {
    let mut out: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    let mut rs = Vec::new();
    files_with_ext(&crate_dir().join("src"), "rs", &mut rs);
    for path in rs {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for (bridge, prop) in scan_calls(&text, ".set_") {
            out.entry((bridge, to_slint_name(&prop)))
                .or_default()
                .insert(name.clone());
        }
    }
    out
}

/// Every `(Bridge, property)` the Rust reads back — legitimate state, not render.
fn read_back_by_rust() -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    let mut rs = Vec::new();
    files_with_ext(&crate_dir().join("src"), "rs", &mut rs);
    for path in rs {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for (bridge, prop) in scan_calls(&text, ".get_") {
            out.insert((bridge, to_slint_name(&prop)));
        }
    }
    out
}

/// Find `SomeBridge::get(<anything>)<verb><name>(`.
fn scan_calls(text: &str, verb: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for (idx, _) in text.match_indices("Bridge::get(") {
        // Walk back over the bridge identifier.
        let head = &text[..idx + "Bridge".len()];
        let start = head
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let bridge = head[start..].to_string();
        let Some(close) = text[idx..].find(')') else {
            continue;
        };
        let rest = &text[idx + close + 1..];
        let Some(stripped) = rest.strip_prefix(verb) else {
            continue;
        };
        let end = stripped
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(stripped.len());
        if end > 0 {
            found.push((bridge, stripped[..end].to_string()));
        }
    }
    found
}

/// A property counts as consumed by the UI only when a `.slint` file reads it
/// THROUGH THE BRIDGE — `SomeBridge.the-property`.
///
/// Matching the bare property name is not enough, and that laxness is not
/// hypothetical: the first version of this test accepted a pass-through line
/// like `spectrum-rows: [];` as a read, so deleting the real
/// `AnalyzerBridge.spectrum-rows` binding left the test green. A mutation run
/// caught it. Anchor on the bridge or the guard guards nothing.
fn consumed_by_slint(bridge: &str, prop: &str) -> bool {
    let mut slint = Vec::new();
    files_with_ext(&crate_dir().join("ui"), "slint", &mut slint);
    let needle = format!("{bridge}.{prop}");
    for path in slint {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for line in text.lines() {
            if let Some(at) = line.find(&needle) {
                // Reject `Bridge.prop-with-longer-name` matching `prop`.
                let after = line[at + needle.len()..].chars().next();
                if !matches!(after, Some(c) if c.is_alphanumeric() || c == '-' || c == '_') {
                    return true;
                }
            }
        }
    }
    false
}

/// Published by Rust, rendered by nobody — and already so BEFORE #873.
///
/// Verified against `origin/release/v0.4.0`: each of these appears in the UI
/// only as a declaration and as pass-through plumbing (`foo: root.foo;`), never
/// as a `text:`, an `if`, or any other read that reaches the screen. The split
/// deleted the plumbing and left the publish, which is what makes them visible
/// here — it did not break them; they never worked.
///
/// They are NOT permission to add more: the list only shrinks. Deciding whether
/// each is a lost feature (the drawer's status message is written from twelve
/// call sites — somebody meant the user to read it) or dead weight to delete is
/// tracked separately.
const KNOWN_DEAD_BEFORE_873: &[(&str, &str)] = &[
    // Declared and passed hand to hand, never rendered — checked line by line
    // against the base. The drawer's status message is the loudest: twelve call
    // sites write it and no screen has ever shown it.
    ("BlockEditorBridge", "block-drawer-status-message"),
    ("BlockEditorBridge", "block-drawer-title"),
    ("BlockEditorBridge", "block-picker-title"),
    ("BlockEditorBridge", "show-block-model-picker"),
    ("ChainEditorBridge", "selected-chain-input-device-index"),
    ("ChainEditorBridge", "selected-chain-output-device-index"),
    // Survives in the base only inside comments ("`io-binding-names` stays for
    // the chain-level endpoint picker") — the picker never wired it up.
    ("SettingsBridge", "io-binding-names"),
    // `LanguageSelector` reads it, but #513 removed the last instantiation of
    // that component; language now lives in Settings → Language. The publish
    // outlived its only reader.
    ("SettingsBridge", "language-codes"),
];

#[test]
fn every_bridge_property_the_rust_publishes_is_consumed() {
    let published = published();
    assert!(
        published.len() > 20,
        "the scanner found almost nothing ({}) — it broke, fix the test before trusting it",
        published.len()
    );
    let rust_reads = read_back_by_rust();

    let mut orphans = Vec::new();
    let mut stale_debt = Vec::new();
    for (bridge, prop) in KNOWN_DEAD_BEFORE_873 {
        let key = (bridge.to_string(), prop.to_string());
        let still_published = published.contains_key(&key);
        let now_consumed = consumed_by_slint(bridge, prop) || rust_reads.contains(&key);
        if !still_published || now_consumed {
            stale_debt.push(format!("  {bridge}.{prop}"));
        }
    }
    assert!(
        stale_debt.is_empty(),
        "these are on the pre-#873 dead list but no longer belong there — \
         the list only shrinks, so delete the entr{}:\n{}\n",
        if stale_debt.len() == 1 { "y" } else { "ies" },
        stale_debt.join("\n")
    );

    for ((bridge, prop), writers) in &published {
        if rust_reads.contains(&(bridge.clone(), prop.clone())) {
            continue;
        }
        if consumed_by_slint(bridge, prop) {
            continue;
        }
        if KNOWN_DEAD_BEFORE_873
            .iter()
            .any(|(b, p)| b == bridge && p == prop)
        {
            continue;
        }
        orphans.push(format!(
            "  {bridge}.{prop}\n      published by: {}",
            writers.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }

    assert!(
        orphans.is_empty(),
        "{} bridge propert{} published by Rust that no surface reads and Rust \
         never reads back. Each one is a feature that silently renders nothing \
         — the tuner/spectrum blank-window bug (#873). Wire the surface to the \
         bridge, or stop publishing it:\n\n{}\n",
        orphans.len(),
        if orphans.len() == 1 { "y" } else { "ies" },
        orphans.join("\n")
    );
}
