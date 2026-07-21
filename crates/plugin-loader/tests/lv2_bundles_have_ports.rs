//! Integration regression: every LV2 bundle in the OpenRig-plugins
//! repo must surface at least one control port through
//! `scan_lv2_ports`. If a new bundle ships a manifest that the
//! parser can't reconcile with its TTLs, this test names it. Issue #287.
//!
//! Skipped when the OpenRig-plugins repo isn't available locally —
//! resolved from `OPENRIG_OWNER_PLUGINS` or a sibling `OpenRig-plugins`
//! checkout, so it runs on any developer machine that has both repos.

use std::fs;
use std::path::{Path, PathBuf};

/// The LV2 subtree of the owner's capture repo, from `OPENRIG_OWNER_PLUGINS`
/// or a sibling `OpenRig-plugins` checkout at any depth. `None` when absent.
fn plugins_repo_lv2() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("OPENRIG_OWNER_PLUGINS") {
        let lv2 = PathBuf::from(p).join("lv2");
        if lv2.is_dir() {
            return Some(lv2);
        }
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let cand = dir.join("OpenRig-plugins/plugins/source/lv2");
        if cand.is_dir() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn read_plugin_uri(manifest_path: &Path) -> Option<String> {
    let text = fs::read_to_string(manifest_path).ok()?;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("plugin_uri:") {
            let uri = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if !uri.is_empty() {
                return Some(uri);
            }
        }
    }
    None
}

fn bundle_data_dir(bundle: &Path) -> PathBuf {
    let data = bundle.join("data");
    if data.is_dir() {
        data
    } else {
        bundle.to_path_buf()
    }
}

#[test]
#[ignore = "depends on the OpenRig-plugins repo being checked out at a fixed path; run with `cargo test -- --ignored`"]
fn every_lv2_bundle_surfaces_at_least_one_control_port() {
    let Some(root) = plugins_repo_lv2() else {
        eprintln!(
            "skip: OpenRig-plugins LV2 tree not present (set OPENRIG_OWNER_PLUGINS=<OpenRig-plugins/plugins/source>)"
        );
        return;
    };
    let root = root.as_path();

    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for entry in fs::read_dir(root).expect("read lv2 dir") {
        let entry = entry.expect("dir entry");
        let bundle = entry.path();
        if !bundle.is_dir() {
            continue;
        }
        let manifest = bundle.join("manifest.yaml");
        if !manifest.is_file() {
            continue;
        }
        let Some(plugin_uri) = read_plugin_uri(&manifest) else {
            failures.push(format!(
                "{}: manifest.yaml has no plugin_uri",
                bundle.file_name().unwrap().to_string_lossy()
            ));
            continue;
        };
        let data_dir = bundle_data_dir(&bundle);
        checked += 1;
        match plugin_loader::dispatch::scan_lv2_ports(&data_dir, &plugin_uri) {
            Ok(ports) => {
                let control = ports
                    .iter()
                    .filter(|p| p.role == plugin_loader::dispatch::Lv2PortRole::ControlIn)
                    .count();
                if control == 0 {
                    failures.push(format!(
                        "{}: 0 control ports (uri={plugin_uri})",
                        bundle.file_name().unwrap().to_string_lossy()
                    ));
                }
            }
            Err(err) => {
                failures.push(format!(
                    "{}: scan_lv2_ports failed (uri={plugin_uri}): {err}",
                    bundle.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }

    assert_ne!(checked, 0, "no LV2 bundles found under {}", root.display());
    assert!(
        failures.is_empty(),
        "{}/{} LV2 bundles surface no control ports:\n  - {}",
        failures.len(),
        checked,
        failures.join("\n  - ")
    );
}
