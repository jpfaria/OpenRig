//! #1105: every LV2 package shipped in this repo must give each of its
//! parameters a default. A parameter without one makes `params: {}` (a new
//! block) fail with "missing required parameter", and the chain drops the
//! block silently.

use plugin_loader::manifest::Backend;
use std::path::PathBuf;

#[test]
fn every_shipped_lv2_parameter_has_a_default() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins/source/lv2");
    let mut missing: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for package in plugin_loader::discover::discover(&root)
        .expect("read plugins/source/lv2")
        .into_iter()
        .flatten()
    {
        let Backend::Lv2 {
            plugin_uri,
            binaries,
            ..
        } = &package.manifest.backend
        else {
            continue;
        };
        checked += 1;
        for spec in super::lv2_parameters(&package, plugin_uri, binaries) {
            if spec.default_value.is_none() {
                missing.push(format!("{}: {}", package.manifest.id, spec.path));
            }
        }
    }
    assert!(checked > 0, "no LV2 packages under {}", root.display());
    assert!(
        missing.is_empty(),
        "{} parameter(s) without a default:\n  - {}",
        missing.len(),
        missing.join("\n  - ")
    );
}
