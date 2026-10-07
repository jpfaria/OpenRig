//! #879: the TONE3000 section of the per-machine `config.yaml`.

use super::Tone3000Config;
use crate::AppConfig;

const KEY: &str = "t3k_cs_fixture_key_0123456789";

#[test]
fn an_old_config_without_tone3000_has_no_key() {
    let config: AppConfig = serde_yaml::from_str("recent_projects: []\n").expect("parse");
    assert_eq!(config.tone3000, Tone3000Config::default());
    assert!(config.tone3000.api_key.is_none());
}

#[test]
fn the_key_round_trips_through_the_config() {
    let config = AppConfig {
        tone3000: Tone3000Config {
            api_key: Some(KEY.into()),
        },
        ..AppConfig::default()
    };
    let yaml = serde_yaml::to_string(&config).expect("serialize");
    let back: AppConfig = serde_yaml::from_str(&yaml).expect("parse");
    assert_eq!(back.tone3000.api_key.as_deref(), Some(KEY));
}

#[test]
fn no_key_writes_no_section() {
    let yaml = serde_yaml::to_string(&AppConfig::default()).expect("serialize");
    assert!(!yaml.contains("tone3000"), "{yaml}");
}

#[test]
fn debug_output_hides_the_key() {
    let config = Tone3000Config {
        api_key: Some(KEY.into()),
    };
    let shown = format!("{config:?}");
    assert!(!shown.contains(KEY), "{shown}");
    assert!(shown.contains("configured"), "{shown}");
}
