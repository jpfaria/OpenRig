//! The drums section of the per-machine `config.yaml`.

use super::DrumsConfig;
use crate::AppConfig;

#[test]
fn an_old_config_without_drums_opens_with_defaults() {
    let config: AppConfig = serde_yaml::from_str("recent_projects: []\n").expect("parse");
    assert_eq!(config.drums, DrumsConfig::default());
    assert!(config.drums.kit.is_none());
}

#[test]
fn round_trips_through_yaml() {
    let drums = DrumsConfig {
        volume: 0.6,
        kit: Some("black-pearl".into()),
        groove: Some("funk-03".into()),
        output_device: Some("main\u{1f}Out 1/2".into()),
    };
    let yaml = serde_yaml::to_string(&drums).expect("serialize");
    let back: DrumsConfig = serde_yaml::from_str(&yaml).expect("parse");
    assert_eq!(back, drums);
}

#[test]
fn has_no_playing_flag() {
    let yaml = serde_yaml::to_string(&DrumsConfig::default()).expect("serialize");
    assert!(
        !yaml.contains("playing") && !yaml.contains("enabled"),
        "the drums always boot stopped, so the transport is never persisted"
    );
}

#[test]
fn an_old_config_with_a_drums_tempo_still_opens() {
    let config: DrumsConfig = serde_yaml::from_str("bpm: 92.5\nvolume: 0.6\n").expect("parse");
    assert_eq!(config.volume, 0.6);
}
