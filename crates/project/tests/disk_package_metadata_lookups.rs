//! Issue #414 — `model_display_name`, `model_brand` and `model_type_label`
//! must resolve disk-package models (NAM/IR/LV2) via the plugin_loader
//! registry, not just the native block_* tables. Previously they only
//! consulted natives and returned `""` for NAM blocks, which collapsed the
//! hover tooltip (gated by `display_name != ""`).

use project::catalog::{model_brand, model_display_name, model_type_label};

fn init_plugins() {
    use std::path::PathBuf;
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let candidates = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../../../OpenRig-plugins/plugins/source"),
            PathBuf::from(
                "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source",
            ),
        ];
        let roots: Vec<PathBuf> = candidates.into_iter().filter(|p| p.is_dir()).collect();
        if !roots.is_empty() {
            plugin_loader::registry::init_many(&roots);
        }
    });
}

/// Pick the first disk-package model id from `supported_block_models(amp)`
/// that isn't a native amp — so the test is resilient to packages being
/// added/removed in OpenRig-plugins.
fn first_nam_amp_id() -> String {
    use project::catalog::supported_block_models;
    supported_block_models("amp")
        .expect("amp catalog")
        .into_iter()
        .find(|m| m.model_id.starts_with("nam_"))
        .map(|m| m.model_id)
        .expect("at least one NAM amp expected in OpenRig-plugins")
}

#[test]
fn nam_amp_display_name_resolves() {
    init_plugins();
    let id = first_nam_amp_id();
    let name = model_display_name("amp", &id);
    assert!(
        !name.is_empty(),
        "expected non-empty display_name for NAM amp `{}` — tooltip hover relies on this",
        id,
    );
}

#[test]
fn nam_amp_type_label_is_nam() {
    init_plugins();
    let id = first_nam_amp_id();
    let label = model_type_label("amp", &id);
    assert_eq!(
        label, "NAM",
        "NAM disk-package should report NAM type label"
    );
}

#[test]
fn nam_amp_brand_resolves() {
    init_plugins();
    let id = first_nam_amp_id();
    let brand = model_brand("amp", &id);
    assert!(
        !brand.is_empty(),
        "expected non-empty brand for NAM amp `{}` (manifest.brand)",
        id,
    );
}

/// Native model still resolves through its block-* registry (untouched
/// fast path).
#[test]
fn native_model_display_name_unchanged() {
    init_plugins();
    let name = model_display_name("dynamics", "compressor_studio_clean");
    assert!(!name.is_empty(), "native model should keep resolving");
}

/// Unknown model returns empty (no panic / no false-positive).
#[test]
fn unknown_model_returns_empty() {
    init_plugins();
    assert_eq!(model_display_name("amp", "does_not_exist_xyz"), "");
    assert_eq!(model_brand("amp", "does_not_exist_xyz"), "");
    assert_eq!(model_type_label("amp", "does_not_exist_xyz"), "");
}
