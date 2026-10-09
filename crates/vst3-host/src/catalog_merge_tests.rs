use super::merge_discovered;
use crate::discovery::Vst3PluginInfo;
use std::path::PathBuf;

fn info(bundle: &str, name: &str) -> Vst3PluginInfo {
    Vst3PluginInfo {
        uid: [0; 16],
        name: name.into(),
        vendor: String::new(),
        category: String::new(),
        bundle_path: PathBuf::from(bundle),
        params: Vec::new(),
        num_audio_inputs: 0,
        num_audio_outputs: 0,
    }
}

#[test]
fn bundled_copy_wins_over_system_copy_with_the_same_id() {
    let system = vec![info(
        "/Library/Audio/Plug-Ins/VST3/Airwindows Consolidated.vst3",
        "Airwindows Consolidated",
    )];
    let bundled = vec![info(
        "/plugins/vst3/airwindows_consolidated/bundles/Airwindows Consolidated.vst3",
        "Airwindows Consolidated",
    )];
    let merged = merge_discovered(system, bundled);
    assert_eq!(merged.len(), 1);
    assert!(merged[0].bundle_path.starts_with("/plugins"));
}

#[test]
fn plugins_found_in_only_one_place_are_all_kept() {
    let system = vec![info("/Library/Audio/Plug-Ins/VST3/A.vst3", "A")];
    let bundled = vec![info("/plugins/vst3/b/bundles/B.vst3", "B")];
    let merged = merge_discovered(system, bundled);
    let names: Vec<_> = merged.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"A") && names.contains(&"B"));
}
