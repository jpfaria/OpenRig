use super::*;
use domain::value_objects::ParameterValue;

/// A cab whose `preset` axis was renamed to a `distance` knob after a
/// project saved one of its blocks.
fn edited_cab(id: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join(id);
    std::fs::create_dir_all(dir.join("captures")).unwrap();
    std::fs::create_dir_all(dir.join("versions")).unwrap();
    for file in ["000.wav", "001.wav"] {
        std::fs::write(dir.join("captures").join(file), b"RIFF").unwrap();
    }
    let head = format!(
        "manifest_version: 1\nid: {id}\ndisplay_name: Edited Cab\ntype: cab\nbackend: ir\n"
    );
    let before = "parameters:\n  - name: preset\n    values: [Cap, Cone]\ncaptures:\n  - values: { preset: Cap }\n    file: captures/000.wav\n  - values: { preset: Cone }\n    file: captures/001.wav\n";
    let after = "parameters:\n  - name: distance\n    values: [0, 2]\ncaptures:\n  - values: { distance: 0 }\n    file: captures/000.wav\n  - values: { distance: 2 }\n    file: captures/001.wav\n";
    std::fs::write(dir.join("versions/1.yaml"), format!("{head}{before}")).unwrap();
    std::fs::write(dir.join("versions/2.yaml"), format!("{head}{after}")).unwrap();
    std::fs::write(dir.join("manifest.yaml"), format!("{head}{after}")).unwrap();
    plugin_loader::registry::load_one(id, &[root.path().to_path_buf()]).unwrap();
    root
}

#[test]
fn a_block_saved_before_its_plugin_was_edited_loads_on_the_same_capture() {
    let id = "infra_yaml_edited_cab";
    let _root = edited_cab(id);
    let raw: Value = serde_yaml::from_str("preset: Cone").unwrap();

    let params = load_model_params(block_core::EFFECT_TYPE_CAB, id, raw).unwrap();

    assert_eq!(params.get("distance"), Some(&ParameterValue::Float(2.0)));
    assert_eq!(params.get("preset"), None);
    let _ = plugin_loader::registry::unload(id);
}
