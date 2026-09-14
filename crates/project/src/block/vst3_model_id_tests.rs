use super::*;
use std::path::PathBuf;
use vst3_host::{Vst3CatalogEntry, Vst3PluginInfo};

fn entry(model_id: &'static str, bundle: &str) -> Vst3CatalogEntry {
    Vst3CatalogEntry {
        model_id,
        display_name: model_id,
        brand: "",
        category: "Fx",
        info: Vst3PluginInfo {
            uid: [0; 16],
            name: model_id.to_string(),
            vendor: String::new(),
            category: "Fx".to_string(),
            bundle_path: PathBuf::from(bundle),
            params: Vec::new(),
            num_audio_inputs: 2,
            num_audio_outputs: 2,
        },
    }
}

#[test]
fn package_bundle_resolves_to_the_entry_discovered_from_it() {
    let catalog = [
        entry("vst3:Other:Other", "/plugins/vst3/other/bundles/Other.vst3"),
        entry(
            "vst3:RoomReverb:Room_Reverb",
            "/plugins/vst3/room_reverb/bundles/RoomReverb.vst3",
        ),
    ];
    let found = entry_for_bundle(
        &catalog,
        Path::new("/plugins/vst3/room_reverb/bundles/RoomReverb.vst3"),
    );
    assert_eq!(
        found.map(|e| e.model_id),
        Some("vst3:RoomReverb:Room_Reverb")
    );
}

#[test]
fn bundle_the_catalog_never_discovered_resolves_to_nothing() {
    let catalog = [entry(
        "vst3:RoomReverb:Room_Reverb",
        "/plugins/vst3/room_reverb/bundles/RoomReverb.vst3",
    )];
    assert!(entry_for_bundle(&catalog, Path::new("/elsewhere/Missing.vst3")).is_none());
}

#[test]
fn same_bundle_reached_through_a_different_spelling_still_resolves() {
    let root = std::env::temp_dir().join(format!("openrig-938-vst3-{}", std::process::id()));
    let bundle = root.join("vst3/room_reverb/bundles/RoomReverb.vst3");
    std::fs::create_dir_all(&bundle).unwrap();
    let catalog = [entry(
        "vst3:RoomReverb:Room_Reverb",
        Box::leak(bundle.to_string_lossy().into_owned().into_boxed_str()),
    )];
    let spelled = root.join("vst3/room_reverb/./bundles/../bundles/RoomReverb.vst3");
    assert_eq!(
        entry_for_bundle(&catalog, &spelled).map(|e| e.model_id),
        Some("vst3:RoomReverb:Room_Reverb")
    );
}
