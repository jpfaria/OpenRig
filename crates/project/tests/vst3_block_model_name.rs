//! #398 — a VST3 block shows its plugin's name, maker and kind under it, like
//! every other block. `model_display_name`/`model_brand`/`model_type_label`
//! had no VST3 arm and returned `""` for a `vst3:{bundle}:{class}` model, so
//! the chain graph drew the block with no name.

use std::fs;
use std::path::PathBuf;

use project::catalog::{model_brand, model_display_name, model_type_label};

const NAME: &str = "OpenRigFixtureVerb398";
const VENDOR: &str = "FixtureWorks";

/// A catalog with one VST3 bundle discovered from a plugins-root-shaped
/// folder, returning that plugin's model id.
fn fixture_model_id() -> &'static str {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("vst3_block_model_name");
    let resources = root
        .join("vst3/fixture_verb/bundles")
        .join(format!("{NAME}.vst3"))
        .join("Contents/Resources");
    fs::create_dir_all(&resources).expect("create fixture bundle");
    fs::write(
        resources.join("moduleinfo.json"),
        format!(
            r#"{{
  "Factory Info": {{ "Vendor": "{VENDOR}" }},
  "Classes": [
    {{ "CID": "398398398398398398398398398398AB", "Category": "Audio Module Class", "Name": "{NAME}" }}
  ]
}}"#
        ),
    )
    .expect("write moduleinfo.json");
    vst3_host::init_vst3_catalog(48_000.0, &[root.join("vst3")]);
    vst3_host::vst3_catalog()
        .iter()
        .find(|e| e.display_name == NAME)
        .map(|e| e.model_id)
        .expect("the fixture bundle is in the catalog")
}

#[test]
fn a_vst3_block_is_named_after_its_plugin() {
    let id = fixture_model_id();
    assert_eq!(model_display_name("vst3", id), NAME);
    assert_eq!(model_brand("vst3", id), VENDOR);
    assert_eq!(model_type_label("vst3", id), "VST3");
}

#[test]
fn a_vst3_model_missing_from_the_catalog_has_no_name() {
    fixture_model_id();
    assert_eq!(model_display_name("vst3", "vst3:Missing:Missing"), "");
    assert_eq!(model_type_label("vst3", "vst3:Missing:Missing"), "");
}
