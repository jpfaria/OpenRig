//! One-shot tool that walks every LV2 bundle under
//! `OpenRig-plugins/plugins/source/lv2/` and rewrites the
//! `plugin_uri:` line of each `manifest.yaml` to match the URI
//! actually declared inside the bundle's TTL.
//!
//! Idempotent: bundles whose manifest URI already matches the TTL
//! are left untouched.
//!
//! Usage:
//!   cargo run -p plugin-loader --example fix_lv2_manifest_uris -- [/path/to/lv2/dir]
//!
//! Default path: ~/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source/lv2

use std::fs;
use std::path::PathBuf;

const DEFAULT_LV2_DIR: &str =
    "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source/lv2";

fn main() {
    let lv2_dir: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_LV2_DIR));

    if !lv2_dir.is_dir() {
        eprintln!("error: {} is not a directory", lv2_dir.display());
        std::process::exit(2);
    }

    let mut fixed = 0usize;
    let mut already_ok = 0usize;
    let mut skipped: Vec<String> = Vec::new();
    let mut total = 0usize;

    for entry in fs::read_dir(&lv2_dir).expect("read lv2 dir") {
        let entry = entry.expect("dir entry");
        let bundle = entry.path();
        if !bundle.is_dir() {
            continue;
        }
        let manifest_path = bundle.join("manifest.yaml");
        if !manifest_path.is_file() {
            continue;
        }
        total += 1;
        let bundle_name = bundle.file_name().unwrap().to_string_lossy().to_string();

        let data_dir = if bundle.join("data").is_dir() {
            bundle.join("data")
        } else {
            bundle.clone()
        };

        let Some(actual_uri) = plugin_loader::dispatch_infer::infer_plugin_uri(&data_dir) else {
            skipped.push(format!(
                "{bundle_name}: could not infer URI (zero or multiple plugins, or no .ttl)"
            ));
            continue;
        };

        let manifest_text = match fs::read_to_string(&manifest_path) {
            Ok(t) => t,
            Err(e) => {
                skipped.push(format!("{bundle_name}: read manifest: {e}"));
                continue;
            }
        };

        let Some(current_uri) = read_plugin_uri_line(&manifest_text) else {
            skipped.push(format!("{bundle_name}: manifest has no plugin_uri line"));
            continue;
        };

        if current_uri == actual_uri {
            already_ok += 1;
            continue;
        }

        let new_text = rewrite_plugin_uri(&manifest_text, &actual_uri);
        if new_text == manifest_text {
            skipped.push(format!(
                "{bundle_name}: rewrite produced identical text (regex bug?)"
            ));
            continue;
        }

        if let Err(e) = fs::write(&manifest_path, new_text) {
            skipped.push(format!("{bundle_name}: write manifest: {e}"));
            continue;
        }
        println!("{bundle_name}: {current_uri} -> {actual_uri}");
        fixed += 1;
    }

    println!();
    println!("checked {total} bundle(s)");
    println!("rewrote {fixed} manifest(s)");
    println!("already-correct {already_ok} manifest(s)");
    if !skipped.is_empty() {
        println!("skipped {}:", skipped.len());
        for s in &skipped {
            println!("  - {s}");
        }
    }
}

fn read_plugin_uri_line(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("plugin_uri:") {
            let value = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn rewrite_plugin_uri(text: &str, new_uri: &str) -> String {
    let mut out = String::with_capacity(text.len() + new_uri.len());
    let mut replaced = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if !replaced && trimmed.starts_with("plugin_uri:") {
            // Preserve original indentation.
            let indent_len = line.len() - trimmed.len();
            out.push_str(&line[..indent_len]);
            out.push_str("plugin_uri: ");
            out.push_str(new_uri);
            // Keep the original line ending (newline if present).
            if line.ends_with('\n') {
                out.push('\n');
            }
            replaced = true;
            continue;
        }
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_preserves_other_lines_and_indentation() {
        let original = "manifest_version: 1\n\
            id: lv2_x\n\
            plugin_uri: http://wrong.example/Plugin\n\
            backend: lv2\n";
        let new_text = rewrite_plugin_uri(original, "http://right.example/Plugin");
        assert_eq!(
            new_text,
            "manifest_version: 1\n\
             id: lv2_x\n\
             plugin_uri: http://right.example/Plugin\n\
             backend: lv2\n"
        );
    }

    #[test]
    fn read_plugin_uri_strips_quotes() {
        assert_eq!(
            read_plugin_uri_line("plugin_uri: \"http://x.example/y\"\n"),
            Some("http://x.example/y".to_string())
        );
    }
}
