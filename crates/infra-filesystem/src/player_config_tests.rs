use super::*;
use crate::{AppConfig, AssetPaths};

#[test]
fn a_config_without_a_player_section_gets_the_defaults() {
    let config: AppConfig = serde_yaml::from_str("{}").unwrap();
    assert_eq!(config.player, PlayerConfig::default());
    assert_eq!(config.player.volume, 0.8);
    assert_eq!(config.player.output_device, None);
}

#[test]
fn the_player_section_round_trips() {
    let config = AppConfig {
        player: PlayerConfig {
            volume: 0.35,
            output_device: Some("main\u{1f}Out 1/2".into()),
        },
        ..AppConfig::default()
    };
    let yaml = serde_yaml::to_string(&config).unwrap();
    let restored: AppConfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(restored.player, config.player);
}

#[test]
fn the_backing_tracks_folder_round_trips() {
    let paths = AssetPaths {
        backing_tracks_path: Some("/music/jams".into()),
        ..AssetPaths::default()
    };
    let yaml = serde_yaml::to_string(&paths).unwrap();
    let restored: AssetPaths = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(
        restored.backing_tracks_path.as_deref(),
        Some(std::path::Path::new("/music/jams"))
    );
}

#[test]
fn user_backing_tracks_default_to_the_user_data_root() {
    assert_eq!(
        crate::default_backing_tracks_path(),
        crate::user_data_root().join("backing-tracks")
    );
}

#[test]
fn bundled_backing_tracks_ship_with_the_app_assets() {
    assert_eq!(
        crate::bundled_backing_tracks_path(),
        crate::detect_data_root().join("assets/backing-tracks")
    );
}

#[test]
fn resolving_asset_paths_keeps_the_backing_tracks_override() {
    let paths = AssetPaths {
        backing_tracks_path: Some("/music/jams".into()),
        ..AssetPaths::default()
    };
    let resolved = crate::resolve_asset_paths(paths);
    assert_eq!(
        resolved.backing_tracks_path.as_deref(),
        Some(std::path::Path::new("/music/jams"))
    );
}
