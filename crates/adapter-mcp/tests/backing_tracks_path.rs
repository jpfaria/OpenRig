//! `openrig://paths` names the user's backing-track folder, so an MCP client
//! knows where to drop a track before loading it into the player.

use std::path::PathBuf;

use application::query::ResolvedPaths;
use infra_filesystem::AssetPaths;
use serde_json::Value;

fn reported(paths: &AssetPaths) -> Option<String> {
    let json: Value =
        serde_json::from_str(&ResolvedPaths::from_app_config(paths).to_json()).expect("JSON");
    json.get("backing_tracks_path")
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[test]
fn the_default_backing_tracks_folder_is_reported() {
    assert_eq!(
        reported(&AssetPaths::default()),
        Some(
            infra_filesystem::default_backing_tracks_path()
                .to_string_lossy()
                .into_owned()
        )
    );
}

#[test]
fn an_overridden_backing_tracks_folder_is_reported() {
    let paths = AssetPaths {
        backing_tracks_path: Some(PathBuf::from("/music/tracks")),
        ..AssetPaths::default()
    };
    assert_eq!(reported(&paths), Some("/music/tracks".to_string()));
}
